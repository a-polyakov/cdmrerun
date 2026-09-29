//! Модальные окна: создание, переименование, перенос, удаление, запуск, импорт.

use egui::{Button, Key, RichText, TextEdit, Ui};

use super::details::value_widget;
use crate::app::{App, Dialog};
use crate::i18n::{Strings, fill1};
use crate::model::{Parameter, Selection};

#[derive(PartialEq, Eq)]
enum Outcome {
    Stay,
    Confirm,
    Cancel,
    Extra,
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let Some(mut dialog) = app.dialog.take() else {
        return;
    };
    let s = app.s();

    let outcome = match &mut dialog {
        Dialog::NewFolder { name, .. } => {
            name_dialog(ctx, s, s.dlg_new_folder, s.dlg_folder_name, name)
        }
        Dialog::NewCommand { name, .. } => {
            name_dialog(ctx, s, s.dlg_new_command, s.dlg_command_name, name)
        }
        Dialog::Rename { name, .. } => name_dialog(ctx, s, s.dlg_rename, s.dlg_new_name, name),
        Dialog::Move {
            target,
            parent_id,
            index,
        } => move_dialog(app, ctx, target, parent_id.as_deref(), *index),
        Dialog::Delete { target, summary } => delete_dialog(app, ctx, target, summary),
        Dialog::DeleteRun { when, .. } => delete_run_dialog(ctx, s, when),
        Dialog::Run { command_id, params } => run_dialog(app, ctx, command_id, params),
        Dialog::Unsaved { .. } => unsaved_dialog(ctx, s),
         Dialog::Import { path, link, .. } => import_dialog(ctx, s, path, link),
         Dialog::About => about_dialog(ctx, s),
     };

    match (outcome, dialog) {
        (Outcome::Stay, dialog) => app.dialog = Some(dialog),

        (Outcome::Confirm, Dialog::NewFolder { parent_id, name }) if !name.trim().is_empty() => {
            app.create_folder(parent_id, name);
        }
        (Outcome::Confirm, Dialog::NewCommand { parent_id, name }) if !name.trim().is_empty() => {
            app.create_command(parent_id, name);
        }
        (Outcome::Confirm, Dialog::Rename { target, name }) => app.rename(&target, name),
        (
            Outcome::Confirm,
            Dialog::Move {
                target,
                parent_id,
                index,
            },
        ) => app.move_to(&target, parent_id, index),
        (Outcome::Confirm, Dialog::Delete { target, .. }) => app.delete(&target),
        (Outcome::Confirm, Dialog::DeleteRun { log_id, .. }) => app.delete_log(&log_id),
        (Outcome::Confirm, Dialog::Run { command_id, params }) => {
            app.start_run(&command_id, params);
        }
        (
            Outcome::Confirm,
            Dialog::Import {
                path,
                link,
                parent_id,
            },
        ) => app.import_command(path.trim(), link, parent_id),
        (Outcome::Confirm, Dialog::Unsaved { next }) => {
            // «Сохранить и продолжить»
            app.save_editor();
            if let Some(next) = next {
                app.select(next);
            }
        }
        (Outcome::Extra, Dialog::Unsaved { next }) => {
            // «Отбросить изменения»
            app.revert_editor();
            if let Some(next) = next {
                app.select(next);
            }
        }
        _ => {}
    }
}

/// Общая обвязка модального окна: заголовок, содержимое, кнопки.
fn modal(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut Ui) -> Outcome,
) -> Outcome {
    let response = egui::Modal::new(egui::Id::new(id)).show(ctx, |ui| {
        ui.set_width(width);
        ui.heading(title);
        ui.separator();
        let outcome = content(ui);
        ui.add_space(4.0);
        outcome
    });

    if response.should_close() {
        Outcome::Cancel
    } else {
        response.inner
    }
}

