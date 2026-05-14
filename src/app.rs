use crate::theme::{
    self, card, colored_button, ghost_button, icon_button, info_icon, paint_pokeball, pill,
    pill_dot, ACCENT, BAD, BG, BORDER, GOOD, SHINY, SURFACE, SURFACE_2, TEXT, TEXT_DIM, WARN,
};
use shiny_counter::capture::{capture, list_sources, sample_color, SourceInfo};
use shiny_counter::counter::{CounterEvent, CounterState};
use shiny_counter::i18n::{self, parse_hex, pluralize, Lang};
use shiny_counter::os_accent;
use shiny_counter::server::CounterServer;
use shiny_counter::storage;
use shiny_counter::types::{
    CaptureSource, Color, Config, HitRecord, LogEntry, PickerPoint, Preset, SessionRecord,
    MAX_PICKERS, MIN_PICKERS,
};
use shiny_counter::update::{self, UpdateChannel, UpdateStatus};

use eframe::egui;
use image::RgbaImage;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;

#[derive(Clone)]
struct PickClick {
    x: i32,
    y: i32,
    color: Color,
}

struct PickSession {
    image: RgbaImage,
    texture: egui::TextureHandle,
    clicks: Vec<Option<PickClick>>,
    current: usize,
}

enum Mode {
    Idle,
    Picking(PickSession),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingConfirm {
    None,
    ResetCounter,
    DeletePreset,
    ClearHistory,
    DeletePicker(usize),
}

pub struct ShinyApp {
    config: Config,
    counter: CounterState,
    running: bool,
    last_tick: Instant,
    last_sample: Vec<Color>,
    status: String,
    server: Option<CounterServer>,
    server_error: Option<String>,
    sources: Vec<SourceInfo>,
    sources_refreshed_at: Instant,
    mode: Mode,
    rename_buf: String,
    note_buf: String,
    hex_buf: HashMap<usize, String>,
    dirty: bool,
    last_save: Instant,
    theme_installed: bool,
    show_settings: bool,
    show_history: bool,
    pending_confirm: PendingConfirm,
    os_accent: Color,
    journal_page: usize,
    expanded_sessions: HashSet<usize>,
    session_pages: HashMap<usize, usize>,
    update_channel: UpdateChannel,
    update_prompt_dismissed_for: Option<String>,
    update_auto_opened_for: Option<String>,
    /// Set to `true` when an auto-download was kicked off (setting enabled).
    /// Used to suppress the "Downloading…" modal so the operation stays
    /// invisible until the asset is fully on disk — at which point we pop
    /// the "Download complete" modal once.
    update_auto_initiated: bool,
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
        app
    }

