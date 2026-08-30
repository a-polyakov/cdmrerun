//! Главное меню приложения.

use egui::containers::menu::{MenuBar, MenuButton, SubMenuButton};
use egui::{Button, Ui};

use crate::app::{App, Dialog};
use crate::i18n::Lang;

pub fn menu_bar(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    egui::Panel::top("menu_bar").show(ui, |ui| {
        MenuBar::new().ui(ui, |ui| {
            MenuButton::new(s.menu_file).ui(ui, |ui| {
                if ui.button(s.menu_import).clicked() {
                    app.open_import_dialog();
                    ui.close();
                }
                let has_command = app.editor.command_id.is_some();
                if ui
                    .add_enabled(has_command, Button::new(s.menu_export))
                    .clicked()
                {
                    app.export_command();
                    ui.close();
                }
                ui.separator();
                if ui.button(s.menu_quit).clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            MenuButton::new(s.menu_settings).ui(ui, |ui| {
                SubMenuButton::new(s.menu_language).ui(ui, |ui| {
                    for lang in Lang::ALL {
                        if ui.selectable_label(app.lang == lang, lang.label()).clicked() {
                            app.set_lang(lang);
                            ui.close();
                        }
                    }
                 });
             });

             MenuButton::new(s.menu_help).ui(ui, |ui| {
                if ui.button(s.menu_about).clicked() {
                    app.dialog = Some(Dialog::About);
                    ui.close();
                }
            });
        });
    });
}
