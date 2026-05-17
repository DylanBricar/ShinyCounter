use super::state::ShinyApp;
use crate::theme::{
    card, colored_button, ghost_button, icon_button, info_icon, pill, BAD, GOOD, SHINY, TEXT,
    TEXT_DIM,
};
use eframe::egui;

impl ShinyApp {
    pub(super) fn render_bottom(&mut self, ui: &mut egui::Ui) {
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
                            .margin(egui::Margin::symmetric(10, 10))
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
                            .margin(egui::Margin::symmetric(10, 10))
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
                            .margin(egui::Margin::symmetric(10, 10)),
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
                        .margin(egui::Margin::symmetric(10, 10))
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
                        .margin(egui::Margin::symmetric(10, 10))
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
                                    if icon_button(ui, "x", BAD)
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
                self.broadcast_state();
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
}