/// Кнопки «подтвердить/отмена» в правом нижнем углу.
fn buttons(ui: &mut Ui, s: &Strings, confirm: &str, danger: bool, enabled: bool) -> Outcome {
    let mut outcome = Outcome::Stay;
    ui.separator();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if danger {
                RichText::new(confirm)
                    .color(ui.visuals().error_fg_color)
                    .strong()
            } else {
                RichText::new(confirm).strong()
            };
            if ui.add_enabled(enabled, Button::new(label)).clicked() {
                outcome = Outcome::Confirm;
            }
            if ui.button(s.btn_cancel).clicked() {
                outcome = Outcome::Cancel;
            }
        });
    });
    outcome
}

fn name_dialog(
    ctx: &egui::Context,
    s: &Strings,
    title: &str,
    label: &str,
    name: &mut String,
) -> Outcome {
    modal(ctx, "name_dialog", title, 380.0, |ui| {
        ui.label(label);
        let response = ui.add(TextEdit::singleline(name).desired_width(f32::INFINITY));
        let valid = !name.trim().is_empty();

        // Enter в однострочном поле снимает с него фокус — это и есть «Готово».
        // Проверять надо до request_focus(): тот вернёт фокус в поле в этом же кадре,
        // и lost_focus() снова станет false.
        let submitted = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        if !submitted && ui.memory(|memory| memory.focused().is_none()) {
            response.request_focus();
        }
        if submitted && valid {
            return Outcome::Confirm;
        }
        buttons(ui, s, s.btn_ok, false, valid)
    })
}

/// Место назначения уже выбрано перетаскиванием — здесь только подтверждают его.
fn move_dialog(
    app: &App,
    ctx: &egui::Context,
    target: &Selection,
    parent_id: Option<&str>,
    index: usize,
) -> Outcome {
    let s = app.s();
    let title = fill1(s.dlg_move_title, app.display_name(target));
    modal(ctx, "move_dialog", &title, 380.0, |ui| {
        move_summary(app, ui, target, parent_id, index);
        buttons(ui, s, s.act_move, false, true)
    })
}

/// Что именно изменится: родителя показываем, только если он другой.
fn move_summary(app: &App, ui: &mut Ui, target: &Selection, parent: Option<&str>, index: usize) {
    let s = app.s();
    let old_parent = app.parent_of(target);
    if old_parent.as_deref() != parent {
        ui.label(RichText::new(s.dlg_move_parent).strong());
        ui.label(fill1(
            s.dlg_move_was,
            app.parent_name(old_parent.as_deref()),
        ));
        ui.label(fill1(s.dlg_move_now, app.parent_name(parent)));
        ui.add_space(6.0);
    }
    ui.label(RichText::new(s.dlg_move_number).strong());
    ui.label(fill1(s.dlg_move_was, app.position_of(target)));
    ui.label(fill1(s.dlg_move_now, index + 1));
}

fn delete_dialog(app: &App, ctx: &egui::Context, target: &Selection, summary: &str) -> Outcome {
    let s = app.s();
    let name = app.display_name(target);
    let question = match target {
        Selection::Folder(_) => fill1(s.dlg_delete_folder_q, name),
        Selection::Command(_) => fill1(s.dlg_delete_command_q, name),
    };
    modal(ctx, "delete_dialog", s.dlg_delete_title, 460.0, |ui| {
        ui.label(RichText::new(question).strong());
        ui.add_space(4.0);
        ui.label(summary);
        ui.add_space(4.0);
        ui.label(RichText::new(s.dlg_delete_warn).color(ui.visuals().warn_fg_color));
        buttons(ui, s, s.act_delete, true, true)
    })
}

