use super::helpers::color_swatch;
use super::state::{PickClick, PickSession, ShinyApp};
use crate::theme::{
    card, colored_button, ghost_button, pill, ACCENT, BORDER, SHINY, SURFACE, SURFACE_2, TEXT,
};
use eframe::egui;
use shiny_counter::capture::sample_color;
use shiny_counter::types::MAX_PICKERS;
use shiny_counter::types::MIN_PICKERS;

impl ShinyApp {
    pub(super) fn render_picking(
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
                                                            .color(crate::theme::TEXT_DIM)
                                                            .small(),
                                                        );
                                                    } else {
                                                        ui.label(
                                                            egui::RichText::new(self.s().empty)
                                                                .color(crate::theme::TEXT_DIM)
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
