use crate::capture::{capture, sample_color};
use crate::counter::{CounterEvent, CounterState};
use crate::types::{CaptureSource, Color};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Public event type — sent from worker to UI thread via the pending queue.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum SampleEvent {
    /// A group incremented its counter.
    Incremented { group_idx: usize, new_count: u32 },
    /// A group re-armed after being disarmed.
    Armed { group_idx: usize },
    /// The capture source failed (error message).
    CaptureError(String),
}

// ---------------------------------------------------------------------------
// Shared config pushed from the UI thread into the worker.
// ---------------------------------------------------------------------------

/// Per-group sampling config — cloned from Preset data so the worker doesn't
/// need to touch AppState directly.
#[derive(Debug, Clone)]
pub struct GroupConfig {
    pub pickers: Vec<(i32, i32, Color)>, // (x, y, target)
}

/// All parameters the worker needs. Replaced atomically when anything changes.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub groups: Vec<GroupConfig>,
    pub tolerance: u8,
    pub interval_ms: u64,
}

// ---------------------------------------------------------------------------
// Shared state between worker thread and UI thread.
// ---------------------------------------------------------------------------

struct Shared {
    /// Pending events from the worker — consumed by the UI thread each frame.
    events: Mutex<Vec<SampleEvent>>,
    /// Live sample colors per group (for the picker row display in the UI).
    live_samples: Mutex<Vec<Vec<Color>>>,
    /// Current config — written by UI thread, read by worker.
    config: Mutex<WorkerConfig>,
    /// Counter states, one per group — owned by the worker thread exclusively.
    counters: Mutex<Vec<CounterState>>,
    /// Per-group counts, kept in sync by the worker after each increment.
    counts: Mutex<Vec<u32>>,
    /// Set to false to stop the worker thread.
    alive: AtomicBool,
    /// Incremented each time the worker completes a full tick.
    tick_seq: AtomicU64,
}

// ---------------------------------------------------------------------------
// Public handle
// ---------------------------------------------------------------------------

pub struct CaptureWorker {
    shared: Arc<Shared>,
    source: CaptureSource,
}

impl CaptureWorker {
    pub fn start(source: CaptureSource, cfg: WorkerConfig) -> Self {
        let n_groups = cfg.groups.len();
        let shared = Arc::new(Shared {
            events: Mutex::new(Vec::new()),
            live_samples: Mutex::new(vec![Vec::new(); n_groups]),
            config: Mutex::new(cfg),
            counters: Mutex::new(vec![CounterState::default(); n_groups]),
            counts: Mutex::new(vec![0; n_groups]),
            alive: AtomicBool::new(true),
            tick_seq: AtomicU64::new(0),
        });

        let shared_clone = Arc::clone(&shared);
        let source_clone = source.clone();

        thread::Builder::new()
            .name("shiny-sampler".into())
            .spawn(move || worker_loop(shared_clone, source_clone))
            .expect("failed to spawn sampler thread");

        Self { shared, source }
    }

    pub fn source_changed(&self, current: &CaptureSource) -> bool {
        !sources_equal(&self.source, current)
    }

    pub fn update_config(&self, cfg: WorkerConfig) {
        let n = cfg.groups.len();
        *self.shared.config.lock() = cfg;
        self.shared.counters.lock().resize_with(n, CounterState::default);
        self.shared.counts.lock().resize(n, 0);
        self.shared.live_samples.lock().resize_with(n, Vec::new);
    }

    pub fn set_counts(&self, counts: &[u32]) {
        let mut c = self.shared.counts.lock();
        for (i, &v) in counts.iter().enumerate() {
            if let Some(slot) = c.get_mut(i) {
                *slot = v;
            }
        }
    }

    pub fn reset_counters(&self) {
        for cs in self.shared.counters.lock().iter_mut() {
            cs.reset();
        }
        for c in self.shared.counts.lock().iter_mut() {
            *c = 0;
        }
    }

    pub fn reset_counter(&self, gi: usize) {
        if let Some(cs) = self.shared.counters.lock().get_mut(gi) {
            cs.reset();
        }
    }

    pub fn set_count(&self, gi: usize, value: u32) {
        if let Some(slot) = self.shared.counts.lock().get_mut(gi) {
            *slot = value;
        }
    }