    fn s(&self) -> &'static i18n::Strings {
        i18n::strings(self.config.language)
    }

    fn active_idx(&self) -> usize {
        self.config
            .active_preset_index
            .min(self.config.presets.len().saturating_sub(1))
    }

    fn active(&self) -> &Preset {
        &self.config.presets[self.active_idx()]
    }

    fn active_mut(&mut self) -> &mut Preset {
        let i = self.active_idx();
        &mut self.config.presets[i]
    }

    fn sync_hex_buf(&mut self) {
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

    fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn flush_save(&mut self) {
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

    fn tick(&mut self, ctx: &egui::Context) {
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

        if let Some(s) = &self.server {
            s.update(
                count,
                self.active().name.clone(),
                self.counter.is_armed(),
                self.config.server_styled,
            );
        }
        if matches!(evt, CounterEvent::Incremented) {
            self.write_output_file();
        }
    }

    fn write_output_file(&mut self) {
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

    fn record_hit(&mut self, count: u32) {
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

    fn open_session(&mut self) {
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

    fn close_session(&mut self) {
        let now = epoch_now();
        let stamp = format_local_now(self.config.language);
        if let Some(last) = self.active_mut().sessions.last_mut() {
            if last.is_open() {
                last.ended_at_epoch = Some(now);
                last.ended_at = Some(stamp);
            }
        }
    }

    fn accent(&self) -> Color {
        self.active().accent_color.unwrap_or(self.os_accent)
    }

    fn accent32(&self) -> egui::Color32 {
        let c = self.accent();
        egui::Color32::from_rgb(c.r, c.g, c.b)
    }

    fn ensure_server(&mut self) {
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

    fn add_log(&mut self, note: String) {
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

    fn begin_pick(&mut self, ctx: &egui::Context) {
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

    fn commit_pick(&mut self, session: PickSession) {
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

    fn refresh_sources_if_stale(&mut self) {
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

// ── IDLE / MAIN VIEW ─────────────────────────────────────────────────────────

impl ShinyApp {
    fn render_idle(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.render_header(ui);
        ui.add_space(12.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let avail_w = ui.available_width();
                let body_h: f32 = 460.0;
                ui.horizontal_top(|ui| {
                    let left_w = (avail_w * 0.34).clamp(300.0, 440.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_w, body_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.render_counter_card(ui),
                    );
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), body_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| self.render_pickers_card(ctx, ui),
                    );
                });

                ui.add_space(12.0);
                self.render_bottom(ui);
                ui.add_space(8.0);
                self.render_history(ui);
            });

        ui.add_space(4.0);
        self.render_status_bar(ui);
    }

    fn render_header(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let (logo_rect, _) =
                    ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                paint_pokeball(ui.painter(), logo_rect.center(), 12.5);
                ui.label(
                    egui::RichText::new(self.s().app_title)
                        .size(20.0)
                        .strong()
                        .color(TEXT),
                );
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(6.0);

                // Preset selector with wider list
                ui.label(egui::RichText::new(self.s().preset).color(TEXT_DIM).small());
                let current_name = self.active().name.clone();
                let mut selected_idx = self.active_idx();
                let n_presets = self.config.presets.len() as f32;
                let combo_h = (n_presets * 32.0 + 16.0).clamp(60.0, 480.0);
                egui::ComboBox::from_id_salt("preset_combo")
                    .width(240.0)
                    .height(combo_h)
                    .selected_text(egui::RichText::new(current_name).color(TEXT).strong())
                    .show_ui(ui, |ui| {
                        ui.set_min_width(240.0);
                        for (i, p) in self.config.presets.iter().enumerate() {
                            ui.selectable_value(&mut selected_idx, i, &p.name);
                        }
                    });
                if selected_idx != self.active_idx() {
                    self.config.active_preset_index = selected_idx;
                    self.counter.reset();
                    self.sync_hex_buf();
                    self.mark_dirty();
                }
                if self.config.presets.len() > 1
                    && icon_button(ui, "×", BAD)
                        .on_hover_text(self.s().delete_preset)
                        .clicked()
                {
                    self.pending_confirm = PendingConfirm::DeletePreset;
                }

                if ghost_button(ui, self.s().new).clicked() {
                    let n = self.config.presets.len();
                    self.config
                        .presets
                        .push(Preset::new(format!("Preset {}", n + 1)));
                    self.config.active_preset_index = self.config.presets.len() - 1;
                    self.counter.reset();
                    self.sync_hex_buf();
                    self.mark_dirty();
                }
                if ghost_button(ui, self.s().duplicate).clicked() {
                    let mut copy = self.active().clone();
                    copy.name = format!("{} (copy)", copy.name);
                    copy.count = 0;
                    copy.hits.clear();
                    self.config.presets.push(copy);
                    self.config.active_preset_index = self.config.presets.len() - 1;
                    self.sync_hex_buf();
                    self.mark_dirty();
                }
                let settings_lbl = if self.show_settings {
                    self.s().hide_settings
                } else {
                    self.s().settings
                };
                if ghost_button(ui, settings_lbl).clicked() {
                    self.show_settings = !self.show_settings;
                }
            });

            if self.show_settings {
                ui.add_space(10.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(self.s().rename).color(TEXT_DIM).small());
                    if self.rename_buf.is_empty() {
                        self.rename_buf = self.active().name.clone();
                    }
                    let hint = self.active().name.clone();
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rename_buf)
                            .desired_width(200.0)
                            .margin(egui::Margin::symmetric(10.0, 10.0))
                            .font(egui::TextStyle::Button)
                            .hint_text(hint),
                    );
                    if ghost_button(ui, self.s().apply).clicked()
                        && !self.rename_buf.trim().is_empty()
                    {
                        self.active_mut().name = self.rename_buf.trim().to_string();
                        self.mark_dirty();
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(self.s().language)
                            .color(TEXT_DIM)
                            .small(),
                    );
                    let mut lang = self.config.language;
                    let cur_label = lang.label();
                    egui::ComboBox::from_id_salt("lang_combo")
                        .width(140.0)
                        .selected_text(cur_label)
                        .show_ui(ui, |ui| {
                            for l in Lang::all() {
                                ui.selectable_value(&mut lang, *l, l.label());
                            }
                        });
                    if lang != self.config.language {
                        self.config.language = lang;
                        self.mark_dirty();
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new(self.s().accent_color)
                            .color(TEXT_DIM)
                            .small(),
                    );
                    info_icon(ui, self.s().info_accent);
                    let current = self.accent();
                    let mut rgb = [
                        current.r as f32 / 255.0,
                        current.g as f32 / 255.0,
                        current.b as f32 / 255.0,
                    ];
                    if ui.color_edit_button_rgb(&mut rgb).changed() {
                        let new_c = Color::new(
                            (rgb[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                            (rgb[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                            (rgb[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                        );
                        self.active_mut().accent_color = Some(new_c);
                        self.mark_dirty();
                    }
                    if ghost_button(ui, self.s().use_os_accent).clicked() {
                        self.active_mut().accent_color = None;
                        self.mark_dirty();
                    }
                    ui.separator();
                    if ghost_button(ui, self.s().refresh).clicked() {
                        self.sources = list_sources();
                        self.sources_refreshed_at = Instant::now();
                    }
                });
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    let mut auto = self.config.auto_download_updates;
                    if ui
                        .checkbox(&mut auto, self.s().update_auto_download)
                        .changed()
                    {
                        self.config.auto_download_updates = auto;
                        self.mark_dirty();
                    }
                    info_icon(ui, self.s().info_auto_update);
                    ui.separator();
                    if ghost_button(ui, self.s().update_check).clicked() {
                        update::spawn_check(self.update_channel.clone());
                    }
                    match self.update_channel.status() {
                        UpdateStatus::Checking => {
                            ui.label(
                                egui::RichText::new(self.s().update_checking)
                                    .color(TEXT_DIM)
                                    .small(),
                            );
                        }
                        UpdateStatus::UpToDate { current } => {
                            pill(
                                ui,
                                &format!("{} (v{current})", self.s().update_uptodate),
                                GOOD,
                                GOOD,
                            );
                        }
                        UpdateStatus::Available(info) => {
                            pill(ui, &format!("v{}", info.latest_version), SHINY, SHINY);
                        }
                        UpdateStatus::Downloading { percent, .. } => {
                            pill(
                                ui,
                                &format!("{} {percent}%", self.s().update_downloading),
                                SHINY,
                                SHINY,
                            );
                        }
                        UpdateStatus::Downloaded { info, .. } => {
                            pill(
                                ui,
                                &format!(
                                    "{} v{}",
                                    self.s().update_downloaded_title,
                                    info.latest_version
                                ),
                                GOOD,
                                GOOD,
                            );
                        }
                        UpdateStatus::Error(e) => {
                            ui.colored_label(BAD, format!("{}: {e}", self.s().update_error));
                        }
                        UpdateStatus::Idle => {}
                    }
                });
            }
        });
    }

    fn render_source_combo(&mut self, ui: &mut egui::Ui) {
        let current_short = self
            .sources
            .iter()
            .find(|s| same_source(&s.source, &self.config.capture))
            .map(|s| s.short_label.clone())
            .unwrap_or_else(|| match &self.config.capture {
                CaptureSource::Monitor { index } => format!("Écran {}", index),
                CaptureSource::Window { app, title, .. } => {
                    let name = if !app.trim().is_empty() { app } else { title };
                    format!("Fenêtre: {}", short_str(name, 22))
                }
            });

        let mut chosen: Option<CaptureSource> = None;
        egui::ComboBox::from_id_salt("source_combo")
            .width(220.0)
            .height(360.0)
            .selected_text(egui::RichText::new(current_short).color(TEXT))
            .show_ui(ui, |ui| {
                ui.set_min_width(360.0);
                ui.label(
                    egui::RichText::new(self.s().source_picker)
                        .color(TEXT_DIM)
                        .small(),
                );
                ui.separator();
                for s in &self.sources {
                    let is_sel = same_source(&s.source, &self.config.capture);
                    if ui.selectable_label(is_sel, &s.label).clicked() {
                        chosen = Some(s.source.clone());
                    }
                }
            });
        if let Some(c) = chosen {
            self.config.capture = c;
            self.mark_dirty();
        }
    }

    fn render_counter_card(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(self.s().encounters)
                        .color(TEXT_DIM)
                        .size(12.0)
                        .strong(),
                );
                ui.add_space(6.0);
                let count = self.active().count;
                ui.label(
                    egui::RichText::new(format!("{count}"))
                        .color(TEXT)
                        .size(84.0)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 110.0).max(0.0) * 0.5);
                        if self.counter.is_armed() {
                            pill_dot(ui, self.s().armed, GOOD, GOOD);
                        } else {
                            pill_dot(ui, self.s().locked, WARN, WARN);
                        }
                        info_icon(ui, self.s().info_armed);
                    });
                });
            });

            ui.add_space(18.0);
            ui.vertical_centered_justified(|ui| {
                let (label, fill, stroke) = if self.running {
                    (self.s().stop_watching, BAD, BAD.linear_multiply(1.2))
                } else {
                    (self.s().start_watching, GOOD, GOOD.linear_multiply(1.2))
                };
                if colored_button(ui, label, fill, stroke).clicked() {
                    self.running = !self.running;
                    if self.running {
                        self.last_tick = Instant::now() - Duration::from_secs(10);
                        self.open_session();
                        self.status = format!(
                            "{} {:.2}s",
                            self.s().watching_msg,
                            self.active().interval_ms as f32 / 1000.0
                        );
                    } else {
                        self.close_session();
                        // Drop the live sample so the per-row "match/differ" border
                        // colour returns to neutral once watching has been stopped.
                        self.last_sample.clear();
                        self.status = self.s().paused.into();
                    }
                    self.mark_dirty();
                }
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let w = ui.available_width();
                ui.spacing_mut().item_spacing.x = 8.0;
                let bw = (w - 16.0) / 3.0;
                if ui
                    .add_sized([bw, 36.0], egui::Button::new("-1").rounding(10.0))
                    .clicked()
                {
                    let c = self.active().count.saturating_sub(1);
                    self.active_mut().count = c;
                    self.mark_dirty();
                    self.write_output_file();
                }
                if ui
                    .add_sized([bw, 36.0], egui::Button::new("+1").rounding(10.0))
                    .clicked()
                {
                    self.active_mut().count = self.active().count.saturating_add(1);
                    self.mark_dirty();
                    self.write_output_file();
                }
                if ui
                    .add_sized([bw, 36.0], egui::Button::new(self.s().reset).rounding(10.0))
                    .clicked()
                {
                    self.pending_confirm = PendingConfirm::ResetCounter;
                }
            });
            ui.add_space(6.0);
            ui.vertical_centered_justified(|ui| {
                if ghost_button(ui, self.s().rearm).clicked() {
                    self.counter.reset();
                    self.status = self.s().manual_rearm.into();
                }
            });
        });
    }

    fn render_pickers_card(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let accent = self.accent32();
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(self.s().color_pickers)
                        .size(15.0)
                        .strong(),
                );
                pill(
                    ui,
                    &format!("{}/{}", self.active().pickers.len(), MAX_PICKERS),
                    accent,
                    accent,
                );
                ui.add_space(6.0);
                let acc = self.accent32();
                if colored_button(ui, self.s().pick_on_screen, acc, acc).clicked() {
                    self.begin_pick(ctx);
                }
                info_icon(ui, self.s().info_pick);
                let can_add = self.active().pickers.len() < MAX_PICKERS;
                ui.add_enabled_ui(can_add, |ui| {
                    if ghost_button(ui, self.s().add_slot).clicked() {
                        self.active_mut().pickers.push(PickerPoint::default());
                        let i = self.active().pickers.len() - 1;
                        self.hex_buf.insert(i, "#000000".into());
                        self.mark_dirty();
                    }
                });
            });
            ui.add_space(6.0);
            // Source selector — placed right under Pick controls so it stays visible.
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(self.s().source).color(TEXT_DIM).small());
                info_icon(ui, self.s().info_source);
                self.render_source_combo(ui);
            });

            ui.add_space(10.0);
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let n = self.active().pickers.len();
                    for i in 0..n {
                        let live_i = self.last_sample.get(i).copied();
                        self.render_picker_row(ui, i, live_i);
                    }
                });
        });
    }

    /// Renders a single picker row. Deletion is routed through the confirm modal.
    fn render_picker_row(&mut self, ui: &mut egui::Ui, i: usize, live_color: Option<Color>) {
        let p = self.active().pickers[i];
        let tol = self.active().tolerance;
        let matches = live_color
            .map(|l| l.matches(p.target, tol))
            .unwrap_or(false);
        let border_color = match (live_color.is_some(), matches) {
            (true, true) => GOOD,
            (true, false) => BAD,
            _ => BORDER,
        };
        let accent = self.accent32();

        egui::Frame::none()
            .fill(SURFACE_2)
            .stroke(egui::Stroke::new(1.0, border_color))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::symmetric(12.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    // Slot badge (preset accent)
                    let badge_bg = if matches && live_color.is_some() {
                        GOOD
                    } else {
                        accent
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                    ui.painter()
                        .circle_filled(rect.center(), 13.0, badge_bg.linear_multiply(0.55));
                    ui.painter().circle_stroke(
                        rect.center(),
                        12.0,
                        egui::Stroke::new(1.5, badge_bg),
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        format!("{}", i + 1),
                        egui::FontId::proportional(12.5),
                        TEXT,
                    );

                    let mut x = p.x;
                    let mut y = p.y;
                    if ui
                        .add(
                            egui::DragValue::new(&mut x)
                                .speed(1.0)
                                .range(0..=10_000)
                                .prefix("x:")
                                .max_decimals(0),
                        )
                        .changed()
                    {
                        self.active_mut().pickers[i].x = x;
                        self.mark_dirty();
                    }
                    if ui
                        .add(
                            egui::DragValue::new(&mut y)
                                .speed(1.0)
                                .range(0..=10_000)
                                .prefix("y:")
                                .max_decimals(0),
                        )
                        .changed()
                    {
                        self.active_mut().pickers[i].y = y;
                        self.mark_dirty();
                    }

                    // Custom color popup with hex paste-able input.
                    let popup_id = ui.make_persistent_id(("color_popup", i));
                    let swatch_resp = ui.add(
                        egui::Button::new("")
                            .fill(egui::Color32::from_rgb(p.target.r, p.target.g, p.target.b))
                            .stroke(egui::Stroke::new(1.0, BORDER))
                            .rounding(egui::Rounding::same(6.0))
                            .min_size(egui::vec2(34.0, 30.0)),
                    );
                    if swatch_resp.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(popup_id));
                    }
                    let mut commit_color: Option<Color> = None;
                    egui::popup::popup_below_widget(
                        ui,
                        popup_id,
                        &swatch_resp,
                        egui::PopupCloseBehavior::CloseOnClickOutside,
                        |ui| {
                            ui.set_min_width(260.0);
                            ui.spacing_mut().slider_width = 200.0;
                            let mut c32 =
                                egui::Color32::from_rgb(p.target.r, p.target.g, p.target.b);
                            let changed = egui::widgets::color_picker::color_picker_color32(
                                ui,
                                &mut c32,
                                egui::widgets::color_picker::Alpha::Opaque,
                            );
                            if changed {
                                let [r, g, b, _] = c32.to_array();
                                commit_color = Some(Color::new(r, g, b));
                            }
                            ui.add_space(6.0);
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Hex").color(TEXT_DIM).small());
                                let buf =
                                    self.hex_buf.entry(i).or_insert_with(|| p.target.to_hex());
                                let resp = ui.add(
                                    egui::TextEdit::singleline(buf)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(110.0)
                                        .hint_text("#RRGGBB"),
                                );
                                let enter = resp.ctx.input(|i| i.key_pressed(egui::Key::Enter));
                                if resp.lost_focus() || enter {
                                    if let Some((r, g, b)) = parse_hex(buf) {
                                        let new_c = Color::new(r, g, b);
                                        if new_c != p.target {
                                            commit_color = Some(new_c);
                                        }
                                        *self.hex_buf.entry(i).or_default() = new_c.to_hex();
                                    } else {
                                        *self.hex_buf.entry(i).or_default() = p.target.to_hex();
                                    }
                                }
                                if ui.small_button("Copier").clicked() {
                                    ui.ctx().copy_text(p.target.to_hex());
                                }
                            });
                        },
                    );
                    if let Some(new_c) = commit_color {
                        self.active_mut().pickers[i].target = new_c;
                        self.hex_buf.insert(i, new_c.to_hex());
                        self.mark_dirty();
                    }

                    ui.label(
                        egui::RichText::new(p.target.to_hex())
                            .monospace()
                            .color(TEXT_DIM)
                            .small(),
                    );

                    if let Some(l) = live_color {
                        ui.separator();
                        ui.label(egui::RichText::new(self.s().live).color(TEXT_DIM).small());
                        color_swatch(ui, l, 22.0);
                    }

                    ui.add_space(8.0);
                    let can_remove = self.active().pickers.len() > MIN_PICKERS;
                    let col = if can_remove { BAD } else { TEXT_DIM };
                    let resp = icon_button(ui, "×", col).on_hover_text(self.s().remove_slot_tip);
                    if can_remove && resp.clicked() {
                        self.pending_confirm = PendingConfirm::DeletePicker(i);
                    }
                });
            });
        ui.add_space(6.0);
    }

    fn render_bottom(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(self.s().interval)
                        .color(TEXT_DIM)
                        .small(),
                );
                info_icon(ui, self.s().info_interval);
                let mut interval = self.active().interval_ms as i64;
                if ui
                    .add(
                        egui::Slider::new(&mut interval, 50..=10_000)
                            .suffix(" ms")
                            .step_by(10.0)
                            .clamping(egui::SliderClamping::Always),
                    )
                    .changed()
                {
                    self.active_mut().interval_ms = interval.max(50) as u64;
                    self.mark_dirty();
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(self.s().tolerance)
                        .color(TEXT_DIM)
                        .small(),
                );
                info_icon(ui, self.s().info_tolerance);
                let mut tol = self.active().tolerance as i32;
                if ui
                    .add(
                        egui::Slider::new(&mut tol, 0..=128).clamping(egui::SliderClamping::Always),
                    )
                    .changed()
                {
                    self.active_mut().tolerance = tol.clamp(0, 128) as u8;
                    self.mark_dirty();
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(self.s().preset_notes)
                        .color(TEXT_DIM)
                        .small(),
                );
                let mut notes = self.active().notes.clone();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut notes)
                            .desired_width(220.0)
                            .margin(egui::Margin::symmetric(10.0, 10.0))
                            .font(egui::TextStyle::Button)
                            .hint_text(self.s().preset_notes),
                    )
                    .changed()
                {
                    self.active_mut().notes = notes;
                    self.mark_dirty();
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(self.s().server_label)
                        .color(TEXT)
                        .strong(),
                );
                ui.add_space(6.0);
                info_icon(ui, self.s().info_obs_overlay);
                let mut enabled = self.config.server_enabled;
                if ui.checkbox(&mut enabled, "").changed() {
                    self.config.server_enabled = enabled;
                    self.mark_dirty();
                }
                ui.label(self.s().port);
                let mut port_buf = self.config.server_port.to_string();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut port_buf)
                            .desired_width(80.0)
                            .margin(egui::Margin::symmetric(10.0, 10.0))
                            .font(egui::TextStyle::Button),
                    )
                    .changed()
                {
                    if let Ok(p) = port_buf.parse::<u16>() {
                        if p != 0 && p != self.config.server_port {
                            self.config.server_port = p;
                            self.mark_dirty();
                        }
                    }
                }
                let mut styled = self.config.server_styled;
                if ui.checkbox(&mut styled, self.s().server_styled).changed() {
                    self.config.server_styled = styled;
                    self.mark_dirty();
                }
                info_icon(ui, self.s().info_server_style);
            });
            if self.config.server_enabled {
                ui.add_space(4.0);
                let url = format!("http://127.0.0.1:{}/", self.config.server_port);
                ui.horizontal_wrapped(|ui| {
                    let mut url_mut = url.clone();
                    // Copy-pasteable read-only text. The user can select all
                    // and Ctrl+C, or click the dedicated button below.
                    ui.add(
                        egui::TextEdit::singleline(&mut url_mut)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(240.0)
                            .margin(egui::Margin::symmetric(10.0, 10.0)),
                    );
                    if ghost_button(ui, self.s().copy).clicked() {
                        ui.ctx().copy_text(url.clone());
                        self.status = format!("{}: {}", self.s().copied, url);
                    }
                    ui.label(
                        egui::RichText::new(self.s().add_as_obs_source)
                            .color(TEXT_DIM)
                            .small(),
                    );
                });
            }
            if let Some(err) = &self.server_error {
                ui.colored_label(BAD, format!("({err})"));
            }

            ui.add_space(8.0);

            // File output row: lets the user point an OBS Text-from-File
            // source (or any external reader) at a path we keep in sync.
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    egui::RichText::new(self.s().file_output)
                        .color(TEXT)
                        .strong(),
                );
                ui.add_space(6.0);
                info_icon(ui, self.s().info_file_output);
                let mut enabled = self.active().output_file_enabled;
                if ui
                    .checkbox(&mut enabled, self.s().file_output_enabled)
                    .changed()
                {
                    self.active_mut().output_file_enabled = enabled;
                    self.mark_dirty();
                    if enabled {
                        self.write_output_file();
                    }
                }
                let mut path_str = self
                    .active()
                    .output_file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut path_str)
                        .desired_width(280.0)
                        .margin(egui::Margin::symmetric(10.0, 10.0))
                        .font(egui::TextStyle::Button)
                        .hint_text(self.s().file_output_path),
                );
                if resp.changed() {
                    let trimmed = path_str.trim();
                    self.active_mut().output_file = if trimmed.is_empty() {
                        None
                    } else {
                        Some(std::path::PathBuf::from(trimmed))
                    };
                    self.mark_dirty();
                }
                if ghost_button(ui, self.s().file_output_browse).clicked() {
                    let mut dialog = rfd::FileDialog::new()
                        .set_title(self.s().file_output)
                        .set_file_name("shiny_count.txt")
                        .add_filter("Text", &["txt"]);
                    if let Some(parent) =
                        self.active().output_file.as_ref().and_then(|p| p.parent())
                    {
                        dialog = dialog.set_directory(parent);
                    }
                    if let Some(picked) = dialog.save_file() {
                        self.active_mut().output_file = Some(picked);
                        self.mark_dirty();
                        if self.active().output_file_enabled {
                            self.write_output_file();
                        }
                    }
                }
                if self.active().output_file.is_some()
                    && ghost_button(ui, self.s().file_output_clear).clicked()
                {
                    self.active_mut().output_file = None;
                    self.mark_dirty();
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(self.s().journal).size(15.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ghost_button(ui, self.s().clear_log).clicked() {
                        self.config.log.clear();
                        self.mark_dirty();
                    }
                    if ghost_button(ui, self.s().snapshot).clicked() {
                        let note = format!("snapshot = {}", self.active().count);
                        self.add_log(note);
                    }
                });
            });
            ui.horizontal(|ui| {
                let avail = ui.available_width();
                let hint = self.s().note_hint;
                ui.add(
                    egui::TextEdit::singleline(&mut self.note_buf)
                        .desired_width((avail - 110.0).max(180.0))
                        .margin(egui::Margin::symmetric(10.0, 10.0))
                        .font(egui::TextStyle::Button)
                        .hint_text(hint),
                );
                let acc = self.accent32();
                if colored_button(ui, self.s().add_note, acc, acc).clicked()
                    && !self.note_buf.trim().is_empty()
                {
                    let n = self.note_buf.trim().to_string();
                    self.add_log(n);
                    self.note_buf.clear();
                }
            });
            ui.add_space(6.0);
            const PAGE_SIZE: usize = 15;
            let total = self.config.log.len();
            let pages = total.div_ceil(PAGE_SIZE).max(1);
            if self.journal_page >= pages {
                self.journal_page = pages - 1;
            }
            let page = self.journal_page;
            let start = page * PAGE_SIZE;
            let end = (start + PAGE_SIZE).min(total);

            let row_h = 32.0;
            let target_h = (PAGE_SIZE as f32 * row_h).min((end - start).max(1) as f32 * row_h);
            let mut delete_idx: Option<usize> = None;
            let mut restore_idx: Option<usize> = None;
            let accent = self.accent32();
            let accent_hot = accent;
            egui::ScrollArea::vertical()
                .id_salt("journal_scroll")
                .min_scrolled_height(target_h.max(160.0))
                .max_height(PAGE_SIZE as f32 * row_h)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if total == 0 {
                        ui.label(
                            egui::RichText::new(self.s().no_entries)
                                .color(TEXT_DIM)
                                .italics(),
                        );
                    }
                    for i in start..end {
                        let entry = self.config.log[i].clone();
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(&entry.timestamp)
                                    .color(TEXT_DIM)
                                    .monospace()
                                    .small(),
                            );
                            pill(ui, &entry.preset_name, accent, accent_hot);
                            ui.label(
                                egui::RichText::new(format!("#{}", entry.count_at_event))
                                    .monospace()
                                    .color(SHINY),
                            );
                            ui.label(&entry.note);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if icon_button(ui, "×", BAD)
                                        .on_hover_text(self.s().delete_entry)
                                        .clicked()
                                    {
                                        delete_idx = Some(i);
                                    }
                                    if icon_button(ui, "R", GOOD)
                                        .on_hover_text(self.s().restore_count)
                                        .clicked()
                                    {
                                        restore_idx = Some(i);
                                    }
                                },
                            );
                        });
                    }
                });
            if let Some(i) = delete_idx {
                self.config.log.remove(i);
                self.mark_dirty();
            }
            if let Some(i) = restore_idx {
                let val = self.config.log[i].count_at_event;
                self.active_mut().count = val;
                self.mark_dirty();
                self.write_output_file();
            }
            if pages > 1 {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_prev = self.journal_page > 0;
                    let can_next = self.journal_page + 1 < pages;
                    ui.add_enabled_ui(can_prev, |ui| {
                        if ghost_button(ui, self.s().page_prev).clicked() {
                            self.journal_page -= 1;
                        }
                    });
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {} / {}",
                            self.s().page,
                            self.journal_page + 1,
                            pages
                        ))
                        .color(TEXT_DIM)
                        .small(),
                    );
                    ui.add_enabled_ui(can_next, |ui| {
                        if ghost_button(ui, self.s().page_next).clicked() {
                            self.journal_page += 1;
                        }
                    });
                });
            }
        });
    }

    fn render_history(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            let total_hits: usize = self.active().sessions.iter().map(|s| s.hits.len()).sum();
            let n_sessions = self.active().sessions.len();
            let lang = self.config.language;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(self.s().history).size(15.0).strong());
                info_icon(ui, self.s().info_session);
                let sess_word = pluralize(lang, n_sessions, self.s().session, self.s().sessions);
                let hit_word =
                    pluralize(lang, total_hits, self.s().hit_singular, self.s().hit_plural);
                pill(
                    ui,
                    &format!("{n_sessions} {sess_word}"),
                    self.accent32(),
                    self.accent32(),
                );
                pill(ui, &format!("{total_hits} {hit_word}"), SHINY, SHINY);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if self.show_history {
                        self.s().hide_settings
                    } else {
                        self.s().history
                    };
                    if ghost_button(ui, label).clicked() {
                        self.show_history = !self.show_history;
                    }
                    if !self.active().sessions.is_empty()
                        && ghost_button(ui, self.s().clear_history).clicked()
                    {
                        self.pending_confirm = PendingConfirm::ClearHistory;
                    }
                });
            });
            if !self.show_history {
                return;
            }
            ui.add_space(6.0);
            if self.active().sessions.is_empty() {
                ui.label(
                    egui::RichText::new(self.s().no_sessions)
                        .color(TEXT_DIM)
                        .italics(),
                );
                return;
            }
            let lang = self.config.language;
            let accent = self.accent32();
            // Iterate sessions most recent first.
            let n = self.active().sessions.len();
            for rev_idx in 0..n {
                let idx = n - 1 - rev_idx;
                self.render_session_block(ui, idx, lang, accent);
            }
        });
    }

    fn render_session_block(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        lang: Lang,
        accent: egui::Color32,
    ) {
        // Pull just the scalars / strings we need — the full session may carry
        // thousands of hits, cloning it every frame would be wasteful.
        let (total, duration, started, ended, is_open) = {
            let s = &self.active().sessions[idx];
            (
                s.hits.len(),
                format_delta(s.duration_secs()),
                format_datetime(datetime_from_epoch(s.started_at_epoch), lang),
                s.ended_at_epoch
                    .map(|e| format_datetime(datetime_from_epoch(e), lang)),
                s.is_open(),
            )
        };
        let is_expanded = self.expanded_sessions.contains(&idx);

        let bg = SURFACE_2;
        let stroke_col = if is_open { GOOD } else { BORDER };

        let mut toggle = false;
        egui::Frame::none()
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, stroke_col))
            .rounding(egui::Rounding::same(10.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let arrow = if is_expanded { "-" } else { "+" };
                    let arrow_resp = ui.add(
                        egui::Button::new(
                            egui::RichText::new(arrow).color(TEXT).strong().size(14.0),
                        )
                        .fill(BG)
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .rounding(egui::Rounding::same(6.0))
                        .min_size(egui::vec2(28.0, 28.0)),
                    );
                    if arrow_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if arrow_resp.clicked() {
                        toggle = true;
                    }
                    pill(
                        ui,
                        &format!("{} #{}", self.s().session, idx + 1),
                        accent,
                        accent,
                    );
                    if is_open {
                        pill_dot(ui, self.s().open_session, GOOD, GOOD);
                    }
                    ui.label(
                        egui::RichText::new(&started)
                            .color(TEXT_DIM)
                            .monospace()
                            .small(),
                    );
                    if let Some(e) = &ended {
                        ui.label(
                            egui::RichText::new(self.s().time_to)
                                .color(TEXT_DIM)
                                .small(),
                        );
                        ui.label(egui::RichText::new(e).color(TEXT_DIM).monospace().small());
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let w = pluralize(lang, total, self.s().hit_singular, self.s().hit_plural);
                    pill(ui, &format!("{total} {w}"), SHINY, SHINY);
                    ui.label(
                        egui::RichText::new(format!("{} : {duration}", self.s().duration))
                            .color(TEXT_DIM)
                            .small(),
                    );
                });

                if is_expanded {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    if total == 0 {
                        ui.label(
                            egui::RichText::new(self.s().no_hits)
                                .color(TEXT_DIM)
                                .italics(),
                        );
                    } else {
                        // Pagination
                        const PAGE_SIZE: usize = 15;
                        let pages = total.div_ceil(PAGE_SIZE);
                        let page = self
                            .session_pages
                            .get(&idx)
                            .copied()
                            .unwrap_or(0)
                            .min(pages - 1);
                        let start = page * PAGE_SIZE;
                        let end = (start + PAGE_SIZE).min(total);
                        // Clone only the visible page rather than the whole
                        // session's hit list.
                        let hits_page: Vec<HitRecord> = self.active().sessions[idx]
                            .hits
                            .iter()
                            .rev()
                            .skip(start)
                            .take(end - start)
                            .cloned()
                            .collect();
                        for h in &hits_page {
                            ui.horizontal(|ui| {
                                pill(ui, &format!("#{}", h.index), SHINY, SHINY);
                                ui.label(
                                    egui::RichText::new(&h.timestamp)
                                        .monospace()
                                        .color(TEXT)
                                        .small(),
                                );
                                ui.label(
                                    egui::RichText::new(format!(
                                        "+{}  ({})",
                                        format_delta(h.delta_secs),
                                        self.s().since_previous
                                    ))
                                    .color(TEXT_DIM)
                                    .small(),
                                );
                            });
                        }
                        if pages > 1 {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let can_prev = page > 0;
                                let can_next = page + 1 < pages;
                                ui.add_enabled_ui(can_prev, |ui| {
                                    if ghost_button(ui, self.s().page_prev).clicked() {
                                        self.session_pages.insert(idx, page - 1);
                                    }
                                });
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} {} / {}",
                                        self.s().page,
                                        page + 1,
                                        pages
                                    ))
                                    .color(TEXT_DIM)
                                    .small(),
                                );
                                ui.add_enabled_ui(can_next, |ui| {
                                    if ghost_button(ui, self.s().page_next).clicked() {
                                        self.session_pages.insert(idx, page + 1);
                                    }
                                });
                            });
                        }
                    }
                }
            });
        if toggle {
            if is_expanded {
                self.expanded_sessions.remove(&idx);
            } else {
                self.expanded_sessions.insert(idx);
            }
        }
        ui.add_space(6.0);
    }

    fn render_confirm_modal(&mut self, ctx: &egui::Context) {
        if self.pending_confirm == PendingConfirm::None {
            return;
        }
        let (title, msg, action_label) = match self.pending_confirm {
            PendingConfirm::ResetCounter => (
                self.s().confirm_reset_title,
                self.s().confirm_reset_msg,
                self.s().action_reset,
            ),
            PendingConfirm::DeletePreset => (
                self.s().confirm_delete_preset_title,
                self.s().confirm_delete_preset_msg,
                self.s().action_delete,
            ),
            PendingConfirm::ClearHistory => (
                self.s().confirm_clear_history_title,
                self.s().confirm_clear_history_msg,
                self.s().action_clear,
            ),
            PendingConfirm::DeletePicker(_) => (
                self.s().confirm_delete_picker_title,
                self.s().confirm_delete_picker_msg,
                self.s().action_delete,
            ),
            PendingConfirm::None => return,
        };

        // Dark backdrop that captures clicks so the rest of the UI is "inert".
        let backdrop_id = egui::Id::new("modal_backdrop");
        egui::Area::new(backdrop_id)
            .order(egui::Order::Middle)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(true)
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.painter()
                    .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(180));
                ui.allocate_response(screen_rect.size(), egui::Sense::click_and_drag());
            });

        // Esc dismisses
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.pending_confirm = PendingConfirm::None;
            return;
        }

        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new(title)
            .id(egui::Id::new("confirm_modal_window"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .auto_sized()
            .frame(
                egui::Frame::none()
                    .fill(SURFACE)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .rounding(egui::Rounding::same(14.0))
                    .inner_margin(egui::Margin::same(20.0))
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0.0, 8.0),
                        blur: 32.0,
                        spread: 0.0,
                        color: egui::Color32::from_black_alpha(180),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(440.0);
                ui.label(egui::RichText::new(title).size(17.0).strong().color(TEXT));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(msg).color(TEXT_DIM).size(13.5));
                ui.add_space(18.0);
                // Buttons aligned bottom-right within a single row.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if colored_button(ui, action_label, BAD, BAD.linear_multiply(1.2)).clicked()
                        {
                            confirmed = true;
                        }
                        ui.add_space(8.0);
                        if ghost_button(ui, self.s().cancel).clicked() {
                            cancelled = true;
                        }
                    });
                });
            });

        if cancelled {
            self.pending_confirm = PendingConfirm::None;
            return;
        }
        if confirmed {
            match self.pending_confirm {
                PendingConfirm::ResetCounter => {
                    self.active_mut().count = 0;
                    self.active_mut().sessions.clear();
                    self.counter.reset();
                    self.expanded_sessions.clear();
                    self.session_pages.clear();
                    self.write_output_file();
                    self.mark_dirty();
                }
                PendingConfirm::DeletePreset => {
                    if self.config.presets.len() > 1 {
                        let i = self.active_idx();
                        self.config.presets.remove(i);
                        self.config.active_preset_index = 0;
                        self.counter.reset();
                        self.sync_hex_buf();
                        self.expanded_sessions.clear();
                        self.session_pages.clear();
                        self.mark_dirty();
                    }
                }
                PendingConfirm::ClearHistory => {
                    self.active_mut().sessions.clear();
                    self.expanded_sessions.clear();
                    self.session_pages.clear();
                    self.mark_dirty();
                }
                PendingConfirm::DeletePicker(i) => {
                    if self.active().pickers.len() > MIN_PICKERS && i < self.active().pickers.len()
                    {
                        self.active_mut().pickers.remove(i);
                        self.sync_hex_buf();
                        // Drop any live samples — indexing may shift.
                        self.last_sample.clear();
                        self.mark_dirty();
                    }
                }
                PendingConfirm::None => {}
            }
            self.pending_confirm = PendingConfirm::None;
        }
    }

    fn render_update_modal(&mut self, ctx: &egui::Context) {
        let status = self.update_channel.status();

        // Active version from any update state that should pop the modal.
        let version_in_play = match &status {
            UpdateStatus::Available(i)
            | UpdateStatus::Downloading { info: i, .. }
            | UpdateStatus::Downloaded { info: i, .. } => Some(i.latest_version.clone()),
            _ => None,
        };
        let Some(version) = version_in_play else {
            return;
        };

        // Snooze gating.
        let now = epoch_now();
        if self
            .config
            .update_snoozes
            .iter()
            .any(|s| s.version == version && s.until_epoch > now)
        {
            return;
        }

        // Auto-download path: start the download silently the first time we
        // see this version with the setting on.
        if let UpdateStatus::Available(info) = &status {
            if self.config.auto_download_updates
                && info.asset.is_some()
                && self.update_auto_opened_for.as_deref() != Some(&info.latest_version)
            {
                self.update_auto_opened_for = Some(info.latest_version.clone());
                self.update_auto_initiated = true;
                update::spawn_download(self.update_channel.clone(), info.clone());
                return;
            }
        }

        // While an auto-download is streaming, stay silent — the user opted
        // in to "do it for me", so a progress modal would be intrusive. The
        // Settings pill keeps showing the percentage. The modal will reopen
        // automatically once the status transitions to `Downloaded`.
        if self.update_auto_initiated
            && matches!(&status, UpdateStatus::Downloading { .. })
        {
            return;
        }

        if self.update_prompt_dismissed_for.as_deref() == Some(&version) {
            return;
        }

        // Backdrop
        let backdrop_id = egui::Id::new("update_backdrop");
        egui::Area::new(backdrop_id)
            .order(egui::Order::Middle)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .interactable(true)
            .show(ctx, |ui| {
                let screen_rect = ctx.screen_rect();
                ui.painter()
                    .rect_filled(screen_rect, 0.0, egui::Color32::from_black_alpha(180));
                ui.allocate_response(screen_rect.size(), egui::Sense::click_and_drag());
            });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.update_prompt_dismissed_for = Some(version.clone());
            return;
        }

        let acc = self.accent32();
        let mut action_download = false;
        let mut action_open_file: Option<std::path::PathBuf> = None;
        let mut action_open_page: Option<update::UpdateInfo> = None;
        let mut action_snooze = false;
        let mut action_close = false;

        egui::Window::new("update_modal")
            .id(egui::Id::new("update_modal_window"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .auto_sized()
            .frame(
                egui::Frame::none()
                    .fill(SURFACE)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .rounding(egui::Rounding::same(14.0))
                    .inner_margin(egui::Margin::same(20.0))
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0.0, 8.0),
                        blur: 32.0,
                        spread: 0.0,
                        color: egui::Color32::from_black_alpha(180),
                    }),
            )
            .show(ctx, |ui| {
                ui.set_width(480.0);
                match &status {
                    UpdateStatus::Available(info) => {
                        ui.label(
                            egui::RichText::new(self.s().update_available_title)
                                .size(17.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.add_space(6.0);
                        pill(ui, &info.release_name, SHINY, SHINY);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(self.s().update_available_msg)
                                .color(TEXT_DIM)
                                .size(13.5),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "v{}  ->  v{}",
                                info.current_version, info.latest_version
                            ))
                            .color(TEXT_DIM)
                            .monospace()
                            .small(),
                        );
                        if info.asset.is_none() {
                            ui.add_space(6.0);
                            ui.colored_label(BAD, self.s().update_no_asset);
                        }
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if info.asset.is_some() {
                                        if colored_button(ui, self.s().update_download, acc, acc)
                                            .clicked()
                                        {
                                            action_download = true;
                                        }
                                    } else if colored_button(ui, self.s().update_open, acc, acc)
                                        .clicked()
                                    {
                                        action_open_page = Some(info.clone());
                                    }
                                    ui.add_space(8.0);
                                    if ghost_button(ui, self.s().update_later).clicked() {
                                        action_snooze = true;
                                    }
                                },
                            );
                        });
                    }
                    UpdateStatus::Downloading { info, percent } => {
                        ui.label(
                            egui::RichText::new(self.s().update_downloading)
                                .size(17.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.add_space(6.0);
                        pill(ui, &info.release_name, SHINY, SHINY);
                        ui.add_space(10.0);
                        ui.add(
                            egui::ProgressBar::new(*percent as f32 / 100.0)
                                .show_percentage()
                                .desired_width(440.0),
                        );
                        if let Some(asset) = &info.asset {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}  ({})",
                                    asset.name,
                                    format_size(asset.size)
                                ))
                                .color(TEXT_DIM)
                                .monospace()
                                .small(),
                            );
                        }
                    }
                    UpdateStatus::Downloaded { info, path } => {
                        ui.label(
                            egui::RichText::new(self.s().update_downloaded_title)
                                .size(17.0)
                                .strong()
                                .color(TEXT),
                        );
                        ui.add_space(6.0);
                        pill(ui, &info.release_name, SHINY, SHINY);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(self.s().update_downloaded_msg)
                                .color(TEXT_DIM)
                                .size(13.5),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new(path.display().to_string())
                                .color(TEXT_DIM)
                                .monospace()
                                .small(),
                        );
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if colored_button(ui, self.s().update_open_file, acc, acc)
                                        .clicked()
                                    {
                                        action_open_file = Some(path.clone());
                                    }
                                    ui.add_space(8.0);
                                    if ghost_button(ui, self.s().cancel).clicked() {
                                        action_close = true;
                                    }
                                },
                            );
                        });
                    }
                    _ => {}
                }
            });

        if action_download {
            if let UpdateStatus::Available(info) = &status {
                update::spawn_download(self.update_channel.clone(), info.clone());
            }
        }
        if let Some(page_info) = action_open_page {
            update::open_release_page(&page_info);
            self.snooze_version(&version);
        }
        if let Some(path) = action_open_file {
            let _ = update::open_path(&path);
            self.snooze_version(&version);
            self.update_channel.set(UpdateStatus::Idle);
            self.update_auto_initiated = false;
        }
        if action_snooze {
            self.snooze_version(&version);
            self.update_channel.set(UpdateStatus::Idle);
            self.update_auto_initiated = false;
        }
        if action_close {
            self.update_channel.set(UpdateStatus::Idle);
            self.update_auto_initiated = false;
        }
    }

    fn snooze_version(&mut self, version: &str) {
        let until = epoch_now() + update::SNOOZE_DURATION_SECS;
        let snoozes = &mut self.config.update_snoozes;
        if let Some(existing) = snoozes.iter_mut().find(|s| s.version == version) {
            existing.until_epoch = until;
        } else {
            snoozes.push(shiny_counter::types::UpdateSnooze {
                version: version.to_string(),
                until_epoch: until,
            });
        }
        self.update_prompt_dismissed_for = Some(version.to_string());
        self.mark_dirty();
    }

    fn render_status_bar(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let dot = if self.running { GOOD } else { TEXT_DIM };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, dot);
            ui.label(egui::RichText::new(&self.status).color(TEXT_DIM).italics());
        });
    }
}

