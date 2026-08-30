//! Центральная панель: свойства выбранной команды.

use egui::{Button, ComboBox, RichText, TextEdit, Ui};

use super::{history, mono};
use crate::app::App;
use crate::i18n::{fill1, fill2};
use crate::model::{ParamType, Parameter, Selection};

pub fn central_panel(app: &mut App, ui: &mut Ui) {
    egui::CentralPanel::default().show(ui, |ui| match app.selection.clone() {
        Some(Selection::Command(id)) => command_view(app, ui, &id),
        Some(Selection::Folder(id)) => folder_view(app, ui, &id),
        None => {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(app.s().pick_command).weak());
            });
        }
    });
}

fn folder_view(app: &mut App, ui: &mut Ui, id: &str) {
    let s = app.s();
    if app.folder(id).is_none() {
        return;
    }
    let (folders, commands) = app.subtree(id);
    ui.add_space(2.0);
    ui.label(RichText::new(app.path_of(&Selection::Folder(id.to_owned()))).strong());
    ui.separator();
    ui.label(fill2(s.folder_stats, folders.len() - 1, commands.len()));
}

fn command_view(app: &mut App, ui: &mut Ui, id: &str) {
    let s = app.s();
    if app.command(id).is_none() {
        return;
    }
    if app.editor.command_id.as_deref() != Some(id) {
        app.select(Selection::Command(id.to_owned()));
    }

    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(app.path_of(&Selection::Command(id.to_owned()))).strong());
        if app.editor_dirty() {
            ui.label(RichText::new(s.unsaved_mark).color(ui.visuals().warn_fg_color));
        }
    });
    action_bar(app, ui, id);
    if let Some(run) = app.current_run() {
        ui.add_space(4.0);
        super::run_progress(ui, app.lang, run);
    }
    ui.separator();

    // Нижняя половина — история, верхняя делится на свойства и параметры.
    let half_height = (ui.available_height() * 0.5).max(150.0);
    egui::Panel::bottom("command_history")
        .resizable(true)
        .default_size(half_height)
        .size_range(120.0..=1400.0)
        .show(ui, |ui| history::history_tabs(app, ui));

    egui::CentralPanel::default().show(ui, |ui| {
        let half_width = (ui.available_width() * 0.5).max(260.0);
        egui::Panel::left("command_props")
            .resizable(true)
            .default_size(half_width)
            .size_range(260.0..=1600.0)
            .show(ui, |ui| properties(app, ui));
        egui::CentralPanel::default().show(ui, |ui| params_editor(app, ui));
    });
}

fn action_bar(app: &mut App, ui: &mut Ui, id: &str) {
    let s = app.s();
    ui.horizontal(|ui| {
        let running = app.is_running(id);
        if ui
            .add_enabled(!running, Button::new(s.btn_run))
            .on_hover_text(s.btn_run_tip)
            .clicked()
        {
            app.request_run();
        }
        if running && ui.button(s.btn_stop).clicked() {
            app.cancel_run(id);
        }

        ui.separator();
        let dirty = app.editor_dirty();
        if ui.add_enabled(dirty, Button::new(s.btn_save)).clicked() {
            app.save_editor();
        }
        if ui.add_enabled(dirty, Button::new(s.btn_revert)).clicked() {
            app.revert_editor();
        }
        if ui.button(s.act_export).clicked() {
            app.export_command();
        }
    });
}

fn properties(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    egui::Grid::new("command_fields")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            ui.label(s.field_name);
            ui.add(TextEdit::singleline(&mut app.editor.name).desired_width(f32::INFINITY));
            ui.end_row();

            ui.label(s.field_comment);
            ui.add(
                TextEdit::multiline(&mut app.editor.comment)
                    .desired_rows(2)
                    .hint_text(s.comment_hint)
                    .desired_width(f32::INFINITY),
            );
            ui.end_row();
        });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(s.script).strong());
        if let Some(path) = app.linked_path() {
            ui.label(RichText::new(fill1(s.linked_to, &path)).weak().small())
                .on_hover_text(path);
            if ui
                .small_button(s.btn_reload_file)
                .on_hover_text(s.btn_reload_file_tip)
                .clicked()
            {
                app.reload_linked_script();
            }
            if ui
                .small_button(s.btn_unlink)
                .on_hover_text(s.btn_unlink_tip)
                .clicked()
            {
                app.unlink_script();
            }
        }
    });

    // Скролл у самого редактора, иначе длинный скрипт растягивает всю панель.
    egui::ScrollArea::vertical()
        .id_salt("script_editor")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                TextEdit::multiline(&mut app.editor.script)
                    .code_editor()
                    .desired_rows(10)
                    .desired_width(f32::INFINITY),
            );
            ui.label(RichText::new(s.script_hint).weak().small());
        });
}

