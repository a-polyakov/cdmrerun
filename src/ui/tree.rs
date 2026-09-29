//! Левая панель: дерево групп и команд.

use egui::containers::menu::MenuButton;
use egui::{
    Button, Color32, CursorIcon, FontId, Id, LayerId, Order, Rect, RichText, Sense, Shape, Stroke,
    StrokeKind, Ui, vec2,
};

use crate::app::{App, Dialog, DropPos, TreeRow};
use crate::model::Selection;

/// Отступ одного уровня вложенности.
const INDENT: f32 = 14.0;

pub fn side_panel(app: &mut App, ui: &mut Ui) {
    let s = app.s();
    egui::Panel::left("tree_panel")
        .resizable(true)
        .default_size(280.0)
        .size_range(200.0..=520.0)
        .show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(s.tree_title).on_hover_text(s.tree_drag_tip);
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
                    let mut drop: Option<(Selection, DropPos)> = None;
                    for row in &rows {
                        tree_row(app, ui, row, &mut drop);
                    }
                    // Пустое место под деревом — перенос в конец корня.
                    if let Some(last_root) = rows.iter().rfind(|row| row.depth == 0) {
                        empty_space(app, ui, last_root, &mut drop);
                    }
                    finish_drag(app, ui, drop);
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

fn tree_row(app: &mut App, ui: &mut Ui, row: &TreeRow, drop: &mut Option<(Selection, DropPos)>) {
    let id = row.selection.id().to_owned();
    let line = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.add_space(row.depth as f32 * INDENT);

        if row.is_folder {
            let open = app.expanded.contains(&id);
            if arrow(ui, open, row.has_children).clicked() {
                app.toggle_expanded(&id);
            }
        } else {
            ui.add_space(INDENT);
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
        // click_and_drag: короткий клик открывает узел, а с зажатой кнопкой
        // начинается перенос — прежний клик от этого не страдает.
        let mut response = ui.add(
            Button::selectable(selected, label)
                .sense(Sense::click_and_drag())
                .truncate(),
        );
        if running {
            response = response.on_hover_text(app.s().run_running_title);
        }
        response
    });

    let response = line.inner;
    // Мишень переноса — вся строка панели, а не только надпись.
    let row_rect = Rect::from_x_y_ranges(ui.max_rect().x_range(), line.response.rect.y_range());

    if response.drag_started() {
        app.drag = Some(row.selection.clone());
    }
    if response.clicked() {
        app.request_select(row.selection.clone());
    }
    if response.double_clicked() && row.is_folder {
        app.toggle_expanded(&id);
    }
    response.context_menu(|ui| context_menu(app, ui, row));

    let Some(dragged) = app.drag.clone() else {
        return;
    };
    if dragged == row.selection {
        drag_ghost(ui, &row.name);
        return;
    }
    let Some(pos) = hovered_part(ui, row_rect, row.is_folder) else {
        return;
    };
    if app.drop_place(&dragged, &row.selection, pos).is_some() {
        paint_drop_hint(ui, row_rect, pos, row.depth);
        *drop = Some((row.selection.clone(), pos));
    }
}

