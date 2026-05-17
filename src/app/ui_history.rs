use super::helpers::{datetime_from_epoch, format_datetime, format_delta};
use super::state::{PendingConfirm, ShinyApp};
use crate::theme::{
    card, ghost_button, info_icon, pill, pill_dot, BG, BORDER, GOOD, SHINY, SURFACE_2, TEXT,
    TEXT_DIM,
};
use eframe::egui;
use shiny_counter::i18n::{pluralize, Lang};
use shiny_counter::types::HitRecord;

impl ShinyApp {
    pub(super) fn render_history(&mut self, ui: &mut egui::Ui) {
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

    pub(super) fn render_session_block(
        &mut self,
        ui: &mut egui::Ui,
        idx: usize,
        lang: Lang,
        accent: egui::Color32,
    ) {
        // Pull just the scalars / strings we need - the full session may carry
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
        egui::Frame::NONE
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, stroke_col))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::same(12))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let arrow = if is_expanded { "-" } else { "+" };
                    let arrow_resp = ui.add(
                        egui::Button::new(
                            egui::RichText::new(arrow).color(TEXT).strong().size(14.0),
                        )
                        .fill(BG)
                        .stroke(egui::Stroke::new(1.0, BORDER))
                        .corner_radius(egui::CornerRadius::same(6))
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
}
