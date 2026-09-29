use super::*;

/// Состояние без окна — для проверки операций над деревом и файлами.
fn app_with(folders: Vec<Folder>, commands: Vec<Command>) -> App {
    let root = std::env::temp_dir().join(format!("cmdrerun-app-test-{}", new_id()));
    let storage = Storage::with_root(root);
    storage.init().expect("init");
    App {
        storage,
        folders,
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
        lang: Lang::default(),
        drag: None,
        output_window: None,
    }
}

fn sample() -> (App, String, String) {
    let root = Folder::new("Деплой", None);
    let child = Folder::new("Стенды", Some(root.id.clone()));
    let command = Command::new("Перезапуск", Some(child.id.clone()));
    let (root_id, child_id) = (root.id.clone(), child.id.clone());
    let app = app_with(vec![root, child], vec![command]);
    (app, root_id, child_id)
}

#[test]
fn subtree_collects_all_descendants() {
    let (app, root_id, child_id) = sample();
    let (folders, commands) = app.subtree(&root_id);
    assert_eq!(folders.len(), 2);
    assert!(folders.contains(&child_id));
    assert_eq!(commands.len(), 1);
}

#[test]
fn folder_cannot_be_moved_into_its_own_subtree() {
    let (app, root_id, child_id) = sample();
    let target = Selection::Folder(root_id.clone());
    assert!(!app.can_move(&target, Some(&child_id)));
    assert!(!app.can_move(&target, Some(&root_id)));
    assert!(app.can_move(&target, None));
    // Команду можно переносить куда угодно.
    let command_id = app.commands[0].id.clone();
    assert!(app.can_move(&Selection::Command(command_id), Some(&child_id)));
}

#[test]
fn tree_shows_nested_items_only_when_expanded() {
    let (mut app, root_id, child_id) = sample();
    assert_eq!(app.tree_rows().len(), 1);

    app.expanded.insert(root_id);
    assert_eq!(app.tree_rows().len(), 2);

    app.expanded.insert(child_id);
    let rows = app.tree_rows();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[2].name, "Перезапуск");
    assert_eq!(rows[2].depth, 2);
}

#[test]
fn items_with_a_missing_parent_show_up_at_the_root() {
    // Папку удалили в обход приложения — команда не должна пропасть из дерева.
    let orphan = Command::new("Осиротевшая", Some("нет-такой-папки".to_owned()));
    let app = app_with(Vec::new(), vec![orphan]);
    let rows = app.tree_rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].depth, 0);
}

