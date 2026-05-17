use super::helpers::{epoch_now, format_size};
use super::state::{PendingConfirm, ShinyApp};
use crate::theme::{
    colored_button, ghost_button, pill, BAD, BORDER, SHINY, SURFACE, TEXT, TEXT_DIM,
};
use eframe::egui;
use shiny_counter::types::MIN_PICKERS;
use shiny_counter::update::{self, UpdateStatus};

impl ShinyApp {
    pub(super) fn render_confirm_modal(&mut self, ctx: &egui::Context) {
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
                let screen_rect = ctx.content_rect();
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
                egui::Frame::NONE
                    .fill(SURFACE)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .corner_radius(egui::CornerRadius::same(14))
                    .inner_margin(egui::Margin::same(20))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 32,
                        spread: 0,
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
                    self.broadcast_state();
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
                        self.broadcast_state();
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
                        // Drop any live samples - indexing may shift.
                        self.last_sample.clear();
                        self.mark_dirty();
                    }
                }
                PendingConfirm::None => {}
            }
            self.pending_confirm = PendingConfirm::None;
        }
    }

    pub(super) fn render_update_modal(&mut self, ctx: &egui::Context) {
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

        // While an auto-download is streaming, stay silent - the user opted
        // in to "do it for me", so a progress modal would be intrusive. The
        // Settings pill keeps showing the percentage. The modal will reopen
        // automatically once the status transitions to `Downloaded`.
        if self.update_auto_initiated && matches!(&status, UpdateStatus::Downloading { .. }) {
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
                let screen_rect = ctx.content_rect();
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
                egui::Frame::NONE
                    .fill(SURFACE)
                    .stroke(egui::Stroke::new(1.0, BORDER))
                    .corner_radius(egui::CornerRadius::same(14))
                    .inner_margin(egui::Margin::same(20))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 8],
                        blur: 32,
                        spread: 0,
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

    pub(super) fn snooze_version(&mut self, version: &str) {
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
}