// ── PICK MODE ────────────────────────────────────────────────────────────────

impl ShinyApp {
    fn render_picking(
        &mut self,
        ctx: &egui::Context,
        panel: egui::CentralPanel,
        mut session: PickSession,
    ) -> Option<PickSession> {
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.status = self.s().pick_cancelled.into();
            return None;
        }
        let mut keep = true;
        let mut commit = false;
        panel.show(ctx, |ui| {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(self.s().pick_on_screen_title)
                            .strong()
                            .size(18.0)
                            .color(TEXT),
                    );
                    let assigned = session.clicks.iter().filter(|c| c.is_some()).count();
                    let acc = self.accent32();
                    pill(
                        ui,
                        &format!(
                            "{} / {} {}",
                            assigned,
                            session.clicks.len(),
                            self.s().placed
                        ),
                        acc,
                        acc,
                    );
                    pill(
                        ui,
                        &format!("#{} {}", session.current + 1, self.s().slot_active),
                        SHINY,
                        SHINY,
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let acc = self.accent32();
                        if colored_button(ui, self.s().apply_picks, acc, acc).clicked() {
                            commit = true;
                        }
                        if ghost_button(ui, self.s().cancel).clicked() {
                            keep = false;
                        }
                        if ghost_button(ui, self.s().clear_all).clicked() {
                            for c in session.clicks.iter_mut() {
                                *c = None;
                            }
                            session.current = 0;
                        }
                        let can_add = session.clicks.len() < MAX_PICKERS;
                        let can_remove = session.clicks.len() > MIN_PICKERS;
                        ui.add_enabled_ui(can_add, |ui| {
                            if ghost_button(ui, self.s().add_slot).clicked() {
                                session.clicks.push(None);
                                session.current = session.clicks.len() - 1;
                            }
                        });
                        ui.add_enabled_ui(can_remove, |ui| {
                            if ghost_button(ui, "−").clicked() {
                                session.clicks.pop();
                                if session.current >= session.clicks.len() {
                                    session.current = session.clicks.len().saturating_sub(1);
                                }
                            }
                        });
                    });
                });
            });

            ui.add_space(10.0);

            ui.horizontal_top(|ui| {
                let total = ui.available_size();
                let side_w: f32 = 240.0;
                let img_area = egui::vec2(total.x - side_w - 14.0, total.y);

                ui.allocate_ui_with_layout(
                    img_area,
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        egui::Frame::none()
                            .fill(SURFACE)
                            .stroke(egui::Stroke::new(1.0, BORDER))
                            .rounding(egui::Rounding::same(12.0))
                            .inner_margin(egui::Margin::same(8.0))
                            .show(ui, |ui| {
                                let avail = ui.available_size();
                                let img_w = session.image.width() as f32;
                                let img_h = session.image.height() as f32;
                                let scale = (avail.x / img_w).min(avail.y / img_h).max(0.02);
                                let draw_size = egui::vec2(img_w * scale, img_h * scale);
                                let sized = egui::load::SizedTexture::from_handle(&session.texture);
                                let response = ui.add(
                                    egui::Image::from_texture(sized)
                                        .fit_to_exact_size(draw_size)
                                        .sense(egui::Sense::click()),
                                );
                                let rect = response.rect;
                                let painter = ui.painter_at(rect);
                                for (i, c) in session.clicks.iter().enumerate() {
                                    if let Some(click) = c {
                                        let pos = egui::pos2(
                                            rect.left() + click.x as f32 * scale,
                                            rect.top() + click.y as f32 * scale,
                                        );
                                        let color =
                                            if i == session.current { SHINY } else { ACCENT };
                                        painter.circle_filled(
                                            pos,
                                            13.0,
                                            color.linear_multiply(0.85),
                                        );
                                        painter.circle_stroke(
                                            pos,
                                            13.0,
                                            egui::Stroke::new(2.0, egui::Color32::WHITE),
                                        );
                                        painter.text(
                                            pos,
                                            egui::Align2::CENTER_CENTER,
                                            format!("{}", i + 1),
                                            egui::FontId::proportional(13.0),
                                            egui::Color32::BLACK,
                                        );
                                    }
                                }
                                if response.hovered() {
                                    if let Some(p) = response.hover_pos() {
                                        painter.line_segment(
                                            [
                                                egui::pos2(rect.left(), p.y),
                                                egui::pos2(rect.right(), p.y),
                                            ],
                                            egui::Stroke::new(1.0, ACCENT.linear_multiply(0.6)),
                                        );
                                        painter.line_segment(
                                            [
                                                egui::pos2(p.x, rect.top()),
                                                egui::pos2(p.x, rect.bottom()),
                                            ],
                                            egui::Stroke::new(1.0, ACCENT.linear_multiply(0.6)),
                                        );
                                    }
                                }
                                if response.clicked() {
                                    if let Some(p) = response.interact_pointer_pos() {
                                        let local = p - rect.left_top();
                                        let sx = (local.x / scale).round() as i32;
                                        let sy = (local.y / scale).round() as i32;
                                        let color = sample_color(&session.image, sx, sy)
                                            .unwrap_or_default();
                                        let idx = session.current.min(session.clicks.len() - 1);
                                        session.clicks[idx] = Some(PickClick {
                                            x: sx,
                                            y: sy,
                                            color,
                                        });
                                        let n = session.clicks.len();
                                        let next = (idx + 1..n)
                                            .find(|i| session.clicks[*i].is_none())
                                            .unwrap_or((idx + 1).min(n - 1));
                                        session.current = next;
                                    }
                                }
                            });
                    },
                );

                ui.allocate_ui_with_layout(
                    egui::vec2(side_w, total.y),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        egui::Frame::none()
                            .fill(SURFACE)
                            .stroke(egui::Stroke::new(1.0, BORDER))
                            .rounding(egui::Rounding::same(12.0))
                            .inner_margin(egui::Margin::same(12.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(self.s().slots)
                                        .size(14.0)
                                        .strong()
                                        .color(TEXT),
                                );
                                ui.add_space(6.0);
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    for i in 0..session.clicks.len() {
                                        let is_current = i == session.current;
                                        let bg = if is_current {
                                            ACCENT.linear_multiply(0.18)
                                        } else {
                                            SURFACE_2
                                        };
                                        let resp = egui::Frame::none()
                                            .fill(bg)
                                            .stroke(egui::Stroke::new(
                                                1.0,
                                                if is_current { ACCENT } else { BORDER },
                                            ))
                                            .rounding(egui::Rounding::same(8.0))
                                            .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!("#{}", i + 1))
                                                            .strong()
                                                            .color(if is_current {
                                                                SHINY
                                                            } else {
                                                                TEXT
                                                            }),
                                                    );
                                                    if let Some(c) = &session.clicks[i] {
                                                        color_swatch(ui, c.color, 18.0);
                                                        ui.label(
                                                            egui::RichText::new(format!(
                                                                "{},{}",
                                                                c.x, c.y
                                                            ))
                                                            .monospace()
                                                            .color(TEXT_DIM)
                                                            .small(),
                                                        );
                                                    } else {
                                                        ui.label(
                                                            egui::RichText::new(self.s().empty)
                                                                .color(TEXT_DIM)
                                                                .italics()
                                                                .small(),
                                                        );
                                                    }
                                                });
                                            })
                                            .response;
                                        let resp = resp.interact(egui::Sense::click());
                                        if resp.clicked() {
                                            session.current = i;
                                        }
                                        ui.add_space(4.0);
                                    }
                                });
                            });
                    },
                );
            });
        });
        if commit {
            self.commit_pick(session);
            return None;
        }
        if keep {
            Some(session)
        } else {
            self.status = self.s().pick_cancelled.into();
            None
        }
    }
}