fn delete_run_dialog(ctx: &egui::Context, s: &Strings, when: &str) -> Outcome {
    let title = s.dlg_delete_run_title;
    modal(ctx, "delete_run_dialog", title, 440.0, |ui| {
        ui.label(RichText::new(fill1(s.dlg_delete_run_q, when)).strong());
        ui.add_space(4.0);
        ui.label(s.dlg_delete_run_info);
        ui.add_space(4.0);
        ui.label(RichText::new(s.dlg_delete_warn).color(ui.visuals().warn_fg_color));
        buttons(ui, s, s.act_delete, true, true)
    })
}

fn run_dialog(
    app: &App,
    ctx: &egui::Context,
    command_id: &str,
    params: &mut [Parameter],
) -> Outcome {
    let s = app.s();
    let name = app
        .command(command_id)
        .map_or_else(String::new, |command| command.name.clone());
    let title = fill1(s.dlg_run_title, name);
    modal(ctx, "run_dialog", &title, 520.0, |ui| {
        ui.label(s.dlg_run_params);
        ui.add_space(4.0);
        egui::ScrollArea::vertical()
            .max_height(360.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (index, param) in params.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&param.name).strong());
                        ui.label(RichText::new(param.param_type.label()).weak().small());
                    });
                    value_widget(ui, param, egui::Id::new(("run_param", index)));
                    ui.add_space(6.0);
                }
            });
        buttons(ui, s, s.btn_run, false, true)
    })
}

fn unsaved_dialog(ctx: &egui::Context, s: &Strings) -> Outcome {
    modal(ctx, "unsaved_dialog", s.dlg_unsaved_title, 440.0, |ui| {
        ui.label(s.dlg_unsaved_text);
        ui.add_space(6.0);
        let mut outcome = Outcome::Stay;
        ui.separator();
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(Button::new(RichText::new(s.dlg_unsaved_save).strong()))
                    .clicked()
                {
                    outcome = Outcome::Confirm;
                }
                if ui.button(s.dlg_unsaved_discard).clicked() {
                    outcome = Outcome::Extra;
                }
                if ui.button(s.dlg_unsaved_stay).clicked() {
                    outcome = Outcome::Cancel;
                }
            });
        });
        outcome
    })
}

fn import_dialog(
    ctx: &egui::Context,
    s: &Strings,
    path: &mut String,
    link: &mut bool,
) -> Outcome {
    modal(ctx, "import_dialog", s.dlg_import_title, 560.0, |ui| {
        ui.horizontal(|ui| {
            ui.label(s.dlg_import_file);
            ui.add(TextEdit::singleline(path).desired_width(340.0));
            if ui.button(s.dlg_import_browse).clicked()
                && let Some(picked) = rfd::FileDialog::new().pick_file()
            {
                *path = picked.display().to_string();
            }
        });

        // Экспорт команды импортируется целиком: связывать с файлом там нечего.
        let is_export = std::path::Path::new(path.trim())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));

        ui.add_space(8.0);
        ui.label(s.dlg_import_mode);
        ui.add_enabled_ui(!is_export, |ui| {
            ui.radio_value(link, false, s.dlg_import_copy);
            ui.radio_value(link, true, s.dlg_import_link);
        });
        if is_export {
            ui.add_space(4.0);
            ui.label(RichText::new(s.dlg_import_json).weak());
        }

        buttons(ui, s, s.btn_import, false, !path.trim().is_empty())
    })
}

fn about_dialog(ctx: &egui::Context, s: &Strings) -> Outcome {
    modal(ctx, "about", s.dlg_about_title, 620.0, |ui| {
        ui.label(RichText::new("cmdrerun").heading());
        ui.label(
            RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                .weak()
                .small(),
        );
        ui.add_space(8.0);
        egui::ScrollArea::vertical()
            .max_height(340.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.label(s.about_text);
            });
        ui.add_space(6.0);
        ui.separator();
        let mut outcome = Outcome::Stay;
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(s.btn_close).clicked() {
                    outcome = Outcome::Cancel;
                }
            });
        });
        outcome
    })
}

#[cfg(test)]
#[path = "dialogs_tests.rs"]
mod tests;