fn params_editor(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    ui.horizontal(|ui| {
        ui.label(RichText::new(s.params).strong());
        ui.label(RichText::new(format!("({})", app.editor.params.len())).weak());
        if ui.small_button(s.params_add).clicked() {
            let name = format!("PARAM{}", app.editor.params.len() + 1);
            app.editor.params.push(Parameter::new(name));
        }
    });
    ui.add_space(4.0);

    if app.editor.params.is_empty() {
        ui.label(RichText::new(s.params_empty).weak());
        return;
    }

    let mut remove: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;
    let count = app.editor.params.len();

    egui::ScrollArea::vertical()
        .id_salt("params_list")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for index in 0..count {
                let id = egui::Id::new(("param", index));
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(mono(format!("{}.", index + 1)).weak());
                        ui.add(
                            TextEdit::singleline(&mut app.editor.params[index].name)
                                .desired_width(140.0)
                                .hint_text(s.param_name_hint),
                        );
                        type_selector(ui, &mut app.editor.params[index], id);

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(Button::new(RichText::new("×").strong()))
                                .on_hover_text(s.param_remove_tip)
                                .clicked()
                            {
                                remove = Some(index);
                            }
                            if ui.add_enabled(index + 1 < count, Button::new("↓")).clicked() {
                                swap = Some((index, index + 1));
                            }
                            if ui.add_enabled(index > 0, Button::new("↑")).clicked() {
                                swap = Some((index, index - 1));
                            }
                        });
                    });

                    ui.horizontal_top(|ui| {
                        ui.label(s.param_default);
                        value_widget(ui, &mut app.editor.params[index], id.with("value"));
                    });

                    if let ParamType::Choice(options) = app.editor.params[index].param_type.clone()
                    {
                        ui.horizontal_top(|ui| {
                            ui.label(s.param_choices);
                            let mut text = options.join("\n");
                            if ui
                                .add(
                                    TextEdit::multiline(&mut text)
                                        .desired_rows(3)
                                        .hint_text(s.param_choices_hint)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                app.editor.params[index].param_type =
                                    ParamType::Choice(text.split('\n').map(str::to_owned).collect());
                            }
                        });
                    }
                });
            }
        });

    if let Some(index) = remove {
        app.editor.params.remove(index);
    }
    if let Some((a, b)) = swap {
        app.editor.params.swap(a, b);
    }
}

fn type_selector(ui: &mut Ui, param: &mut Parameter, id: egui::Id) {
    let mut kind = param.param_type.kind_index();
    let before = kind;
    ComboBox::from_id_salt(id.with("type"))
        .width(105.0)
        .selected_text(ParamType::KINDS[kind])
        .show_ui(ui, |ui| {
            for (index, label) in ParamType::KINDS.iter().enumerate() {
                ui.selectable_value(&mut kind, index, *label);
            }
        });
    if kind != before {
        let choices = match &param.param_type {
            ParamType::Choice(options) => options.clone(),
            _ => vec!["A".to_owned(), "B".to_owned()],
        };
        param.param_type = ParamType::from_kind_index(kind, choices);
    }
}

/// Поле ввода значения, соответствующее типу параметра.
pub fn value_widget(ui: &mut Ui, param: &mut Parameter, id: egui::Id) {
    match param.param_type.clone() {
        ParamType::String => {
            ui.add(TextEdit::singleline(&mut param.value).desired_width(f32::INFINITY));
        }
        ParamType::Text => {
            ui.add(
                TextEdit::multiline(&mut param.value)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
        }
        ParamType::Boolean => {
            let mut checked = param.value == "true";
            if ui.checkbox(&mut checked, "true").changed() {
                param.value = checked.to_string();
            }
        }
        ParamType::Password => {
            ui.add(
                TextEdit::singleline(&mut param.value)
                    .password(true)
                    .desired_width(f32::INFINITY),
            );
        }
        ParamType::Choice(options) => {
            let options: Vec<String> = options
                .into_iter()
                .filter(|option| !option.trim().is_empty())
                .collect();
            ComboBox::from_id_salt(id)
                .width(220.0)
                .selected_text(if param.value.is_empty() {
                    "—".to_owned()
                } else {
                    param.value.clone()
                })
                .show_ui(ui, |ui| {
                    for option in &options {
                        ui.selectable_value(&mut param.value, option.clone(), option);
                    }
                });
        }
    }
}
