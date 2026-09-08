//! Отрисовка интерфейса.

pub mod details;
pub mod dialogs;
pub mod history;
pub mod menu;
pub mod output_window;
pub mod tree;

use chrono::{DateTime, Local};
use egui::{Color32, RichText, TextWrapMode, Ui};

use crate::app::App;
use crate::diff::{DiffLine, LineKind, line_colors};
use crate::exec::ActiveRun;
use crate::i18n::{Lang, Strings, fill1};
use crate::model;

/// Иконка «открыть в отдельном окне»: рамка с уголками.
pub const ICON_WINDOW: &str = "⛶";
/// Иконка «скопировать в буфер обмена»: два листа один за другим.
/// Обе иконки выбраны из того, что есть в шрифтах egui, — иначе вместо них
/// рисуется пустой квадрат.
pub const ICON_COPY: &str = "🗐";

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

/// Фон блоков вывода — чуть темнее (светлее) обычного, как у терминала.
fn output_fill(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(0x16, 0x18, 0x1C)
    } else {
        Color32::from_rgb(0xF2, 0xF3, 0xF5)
    }
}

/// Объединённый вывод команды: строки идут в порядке появления,
/// пришедшие из stderr подсвечены красным.
///
/// Строки рисуем по одной (а не единым текстом), потому что цвет у них разный;
/// `show_rows` при этом отрисовывает только видимую часть, так что длинный
/// вывод не стоит ничего лишнего.
pub fn output_view(ui: &mut Ui, output: &str, id: &str, max_height: Option<f32>, follow: bool) {
    let lines = model::output_lines(output);
    let error_color = ui.visuals().error_fg_color;
    egui::Frame::new()
        .fill(output_fill(ui))
        .inner_margin(egui::Margin::same(6))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if lines.is_empty() {
                ui.label(mono("—").weak());
                return;
            }
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            // Без потолка по высоте вывод занимает всё, что дают: так он выглядит
            // в отдельном окне. С потолком — тянется по содержимому до него.
            // Строки не переносятся (см. TextWrapMode::Extend ниже), поэтому длинные
            // строки открываются горизонтальной прокруткой, а не обрезаются.
            let mut area = egui::ScrollArea::both()
                .id_salt(id)
                .auto_shrink([false, max_height.is_some()])
                .stick_to_bottom(follow);
            if let Some(height) = max_height {
                area = area.max_height(height);
            }
            area.show_rows(ui, row_height, lines.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (is_error, text) in &lines[range] {
                    let mut rich = mono(*text);
                    if *is_error {
                        rich = rich.color(error_color);
                    }
                    ui.add(egui::Label::new(rich).wrap_mode(TextWrapMode::Extend));
                }
            });
        });
}

/// Заголовок блока вывода: подпись, кнопки «в окно» и «скопировать», подсказка.
///
/// Возвращает, что нажали: открыть отдельное окно и скопировать вывод.
pub fn output_header(ui: &mut Ui, s: &Strings, output: &str) -> (bool, bool) {
    let mut open_window = false;
    let mut copied = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(s.run_output).strong());
        if ui
            .small_button(ICON_WINDOW)
            .on_hover_text(s.run_output_window)
            .clicked()
        {
            open_window = true;
        }
        if ui
            .small_button(ICON_COPY)
            .on_hover_text(s.run_output_copy)
            .clicked()
        {
            ui.ctx().copy_text(model::plain_output(output));
            copied = true;
        }
        ui.label(RichText::new(s.run_output_hint).weak().small());
    });
    (open_window, copied)
}

/// Моноширинный блок с фоном для вывода скрипта.
pub fn output_block(ui: &mut Ui, text: &str, color: Option<Color32>, id: &str) {
    egui::Frame::new()
        .fill(output_fill(ui))
        .inner_margin(egui::Margin::same(6))
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // both(): строки не переносятся (TextWrapMode::Extend ниже), поэтому длинные
            // строки скрипта открываются горизонтальной прокруткой, а не обрезаются.
            egui::ScrollArea::both()
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
