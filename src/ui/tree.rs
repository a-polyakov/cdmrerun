//! Левая панель: дерево групп и команд.

use egui::containers::menu::MenuButton;
use egui::{Button, RichText, Sense, Shape, Stroke, Ui, vec2};

use crate::app::{App, Dialog, TreeRow};
use crate::model::Selection;

pub fn side_panel(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    egui::Panel::left("tree_panel")
        .resizable(true)
        .default_size(280.0)
        .size_range(200.0..=520.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(s.tree_title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    add_menu(app, ui);
                });
            });
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let rows = app.tree_rows();
                    if rows.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(s.tree_empty).weak());
                        return;
                    }
                    for row in rows {
                        tree_row(app, ui, &row);
                    }
                });
        });
}

/// Единственная кнопка панели: что именно создавать — выбирается в меню.
fn add_menu(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    let parent = app.target_parent();
    MenuButton::new(s.tree_add).ui(ui, |ui| {
        if ui.button(s.tree_group).clicked() {
            app.dialog = Some(Dialog::NewFolder {
                parent_id: parent.clone(),
                name: String::new(),
            });
            ui.close();
        }
        if ui.button(s.tree_command).clicked() {
            app.dialog = Some(Dialog::NewCommand {
                parent_id: parent.clone(),
                name: String::new(),
            });
            ui.close();
        }
        ui.separator();
        if ui.button(s.act_import).clicked() {
            app.open_import_dialog();
            ui.close();
        }
    });
}

fn tree_row(app: &mut App, ui: &mut Ui, row: &TreeRow) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add_space(row.depth as f32 * 14.0);

        let id = row.selection.id().to_owned();
        if row.is_folder {
            let open = app.expanded.contains(&id);
            if arrow(ui, open, row.has_children).clicked() {
                app.toggle_expanded(&id);
            }
        } else {
            ui.add_space(14.0);
        }

        let selected = app.selection.as_ref() == Some(&row.selection);
        let running = !row.is_folder && app.is_running(&id);
        let label = if row.is_folder {
            RichText::new(&row.name).strong()
        } else if running {
            RichText::new(format!("● $ {}", row.name)).color(ui.visuals().warn_fg_color)
        } else {
            RichText::new(format!("$ {}", row.name))
        };
        let mut response = ui.selectable_label(selected, label);
        if running {
            response = response.on_hover_text(app.s().run_running_title);
        }

        if response.clicked() {
            app.request_select(row.selection.clone());
        }
        if response.double_clicked() && row.is_folder {
            app.toggle_expanded(&id);
        }
        response.context_menu(|ui| context_menu(app, ui, row));
    });
}

fn context_menu(app: &mut App, ui: &mut Ui, row: &TreeRow) {
    let s = app.s();
    let target = row.selection.clone();
    if row.is_folder {
        if ui.button(s.ctx_group_inside).clicked() {
            app.dialog = Some(Dialog::NewFolder {
                parent_id: Some(target.id().to_owned()),
                name: String::new(),
            });
            ui.close();
        }
        if ui.button(s.ctx_command_inside).clicked() {
            app.dialog = Some(Dialog::NewCommand {
                parent_id: Some(target.id().to_owned()),
                name: String::new(),
            });
            ui.close();
        }
        ui.separator();
    } else {
        if ui.button(s.act_run).clicked() {
            app.request_select(target.clone());
            app.request_run();
            ui.close();
        }
        if ui.button(s.act_export).clicked() {
            app.request_select(target.clone());
            app.export_command();
            ui.close();
        }
        ui.separator();
    }

    if ui.button(s.act_rename).clicked() {
        let name = app.display_name(&target);
        app.dialog = Some(Dialog::Rename {
            target: target.clone(),
            name,
        });
        ui.close();
    }
    if ui.button(s.act_move).clicked() {
        let parent_id = match &target {
            Selection::Folder(id) => app.folder(id).and_then(|f| f.parent_id.clone()),
            Selection::Command(id) => app.command(id).and_then(|c| c.parent_id.clone()),
        };
        app.dialog = Some(Dialog::Move {
            target: target.clone(),
            parent_id,
        });
        ui.close();
    }
    let delete = Button::new(RichText::new(s.act_delete).color(ui.visuals().error_fg_color));
    if ui.add(delete).clicked() {
        let summary = app.delete_summary(&target);
        app.dialog = Some(Dialog::Delete { target, summary });
        ui.close();
    }
}

/// Треугольник раскрытия узла (рисуем сами, чтобы не зависеть от шрифта).
fn arrow(ui: &mut Ui, open: bool, visible: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::click());
    if visible && ui.is_rect_visible(rect) {
        let color = ui.style().interact(&response).fg_stroke.color;
        let rect = rect.shrink(4.0);
        let points = if open {
            vec![rect.left_top(), rect.right_top(), rect.center_bottom()]
        } else {
            vec![rect.left_top(), rect.right_center(), rect.left_bottom()]
        };
        ui.painter()
            .add(Shape::convex_polygon(points, color, Stroke::NONE));
    }
    response
}
