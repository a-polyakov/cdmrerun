//! Нижняя часть панели команды: история запусков и история изменений.

use egui::{Button, Color32, RichText, Ui};

use super::{
    diff_legend, diff_view, fmt_duration, fmt_time, mono, output_block, output_header, output_view,
    run_progress,
};
use crate::app::{App, BottomTab, Dialog};
use crate::diff;
use crate::i18n::fill1;
use crate::model::ExecutionLog;

/// Две вкладки истории. Показываются только для команды.
pub fn history_tabs(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.selectable_value(
            &mut app.bottom_tab,
            BottomTab::Runs,
            fill1(s.tab_runs, app.logs.len()),
        );
        ui.selectable_value(
            &mut app.bottom_tab,
            BottomTab::Changes,
            fill1(s.tab_changes, app.changes.len()),
        );
    });
    ui.separator();

    match app.bottom_tab {
        BottomTab::Runs => runs_tab(app, ui),
        BottomTab::Changes => changes_tab(app, ui),
    }
}

// ---------------- история запусков ----------------

fn runs_tab(app: &mut App, ui: &mut Ui) {
    egui::Panel::left("runs_list")
        .resizable(true)
        .default_size(260.0)
        .size_range(160.0..=460.0)
        .show(ui, |ui| runs_list(app, ui));

    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("run_details")
            .auto_shrink([false, false])
            .show(ui, |ui| run_details(app, ui));
    });
}

/// Строка списка запусков, подготовленная к отрисовке.
struct RunRow {
    index: usize,
    log_id: String,
    when: String,
    label: String,
    color: Color32,
}

fn runs_list(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    let lang = app.lang;
    // Читаем один раз в самом начале: `current_run()` держит `&App` целиком,
    // а ниже нужно менять `app.selected_log` — так заимствования не пересекутся.
    let live_start = app.current_run().map(|run| run.start_time);
    egui::ScrollArea::vertical()
        .id_salt("runs_list_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if let Some(start_time) = live_start {
                let text = RichText::new(format!(
                    "● {} · {}",
                    s.run_running_row,
                    fmt_time(lang, &start_time)
                ))
                .color(ui.visuals().warn_fg_color);
                if ui
                    .selectable_label(app.selected_log.is_none(), text)
                    .clicked()
                {
                    app.selected_log = None;
                }
            }

            if app.logs.is_empty() {
                // "Ещё не запускалась" относится к завершённым запускам — если сейчас
                // что-то выполняется, эта надпись лишняя и только путает.
                if live_start.is_none() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(s.runs_empty).weak());
                }
                return;
            }

            // Готовим строки заранее: дальше нужен `&mut app`, а список — только для чтения.
            let rows: Vec<RunRow> = (0..app.logs.len())
                .rev()
                .map(|index| {
                    let log = &app.logs[index];
                    let (mark, color) = status_mark(ui, log);
                    let when = fmt_time(lang, &log.start_time);
                    RunRow {
                        index,
                        log_id: log.id.clone(),
                        label: format!(
                            "{mark} {when} · {}",
                            log.duration_secs()
                                .map_or_else(|| "—".to_owned(), |secs| fmt_duration(lang, secs))
                        ),
                        when,
                        color,
                    }
                })
                .collect();

            let mut rerun: Option<String> = None;
            let mut delete: Option<(String, String)> = None;
            for row in rows {
                let selected = app.selected_log == Some(row.index);
                let response =
                    ui.selectable_label(selected, RichText::new(&row.label).color(row.color));
                if response.clicked() {
                    app.selected_log = Some(row.index);
                }
                response.context_menu(|ui| {
                    if ui.button(s.act_rerun).clicked() {
                        rerun = Some(row.log_id.clone());
                        ui.close();
                    }
                    let remove =
                        Button::new(RichText::new(s.act_delete).color(ui.visuals().error_fg_color));
                    if ui.add(remove).clicked() {
                        delete = Some((row.log_id.clone(), row.when.clone()));
                        ui.close();
                    }
                });
            }
            if let Some(log_id) = rerun {
                app.rerun_log(&log_id);
            }
            if let Some((log_id, when)) = delete {
                app.dialog = Some(Dialog::DeleteRun { log_id, when });
            }
        });
}

fn status_mark(ui: &Ui, log: &ExecutionLog) -> (&'static str, Color32) {
    if log.is_success() {
        ("●", Color32::from_rgb(0x2E, 0xA0, 0x43))
    } else if log.exit_code.is_none() {
        ("■", ui.visuals().warn_fg_color)
    } else {
        ("●", ui.visuals().error_fg_color)
    }
}

