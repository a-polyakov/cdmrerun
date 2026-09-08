//! Отдельное окно с выводом команды.
//!
//! Просят его кнопкой рядом с выводом. Если система (или бэкенд) не умеет
//! несколько окон, egui сам покажет содержимое плавающим окном внутри
//! главного — код от этого не меняется.

use egui::{Context, RichText, Ui, ViewportBuilder, ViewportId};

use super::{ICON_COPY, output_view};
use crate::app::App;
use crate::model;

pub fn show(app: &mut App, ctx: &Context) {
    if app.output_window.is_none() {
        return;
    }
    // Показывать нечего: запись удалили или ушли на другую команду.
    if app.window_output().is_none() {
        app.output_window = None;
        return;
    }

    let s = app.s();
    let title = app
        .output_window
        .as_ref()
        .map(|window| window.title.clone())
        .unwrap_or_default();
    // За живым запуском окно следует само; готовую запись показываем с начала.
    let follow = app
        .output_window
        .as_ref()
        .is_some_and(|window| window.log_id.is_none());
    let mut close = false;
    let mut copied = false;

    {
        let output = app.window_output().unwrap_or_default();
        ctx.show_viewport_immediate(
            ViewportId::from_hash_of("output_window"),
            ViewportBuilder::default()
                .with_title(format!("cmdrerun — {title}"))
                .with_inner_size([900.0, 620.0]),
            |ui: &mut Ui, _class| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&title).strong());
                        if ui
                            .small_button(ICON_COPY)
                            .on_hover_text(s.run_output_copy)
                            .clicked()
                        {
                            ui.ctx().copy_text(model::plain_output(output));
                            copied = true;
                        }
                        ui.label(RichText::new(s.run_output_hint).weak().small());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(s.btn_close).clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.separator();
                    output_view(ui, output, "window_output", None, follow);
                });
                if ui.input(|i| i.viewport().close_requested()) {
                    close = true;
                }
            },
        );
    }

    if copied {
        app.set_status(s.st_copied);
    }
    if close {
        app.output_window = None;
    }
}
