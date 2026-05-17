use super::helpers::{epoch_now, format_local_now, write_atomic};
use crate::theme;
use eframe::egui;
use image::RgbaImage;
use shiny_counter::capture::{capture, list_sources, sample_color, SourceInfo};
use shiny_counter::counter::{CounterEvent, CounterState};
use shiny_counter::i18n;
use shiny_counter::os_accent;
use shiny_counter::server::CounterServer;
use shiny_counter::storage;
use shiny_counter::types::{
    Color, Config, HitRecord, LogEntry, PickerPoint, Preset, SessionRecord, MAX_PICKERS,
    MIN_PICKERS,
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
    DeletePicker(usize),
}

pub struct ShinyApp {
    pub(super) config: Config,
    pub(super) counter: CounterState,
    pub(super) running: bool,
    pub(super) last_tick: Instant,
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

impl ShinyApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut config = storage::load();
        config.ensure_invariants();
        let sources = list_sources();
        let mut app = Self {
            status: i18n::strings(config.language).ready.to_string(),
            config,
            counter: CounterState::default(),
            running: false,
            last_tick: Instant::now() - Duration::from_secs(10),
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
        for p in &mut app.config.presets {
            if let Some(last) = p.sessions.last_mut() {
                if last.is_open() {
                    last.ended_at_epoch = last.hits.last().map(|h| h.epoch_secs);
                }
            }
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

    pub(super) fn sync_hex_buf(&mut self) {
        let snapshot: Vec<(usize, String)> = self
            .active()
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

    pub(super) fn tick(&mut self, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_millis(80));
        if !self.running {
            return;
        }
        let interval = Duration::from_millis(self.active().interval_ms.max(50));
        if self.last_tick.elapsed() < interval {
            return;
        }
        self.last_tick = Instant::now();

        let img = match capture(&self.config.capture) {
            Ok(i) => i,
            Err(e) => {
                self.status = format!("{}: {e}", self.s().capture_error);
                self.running = false;
                return;
            }
        };
        // Sample without cloning the full Preset (which carries the whole
        // session history).
        let n_pickers = self.active().pickers.len();
        let tolerance = self.active().tolerance;
        let mut samples: Vec<Color> = Vec::with_capacity(n_pickers);
        let mut targets: Vec<Color> = Vec::with_capacity(n_pickers);
        for i in 0..n_pickers {
            let p = self.active().pickers[i];
            match sample_color(&img, p.x, p.y) {
                Some(c) => {
                    samples.push(c);
                    targets.push(p.target);
                }
                None => {
                    self.status = format!("{} #{} ({},{})", self.s().picker_oob, i + 1, p.x, p.y);
                    return;
                }
            }
        }
        self.last_sample = samples;

        let mut count = self.active().count;
        let evt = self
            .counter
            .tick(&self.last_sample, &targets, tolerance, &mut count);
        self.active_mut().count = count;

        match evt {
            CounterEvent::Incremented => {
                self.status = format!("{} {count}", self.s().match_count);
                self.record_hit(count);
                self.mark_dirty();
            }
            CounterEvent::Armed => {
                self.status = self.s().rearmed.into();
            }
            CounterEvent::None => {}
        }

        // Always push the latest state to the HTTP server (cheap in-memory
        // update — keeps `is_armed`, count, and preset name live for
        // /count, /count.txt, and /poll). The file write is more expensive
        // and only fires when the count actually moves.
        self.push_server_state();
        if matches!(evt, CounterEvent::Incremented) {
            self.write_output_file();
        }
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
                self.active().count,
                self.active().name.clone(),
                self.counter.is_armed(),
                self.config.server_styled,
            );
        }
    }

    pub(super) fn write_output_file(&mut self) {
        // Snapshot the values we need under an immutable borrow, then release
        // it so the status update below can take a mutable borrow if needed.
        let (enabled, path, count) = {
            let preset = self.active();
            (
                preset.output_file_enabled,
                preset.output_file.clone(),
                preset.count,
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

    pub(super) fn record_hit(&mut self, count: u32) {
        let now = epoch_now();
        let lang = self.config.language;
        let preset = self.active_mut();
        // Ensure an open session exists.
        let need_open = preset.sessions.last().map(|s| !s.is_open()).unwrap_or(true);
        if need_open {
            preset.sessions.push(SessionRecord {
                started_at_epoch: now,
                started_at: format_local_now(lang),
                ended_at_epoch: None,
                ended_at: None,
                hits: Vec::new(),
            });
        }
        let session = preset.sessions.last_mut().expect("session pushed above");
        let prev = session
            .hits
            .last()
            .map(|h| h.epoch_secs)
            .unwrap_or(session.started_at_epoch);
        let delta = (now - prev).max(0);
        let rec = HitRecord {
            timestamp: format_local_now(lang),
            epoch_secs: now,
            delta_secs: delta,
            index: count,
        };
        session.hits.push(rec);
    }

    pub(super) fn open_session(&mut self) {
        let now = epoch_now();
        let stamp = format_local_now(self.config.language);
        let preset = self.active_mut();
        // Close any orphan open session first.
        if let Some(last) = preset.sessions.last_mut() {
            if last.is_open() {
                last.ended_at_epoch = Some(now);
                last.ended_at = Some(stamp.clone());
            }
        }
        preset.sessions.push(SessionRecord {
            started_at_epoch: now,
            started_at: stamp,
            ended_at_epoch: None,
            ended_at: None,
            hits: Vec::new(),
        });
    }

    pub(super) fn close_session(&mut self) {
        let now = epoch_now();
        let stamp = format_local_now(self.config.language);
        if let Some(last) = self.active_mut().sessions.last_mut() {
            if last.is_open() {
                last.ended_at_epoch = Some(now);
                last.ended_at = Some(stamp);
            }
        }
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
                    self.active().count,
                    self.active().name.clone(),
                    self.counter.is_armed(),
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
            count_at_event: self.active().count,
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
                let n = self.active().pickers.len();
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
        self.active_mut().pickers = pickers;
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        let central = egui::CentralPanel::default().frame(
            egui::Frame::none()
                .fill(theme::BG)
                .inner_margin(egui::Margin::same(18.0)),
        );

        match std::mem::replace(&mut self.mode, Mode::Idle) {
            Mode::Idle => {
                central.show(ctx, |ui| self.render_idle(ctx, ui));
            }
            Mode::Picking(session) => {
                let session = self.render_picking(ctx, central, session);
                if let Some(s) = session {
                    self.mode = Mode::Picking(s);
                }
            }
        }
        self.render_confirm_modal(ctx);
        self.render_update_modal(ctx);
        self.flush_save();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = storage::save(&self.config);
    }
}
