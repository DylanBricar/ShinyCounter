use super::helpers::color_swatch;
use super::state::{PendingConfirm, ShinyApp};
use crate::theme::{
    card, colored_button, ghost_button, icon_button, info_icon, pill, pill_dot, BAD, BORDER, GOOD,
    SURFACE_2, TEXT, TEXT_DIM, WARN,
};
use eframe::egui;
use shiny_counter::i18n::parse_hex;
use shiny_counter::types::{Color, PickerPoint, MAX_PICKERS, MIN_PICKERS};
use std::time::{Duration, Instant};

impl ShinyApp {
    pub(super) fn render_counter_card(&mut self, ui: &mut egui::Ui) {
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
                    .add_sized([bw, 36.0], egui::Button::new("-1").corner_radius(10))
                    .clicked()
                {
                    let c = self.active().count.saturating_sub(1);
                    self.active_mut().count = c;
                    self.mark_dirty();
                    self.broadcast_state();
                }
                if ui
                    .add_sized([bw, 36.0], egui::Button::new("+1").corner_radius(10))
                    .clicked()
                {
                    self.active_mut().count = self.active().count.saturating_add(1);
                    self.mark_dirty();
                    self.broadcast_state();
                }
                if ui
                    .add_sized(
                        [bw, 36.0],
                        egui::Button::new(self.s().reset).corner_radius(10),
                    )
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

    pub(super) fn render_pickers_card(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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
            // Source selector - placed right under Pick controls so it stays visible.
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
    pub(super) fn render_picker_row(
        &mut self,
        ui: &mut egui::Ui,
        i: usize,
        live_color: Option<Color>,
    ) {
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

        egui::Frame::NONE
            .fill(SURFACE_2)
            .stroke(egui::Stroke::new(1.0, border_color))
            .corner_radius(egui::CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(12, 6))
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
                            .corner_radius(egui::CornerRadius::same(6))
                            .min_size(egui::vec2(34.0, 30.0)),
                    );
                    let mut commit_color: Option<Color> = None;
                    egui::Popup::from_toggle_button_response(&swatch_resp)
                        .id(popup_id)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
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
                        });
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
                    let resp = icon_button(ui, "x", col).on_hover_text(self.s().remove_slot_tip);
                    if can_remove && resp.clicked() {
                        self.pending_confirm = PendingConfirm::DeletePicker(i);
                    }
                });
            });
        ui.add_space(6.0);
    }
}
