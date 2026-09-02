use super::helpers::{epoch_now, format_local_now, write_atomic};
use crate::theme;
use eframe::egui;
use image::RgbaImage;
use shiny_counter::capture::{capture, list_sources, SourceInfo};
use shiny_counter::capture_worker::{CaptureWorker, GroupConfig, SampleEvent, WorkerConfig};
use shiny_counter::i18n;
use shiny_counter::os_accent;
use shiny_counter::server::CounterServer;
use shiny_counter::storage;
use shiny_counter::types::{
    Color, Config, HitRecord, LogEntry, PickerGroup, PickerPoint, Preset, SessionRecord,
    MAX_PICKERS, MIN_PICKERS,
};
use shiny_counter::update::{self, UpdateChannel};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(super) struct PickClick {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) color: Color,
}

pub(super) struct PickSession {
    pub(super) image: RgbaImage,
    pub(super) texture: egui::TextureHandle,
    pub(super) clicks: Vec<Option<PickClick>>,
    pub(super) current: usize,
}

pub(super) enum Mode {
    Idle,
    Picking(PickSession),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PendingConfirm {
    None,
    ResetCounter,
    DeletePreset,
    ClearHistory,
    DeleteGroup(usize),
    DeletePicker(usize),
}

pub struct ShinyApp {
    pub(super) config: Config,
    /// One CounterState per PickerGroup in the active preset.
    /// Rebuilt whenever the active preset or its group count changes.
    /// Background capture thread — always running when sampling is active.
    /// Replaced when the capture source changes.
    pub(super) capture_worker: Option<CaptureWorker>,
    /// Tracks the last known active preset index so ensure_capture_worker can
    /// detect preset switches and reseed counts accordingly.
    pub(super) worker_preset_index: usize,
    pub(super) running: bool,
    pub(super) last_sample: Vec<Color>,
    pub(super) status: String,
    pub(super) server: Option<CounterServer>,
    pub(super) server_error: Option<String>,
    pub(super) sources: Vec<SourceInfo>,
    pub(super) sources_refreshed_at: Instant,
    pub(super) mode: Mode,
    pub(super) rename_buf: String,
    pub(super) note_buf: String,
    pub(super) hex_buf: HashMap<usize, String>,
    pub(super) dirty: bool,
    /// True when UI data that can affect the sampler changed. This avoids
    /// rebuilding nested picker vectors on every 16 ms repaint.
    pub(super) worker_config_dirty: bool,
    pub(super) last_save: Instant,
    pub(super) theme_installed: bool,
    pub(super) show_settings: bool,
    pub(super) show_history: bool,
    pub(super) pending_confirm: PendingConfirm,
    pub(super) os_accent: Color,
    pub(super) journal_page: usize,
    pub(super) expanded_sessions: HashSet<usize>,
    pub(super) session_pages: HashMap<usize, usize>,
    pub(super) update_channel: UpdateChannel,
    pub(super) update_prompt_dismissed_for: Option<String>,
    pub(super) update_auto_opened_for: Option<String>,
    /// Set to `true` when an auto-download was kicked off (setting enabled).
    /// Used to suppress the "Downloading..." modal so the operation stays
    /// invisible until the asset is fully on disk - at which point we pop
    /// the "Download complete" modal once.
    pub(super) update_auto_initiated: bool,
}

fn close_open_sessions_from_previous_run(config: &mut Config) -> bool {
    let mut changed = false;
    for preset in &mut config.presets {
        for group in &mut preset.groups {
            for session in &mut group.sessions {
                if session.is_open() {
                    if let Some(hit) = session.hits.last() {
                        session.ended_at_epoch = Some(hit.epoch_secs);
                        session.ended_at = Some(hit.timestamp.clone());
                    } else {
                        session.ended_at_epoch = Some(session.started_at_epoch);
                        if !session.started_at.is_empty() {
                            session.ended_at = Some(session.started_at.clone());
                        }
                    }
                    changed = true;
                }
            }
        }
    }
    changed
}

pub(super) fn open_group_session(group: &mut PickerGroup, now: i64, stamp: &str) {
    if let Some(last) = group.sessions.last_mut() {
        if last.is_open() {
            last.ended_at_epoch = Some(now);
            last.ended_at = Some(stamp.to_owned());
        }
    }
    group.sessions.push(SessionRecord {
        started_at_epoch: now,
        started_at: stamp.to_owned(),
        ended_at_epoch: None,
        ended_at: None,
        hits: Vec::new(),
    });
}

fn open_sessions(preset: &mut Preset, now: i64, stamp: &str) {
    for group in &mut preset.groups {
        open_group_session(group, now, stamp);
    }
}

pub(super) fn remove_picker_group(preset: &mut Preset, group_idx: usize) -> bool {
    if preset.groups.len() <= 1 || group_idx >= preset.groups.len() {
        return false;
    }
    let old_active = preset.active_group_index;
    preset.groups.remove(group_idx);
    preset.active_group_index = if old_active > group_idx {
        old_active - 1
    } else if old_active == group_idx {
        group_idx.saturating_sub(1)
    } else {
        old_active
    }
    .min(preset.groups.len().saturating_sub(1));
    preset.count = preset.total_count();
    true
}

fn close_sessions(preset: &mut Preset, now: i64, stamp: &str) {
    for group in &mut preset.groups {
        if let Some(last) = group.sessions.last_mut() {
            if last.is_open() {
                last.ended_at_epoch = Some(now);
                last.ended_at = Some(stamp.to_owned());
            }
        }
    }
}

fn partition_capture_errors(events: Vec<SampleEvent>) -> (Vec<SampleEvent>, Option<String>) {
    let mut error = None;
    let events = events
        .into_iter()
        .filter_map(|event| match event {
            SampleEvent::CaptureError(message) => {
                error = Some(message);
                None
            }
            other => Some(other),
        })
        .collect();
    (events, error)
}

impl ShinyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let storage::LoadOutcome {
            mut config,
            warning,
        } = storage::load_with_warning();
        config.ensure_invariants();
        let sources = list_sources();
        let initial_status =
            warning.unwrap_or_else(|| i18n::strings(config.language).ready.to_string());
        let mut app = Self {
            status: initial_status,
            config,
            capture_worker: None,
            worker_preset_index: usize::MAX,
            running: false,
            last_sample: Vec::new(),
            server: None,
            server_error: None,
            sources,
            sources_refreshed_at: Instant::now(),
            mode: Mode::Idle,
            rename_buf: String::new(),
            note_buf: String::new(),
            hex_buf: HashMap::new(),
            dirty: false,
            worker_config_dirty: true,
            last_save: Instant::now(),
            theme_installed: false,
            show_settings: false,
            show_history: false,
            pending_confirm: PendingConfirm::None,
            os_accent: os_accent::detect(),
            journal_page: 0,
            expanded_sessions: HashSet::new(),
            session_pages: HashMap::new(),
            update_channel: UpdateChannel::new(),
            update_prompt_dismissed_for: None,
            update_auto_opened_for: None,
            update_auto_initiated: false,
        };
        // Fire and forget a one-shot release check on launch.
        update::spawn_check(app.update_channel.clone());
        // Close any leftover open sessions from a previous run.
        if close_open_sessions_from_previous_run(&mut app.config) {
            app.mark_dirty();
        }
        app.sync_hex_buf();
        // Sync the output file to the active preset's current count on boot.
        // The HTTP server is started lazily via ensure_server() and calls
        // s.update() itself, so we only need to handle the file here.
        app.write_output_file();
        app
    }

    pub(super) fn s(&self) -> &'static i18n::Strings {
        i18n::strings(self.config.language)
    }

    pub(super) fn active_idx(&self) -> usize {
        self.config
            .active_preset_index
            .min(self.config.presets.len().saturating_sub(1))
    }

    pub(super) fn active(&self) -> &Preset {
        &self.config.presets[self.active_idx()]
    }

    pub(super) fn active_mut(&mut self) -> &mut Preset {
        let i = self.active_idx();
        &mut self.config.presets[i]
    }

    /// Atomically replace worker configuration and counts after a preset or
    /// group-topology change, invalidating any sample captured beforehand.
    pub(super) fn sync_counters(&mut self) {
        let config = self.make_worker_config();
        let counts: Vec<u32> = self.active().groups.iter().map(|g| g.count).collect();
        let error = self
            .capture_worker
            .as_ref()
            .and_then(|worker| worker.replace_config(config, &counts).err());
        if let Some(error) = error {
            self.handle_worker_config_error(error);
        }
        self.worker_preset_index = self.active_idx();
    }

    pub(super) fn sync_added_group(&mut self, group_idx: usize) {
        let config = self.make_worker_config();
        let counts: Vec<u32> = self.active().groups.iter().map(|g| g.count).collect();
        let error = self
            .capture_worker
            .as_ref()
            .and_then(|worker| worker.insert_group(group_idx, config, &counts).err());
        if let Some(error) = error {
            self.handle_worker_config_error(error);
        }
        self.worker_preset_index = self.active_idx();
    }

    pub(super) fn sync_removed_group(&mut self, group_idx: usize) {
        let config = self.make_worker_config();
        let counts: Vec<u32> = self.active().groups.iter().map(|g| g.count).collect();
        let error = self
            .capture_worker
            .as_ref()
            .and_then(|worker| worker.remove_group(group_idx, config, &counts).err());
        if let Some(error) = error {
            self.handle_worker_config_error(error);
        }
        self.worker_preset_index = self.active_idx();
    }

    /// Whether the active group's counter is armed (delegates to worker).
    pub(super) fn active_counter_is_armed(&self) -> bool {
        let gi = self.active().active_group_index;
        self.capture_worker
            .as_ref()
            .map(|w| w.is_armed(gi))
            .unwrap_or(true)
    }

    /// Reset the active group's counter state (delegates to worker).
    pub(super) fn active_counter_reset(&mut self) {
        let gi = self.active().active_group_index;
        if let Some(w) = &self.capture_worker {
            w.reset_counter(gi);
        }
    }

    /// Reset all groups' counter states (delegates to worker).
    pub(super) fn reset_all_counters(&mut self) {
        if let Some(w) = &self.capture_worker {
            w.reset_counters();
        }
    }

    fn handle_worker_config_error(&mut self, error: std::io::Error) {
        self.stop_capture_worker_and_reconcile();
        self.running = false;
        self.close_all_sessions();
        self.last_sample.clear();
        self.status = format!("{}: {error}", self.s().capture_error);
        self.mark_dirty();
    }

    pub(super) fn sync_hex_buf(&mut self) {
        let snapshot: Vec<(usize, String)> = self
            .active()
            .active_group()
            .pickers
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.target.to_hex()))
            .collect();
        self.hex_buf.clear();
        for (i, hex) in snapshot {
            self.hex_buf.insert(i, hex);
        }
    }

    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
        self.worker_config_dirty = true;
    }

    pub(super) fn flush_save(&mut self) {
        if !self.dirty {
            return;
        }
        if self.last_save.elapsed() < Duration::from_millis(750) {
            return;
        }
        if let Err(e) = storage::save(&self.config) {
            self.status = format!("{}: {e}", self.s().save_failed);
            return;
        }
        self.dirty = false;
        self.last_save = Instant::now();
    }

    /// Build a WorkerConfig from the active preset's current state.
    fn make_worker_config(&self) -> WorkerConfig {
        let preset = self.active();
        WorkerConfig {
            groups: preset
                .groups
                .iter()
                .map(|g| GroupConfig {
                    pickers: g.pickers.iter().map(|p| (p.x, p.y, p.target)).collect(),
                })
                .collect(),
            tolerance: preset.tolerance,
            interval_ms: preset.interval_ms.max(1),
        }
    }

    /// Ensure the background sampler worker is running, pointed at the current
    /// source, and has an up-to-date config. Replaces the worker on source change.
    /// Reseeds counts and drains stale events on preset switch.
    pub(super) fn ensure_capture_worker(&mut self) {
        let current_preset = self.active_idx();
        let preset_changed = current_preset != self.worker_preset_index;

        let source_changed = self
            .capture_worker
            .as_ref()
            .map(|w| w.source_changed(&self.config.capture))
            .unwrap_or(false);
        if preset_changed || source_changed {
            self.stop_capture_worker_and_reconcile();
            if !self.running {
                return;
            }
        }
        let needs_new = self.capture_worker.is_none();

        let counts: Vec<u32> = self.active().groups.iter().map(|g| g.count).collect();

        if needs_new {
            let cfg = self.make_worker_config();
            match CaptureWorker::start(self.config.capture.clone(), cfg, &counts) {
                Ok(worker) => {
                    self.capture_worker = Some(worker);
                    self.worker_preset_index = current_preset;
                    self.worker_config_dirty = false;
                }
                Err(error) => {
                    self.running = false;
                    self.close_all_sessions();
                    self.status = format!("{}: {error}", self.s().capture_error);
                    self.mark_dirty();
                }
            }
        } else if let Some(w) = &self.capture_worker {
            if preset_changed {
                let error = w.replace_config(self.make_worker_config(), &counts).err();
                if let Some(error) = error {
                    self.handle_worker_config_error(error);
                } else {
                    self.worker_preset_index = current_preset;
                    self.worker_config_dirty = false;
                }
            } else if self.worker_config_dirty {
                if let Err(error) = w.update_config(self.make_worker_config()) {
                    self.handle_worker_config_error(error);
                } else {
                    self.worker_config_dirty = false;
                }
            }
        }
    }

    /// Drain events from the worker and apply them to app state.
    /// This is the only place counters/counts are mutated from UI side.
    pub(super) fn tick(&mut self, ctx: &egui::Context) {
        if !self.running {
            // Kill the worker when not sampling — avoids continuous screen capture
            // and the spinning cursor on macOS.
            if self.capture_worker.is_some() {
                self.stop_capture_worker_and_reconcile();
                self.last_sample.clear();
            }
            ctx.request_repaint_after(Duration::from_millis(80));
            return;
        }

        // Start/keep the worker only while actively sampling.
        self.ensure_capture_worker();

        // Repaint frequently enough to drain events without perceptible lag.
        ctx.request_repaint_after(Duration::from_millis(16));

        // Update live_sample for active group display.
        if let Some(w) = &self.capture_worker {
            let gi = self.active().active_group_index;
            self.last_sample = w.live_samples(gi);
        }

        // Drain events produced by the worker thread.
        let events = self
            .capture_worker
            .as_ref()
            .map(|w| w.drain_events())
            .unwrap_or_default();

        let (mut any_incremented, capture_error) = self.apply_sample_events(events);
        if let Some(error) = capture_error {
            let trailing_events = self
                .capture_worker
                .take()
                .map(CaptureWorker::shutdown)
                .unwrap_or_default();
            let (trailing_incremented, trailing_error) = self.apply_sample_events(trailing_events);
            any_incremented |= trailing_incremented;
            self.running = false;
            self.close_all_sessions();
            self.last_sample.clear();
            self.status = format!(
                "{}: {}",
                self.s().capture_error,
                trailing_error.unwrap_or(error)
            );
            self.mark_dirty();
        }

        self.push_server_state();
        if any_incremented {
            self.write_output_file();
        }
    }

    fn apply_sample_events(&mut self, events: Vec<SampleEvent>) -> (bool, Option<String>) {
        let (events, capture_error) = partition_capture_errors(events);
        let mut any_incremented = false;
        for evt in events {
            match evt {
                SampleEvent::Incremented {
                    group_idx: gi,
                    new_count: count,
                } => {
                    if gi >= self.active().groups.len() {
                        continue;
                    }
                    // Sync persisted count from worker.
                    self.active_mut().groups[gi].count = count;
                    self.active_mut().count = self.active().total_count();
                    let gi_str = if self.active().groups.len() > 1 {
                        format!(" (Zone {})", gi + 1)
                    } else {
                        String::new()
                    };
                    self.status = format!("{} {count}{gi_str}", self.s().match_count);
                    self.record_hit_group(gi, count);
                    self.mark_dirty();
                    any_incremented = true;
                }
                SampleEvent::Armed { group_idx: gi } => {
                    if gi == self.active().active_group_index {
                        self.status = self.s().rearmed.into();
                    }
                }
                SampleEvent::CaptureError(_) => {}
            }
        }
        (any_incremented, capture_error)
    }

    pub(super) fn stop_capture_worker_and_reconcile(&mut self) -> Option<String> {
        let events = self
            .capture_worker
            .take()
            .map(CaptureWorker::shutdown)
            .unwrap_or_default();
        let (any_incremented, capture_error) = self.apply_sample_events(events);
        if let Some(error) = &capture_error {
            self.running = false;
            self.close_all_sessions();
            self.status = format!("{}: {error}", self.s().capture_error);
            self.mark_dirty();
        }
        self.push_server_state();
        if any_incremented {
            self.write_output_file();
        }
        capture_error
    }

    /// Push the current active preset's state to the HTTP overlay server and
    /// write the plain-text output file. Call this after any mutation that
    /// changes count or switches presets.
    pub(super) fn broadcast_state(&mut self) {
        self.push_server_state();
        self.write_output_file();
    }

    /// In-memory only: refresh the HTTP server's snapshot. Cheap; safe to
    /// call on every tick. Does NOT touch disk.
    pub(super) fn push_server_state(&mut self) {
        if let Some(s) = &self.server {
            s.update(
                self.active().total_count(),
                self.active().name.clone(),
                self.active_counter_is_armed(),
                self.config.server_styled,
            );
        }
    }

    pub(super) fn write_output_file(&mut self) {
        let (enabled, path, count) = {
            let preset = self.active();
            (
                preset.output_file_enabled,
                preset.output_file.clone(),
                preset.total_count(),
            )
        };
        if !enabled {
            return;
        }
        let Some(path) = path else {
            return;
        };
        // Trailing newline so naive line-oriented readers (some OBS variants,
        // tail -f, etc.) treat the file as a complete record.
        let content = format!("{count}\n");
        if let Err(e) = write_atomic(&path, content.as_bytes()) {
            self.status = format!("{}: {e}", self.s().file_output_error);
        }
    }

    /// Record a hit in a specific group's session history.
    pub(super) fn record_hit_group(&mut self, group_idx: usize, count: u32) {
        let now = epoch_now();
        let lang = self.config.language;
        let preset_idx = self.active_idx();
        let Some(group) = self.config.presets[preset_idx].groups.get_mut(group_idx) else {
            return;
        };
        let need_open = group.sessions.last().map(|s| !s.is_open()).unwrap_or(true);
        if need_open {
            group.sessions.push(SessionRecord {
                started_at_epoch: now,
                started_at: format_local_now(lang),
                ended_at_epoch: None,
                ended_at: None,
                hits: Vec::new(),
            });
        }
        let Some(session) = group.sessions.last_mut() else {
            return;
        };
        let prev = session
            .hits
            .last()
            .map(|h| h.epoch_secs)
            .unwrap_or(session.started_at_epoch);
        let delta = (now - prev).max(0);
        session.hits.push(HitRecord {
            timestamp: format_local_now(lang),
            epoch_secs: now,
            delta_secs: delta,
            index: count,
        });
    }

    /// Record a hit in the active group (convenience wrapper).
    #[allow(dead_code)]
    pub(super) fn record_hit(&mut self, count: u32) {
        let gi = self.active().active_group_index;
        self.record_hit_group(gi, count);
    }

    pub(super) fn open_session(&mut self) {
        let now = epoch_now();
        let stamp = format_local_now(self.config.language);
        open_sessions(self.active_mut(), now, &stamp);
    }

    pub(super) fn close_session(&mut self) {
        let now = epoch_now();
        let stamp = format_local_now(self.config.language);
        close_sessions(self.active_mut(), now, &stamp);
    }

    /// Close the open session in every group of the active preset.
    /// Used when the capture source fails — all groups stop together.
    pub(super) fn close_all_sessions(&mut self) {
        let now = epoch_now();
        let stamp = format_local_now(self.config.language);
        close_sessions(self.active_mut(), now, &stamp);
    }

    pub(super) fn accent(&self) -> Color {
        self.active().accent_color.unwrap_or(self.os_accent)
    }

    pub(super) fn accent32(&self) -> egui::Color32 {
        let c = self.accent();
        egui::Color32::from_rgb(c.r, c.g, c.b)
    }

    pub(super) fn ensure_server(&mut self) {
        if !self.config.server_enabled {
            if self.server.is_some() {
                self.server = None;
            }
            return;
        }
        let needs_restart = self
            .server
            .as_ref()
            .map(|s| s.port != self.config.server_port)
            .unwrap_or(true);
        if !needs_restart {
            return;
        }
        self.server = None;
        match CounterServer::start(self.config.server_port) {
            Ok(s) => {
                self.server_error = None;
                s.update(
                    self.active().total_count(),
                    self.active().name.clone(),
                    self.active_counter_is_armed(),
                    self.config.server_styled,
                );
                self.server = Some(s);
            }
            Err(e) => {
                self.server_error = Some(e.to_string());
                self.config.server_enabled = false;
            }
        }
    }

    pub(super) fn add_log(&mut self, note: String) {
        let entry = LogEntry {
            timestamp: format_local_now(self.config.language),
            preset_name: self.active().name.clone(),
            count_at_event: self.active().total_count(),
            note,
        };
        self.config.log.insert(0, entry);
        if self.config.log.len() > 500 {
            self.config.log.truncate(500);
        }
        self.mark_dirty();
    }

    pub(super) fn begin_pick(&mut self, ctx: &egui::Context) {
        match capture(&self.config.capture) {
            Ok(img) => {
                let size = [img.width() as usize, img.height() as usize];
                let raw = img.as_raw();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, raw);
                let texture =
                    ctx.load_texture("pick_capture", color_image, egui::TextureOptions::LINEAR);
                let n = self.active().active_group().pickers.len();
                self.mode = Mode::Picking(PickSession {
                    image: img,
                    texture,
                    clicks: vec![None; n],
                    current: 0,
                });
            }
            Err(e) => {
                self.status = format!("{}: {e}", self.s().capture_error);
            }
        }
    }

    pub(super) fn commit_pick(&mut self, session: PickSession) {
        let assigned: Vec<PickClick> = session.clicks.into_iter().flatten().collect();
        if assigned.is_empty() {
            self.status = self.s().pick_cancelled.into();
            return;
        }
        let n = assigned.len().clamp(MIN_PICKERS, MAX_PICKERS);
        let mut pickers: Vec<PickerPoint> = Vec::with_capacity(n);
        for i in 0..n {
            if let Some(c) = assigned.get(i) {
                pickers.push(PickerPoint {
                    x: c.x,
                    y: c.y,
                    target: c.color,
                });
            } else {
                pickers.push(PickerPoint::default());
            }
        }
        self.active_mut().active_group_mut().pickers = pickers;
        self.sync_hex_buf();
        self.mark_dirty();
        self.status = format!(
            "{} {} / {}",
            self.s().apply_picks,
            assigned.len(),
            MAX_PICKERS
        );
    }

    pub(super) fn refresh_sources_if_stale(&mut self) {
        if self.sources_refreshed_at.elapsed() > Duration::from_secs(8) {
            self.sources = list_sources();
            self.sources_refreshed_at = Instant::now();
        }
    }
}

