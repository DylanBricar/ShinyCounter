#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod app;
mod icon;
mod theme;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let icon_size = icon::size();
    let icon_data = egui::IconData {
        rgba: icon::rgba(),
        width: icon_size,
        height: icon_size,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 820.0])
            .with_min_inner_size([900.0, 580.0])
            .with_title("Shiny Counter")
            .with_icon(icon_data),
        ..Default::default()
    };

    eframe::run_native(
        "Shiny Counter",
        options,
        Box::new(|cc| Ok(Box::new(app::ShinyApp::new(cc)))),
    )
}