    pub fn drain_events(&self) -> Vec<SampleEvent> {
        std::mem::take(&mut self.shared.events.lock())
    }

    pub fn live_samples(&self, gi: usize) -> Vec<Color> {
        self.shared.live_samples.lock().get(gi).cloned().unwrap_or_default()
    }

    pub fn is_armed(&self, gi: usize) -> bool {
        self.shared.counters.lock().get(gi).map(|c| c.is_armed()).unwrap_or(true)
    }

    pub fn count(&self, gi: usize) -> u32 {
        self.shared.counts.lock().get(gi).copied().unwrap_or(0)
    }

    pub fn tick_seq(&self) -> u64 {
        self.shared.tick_seq.load(Ordering::Relaxed)
    }
}

impl Drop for CaptureWorker {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Worker loop
//
// Strategy: capture the screen ONCE per tick, read all picker pixels from that
// single image, then drop it immediately. The image never leaves this thread —
// no Arc sharing, no memory accumulation across frames.
//
// Peak memory = 1 frame at a time, regardless of interval or group count.
// ---------------------------------------------------------------------------

fn worker_loop(shared: Arc<Shared>, source: CaptureSource) {
    while shared.alive.load(Ordering::Relaxed) {
        let cfg = shared.config.lock().clone();
        let interval = Duration::from_millis(cfg.interval_ms.max(1));

        // Skip tick if no group has any pickers configured.
        let has_pickers = cfg.groups.iter().any(|g| !g.pickers.is_empty());
        if !has_pickers {
            thread::sleep(interval);
            continue;
        }

        let t0 = Instant::now();

        // Single screen capture shared across all groups this tick.
        let img = match capture(&source) {
            Ok(img) => img,
            Err(e) => {
                shared.events.lock().push(SampleEvent::CaptureError(e.to_string()));
                thread::sleep(Duration::from_millis(200));
                continue;
            }
        };

        let mut events = Vec::new();

        for gi in 0..cfg.groups.len() {
            let group = &cfg.groups[gi];
            if group.pickers.is_empty() {
                continue;
            }

            let mut samples = Vec::with_capacity(group.pickers.len());
            let mut targets = Vec::with_capacity(group.pickers.len());
            let mut oob = false;

            for &(x, y, target) in &group.pickers {
                match sample_color(&img, x, y) {
                    Some(c) => {
                        samples.push(c);
                        targets.push(target);
                    }
                    None => {
                        oob = true;
                        break;
                    }
                }
            }

            if oob {
                continue;
            }

            // Update live display samples.
            if let Some(slot) = shared.live_samples.lock().get_mut(gi) {
                *slot = samples.clone();
            }

            // Tick counter: read count, update, write back — each lock acquired separately.
            let current_count = shared.counts.lock().get(gi).copied().unwrap_or(0);
            let mut count = current_count;
            let evt = {
                let mut counters = shared.counters.lock();
                if let Some(cs) = counters.get_mut(gi) {
                    cs.tick(&samples, &targets, cfg.tolerance, &mut count)
                } else {
                    CounterEvent::None
                }
            };
            if count != current_count {
                if let Some(slot) = shared.counts.lock().get_mut(gi) {
                    *slot = count;
                }
            }

            match evt {
                CounterEvent::Incremented => {
                    events.push(SampleEvent::Incremented { group_idx: gi, new_count: count });
                }
                CounterEvent::Armed => {
                    events.push(SampleEvent::Armed { group_idx: gi });
                }
                CounterEvent::None => {}
            }
        }

        // img drops here — immediately freed before the sleep.
        drop(img);

        if !events.is_empty() {
            shared.events.lock().extend(events);
        }
        shared.tick_seq.fetch_add(1, Ordering::Relaxed);

        let elapsed = t0.elapsed();
        if elapsed < interval {
            thread::sleep(interval - elapsed);
        }
    }
}

fn sources_equal(a: &CaptureSource, b: &CaptureSource) -> bool {
    match (a, b) {
        (CaptureSource::Monitor { index: i1 }, CaptureSource::Monitor { index: i2 }) => i1 == i2,
        (CaptureSource::Window { id: id1, .. }, CaptureSource::Window { id: id2, .. }) => {
            id1 == id2
        }
        _ => false,
    }
}
