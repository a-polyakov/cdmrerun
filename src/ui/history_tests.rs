use std::collections::HashSet;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::app::{Dialog, Editor};
use crate::exec::ActiveRun;
use crate::i18n::RU;
use crate::model::Command;
use crate::storage::Storage;

/// Приложение с реально выполняющимся запуском и без единого завершённого —
/// именно так выглядит самый первый запуск команды.
fn app_with_live_run_and_no_history() -> App {
    let root =
        std::env::temp_dir().join(format!("cmdrerun-history-test-{}", crate::model::new_id()));
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

    let mut harness = Harness::new_ui_state(|ui, app: &mut App| runs_list(app, ui), app);
    harness.run();

    // "Команда ещё не запускалась" не должно быть видно рядом с работающим запуском.
    assert!(
        harness.query_by_label_contains(RU.runs_empty).is_none(),
        "плейсхолдер пустой истории показан поверх живого запуска"
    );
    // А сам живой запуск в списке есть.
    assert!(
        harness
            .query_by_label_contains(RU.run_running_row)
            .is_some()
    );

    if let Some(run) = harness.state_mut().runs.first_mut() {
        run.cancel();
    }
    std::fs::remove_dir_all(root).ok();
}
