//! Отрисовка интерфейса.

pub mod details;
pub mod dialogs;
pub mod history;
pub mod menu;
pub mod tree;

use chrono::{DateTime, Local};
use egui::{Color32, RichText, TextWrapMode, Ui};

use crate::app::App;
use crate::diff::{DiffLine, LineKind, line_colors};
use crate::exec::ActiveRun;
use crate::i18n::{Lang, Strings, fill1};

pub fn fmt_time(lang: Lang, time: &DateTime<Local>) -> String {
    time.format(lang.strings().date_format).to_string()
}

pub fn fmt_duration(lang: Lang, secs: f64) -> String {
    let s = lang.strings();
    if secs < 60.0 {
        format!("{secs:.1} {}", s.unit_sec)
    } else {
        let minutes = (secs / 60.0).floor() as i64;
        format!(
            "{minutes} {} {:04.1} {}",
            s.unit_min,
            secs - minutes as f64 * 60.0,
            s.unit_sec
        )
    }
}

pub fn mono(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).monospace()
}

/// Нижняя строка состояния: последнее сообщение и путь к хранилищу.
pub fn status_bar(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            match &app.status {
                Some(status) if status.is_error => {
                    ui.colored_label(ui.visuals().error_fg_color, format!("! {}", status.text));
                }
                Some(status) => {
                    ui.label(&status.text);
                }
                None => {
                    ui.label(s.st_ready);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let path = app.storage.root().display().to_string();
                ui.label(RichText::new(path).weak().small())
                    .on_hover_text(s.storage_dir_tip);
            });
        });
    });
}

/// Полоса прогресса запуска: доля считается от медианы прошлых длительностей.
pub fn run_progress(ui: &mut Ui, lang: Lang, run: &ActiveRun) {
    let s = lang.strings();
    let elapsed = fill1(s.progress_elapsed, fmt_duration(lang, run.elapsed_secs()));
    let bar = match (run.progress(), run.estimate_secs) {
        (Some(progress), Some(estimate)) if run.elapsed_secs() <= estimate => {
            egui::ProgressBar::new(progress).text(format!(
                "{:.0}% · {elapsed} · {}",
                progress * 100.0,
                fill1(s.progress_expected, fmt_duration(lang, estimate))
            ))
        }
        // Скрипт идёт дольше, чем подсказывает история — не притворяемся, что знаем остаток.
        (_, Some(estimate)) => egui::ProgressBar::new(0.99).text(format!(
            "{} · {elapsed} · {}",
            s.progress_longer,
            fill1(s.progress_usually, fmt_duration(lang, estimate))
        )),
        _ => egui::ProgressBar::new(0.0).text(format!(
            "{} · {elapsed} · {}",
            s.progress_running, s.progress_no_estimate
        )),
    };
    ui.add(bar.animate(true));
}

/// Подпись к цветам diff'а.
pub fn diff_legend(ui: &mut Ui, s: &Strings) {
    let dark = ui.visuals().dark_mode;
    ui.horizontal(|ui| {
        for (kind, label) in [
            (LineKind::Added, s.diff_added),
            (LineKind::Removed, s.diff_removed),
            (LineKind::Replaced, s.diff_replaced),
        ] {
            let (fg, bg) = line_colors(kind, dark);
            ui.label(
                RichText::new(format!(" {label} "))
                    .color(fg)
                    .background_color(bg),
            );
        }
    });
}

/// Отрисовка diff'а: `None` в списке — свёрнутый неизменённый участок.
pub fn diff_view(ui: &mut Ui, lines: &[Option<DiffLine>]) {
    let dark = ui.visuals().dark_mode;
    let width = ui.available_width();
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for line in lines {
            match line {
                None => {
                    ui.label(mono("   ⋯").weak());
                }
                Some(line) => {
                    let (fg, bg) = line_colors(line.kind, dark);
                    let number = |value: Option<usize>| match value {
                        Some(number) => format!("{number:>4}"),
                        None => "    ".to_owned(),
                    };
                    let text = format!(
                        "{} {} {} {}",
                        number(line.old_no),
                        number(line.new_no),
                        line.sign(),
                        line.text
                    );
                    egui::Frame::new().fill(bg).show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(mono(text).color(fg));
                    });
                }
            }
        }
    });
}

/// Моноширинный блок с фоном для вывода скрипта.
pub fn output_block(ui: &mut Ui, text: &str, color: Option<Color32>, id: &str) {
    let fill = if ui.visuals().dark_mode {
        Color32::from_rgb(0x16, 0x18, 0x1C)
    } else {
        Color32::from_rgb(0xF2, 0xF3, 0xF5)
    };
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::same(6))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(220.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if text.is_empty() {
                        ui.label(mono("—").weak());
                        return;
                    }
                    let mut rich = mono(text);
                    if let Some(color) = color {
                        rich = rich.color(color);
                    }
                    ui.add(egui::Label::new(rich).wrap_mode(TextWrapMode::Extend));
                });
        });
}
