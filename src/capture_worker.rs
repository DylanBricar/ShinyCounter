use crate::capture::capture_for_sampling;
use crate::counter::{CounterEvent, CounterState};
use crate::types::{CaptureSource, Color};
use parking_lot::Mutex;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread::{self, JoinHandle};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupConfig {
    pub pickers: Vec<(i32, i32, Color)>, // (x, y, target)
}

/// All parameters the worker needs. Replaced atomically when anything changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub groups: Vec<GroupConfig>,
    pub tolerance: u8,
    pub interval_ms: u64,
}

fn validate_topology(cfg: &WorkerConfig, counts: &[u32]) -> std::io::Result<()> {
    if cfg.groups.len() != counts.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "capture topology has {} groups but {} counters",
                cfg.groups.len(),
                counts.len()
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared state between worker thread and UI thread.
// ---------------------------------------------------------------------------

struct VersionedConfig {
    value: WorkerConfig,
    revision: u64,
}

struct RuntimeState {
    events: Vec<SampleEvent>,
    live_samples: Vec<Vec<Color>>,
    counters: Vec<CounterState>,
    counts: Vec<u32>,
}

impl RuntimeState {
    fn new(counts: &[u32]) -> Self {
        Self {
            events: Vec::new(),
            live_samples: vec![Vec::new(); counts.len()],
            counters: vec![CounterState::default(); counts.len()],
            counts: counts.to_vec(),
        }
    }

    fn replace_counts(&mut self, counts: &[u32]) {
        *self = Self::new(counts);
    }

    fn set_count(&mut self, group_idx: usize, value: u32) {
        if let Some(slot) = self.counts.get_mut(group_idx) {
            *slot = value;
        }
        self.events.retain(|event| match event {
            SampleEvent::Incremented { group_idx: gi, .. }
            | SampleEvent::Armed { group_idx: gi } => *gi != group_idx,
            SampleEvent::CaptureError(_) => true,
        });
    }

    fn insert_group(&mut self, group_idx: usize, count: u32) -> bool {
        if group_idx > self.counts.len() {
            return false;
        }
        self.live_samples.insert(group_idx, Vec::new());
        self.counters.insert(group_idx, CounterState::default());
        self.counts.insert(group_idx, count);
        for event in &mut self.events {
            match event {
                SampleEvent::Incremented { group_idx: gi, .. }
                | SampleEvent::Armed { group_idx: gi }
                    if *gi >= group_idx =>
                {
                    *gi += 1;
                }
                SampleEvent::Incremented { .. }
                | SampleEvent::Armed { .. }
                | SampleEvent::CaptureError(_) => {}
            }
        }
        true
    }

    fn remove_group(&mut self, group_idx: usize) -> bool {
        if group_idx >= self.counts.len() {
            return false;
        }
        self.live_samples.remove(group_idx);
        self.counters.remove(group_idx);
        self.counts.remove(group_idx);
        self.events.retain_mut(|event| match event {
            SampleEvent::Incremented { group_idx: gi, .. }
            | SampleEvent::Armed { group_idx: gi } => {
                if *gi == group_idx {
                    return false;
                }
                if *gi > group_idx {
                    *gi -= 1;
                }
                true
            }
            SampleEvent::CaptureError(_) => true,
        });
        true
    }
}

struct Shared {
    /// Detection settings plus a revision used to discard in-flight samples
    /// whenever the UI changes a counter, preset, source, or picker layout.
    config: Mutex<VersionedConfig>,
    /// Counter values, state-machine state, samples, and pending events are
    /// mutated under one lock so UI edits cannot race a sampled increment.
    runtime: Mutex<RuntimeState>,
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
    handle: Option<JoinHandle<()>>,
}

impl CaptureWorker {
    pub fn start(
        source: CaptureSource,
        cfg: WorkerConfig,
        counts: &[u32],
    ) -> std::io::Result<Self> {
        validate_topology(&cfg, counts)?;
        let shared = Arc::new(Shared {
            config: Mutex::new(VersionedConfig {
                value: cfg,
                revision: 0,
            }),
            runtime: Mutex::new(RuntimeState::new(counts)),
            alive: AtomicBool::new(true),
            tick_seq: AtomicU64::new(0),
        });

        let shared_clone = Arc::clone(&shared);
        let source_clone = source.clone();

        let handle = thread::Builder::new()
            .name("shiny-sampler".into())
            .spawn(move || worker_loop(shared_clone, source_clone))?;

        Ok(Self {
            shared,
            source,
            handle: Some(handle),
        })
    }

    pub fn source_changed(&self, current: &CaptureSource) -> bool {
        !sources_equal(&self.source, current)
    }

    pub fn update_config(&self, cfg: WorkerConfig) -> std::io::Result<()> {
        let mut versioned = self.shared.config.lock();
        if versioned.value == cfg {
            return Ok(());
        }
        let n = cfg.groups.len();
        let mut runtime = self.shared.runtime.lock();
        validate_topology(&cfg, &runtime.counts)?;
        versioned.value = cfg;
        versioned.revision = versioned.revision.wrapping_add(1);
        runtime.live_samples.resize_with(n, Vec::new);
        runtime.live_samples.iter_mut().for_each(Vec::clear);
        runtime.counters.resize_with(n, CounterState::default);
        runtime.counts.resize(n, 0);
        runtime.events.retain(|event| match event {
            SampleEvent::Incremented { group_idx, .. } | SampleEvent::Armed { group_idx } => {
                *group_idx < n
            }
            SampleEvent::CaptureError(_) => true,
        });
        self.wake_worker();
        Ok(())
    }

    pub fn replace_config(&self, cfg: WorkerConfig, counts: &[u32]) -> std::io::Result<()> {
        validate_topology(&cfg, counts)?;
        let mut versioned = self.shared.config.lock();
        versioned.value = cfg;
        versioned.revision = versioned.revision.wrapping_add(1);
        self.shared.runtime.lock().replace_counts(counts);
        self.wake_worker();
        Ok(())
    }

    pub fn insert_group(
        &self,
        group_idx: usize,
        cfg: WorkerConfig,
        counts: &[u32],
    ) -> std::io::Result<()> {
        validate_topology(&cfg, counts)?;
        let mut versioned = self.shared.config.lock();
        let mut runtime = self.shared.runtime.lock();
        let old_len = versioned.value.groups.len();
        if group_idx > old_len
            || cfg.groups.len() != old_len.saturating_add(1)
            || runtime.counts.len() != old_len
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid capture group insertion",
            ));
        }
        versioned.value = cfg;
        versioned.revision = versioned.revision.wrapping_add(1);
        runtime.insert_group(group_idx, counts[group_idx]);
        self.wake_worker();
        Ok(())
    }

    pub fn remove_group(
        &self,
        group_idx: usize,
        cfg: WorkerConfig,
        counts: &[u32],
    ) -> std::io::Result<()> {
        validate_topology(&cfg, counts)?;
        let mut versioned = self.shared.config.lock();
        let mut runtime = self.shared.runtime.lock();
        let old_len = versioned.value.groups.len();
        if group_idx >= old_len
            || cfg.groups.len().saturating_add(1) != old_len
            || runtime.counts.len() != old_len
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid capture group removal",
            ));
        }
        versioned.value = cfg;
        versioned.revision = versioned.revision.wrapping_add(1);
        runtime.remove_group(group_idx);
        self.wake_worker();
        Ok(())
    }

    pub fn set_counts(&self, counts: &[u32]) -> std::io::Result<()> {
        let mut versioned = self.shared.config.lock();
        validate_topology(&versioned.value, counts)?;
        versioned.revision = versioned.revision.wrapping_add(1);
        self.shared.runtime.lock().replace_counts(counts);
        self.wake_worker();
        Ok(())
    }

    pub fn reset_counters(&self) {
        self.invalidate_in_flight_sample();
        let mut runtime = self.shared.runtime.lock();
        let count = runtime.counts.len();
        runtime.replace_counts(&vec![0; count]);
    }

    pub fn reset_counter(&self, gi: usize) {
        self.invalidate_in_flight_sample();
        let mut runtime = self.shared.runtime.lock();
        if let Some(cs) = runtime.counters.get_mut(gi) {
            cs.reset();
        }
        runtime
            .events
            .retain(|event| !matches!(event, SampleEvent::Armed { group_idx } if *group_idx == gi));
    }

    pub fn set_count(&self, gi: usize, value: u32) {
        self.invalidate_in_flight_sample();
        self.shared.runtime.lock().set_count(gi, value);
    }

    pub fn drain_events(&self) -> Vec<SampleEvent> {
        std::mem::take(&mut self.shared.runtime.lock().events)
    }

    pub fn live_samples(&self, gi: usize) -> Vec<Color> {
        self.shared
            .runtime
            .lock()
            .live_samples
            .get(gi)
            .cloned()
            .unwrap_or_default()
    }

    pub fn is_armed(&self, gi: usize) -> bool {
        self.shared
            .runtime
            .lock()
            .counters
            .get(gi)
            .map(|c| c.is_armed())
            .unwrap_or(true)
    }

    pub fn count(&self, gi: usize) -> u32 {
        self.shared
            .runtime
            .lock()
            .counts
            .get(gi)
            .copied()
            .unwrap_or(0)
    }

    pub fn tick_seq(&self) -> u64 {
        self.shared.tick_seq.load(Ordering::Relaxed)
    }

    pub fn shutdown(mut self) -> Vec<SampleEvent> {
        self.stop_and_join();
        std::mem::take(&mut self.shared.runtime.lock().events)
    }

    fn invalidate_in_flight_sample(&self) {
        let mut versioned = self.shared.config.lock();
        versioned.revision = versioned.revision.wrapping_add(1);
        self.wake_worker();
    }

    fn wake_worker(&self) {
        if let Some(handle) = &self.handle {
            handle.thread().unpark();
        }
    }

    fn stop_and_join(&mut self) {
        self.shared.alive.store(false, Ordering::Relaxed);
        self.wake_worker();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for CaptureWorker {
    fn drop(&mut self) {
        self.stop_and_join();
    }
}

// ---------------------------------------------------------------------------
// Worker loop
//
// Strategy: where the platform exposes compatible pixel coordinates, capture
// the smallest monitor region containing every picker ONCE per tick. Window
// and macOS Retina capture remain full-frame for correctness. The image never
// leaves this thread — no Arc sharing, no memory accumulation.
//
// Peak memory = 1 frame at a time, regardless of interval or group count.
// ---------------------------------------------------------------------------

fn worker_loop(shared: Arc<Shared>, source: CaptureSource) {
    while shared.alive.load(Ordering::Relaxed) {
        let (cfg, revision) = {
            let versioned = shared.config.lock();
            (versioned.value.clone(), versioned.revision)
        };
        let interval = Duration::from_millis(cfg.interval_ms.max(1));

        // Skip tick if no group has any pickers configured.
        let has_pickers = cfg.groups.iter().any(|g| !g.pickers.is_empty());
        if !has_pickers {
            thread::park_timeout(interval);
            continue;
        }

        let t0 = Instant::now();

        // Single capture shared across all groups this tick. Supported monitor
        // sources are cropped to the configured pickers' bounding rectangle.
        let frame = match capture_for_sampling(
            &source,
            cfg.groups
                .iter()
                .flat_map(|group| group.pickers.iter().map(|&(x, y, _)| (x, y))),
        ) {
            Ok(frame) => frame,
            Err(e) => {
                shared
                    .runtime
                    .lock()
                    .events
                    .push(SampleEvent::CaptureError(e.to_string()));
                shared.alive.store(false, Ordering::Relaxed);
                break;
            }
        };

        let mut sampled_groups = Vec::with_capacity(cfg.groups.len());

        for group in &cfg.groups {
            if group.pickers.is_empty() {
                sampled_groups.push(None);
                continue;
            }

            let mut samples = Vec::with_capacity(group.pickers.len());
            let mut targets = Vec::with_capacity(group.pickers.len());
            let mut oob = false;

            for &(x, y, target) in &group.pickers {
                match frame.color_at(x, y) {
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
                sampled_groups.push(None);
                continue;
            }
            sampled_groups.push(Some((samples, targets)));
        }

        // frame drops here — immediately freed before the sleep.
        drop(frame);

        // Reject a frame captured against stale coordinates/tolerance or before
        // a manual counter edit. Holding the config lock while committing the
        // runtime state makes that validation atomic with UI-side changes.
        if !commit_samples(&shared, &cfg, revision, sampled_groups) {
            continue;
        }
        shared.tick_seq.fetch_add(1, Ordering::Relaxed);

        let elapsed = t0.elapsed();
        if elapsed < interval {
            thread::park_timeout(interval - elapsed);
        }
    }
}

fn commit_samples(
    shared: &Shared,
    cfg: &WorkerConfig,
    revision: u64,
    sampled_groups: Vec<Option<(Vec<Color>, Vec<Color>)>>,
) -> bool {
    let versioned = shared.config.lock();
    if versioned.revision != revision {
        return false;
    }
    let mut runtime = shared.runtime.lock();
    for (gi, sampled) in sampled_groups.into_iter().enumerate() {
        let Some((samples, targets)) = sampled else {
            continue;
        };
        if gi >= runtime.counts.len()
            || gi >= runtime.counters.len()
            || gi >= runtime.live_samples.len()
        {
            continue;
        }

        let mut count = runtime.counts[gi];
        let event = runtime.counters[gi].tick(&samples, &targets, cfg.tolerance, &mut count);
        runtime.counts[gi] = count;
        runtime.live_samples[gi] = samples;

        match event {
            CounterEvent::Incremented => runtime.events.push(SampleEvent::Incremented {
                group_idx: gi,
                new_count: count,
            }),
            CounterEvent::Armed => runtime.events.push(SampleEvent::Armed { group_idx: gi }),
            CounterEvent::None => {}
        }
    }
    true
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_worker(groups: Vec<GroupConfig>, counts: &[u32]) -> CaptureWorker {
        CaptureWorker {
            shared: Arc::new(Shared {
                config: Mutex::new(VersionedConfig {
                    value: WorkerConfig {
                        groups,
                        tolerance: 0,
                        interval_ms: 100,
                    },
                    revision: 0,
                }),
                runtime: Mutex::new(RuntimeState::new(counts)),
                alive: AtomicBool::new(true),
                tick_seq: AtomicU64::new(0),
            }),
            source: CaptureSource::default(),
            handle: None,
        }
    }

    #[test]
    fn manual_count_change_discards_a_stale_increment_event() {
        let mut runtime = RuntimeState::new(&[5]);
        runtime.events.push(SampleEvent::Incremented {
            group_idx: 0,
            new_count: 6,
        });

        runtime.set_count(0, 10);

        assert_eq!(runtime.counts, vec![10]);
        assert!(runtime.events.is_empty());
    }

    #[test]
    fn replacing_preset_resets_all_runtime_state_together() {
        let mut runtime = RuntimeState::new(&[5, 9]);
        runtime.counters[0].tick(&[Color::new(1, 2, 3)], &[Color::new(1, 2, 3)], 0, &mut 5);
        runtime.events.push(SampleEvent::Armed { group_idx: 1 });

        runtime.replace_counts(&[42]);

        assert_eq!(runtime.counts, vec![42]);
        assert_eq!(runtime.counters.len(), 1);
        assert!(runtime.counters[0].is_armed());
        assert_eq!(runtime.live_samples, vec![Vec::<Color>::new()]);
        assert!(runtime.events.is_empty());
    }

    #[test]
    fn detection_change_preserves_committed_increment_and_disarmed_state() {
        let group = GroupConfig {
            pickers: Vec::new(),
        };
        let worker = test_worker(vec![group.clone()], &[5]);
        {
            let mut runtime = worker.shared.runtime.lock();
            let target = [Color::new(1, 2, 3)];
            let mut count = runtime.counts[0];
            runtime.counters[0].tick(&target, &target, 0, &mut count);
            runtime.counts[0] = count;
            runtime.events.push(SampleEvent::Incremented {
                group_idx: 0,
                new_count: count,
            });
        }

        worker
            .update_config(WorkerConfig {
                groups: vec![group],
                tolerance: 1,
                interval_ms: 100,
            })
            .expect("unchanged topology should be accepted");

        let runtime = worker.shared.runtime.lock();
        assert_eq!(runtime.counts, vec![6]);
        assert!(!runtime.counters[0].is_armed());
        assert!(matches!(
            runtime.events.as_slice(),
            [SampleEvent::Incremented {
                group_idx: 0,
                new_count: 6
            }]
        ));
    }

    #[test]
    fn manual_rearm_preserves_an_already_committed_increment() {
        let worker = test_worker(
            vec![GroupConfig {
                pickers: Vec::new(),
            }],
            &[6],
        );
        worker
            .shared
            .runtime
            .lock()
            .events
            .push(SampleEvent::Incremented {
                group_idx: 0,
                new_count: 6,
            });

        worker.reset_counter(0);

        let runtime = worker.shared.runtime.lock();
        assert_eq!(runtime.counts, vec![6]);
        assert!(matches!(
            runtime.events.as_slice(),
            [SampleEvent::Incremented {
                group_idx: 0,
                new_count: 6
            }]
        ));
    }

    #[test]
    fn stale_sample_cannot_overwrite_a_manual_count() {
        let group = GroupConfig {
            pickers: vec![(0, 0, Color::new(1, 2, 3))],
        };
        let worker = test_worker(vec![group.clone()], &[5]);
        let stale_revision = worker.shared.config.lock().revision;
        worker.set_count(0, 10);

        let committed = commit_samples(
            &worker.shared,
            &WorkerConfig {
                groups: vec![group],
                tolerance: 0,
                interval_ms: 100,
            },
            stale_revision,
            vec![Some((vec![Color::new(1, 2, 3)], vec![Color::new(1, 2, 3)]))],
        );

        assert!(!committed);
        assert_eq!(worker.count(0), 10);
        assert!(worker.drain_events().is_empty());
    }

    #[test]
    fn valid_multi_group_sample_commits_every_increment_atomically() {
        let first = GroupConfig {
            pickers: vec![(0, 0, Color::new(1, 2, 3))],
        };
        let second = GroupConfig {
            pickers: vec![(1, 0, Color::new(4, 5, 6))],
        };
        let worker = test_worker(vec![first.clone(), second.clone()], &[5, 10]);
        let revision = worker.shared.config.lock().revision;
        let cfg = WorkerConfig {
            groups: vec![first, second],
            tolerance: 0,
            interval_ms: 100,
        };

        assert!(commit_samples(
            &worker.shared,
            &cfg,
            revision,
            vec![
                Some((vec![Color::new(1, 2, 3)], vec![Color::new(1, 2, 3)])),
                Some((vec![Color::new(4, 5, 6)], vec![Color::new(4, 5, 6)])),
            ],
        ));

        let runtime = worker.shared.runtime.lock();
        assert_eq!(runtime.counts, vec![6, 11]);
        assert_eq!(runtime.live_samples[0], vec![Color::new(1, 2, 3)]);
        assert_eq!(runtime.live_samples[1], vec![Color::new(4, 5, 6)]);
        assert!(!runtime.counters[0].is_armed());
        assert!(!runtime.counters[1].is_armed());
        assert!(matches!(
            runtime.events.as_slice(),
            [
                SampleEvent::Incremented {
                    group_idx: 0,
                    new_count: 6
                },
                SampleEvent::Incremented {
                    group_idx: 1,
                    new_count: 11
                }
            ]
        ));
    }

    #[test]
    fn public_topology_changes_reject_mismatched_counter_lengths() {
        let group = GroupConfig {
            pickers: Vec::new(),
        };
        let invalid = WorkerConfig {
            groups: vec![group.clone(), group.clone()],
            tolerance: 0,
            interval_ms: 100,
        };
        assert!(CaptureWorker::start(CaptureSource::default(), invalid.clone(), &[0]).is_err());

        let worker = test_worker(vec![group], &[7]);
        assert!(worker.replace_config(invalid.clone(), &[7]).is_err());
        assert!(worker.update_config(invalid.clone()).is_err());
        assert!(worker.insert_group(1, invalid.clone(), &[7]).is_err());
        assert!(worker.remove_group(0, invalid, &[7]).is_err());
        assert!(worker.set_counts(&[7, 8]).is_err());
        assert!(worker
            .insert_group(
                2,
                WorkerConfig {
                    groups: vec![
                        GroupConfig {
                            pickers: Vec::new(),
                        },
                        GroupConfig {
                            pickers: Vec::new(),
                        },
                    ],
                    tolerance: 0,
                    interval_ms: 100,
                },
                &[7, 8],
            )
            .is_err());
        assert!(worker
            .remove_group(
                1,
                WorkerConfig {
                    groups: Vec::new(),
                    tolerance: 0,
                    interval_ms: 100,
                },
                &[],
            )
            .is_err());
        assert_eq!(worker.count(0), 7);
    }

    #[test]
    fn adding_a_group_preserves_existing_counter_state_and_events() {
        let group = GroupConfig {
            pickers: Vec::new(),
        };
        let worker = test_worker(vec![group.clone()], &[5]);
        {
            let mut runtime = worker.shared.runtime.lock();
            let target = [Color::new(1, 2, 3)];
            let mut count = runtime.counts[0];
            runtime.counters[0].tick(&target, &target, 0, &mut count);
            runtime.counts[0] = count;
            runtime.events.push(SampleEvent::Incremented {
                group_idx: 0,
                new_count: count,
            });
        }

        worker
            .insert_group(
                1,
                WorkerConfig {
                    groups: vec![group.clone(), group],
                    tolerance: 0,
                    interval_ms: 100,
                },
                &[6, 0],
            )
            .expect("matching topology should be accepted");

        let runtime = worker.shared.runtime.lock();
        assert_eq!(runtime.counts, vec![6, 0]);
        assert!(!runtime.counters[0].is_armed());
        assert!(runtime.counters[1].is_armed());
        assert!(matches!(
            runtime.events.as_slice(),
            [SampleEvent::Incremented {
                group_idx: 0,
                new_count: 6
            }]
        ));
    }

    #[test]
    fn removing_a_group_remaps_pending_events() {
        let groups = vec![
            GroupConfig {
                pickers: Vec::new(),
            },
            GroupConfig {
                pickers: Vec::new(),
            },
        ];
        let worker = test_worker(groups, &[3, 8]);
        worker
            .shared
            .runtime
            .lock()
            .events
            .push(SampleEvent::Incremented {
                group_idx: 1,
                new_count: 8,
            });

        worker
            .remove_group(
                0,
                WorkerConfig {
                    groups: vec![GroupConfig {
                        pickers: Vec::new(),
                    }],
                    tolerance: 0,
                    interval_ms: 100,
                },
                &[8],
            )
            .expect("matching topology should be accepted");

        let runtime = worker.shared.runtime.lock();
        assert_eq!(runtime.counts, vec![8]);
        assert!(matches!(
            runtime.events.as_slice(),
            [SampleEvent::Incremented {
                group_idx: 0,
                new_count: 8
            }]
        ));
    }

    #[test]
    fn dropping_worker_interrupts_a_long_idle_wait() {
        let started = Instant::now();
        let worker = CaptureWorker::start(
            CaptureSource::default(),
            WorkerConfig {
                groups: vec![GroupConfig {
                    pickers: Vec::new(),
                }],
                tolerance: 0,
                interval_ms: 10_000,
            },
            &[0],
        )
        .expect("worker thread should start");

        drop(worker);

        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn shutdown_returns_events_committed_before_the_worker_stops() {
        let worker = test_worker(
            vec![GroupConfig {
                pickers: Vec::new(),
            }],
            &[6],
        );
        worker
            .shared
            .runtime
            .lock()
            .events
            .push(SampleEvent::Incremented {
                group_idx: 0,
                new_count: 6,
            });

        let events = worker.shutdown();

        assert!(matches!(
            events.as_slice(),
            [SampleEvent::Incremented {
                group_idx: 0,
                new_count: 6
            }]
        ));
    }
}