fn color_swatch(ui: &mut egui::Ui, c: Color, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 5.0, egui::Color32::from_rgb(c.r, c.g, c.b));
    ui.painter()
        .rect_stroke(rect, 5.0, egui::Stroke::new(1.0, BORDER));
}

fn short_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn same_source(a: &CaptureSource, b: &CaptureSource) -> bool {
    match (a, b) {
        (CaptureSource::Monitor { index: ai }, CaptureSource::Monitor { index: bi }) => ai == bi,
        (CaptureSource::Window { id: ai, .. }, CaptureSource::Window { id: bi, .. }) => ai == bi,
        _ => false,
    }
}

fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_local_now(lang: Lang) -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    format_datetime(now, lang)
}

fn format_datetime(dt: OffsetDateTime, lang: Lang) -> String {
    let y = dt.year();
    let m = u8::from(dt.month());
    let d = dt.day();
    let hh = dt.hour();
    let mm = dt.minute();
    let ss = dt.second();
    match lang {
        Lang::Fr => format!("{d:02}/{m:02}/{y} {hh:02}:{mm:02}:{ss:02}"),
        Lang::En => {
            let ampm = if hh < 12 { "AM" } else { "PM" };
            let h12 = match hh % 12 {
                0 => 12,
                h => h,
            };
            format!("{}/{}/{y} {:02}:{:02}:{:02} {}", m, d, h12, mm, ss, ampm)
        }
    }
}

fn datetime_from_epoch(epoch_secs: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(epoch_secs).unwrap_or_else(|_| OffsetDateTime::now_utc())
}

/// Write `bytes` to `path` atomically: stream to a sibling `.tmp` file, then
/// rename. Other processes reading the file (e.g. OBS Text Source) only ever
/// see the previous or the new content, never a half-written buffer.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("shiny-counter.txt");
    let tmp = parent.join(format!(".{file_name}.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
    }
    // Best effort: on Windows, rename replaces an existing file iff
    // MoveFileEx with REPLACE_EXISTING — which is what std::fs::rename uses.
    std::fs::rename(&tmp, path)
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn format_delta(seconds: i64) -> String {
    if seconds < 0 {
        return "0s".into();
    }
    let s = seconds as u64;
    if s < 60 {
        return format!("{s}s");
    }
    let m = s / 60;
    let rem = s % 60;
    if m < 60 {
        return format!("{m}m {rem:02}s");
    }
    let h = m / 60;
    let mm = m % 60;
    format!("{h}h {mm:02}m {rem:02}s")
}