impl eframe::App for ShinyApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_installed {
            theme::install(ctx);
            self.theme_installed = true;
        }
        // Apply the active preset's accent every frame so changes propagate
        // immediately to stock egui widgets.
        theme::apply_accent(ctx, self.accent32());
        self.refresh_sources_if_stale();
        self.ensure_server();
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        let central = egui::CentralPanel::default().frame(
            egui::Frame::NONE
                .fill(theme::BG)
                .inner_margin(egui::Margin::same(18)),
        );

        match std::mem::replace(&mut self.mode, Mode::Idle) {
            Mode::Idle => {
                central.show(ui, |ui| self.render_idle(&ctx, ui));
            }
            Mode::Picking(session) => {
                let session = self.render_picking(&ctx, ui, central, session);
                if let Some(s) = session {
                    self.mode = Mode::Picking(s);
                }
            }
        }
        self.render_confirm_modal(&ctx);
        self.render_update_modal(&ctx);
        self.flush_save();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.stop_capture_worker_and_reconcile();
        self.close_session();
        let _ = storage::save(&self.config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_empty_open_sessions_are_closed_to_their_start_time() {
        let mut config = Config::default();
        config.presets[0].groups[0].sessions.push(SessionRecord {
            started_at_epoch: 123,
            started_at: "start".into(),
            ended_at_epoch: None,
            ended_at: None,
            hits: Vec::new(),
        });

        let changed = close_open_sessions_from_previous_run(&mut config);

        assert!(changed);
        let session = &config.presets[0].groups[0].sessions[0];
        assert_eq!(session.ended_at_epoch, Some(123));
        assert!(!session.is_open());
    }

    #[test]
    fn all_previous_open_sessions_are_closed() {
        let mut config = Config::default();
        config.presets[0].groups[0].sessions = vec![
            SessionRecord {
                started_at_epoch: 10,
                started_at: "first".into(),
                ended_at_epoch: None,
                ended_at: None,
                hits: Vec::new(),
            },
            SessionRecord {
                started_at_epoch: 20,
                started_at: "second".into(),
                ended_at_epoch: None,
                ended_at: None,
                hits: vec![HitRecord {
                    timestamp: "hit".into(),
                    epoch_secs: 25,
                    delta_secs: 5,
                    index: 1,
                }],
            },
        ];

        let changed = close_open_sessions_from_previous_run(&mut config);

        assert!(changed);
        assert_eq!(
            config.presets[0].groups[0].sessions[0].ended_at_epoch,
            Some(10)
        );
        assert_eq!(
            config.presets[0].groups[0].sessions[1].ended_at_epoch,
            Some(25)
        );
        assert!(config.presets[0].groups[0]
            .sessions
            .iter()
            .all(|s| !s.is_open()));
    }

    #[test]
    fn watching_sessions_open_and_close_for_every_zone() {
        let mut preset = Preset::new("Multi-zone");
        preset
            .groups
            .push(shiny_counter::types::PickerGroup::new("Zone 2"));

        open_sessions(&mut preset, 100, "start");
        assert!(preset
            .groups
            .iter()
            .all(|group| { group.sessions.len() == 1 && group.sessions[0].is_open() }));

        close_sessions(&mut preset, 125, "end");
        assert!(preset.groups.iter().all(|group| {
            group.sessions[0].ended_at_epoch == Some(125)
                && group.sessions[0].ended_at.as_deref() == Some("end")
        }));
    }

    #[test]
    fn a_new_zone_can_start_its_session_while_watching() {
        let mut group = PickerGroup::new("Zone 2");

        open_group_session(&mut group, 200, "created");

        assert_eq!(group.sessions.len(), 1);
        assert_eq!(group.sessions[0].started_at_epoch, 200);
        assert!(group.sessions[0].is_open());
    }

    #[test]
    fn removing_a_zone_recomputes_total_and_active_index() {
        let mut preset = Preset::new("Multi-zone");
        preset.groups[0].count = 3;
        preset.groups.push(PickerGroup::new("Zone 2"));
        preset.groups[1].count = 7;
        preset.groups.push(PickerGroup::new("Zone 3"));
        preset.groups[2].count = 11;
        preset.active_group_index = 2;
        preset.count = 21;

        assert!(remove_picker_group(&mut preset, 1));

        assert_eq!(preset.groups.len(), 2);
        assert_eq!(preset.active_group_index, 1);
        assert_eq!(preset.count, 14);
    }

    #[test]
    fn removing_zones_keeps_the_same_logical_zone_selected() {
        for (active, removed, expected) in [(1, 0, 0), (1, 1, 0), (1, 2, 1)] {
            let mut preset = Preset::new("Multi-zone");
            preset.groups.push(PickerGroup::new("Zone 2"));
            preset.groups.push(PickerGroup::new("Zone 3"));
            preset.active_group_index = active;

            assert!(remove_picker_group(&mut preset, removed));
            assert_eq!(
                preset.active_group_index, expected,
                "active={active}, removed={removed}"
            );
        }
    }

    #[test]
    fn capture_error_partition_keeps_increments_before_and_after_the_error() {
        for events in [
            vec![
                SampleEvent::Incremented {
                    group_idx: 0,
                    new_count: 1,
                },
                SampleEvent::CaptureError("lost source".into()),
            ],
            vec![
                SampleEvent::CaptureError("lost source".into()),
                SampleEvent::Incremented {
                    group_idx: 0,
                    new_count: 1,
                },
            ],
        ] {
            let (events, error) = partition_capture_errors(events);

            assert_eq!(error.as_deref(), Some("lost source"));
            assert!(matches!(
                events.as_slice(),
                [SampleEvent::Incremented {
                    group_idx: 0,
                    new_count: 1
                }]
            ));
        }
    }
}
