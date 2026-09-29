use std::collections::HashSet;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::app::{BottomTab, Editor};
use crate::model::Command;
use crate::storage::Storage;

/// Три команды в корне: «Первая», «Вторая», «Третья».
fn app_with_three_commands() -> App {
    let root = std::env::temp_dir().join(format!("cmdrerun-tree-test-{}", crate::model::new_id()));
    let storage = Storage::with_root(root);
    storage.init().expect("init");

    let commands = ["Первая", "Вторая", "Третья"]
        .into_iter()
        .map(|name| {
            let command = Command::new(name, None);
            storage.save_command(&command).expect("save");
            command
        })
        .collect();

    App {
        storage,
        folders: Vec::new(),
        commands,
        selection: None,
        expanded: HashSet::new(),
        editor: Editor::default(),
        logs: Vec::new(),
        changes: Vec::new(),
        runs: Vec::new(),
        bottom_tab: BottomTab::Runs,
        selected_log: None,
        selected_change: None,
        dialog: None,
        status: None,
        lang: crate::i18n::Lang::Ru,
        drag: None,
        output_window: None,
    }
}

fn harness() -> Harness<'static, App> {
    let mut harness = Harness::new_ui_state(
        |ui, app: &mut App| side_panel(app, ui),
        app_with_three_commands(),
    );
    harness.run();
    harness
}

#[test]
fn a_short_click_only_opens_the_node() {
    let mut harness = harness();

    harness.get_by_label_contains("Третья").click();
    harness.run();

    let app = harness.state();
    assert_eq!(
        app.display_name(app.selection.as_ref().expect("выбрано")),
        "Третья"
    );
    assert!(
        app.dialog.is_none(),
        "простой клик не должен ничего переносить"
    );

    std::fs::remove_dir_all(harness.state().storage.root()).ok();
}

#[test]
fn dragging_a_node_asks_where_to_put_it() {
    let mut harness = harness();
    let from = harness.get_by_label_contains("Третья").rect().center();
    // Верхняя четверть первой строки — «встать перед ней».
    let first = harness.get_by_label_contains("Вторая").rect();
    let to = egui::pos2(first.center().x, first.top() + 1.0);

    harness.drag_at(from);
    harness.run();
    harness.hover_at(to);
    harness.run();
    assert!(
        harness.state().drag.is_some(),
        "перенос должен был начаться"
    );
    harness.drop_at(to);
    harness.run();

    // Дерево пока не тронуто — сначала спрашиваем.
    match &harness.state().dialog {
        Some(Dialog::Move {
            target,
            parent_id,
            index,
        }) => {
            assert_eq!(harness.state().display_name(target), "Третья");
            assert_eq!(*parent_id, None);
            assert_eq!(*index, 0, "бросили перед первой строкой — значит, номер 1");
        }
        _ => panic!("ожидалось подтверждение переноса"),
    }
    assert!(
        harness.state().drag.is_none(),
        "перенос должен был завершиться"
    );

    std::fs::remove_dir_all(harness.state().storage.root()).ok();
}
