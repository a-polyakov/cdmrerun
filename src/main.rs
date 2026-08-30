//! cmdrerun — запуск shell-скриптов с историей версий и запусков.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod diff;
mod exec;
mod i18n;
mod model;
mod storage;
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("cmdrerun"),
        ..Default::default()
    };

    eframe::run_native(
        "cmdrerun",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