#[test]
fn linked_import_keeps_the_script_in_its_file() {
    let mut app = app_with(Vec::new(), Vec::new());
    let file = app.storage.root().join("deploy.sh");
    std::fs::write(&file, "echo привет\n").expect("write");

    app.import_command(&file.display().to_string(), true, None);
    let command = app.commands.first().expect("импортирована");
    assert_eq!(command.name, "deploy");
    assert_eq!(command.script, "echo привет\n");
    assert_eq!(
        command.script_path.as_deref(),
        Some(file.display().to_string().as_str())
    );

    // Сохранение правки уходит в тот же файл.
    app.editor.script = "echo пока\n".to_owned();
    app.save_editor();
    assert_eq!(std::fs::read_to_string(&file).expect("read"), "echo пока\n");
    // И остаётся версия в истории изменений.
    assert_eq!(app.changes.len(), 1);
    assert_eq!(app.changes[0].old_script, "echo привет\n");

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn exported_json_is_imported_whole_and_never_linked() {
    let mut app = app_with(Vec::new(), Vec::new());
    let mut original = Command::new("Сборка", None);
    original.comment = "комментарий".to_owned();
    original.params = vec![Parameter::new("BRANCH")];
    let file = app.storage.root().join("export.json");
    std::fs::write(
        &file,
        serde_json::to_string(&original.to_export()).expect("json"),
    )
    .expect("write");

    // Даже если попросили связь — у экспорта связывать нечего.
    app.import_command(&file.display().to_string(), true, None);
    let command = app.commands.first().expect("импортирована");
    assert_eq!(command.name, "Сборка");
    assert_eq!(command.comment, "комментарий");
    assert_eq!(command.params.len(), 1);
    assert!(command.script_path.is_none());

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn running_a_dirty_command_saves_it_first() {
    let command = Command::new("Тест", None);
    let id = command.id.clone();
    let mut app = app_with(Vec::new(), vec![command]);
    app.select(Selection::Command(id.clone()));
    app.editor.script = "echo изменено".to_owned();
    assert!(app.editor_dirty());

    app.request_run();
    assert!(!app.editor_dirty());
    assert_eq!(app.command(&id).unwrap().script, "echo изменено");
    assert_eq!(app.changes.len(), 1);
    if let Some(run) = app.runs.first_mut() {
        run.cancel();
    }

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn two_different_commands_run_concurrently() {
    let mut a = Command::new("Сборка", None);
    a.script = "sleep 1\n".to_owned();
    let mut b = Command::new("Тесты", None);
    b.script = "sleep 1\n".to_owned();
    let (a_id, b_id) = (a.id.clone(), b.id.clone());
    let mut app = app_with(Vec::new(), vec![a, b]);

    app.start_run(&a_id, Vec::new());
    app.start_run(&b_id, Vec::new());

    assert_eq!(
        app.runs.len(),
        2,
        "долгая команда не должна блокировать запуск другой"
    );
    assert!(app.is_running(&a_id));
    assert!(app.is_running(&b_id));

    for run in &mut app.runs {
        run.cancel();
    }
    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn starting_the_same_running_command_again_is_a_no_op() {
    let mut command = Command::new("Сборка", None);
    command.script = "sleep 1\n".to_owned();
    let id = command.id.clone();
    let mut app = app_with(Vec::new(), vec![command]);

    app.start_run(&id, Vec::new());
    assert_eq!(app.runs.len(), 1);

    app.start_run(&id, Vec::new());
    assert_eq!(
        app.runs.len(),
        1,
        "повторный запуск уже выполняющейся команды не должен плодить второй процесс"
    );
    assert!(
        app.status.is_some_and(|s| !s.is_error),
        "должно быть сообщение, а не ошибка"
    );

    if let Some(run) = app.runs.first_mut() {
        run.cancel();
    }
    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn one_command_finishing_does_not_disturb_another_still_running() {
    let mut fast = Command::new("Быстрая", None);
    fast.script = "true\n".to_owned();
    let mut slow = Command::new("Медленная", None);
    slow.script = "sleep 3\n".to_owned();
    let (fast_id, slow_id) = (fast.id.clone(), slow.id.clone());
    let mut app = app_with(Vec::new(), vec![fast, slow]);

    app.start_run(&fast_id, Vec::new());
    app.start_run(&slow_id, Vec::new());
    assert_eq!(app.runs.len(), 2);

    let ctx = egui::Context::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.is_running(&fast_id) && std::time::Instant::now() < deadline {
        app.poll_run(&ctx);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    assert!(
        !app.is_running(&fast_id),
        "быстрая команда должна была завершиться"
    );
    assert!(
        app.is_running(&slow_id),
        "завершение одной команды не должно затрагивать другую (сдвиг индексов при удалении из Vec)"
    );

    if let Some(run) = app.runs.first_mut() {
        run.cancel();
    }
    std::fs::remove_dir_all(app.storage.root()).ok();
}

/// Имена узлов корня в том порядке, в каком они видны в дереве.
fn root_order(app: &App) -> Vec<String> {
    app.siblings(None)
        .iter()
        .map(|item| app.display_name(item))
        .collect()
}

#[test]
fn a_node_dropped_higher_up_takes_that_number() {
    let mut app = app_with(
        Vec::new(),
        vec![
            Command::new("А", None),
            Command::new("Б", None),
            Command::new("В", None),
        ],
    );
    // Без номеров порядок алфавитный — как было до появления переносов.
    assert_eq!(root_order(&app), ["А", "Б", "В"]);

    let third = app.siblings(None)[2].clone();
    assert_eq!(app.position_of(&third), 3);
    app.move_to(&third, None, 0);

    assert_eq!(root_order(&app), ["В", "А", "Б"]);
    assert_eq!(app.position_of(&third), 1);
    // Номера пережили перезапуск: они лежат в файлах команд.
    let reloaded = app_with(Vec::new(), app.storage.load_commands());
    assert_eq!(root_order(&reloaded), ["В", "А", "Б"]);

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn dropping_on_a_row_says_where_the_node_lands() {
    let folder = Folder::new("Группа", None);
    let folder_id = folder.id.clone();
    let inside = Command::new("Внутренняя", Some(folder_id.clone()));
    let app = app_with(
        vec![folder],
        vec![
            inside,
            Command::new("Первая", None),
            Command::new("Вторая", None),
        ],
    );
    // В корне: группа, затем команды по алфавиту.
    assert_eq!(root_order(&app), ["Группа", "Вторая", "Первая"]);

    let group = Selection::Folder(folder_id.clone());
    let first = app.siblings(None)[2].clone();
    let second = app.siblings(None)[1].clone();

    // Перед первой строкой корня — нулевое место.
    assert_eq!(
        app.drop_place(&first, &group, DropPos::Before),
        Some((None, 0))
    );
    // После «Второй» — сразу за ней, номер считается уже без переносимого узла.
    assert_eq!(
        app.drop_place(&first, &second, DropPos::After),
        Some((None, 2))
    );
    // В середину группы — внутрь, в конец её содержимого.
    assert_eq!(
        app.drop_place(&first, &group, DropPos::Inside),
        Some((Some(folder_id), 1))
    );
    // Сам на себя и группа внутрь себя — так нельзя.
    assert_eq!(app.drop_place(&group, &group, DropPos::Inside), None);
    assert_eq!(app.drop_place(&first, &first, DropPos::Before), None);

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn a_group_dropped_into_its_own_child_is_refused() {
    let (app, root_id, child_id) = sample();
    let root = Selection::Folder(root_id);
    let child = Selection::Folder(child_id);
    assert_eq!(app.drop_place(&root, &child, DropPos::Inside), None);
    // А вот ребёнка в корень — пожалуйста.
    assert!(app.drop_place(&child, &root, DropPos::Before).is_some());

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn a_deleted_run_disappears_from_the_history_and_from_disk() {
    let command = Command::new("Тест", None);
    let id = command.id.clone();
    let mut app = app_with(Vec::new(), vec![command]);
    app.select(Selection::Command(id.clone()));

    for _ in 0..2 {
        let log = ExecutionLog {
            id: new_id(),
            command_id: id.clone(),
            script: String::new(),
            output: String::new(),
            exit_code: Some(0),
            start_time: chrono::Local::now(),
            end_time: Some(chrono::Local::now()),
            params: Vec::new(),
        };
        app.storage.save_log(&log).expect("save");
        app.logs.push(log);
    }
    app.selected_log = Some(1);

    let first = app.logs[0].id.clone();
    app.delete_log(&first);

    assert_eq!(app.logs.len(), 1);
    assert_eq!(app.storage.load_logs(&id).len(), 1);
    // Выделение съезжает вместе со списком, а не показывает чужой запуск.
    assert_eq!(app.selected_log, Some(0));

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn a_repeated_run_starts_with_the_values_of_the_old_one() {
    let mut command = Command::new("Сборка", None);
    command.params = vec![Parameter::new("BRANCH"), Parameter::new("ENV")];
    let id = command.id.clone();
    let mut app = app_with(Vec::new(), vec![command]);
    app.select(Selection::Command(id.clone()));

    let mut used = Parameter::new("BRANCH");
    used.value = "release".to_owned();
    app.logs.push(ExecutionLog {
        id: new_id(),
        command_id: id.clone(),
        script: String::new(),
        output: String::new(),
        exit_code: Some(0),
        start_time: chrono::Local::now(),
        end_time: Some(chrono::Local::now()),
        params: vec![used],
    });

    let log_id = app.logs[0].id.clone();
    app.rerun_log(&log_id);

    // Форма запуска открыта и заполнена значениями того запуска.
    match app.dialog {
        Some(Dialog::Run { ref params, .. }) => {
            assert_eq!(params[0].value, "release");
            assert_eq!(params[1].value, "");
        }
        _ => panic!("ожидалась форма запуска"),
    }

    std::fs::remove_dir_all(app.storage.root()).ok();
}

#[test]
fn ui_state_survives_a_restart() {
    let folder = Folder::new("Группа", None);
    let command = Command::new("Команда", Some(folder.id.clone()));
    let (folder_id, command_id) = (folder.id.clone(), command.id.clone());
    let mut app = app_with(vec![folder.clone()], vec![command.clone()]);
    app.set_lang(crate::i18n::Lang::En);
    app.toggle_expanded(&folder_id);
    app.select(Selection::Command(command_id.clone()));

    let mut restarted = app_with(vec![folder], vec![command]);
    restarted.restore(app.storage.load_settings());
    assert_eq!(restarted.lang, crate::i18n::Lang::En);
    assert!(restarted.expanded.contains(&folder_id));
    assert_eq!(restarted.selection, Some(Selection::Command(command_id)));

    std::fs::remove_dir_all(app.storage.root()).ok();
    std::fs::remove_dir_all(restarted.storage.root()).ok();
}

#[test]
fn path_of_walks_up_to_the_root() {
    let (app, _, child_id) = sample();
    let command_id = app.commands[0].id.clone();
    assert_eq!(
        app.path_of(&Selection::Command(command_id)),
        "Деплой / Стенды / Перезапуск"
    );
    assert_eq!(app.path_of(&Selection::Folder(child_id)), "Деплой / Стенды");
}
