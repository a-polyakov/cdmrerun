use super::*;
use crate::model::{ChangeLog, ParamType, Parameter, new_id};

fn temp_storage() -> Storage {
    let root = std::env::temp_dir().join(format!("cmdrerun-test-{}", new_id()));
    let storage = Storage::with_root(root);
    storage.init().expect("init");
    storage
}

#[test]
fn commands_survive_a_round_trip_with_every_param_type() {
    let storage = temp_storage();
    let mut command = Command::new("Сборка", None);
    command.comment = "комментарий".to_owned();
    command.params = vec![
        Parameter::new("A"),
        Parameter {
            name: "B".into(),
            value: "многострочный\nтекст".into(),
            param_type: ParamType::Text,
        },
        Parameter {
            name: "C".into(),
            value: "true".into(),
            param_type: ParamType::Boolean,
        },
        Parameter {
            name: "D".into(),
            value: "prod".into(),
            param_type: ParamType::Choice(vec!["dev".into(), "prod".into()]),
        },
        Parameter {
            name: "E".into(),
            value: "секрет".into(),
            param_type: ParamType::Password,
        },
    ];
    storage.save_command(&command).expect("save");

    let loaded = storage.load_commands();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0], command);

    std::fs::remove_dir_all(storage.root()).ok();
}

#[test]
fn history_is_stored_per_command_and_sorted_by_time() {
    let storage = temp_storage();
    let command = Command::new("Тест", None);

    let mut first = ChangeLog::snapshot(&command);
    first.timestamp = chrono::Local::now() - chrono::Duration::hours(2);
    first.old_script = "версия 1".to_owned();
    let mut second = ChangeLog::snapshot(&command);
    second.old_script = "версия 2".to_owned();
    // Пишем в обратном порядке — читаться должно всё равно по времени.
    storage.save_change(&second).expect("save");
    storage.save_change(&first).expect("save");

    let changes = storage.load_changes(&command.id);
    assert_eq!(
        changes
            .iter()
            .map(|c| c.old_script.as_str())
            .collect::<Vec<_>>(),
        ["версия 1", "версия 2"]
    );

    // Другая команда своей истории не видит.
    assert!(storage.load_changes(&new_id()).is_empty());

    storage.delete_command(&command.id).expect("delete");
    assert!(storage.load_changes(&command.id).is_empty());

    std::fs::remove_dir_all(storage.root()).ok();
}

#[test]
fn folders_can_be_saved_and_deleted() {
    let storage = temp_storage();
    let folder = Folder::new("Группа", None);
    storage.save_folder(&folder).expect("save");
    assert_eq!(storage.load_folders(), vec![folder.clone()]);

    storage.delete_folder(&folder.id).expect("delete");
    assert!(storage.load_folders().is_empty());
    // Повторное удаление не должно быть ошибкой.
    storage.delete_folder(&folder.id).expect("delete again");

    std::fs::remove_dir_all(storage.root()).ok();
}
