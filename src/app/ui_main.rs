use super::helpers::{same_source, short_str};
use super::state::{PendingConfirm, ShinyApp};
use crate::theme::{
    card, ghost_button, icon_button, info_icon, paint_pokeball, pill, BAD, GOOD, SHINY, TEXT,
    TEXT_DIM,
};
use eframe::egui;
use shiny_counter::capture::list_sources;
use shiny_counter::i18n::Lang;
use shiny_counter::types::{CaptureSource, Color, Preset};
use shiny_counter::update::{self, UpdateStatus};
use std::time::Instant;

impl ShinyApp {
    pub(super) fn render_idle(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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

    pub(super) fn render_header(&mut self, ui: &mut egui::Ui) {
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
                    self.broadcast_state();
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
                    self.broadcast_state();
                }
                if ghost_button(ui, self.s().duplicate).clicked() {
                    let mut copy = self.active().clone();
                    copy.name = format!("{} (copy)", copy.name);
                    copy.count = 0;
                    copy.hits.clear();
                    // Drop the file-output path: two presets writing to the
                    // same file would silently overwrite each other every
                    // time the user switched between them.
                    copy.output_file = None;
                    copy.output_file_enabled = false;
                    self.config.presets.push(copy);
                    self.config.active_preset_index = self.config.presets.len() - 1;
                    self.sync_hex_buf();
                    self.mark_dirty();
                    self.broadcast_state();
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

    pub(super) fn render_source_combo(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn render_status_bar(&self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let dot = if self.running { GOOD } else { TEXT_DIM };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, dot);
            ui.label(egui::RichText::new(&self.status).color(TEXT_DIM).italics());
        });
    }
}