/// Свободное место под деревом: бросок туда отправляет узел в конец корня,
/// то есть сразу за последней строкой верхнего уровня.
fn empty_space(app: &App, ui: &mut Ui, last: &TreeRow, drop: &mut Option<(Selection, DropPos)>) {
    let height = ui.available_height();
    if height <= 0.0 {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let Some(dragged) = app.drag.clone() else {
        return;
    };
    let Some(pointer) = ui.ctx().pointer_interact_pos() else {
        return;
    };
    if !rect.contains(pointer) || !ui.clip_rect().contains(pointer) {
        return;
    }
    if app
        .drop_place(&dragged, &last.selection, DropPos::After)
        .is_some()
    {
        let line = Rect::from_x_y_ranges(rect.x_range(), rect.top()..=rect.top());
        paint_drop_hint(ui, line, DropPos::Before, 0);
        *drop = Some((last.selection.clone(), DropPos::After));
    }
}

/// Завершает перенос: бросок открывает подтверждение, отпускание мимо — отменяет.
fn finish_drag(app: &mut App, ui: &Ui, drop: Option<(Selection, DropPos)>) {
    let Some(dragged) = app.drag.clone() else {
        return;
    };
    let (released, holding) = ui
        .ctx()
        .input(|i| (i.pointer.any_released(), i.pointer.any_down()));
    if !released && holding {
        return;
    }
    app.drag = None;
    if !released {
        return;
    }
    if let Some((target, pos)) = drop
        && let Some((parent, index)) = app.drop_place(&dragged, &target, pos)
    {
        app.request_move(&dragged, parent, index);
    }
}

/// Часть строки под курсором: сверху и снизу — «рядом», середина группы — «внутрь».
fn hovered_part(ui: &Ui, rect: Rect, is_folder: bool) -> Option<DropPos> {
    let pointer = ui.ctx().pointer_interact_pos()?;
    if !rect.contains(pointer) || !ui.clip_rect().contains(pointer) {
        return None;
    }
    let part = (pointer.y - rect.top()) / rect.height().max(1.0);
    Some(if is_folder {
        // У группы середина строки — «внутрь», края — «рядом».
        if part < 0.25 {
            DropPos::Before
        } else if part > 0.75 {
            DropPos::After
        } else {
            DropPos::Inside
        }
    } else if part < 0.5 {
        DropPos::Before
    } else {
        DropPos::After
    })
}

/// Подсказку рисуем поверх дерева: иначе её закроет следующая строка.
fn overlay(ui: &Ui, name: &str, order: Order) -> egui::Painter {
    ui.ctx()
        .layer_painter(LayerId::new(order, Id::new(name)))
        .with_clip_rect(ui.clip_rect())
}

fn paint_drop_hint(ui: &Ui, rect: Rect, pos: DropPos, depth: usize) {
    let color = ui.visuals().selection.bg_fill;
    let painter = overlay(ui, "tree_drop_hint", Order::Foreground);
    let indent = rect.left() + depth as f32 * INDENT + INDENT;
    match pos {
        DropPos::Inside => {
            painter.rect_stroke(
                rect.shrink(1.0),
                4.0,
                Stroke::new(2.0, color),
                StrokeKind::Inside,
            );
        }
        DropPos::Before => {
            painter.hline(indent..=rect.right(), rect.top(), Stroke::new(2.0, color));
        }
        DropPos::After => {
            painter.hline(
                indent..=rect.right(),
                rect.bottom(),
                Stroke::new(2.0, color),
            );
        }
    }
}

/// «Призрак» переносимого узла у курсора.
fn drag_ghost(ui: &Ui, name: &str) {
    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    let Some(pointer) = ui.ctx().pointer_interact_pos() else {
        return;
    };
    let painter = overlay(ui, "tree_drag_ghost", Order::Tooltip).with_clip_rect(Rect::EVERYTHING);
    let visuals = ui.visuals();
    let text = painter.layout_no_wrap(
        name.to_owned(),
        FontId::proportional(13.0),
        visuals.strong_text_color(),
    );
    let at = pointer + vec2(14.0, 6.0);
    painter.rect_filled(
        Rect::from_min_size(at, text.size()).expand(4.0),
        4.0,
        visuals.selection.bg_fill.gamma_multiply(0.85),
    );
    painter.galley(at, text, Color32::PLACEHOLDER);
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
    // Перенос теперь только перетаскиванием — пункт меню под него убран,
    // чтобы не дублировать один и тот же результат двумя разными путями.
    let delete = Button::new(RichText::new(s.act_delete).color(ui.visuals().error_fg_color));
    if ui.add(delete).clicked() {
        let summary = app.delete_summary(&target);
        app.dialog = Some(Dialog::Delete { target, summary });
        ui.close();
    }
}

/// Треугольник раскрытия узла (рисуем сами, чтобы не зависеть от шрифта).
fn arrow(ui: &mut Ui, open: bool, visible: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(INDENT, INDENT), Sense::click());
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

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