fn run_details(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    let lang = app.lang;

    // Живой запуск показываем, пока он не завершился и не попал в историю.
    if app.selected_log.is_none()
        && let Some(run) = app.current_run()
    {
        ui.horizontal(|ui| {
            ui.label(RichText::new(s.run_running_title).strong());
            ui.label(RichText::new(fmt_time(lang, &run.start_time)).weak());
        });
        run_progress(ui, lang, run);
        ui.add_space(6.0);
        let (open, copy) = output_header(ui, s, &run.output);
        output_view(ui, &run.output, "live_output", Some(260.0), true);
        ui.add_space(6.0);
        egui::CollapsingHeader::new(s.run_script)
            .id_salt("live_script_header")
            .show(ui, |ui| output_block(ui, &run.script, None, "live_script"));

        if open {
            app.open_output_window(None);
        }
        if copy {
            app.set_status(s.st_copied);
        }
        return;
    }

    let Some(log) = app.selected_log.and_then(|index| app.logs.get(index)) else {
        ui.label(RichText::new(s.runs_pick).weak());
        return;
    };

    ui.horizontal_wrapped(|ui| {
        let (mark, color) = status_mark(ui, log);
        ui.label(RichText::new(mark).color(color).strong());
        ui.label(fmt_time(lang, &log.start_time));
        ui.label(RichText::new("·").weak());
        ui.label(fill1(
            s.run_duration,
            log.duration_secs()
                .map_or_else(|| "—".to_owned(), |secs| fmt_duration(lang, secs)),
        ));
        ui.label(RichText::new("·").weak());
        match log.exit_code {
            Some(code) => ui.label(fill1(s.run_exit_code, code)),
            None => ui.label(s.run_no_code),
        };
    });

    if !log.params.is_empty() {
        ui.add_space(4.0);
        ui.label(RichText::new(s.run_params).strong());
        for param in &log.params {
            ui.label(mono(format!(
                "{} : {} = {}",
                param.name,
                param.param_type.label(),
                param.display_value()
            )));
        }
    }

    ui.add_space(6.0);
    let output = log.output.as_str();
    let (open, copy) = output_header(ui, s, output);
    output_view(ui, output, "log_output", Some(260.0), false);
    // Кнопки меняют состояние приложения, а вывод в это время одолжен у него же —
    // поэтому запоминаем намерение и применяем его, когда заимствование кончится.
    let open_window = open.then(|| log.id.clone());
    let copied = copy;

    ui.add_space(6.0);
    egui::CollapsingHeader::new(s.log_script)
        .id_salt("log_script_header")
        .show(ui, |ui| output_block(ui, &log.script, None, "log_script"));

    let lines = app.log_vs_current(log);
    egui::CollapsingHeader::new(if diff::has_changes(&lines) {
        s.log_diff_changed
    } else {
        s.log_diff_same
    })
    .id_salt("log_vs_current")
    .default_open(false)
    .show(ui, |ui| {
        if diff::has_changes(&lines) {
            diff_legend(ui, s);
            diff_view(ui, &diff::collapse_context(&lines, 3));
        } else {
            ui.label(RichText::new(s.log_diff_none).weak());
        }
    });

    if let Some(log_id) = open_window {
        app.open_output_window(Some(log_id));
    }
    if copied {
        app.set_status(s.st_copied);
    }
}

// ---------------- история изменений ----------------

fn changes_tab(app: &mut App, ui: &mut Ui) {
    egui::Panel::left("changes_list")
        .resizable(true)
        .default_size(260.0)
        .size_range(160.0..=460.0)
        .show(ui, |ui| changes_list(app, ui));

    let mut restore: Option<usize> = None;
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("change_details")
            .auto_shrink([false, false])
            .show(ui, |ui| change_details(app, ui, &mut restore));
    });
    if let Some(index) = restore {
        app.restore_version(index);
    }
}

fn changes_list(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    let lang = app.lang;
    egui::ScrollArea::vertical()
        .id_salt("changes_list_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.changes.is_empty() {
                ui.label(RichText::new(s.changes_empty).weak());
                return;
            }
            for index in (0..app.changes.len()).rev() {
                let change = &app.changes[index];
                let selected = app.selected_change == Some(index);
                let label = format!("v{} · {}", index + 1, fmt_time(lang, &change.timestamp));
                if ui.selectable_label(selected, label).clicked() {
                    app.selected_change = Some(index);
                }
            }
        });
}

