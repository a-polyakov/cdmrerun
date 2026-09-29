//! Хранилище на файловой системе.
//!
//! Раскладка каталога данных:
//! ```text
//! <root>/folders/<folder_id>.json
//! <root>/commands/<command_id>.json
//! <root>/history/<command_id>/runs/<ts>__<log_id>.json
//! <root>/history/<command_id>/changes/<ts>__<change_id>.json
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::model::{ChangeLog, Command, ExecutionLog, Folder, Settings};

pub type Result<T> = std::result::Result<T, String>;

fn io_err(path: &Path, err: impl std::fmt::Display) -> String {
    format!("{}: {err}", path.display())
}

pub struct Storage {
    root: PathBuf,
}

impl Storage {
    /// Каталог данных: `$CMDRERUN_HOME`, иначе стандартный каталог данных ОС.
    pub fn new() -> Self {
        let root = match std::env::var_os("CMDRERUN_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => directories::ProjectDirs::from("dev", "cmdrerun", "cmdrerun")
                .map(|dirs| dirs.data_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".cmdrerun")),
        };
        Self::with_root(root)
    }

    /// Хранилище в указанном каталоге (используется в тестах).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn folders_dir(&self) -> PathBuf {
        self.root.join("folders")
    }

    fn commands_dir(&self) -> PathBuf {
        self.root.join("commands")
    }

    fn history_dir(&self, command_id: &str) -> PathBuf {
        self.root.join("history").join(command_id)
    }

    fn runs_dir(&self, command_id: &str) -> PathBuf {
        self.history_dir(command_id).join("runs")
    }

    fn changes_dir(&self, command_id: &str) -> PathBuf {
        self.history_dir(command_id).join("changes")
    }

    pub fn init(&self) -> Result<()> {
        for dir in [self.folders_dir(), self.commands_dir(), self.root.join("history")] {
            fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        }
        Ok(())
    }

    // --- примитивы чтения/записи ---

    /// Пишет во временный файл и переименовывает — чтобы не оставить обрезанный JSON.
    fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
        }
        let data = serde_json::to_vec_pretty(value).map_err(|e| io_err(path, e))?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &data).map_err(|e| io_err(&tmp, e))?;
        fs::rename(&tmp, path).map_err(|e| io_err(path, e))
    }

    /// Читает все `*.json` из каталога, отсортированные по имени файла.
    fn read_dir_json<T: DeserializeOwned>(dir: &Path) -> Vec<(String, T)> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();

        files
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                let data = fs::read(&path).ok()?;
                match serde_json::from_slice::<T>(&data) {
                    Ok(value) => Some((name, value)),
                    Err(err) => {
                        eprintln!("не удалось прочитать {}: {err}", path.display());
                        None
                    }
                }
            })
            .collect()
    }

    // --- папки ---

    pub fn load_folders(&self) -> Vec<Folder> {
        Self::read_dir_json(&self.folders_dir())
            .into_iter()
            .map(|(_, folder)| folder)
            .collect()
    }

    pub fn save_folder(&self, folder: &Folder) -> Result<()> {
        Self::write_json(&self.folders_dir().join(format!("{}.json", folder.id)), folder)
    }

    pub fn delete_folder(&self, id: &str) -> Result<()> {
        let path = self.folders_dir().join(format!("{id}.json"));
        remove_if_exists(&path)
    }

    // --- команды ---

    pub fn load_commands(&self) -> Vec<Command> {
        Self::read_dir_json(&self.commands_dir())
            .into_iter()
            .map(|(_, command)| command)
            .collect()
    }

    pub fn save_command(&self, command: &Command) -> Result<()> {
        Self::write_json(
            &self.commands_dir().join(format!("{}.json", command.id)),
            command,
        )
    }

    /// Удаляет команду вместе со всей её историей.
    pub fn delete_command(&self, id: &str) -> Result<()> {
        remove_if_exists(&self.commands_dir().join(format!("{id}.json")))?;
        let history = self.history_dir(id);
        if history.exists() {
            fs::remove_dir_all(&history).map_err(|e| io_err(&history, e))?;
        }
        Ok(())
    }

    // --- история запусков ---

    pub fn load_logs(&self, command_id: &str) -> Vec<ExecutionLog> {
        Self::read_dir_json(&self.runs_dir(command_id))
            .into_iter()
            .map(|(_, log)| log)
            .collect()
    }

    pub fn save_log(&self, log: &ExecutionLog) -> Result<()> {
        let name = format!("{}__{}.json", ts_prefix(&log.start_time), log.id);
        Self::write_json(&self.runs_dir(&log.command_id).join(name), log)
    }

    /// Удаляет одну запись истории запусков. Имя файла начинается с времени,
    /// поэтому ищем его по идентификатору в хвосте.
    pub fn delete_log(&self, command_id: &str, log_id: &str) -> Result<()> {
        let dir = self.runs_dir(command_id);
        let suffix = format!("__{log_id}.json");
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(());
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(&suffix))
            {
                remove_if_exists(&path)?;
            }
        }
        Ok(())
    }

    // --- настройки и состояние интерфейса ---

    fn settings_path(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn load_settings(&self) -> Settings {
        fs::read(self.settings_path())
            .ok()
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        Self::write_json(&self.settings_path(), settings)
    }

    // --- история изменений ---

    pub fn load_changes(&self, command_id: &str) -> Vec<ChangeLog> {
        Self::read_dir_json(&self.changes_dir(command_id))
            .into_iter()
            .map(|(_, change)| change)
            .collect()
    }

    pub fn save_change(&self, change: &ChangeLog) -> Result<()> {
        let name = format!("{}__{}.json", ts_prefix(&change.timestamp), change.id);
        Self::write_json(&self.changes_dir(&change.command_id).join(name), change)
    }
}

/// Читает связанный со скриптом файл.
pub fn read_text_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| io_err(path, e))
}

/// Пишет скрипт в связанный файл, сохраняя права доступа существующего файла.
pub fn write_text_file(path: &Path, text: &str) -> Result<()> {
    fs::write(path, text).map_err(|e| io_err(path, e))
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(io_err(path, err)),
    }
}

/// Сортируемый префикс имени файла, чтобы история читалась в хронологическом порядке.
fn ts_prefix(time: &chrono::DateTime<chrono::Local>) -> String {
    time.format("%Y%m%d-%H%M%S-%3f").to_string()
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