fn change_details(app: &App, ui: &mut Ui, restore: &mut Option<usize>) {
    let s = app.s();
    let Some(index) = app.selected_change else {
        ui.label(RichText::new(s.changes_pick).weak());
        return;
    };
    let Some(change) = app.changes.get(index) else {
        return;
    };
    let (new_script, new_params, new_comment) = app.version_after(index);
    let newest = index + 1 == app.changes.len();

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(fill1(s.version, index + 1)).strong());
        ui.label(RichText::new(fmt_time(app.lang, &change.timestamp)).weak());
        ui.label(
            RichText::new(if newest {
                s.version_replaced_current
            } else {
                s.version_replaced_next
            })
            .weak(),
        );
        if ui
            .button(s.version_restore)
            .on_hover_text(s.version_restore_tip)
            .clicked()
        {
            *restore = Some(index);
        }
    });
    ui.label(RichText::new(s.diff_explain).weak().small());
    ui.add_space(4.0);
    diff_legend(ui, s);
    ui.add_space(6.0);

    let current_name = app
        .command(&change.command_id)
        .map(|c| c.name.clone())
        .unwrap_or_default();
    let new_name = app
        .changes
        .get(index + 1)
        .map(|next| next.old_name.clone())
        .unwrap_or(current_name);
    if !change.old_name.is_empty() && change.old_name != new_name {
        ui.label(RichText::new(s.field_name).strong());
        diff_view(
            ui,
            &diff::script_diff(&change.old_name, &new_name)
                .into_iter()
                .map(Some)
                .collect::<Vec<_>>(),
        );
        ui.add_space(6.0);
    }

    let script_lines = diff::script_diff(&change.old_script, &new_script);
    ui.label(RichText::new(s.script).strong());
    if diff::has_changes(&script_lines) {
        diff_view(ui, &diff::collapse_context(&script_lines, 3));
    } else {
        ui.label(RichText::new(s.no_changes).weak());
    }

    ui.add_space(8.0);
    let param_lines = diff::params_diff(&change.old_params, &new_params);
    ui.label(RichText::new(s.params).strong());
    if diff::has_changes(&param_lines) {
        diff_view(ui, &param_lines.into_iter().map(Some).collect::<Vec<_>>());
    } else {
        ui.label(RichText::new(s.no_changes).weak());
    }

    let comment_lines = diff::script_diff(&change.old_comment, &new_comment);
    if diff::has_changes(&comment_lines) {
        ui.add_space(8.0);
        ui.label(RichText::new(s.field_comment).strong());
        diff_view(ui, &comment_lines.into_iter().map(Some).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::app::{Dialog, Editor};
    use crate::i18n::RU;
    use crate::exec::ActiveRun;
    use crate::model::Command;
    use crate::storage::Storage;

    /// Приложение с реально выполняющимся запуском и без единого завершённого —
    /// именно так выглядит самый первый запуск команды.
    fn app_with_live_run_and_no_history() -> App {
        let root = std::env::temp_dir().join(format!("cmdrerun-history-test-{}", crate::model::new_id()));
        let storage = Storage::with_root(root);
        storage.init().expect("init");

        let mut command = Command::new("Тест", None);
        command.script = "sleep 5\n".to_owned();
        let run = ActiveRun::start(&command, Vec::new(), None);

        App {
            editor: Editor {
                command_id: Some(command.id.clone()),
                name: command.name.clone(),
                script: command.script.clone(),
                comment: String::new(),
                params: Vec::new(),
            },
            storage,
            folders: Vec::new(),
            commands: vec![command],
            selection: None,
            expanded: HashSet::new(),
            logs: Vec::new(),
            changes: Vec::new(),
            runs: vec![run],
            bottom_tab: BottomTab::Runs,
            selected_log: None,
            selected_change: None,
            dialog: None::<Dialog>,
            status: None,
            lang: crate::i18n::Lang::Ru,
            drag: None,
            output_window: None,
        }
    }

    #[test]
    fn live_run_hides_the_never_ran_placeholder() {
        let app = app_with_live_run_and_no_history();
        let root = app.storage.root().to_path_buf();

        let mut harness = Harness::new_ui_state(
            |ui, app: &mut App| runs_list(app, ui),
            app,
        );
        harness.run();

        // "Команда ещё не запускалась" не должно быть видно рядом с работающим запуском.
        assert!(
            harness.query_by_label_contains(RU.runs_empty).is_none(),
            "плейсхолдер пустой истории показан поверх живого запуска"
        );
        // А сам живой запуск в списке есть.
        assert!(harness.query_by_label_contains(RU.run_running_row).is_some());

        if let Some(run) = harness.state_mut().runs.first_mut() {
            run.cancel();
        }
        std::fs::remove_dir_all(root).ok();
    }
}
