//! Состояние приложения и операции над деревом команд.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::diff;
use crate::exec::{self, ActiveRun};
use crate::i18n::{Lang, Strings, fill1, fill2};
use crate::model::{
    ChangeLog, Command, CommandExport, ExecutionLog, Folder, Parameter, Selection, Settings, new_id,
};
use crate::storage::{self, Storage};
use crate::ui;

/// В пропорциональном шрифте egui нет стрелок и геометрических фигур —
/// добавляем моноширинный Hack запасным вариантом, иначе вместо «↑» рисуется квадрат.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional)
        && !family.iter().any(|name| name == "Hack")
    {
        family.push("Hack".to_owned());
    }
    ctx.set_fonts(fonts);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomTab {
    Runs,
    Changes,
}

/// Рабочая копия выбранной команды: правим здесь, сохраняем по кнопке.
#[derive(Default)]
pub struct Editor {
    pub command_id: Option<String>,
    pub name: String,
    pub script: String,
    pub comment: String,
    pub params: Vec<Parameter>,
}

pub enum Dialog {
    NewFolder {
        parent_id: Option<String>,
        name: String,
    },
    NewCommand {
        parent_id: Option<String>,
        name: String,
    },
    Rename {
        target: Selection,
        name: String,
    },
    Move {
        target: Selection,
        parent_id: Option<String>,
    },
    Delete {
        target: Selection,
        summary: String,
    },
    Run {
        command_id: String,
        params: Vec<Parameter>,
    },
    Unsaved {
        next: Option<Selection>,
    },
    Import {
        path: String,
        link: bool,
        parent_id: Option<String>,
    },
    About,
}

pub struct Status {
    pub text: String,
    pub is_error: bool,
}

/// Строка дерева, подготовленная к отрисовке.
pub struct TreeRow {
    pub selection: Selection,
    pub name: String,
    pub depth: usize,
    pub is_folder: bool,
    pub has_children: bool,
}

pub struct App {
    pub storage: Storage,
    pub folders: Vec<Folder>,
    pub commands: Vec<Command>,
    pub selection: Option<Selection>,
    pub expanded: HashSet<String>,
    pub editor: Editor,
    pub logs: Vec<ExecutionLog>,
    pub changes: Vec<ChangeLog>,
    pub runs: Vec<ActiveRun>,
    pub bottom_tab: BottomTab,
    pub selected_log: Option<usize>,
    pub selected_change: Option<usize>,
    pub dialog: Option<Dialog>,
    pub status: Option<Status>,
    pub lang: Lang,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_fonts(&cc.egui_ctx);
        let storage = Storage::new();
        let mut app = Self {
            folders: Vec::new(),
            commands: Vec::new(),
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
            storage,
        };
        if let Err(err) = app.storage.init() {
            app.set_error(err);
        }
        app.folders = app.storage.load_folders();
        app.commands = app.storage.load_commands();
        app.restore(app.storage.load_settings());
        app
    }

    /// Возвращает интерфейс туда, где его оставили в прошлый раз.
    fn restore(&mut self, settings: Settings) {
        self.lang = settings.lang;
        self.expanded = settings.expanded.into_iter().collect();
        if let Some(selection) = settings.selection.filter(|s| self.exists(s)) {
            self.select(selection);
        }
    }

    /// Строки интерфейса на выбранном языке.
    pub fn s(&self) -> &'static Strings {
        self.lang.strings()
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
        self.persist_ui();
    }

    /// Пишет язык, выделение и раскрытые группы, чтобы их пережил перезапуск.
    fn persist_ui(&mut self) {
        let settings = Settings {
            lang: self.lang,
            selection: self.selection.clone(),
            expanded: self.expanded.iter().cloned().collect(),
        };
        let result = self.storage.save_settings(&settings);
        self.report(result);
    }

    pub fn toggle_expanded(&mut self, folder_id: &str) {
        if !self.expanded.remove(folder_id) {
            self.expanded.insert(folder_id.to_owned());
        }
        self.persist_ui();
    }

    // ---------- статус ----------

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            is_error: false,
        });
    }

    pub fn set_error(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            is_error: true,
        });
    }

    fn report(&mut self, result: crate::storage::Result<()>) {
        if let Err(err) = result {
            self.set_error(err);
        }
    }

    // ---------- доступ к данным ----------

    pub fn folder(&self, id: &str) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn command(&self, id: &str) -> Option<&Command> {
        self.commands.iter().find(|c| c.id == id)
    }

    pub fn exists(&self, selection: &Selection) -> bool {
        match selection {
            Selection::Folder(id) => self.folder(id).is_some(),
            Selection::Command(id) => self.command(id).is_some(),
        }
    }

    pub fn display_name(&self, selection: &Selection) -> String {
        match selection {
            Selection::Folder(id) => self.folder(id).map(|f| f.name.clone()),
            Selection::Command(id) => self.command(id).map(|c| c.name.clone()),
        }
        .unwrap_or_else(|| "—".to_owned())
    }

    /// Путь от корня к элементу — «хлебные крошки».
    pub fn path_of(&self, selection: &Selection) -> String {
        let mut parts = vec![self.display_name(selection)];
        let mut parent = match selection {
            Selection::Folder(id) => self.folder(id).and_then(|f| f.parent_id.clone()),
            Selection::Command(id) => self.command(id).and_then(|c| c.parent_id.clone()),
        };
        while let Some(id) = parent {
            match self.folder(&id) {
                Some(folder) => {
                    parts.push(folder.name.clone());
                    parent = folder.parent_id.clone();
                }
                None => break,
            }
        }
        parts.reverse();
        parts.join(" / ")
    }

    /// Папка, внутрь которой попадёт новый элемент при текущем выделении.
    pub fn target_parent(&self) -> Option<String> {
        match &self.selection {
            Some(Selection::Folder(id)) => Some(id.clone()),
            Some(Selection::Command(id)) => self.command(id).and_then(|c| c.parent_id.clone()),
            None => None,
        }
    }

    fn has_children(&self, folder_id: &str) -> bool {
        self.folders
            .iter()
            .any(|f| f.parent_id.as_deref() == Some(folder_id))
            || self
                .commands
                .iter()
                .any(|c| c.parent_id.as_deref() == Some(folder_id))
    }

    /// Все вложенные папки и команды (включая саму папку).
    pub fn subtree(&self, folder_id: &str) -> (Vec<String>, Vec<String>) {
        let mut folders = vec![folder_id.to_owned()];
        let mut commands = Vec::new();
        let mut index = 0;
        while index < folders.len() {
            let current = folders[index].clone();
            index += 1;
            for folder in &self.folders {
                if folder.parent_id.as_deref() == Some(current.as_str()) {
                    folders.push(folder.id.clone());
                }
            }
            for command in &self.commands {
                if command.parent_id.as_deref() == Some(current.as_str()) {
                    commands.push(command.id.clone());
                }
            }
        }
        (folders, commands)
    }

    pub fn tree_rows(&self) -> Vec<TreeRow> {
        let mut rows = Vec::new();
        self.collect_rows(None, 0, &mut rows);
        rows
    }

    /// Узел попадает в корень, если родителя нет или файл родителя пропал.
    fn belongs_to(&self, parent_id: Option<&str>, level: Option<&str>) -> bool {
        match (parent_id, level) {
            (Some(id), Some(level)) => id == level,
            (Some(id), None) => self.folder(id).is_none(),
            (None, level) => level.is_none(),
        }
    }

    fn collect_rows(&self, parent: Option<&str>, depth: usize, rows: &mut Vec<TreeRow>) {
        let mut folders: Vec<&Folder> = self
            .folders
            .iter()
            .filter(|f| self.belongs_to(f.parent_id.as_deref(), parent))
            .collect();
        folders.sort_by_key(|f| f.name.to_lowercase());
        for folder in folders {
            rows.push(TreeRow {
                selection: Selection::Folder(folder.id.clone()),
                name: folder.name.clone(),
                depth,
                is_folder: true,
                has_children: self.has_children(&folder.id),
            });
            if self.expanded.contains(&folder.id) {
                self.collect_rows(Some(&folder.id), depth + 1, rows);
            }
        }

        let mut commands: Vec<&Command> = self
            .commands
            .iter()
            .filter(|c| self.belongs_to(c.parent_id.as_deref(), parent))
            .collect();
        commands.sort_by_key(|c| c.name.to_lowercase());
        for command in commands {
            rows.push(TreeRow {
                selection: Selection::Command(command.id.clone()),
                name: command.name.clone(),
                depth,
                is_folder: false,
                has_children: false,
            });
        }
    }

    // ---------- выделение и редактор ----------

    /// Переключает выделение; при несохранённых правках сначала спрашивает.
    pub fn request_select(&mut self, selection: Selection) {
        if self.editor_dirty() && self.selection.as_ref() != Some(&selection) {
            self.dialog = Some(Dialog::Unsaved {
                next: Some(selection),
            });
        } else {
            self.select(selection);
        }
    }

    pub fn select(&mut self, selection: Selection) {
        self.selection = Some(selection.clone());
        self.persist_ui();
        match &selection {
            Selection::Command(id) => self.load_command(&id.clone()),
            Selection::Folder(_) => {
                self.editor = Editor::default();
                self.logs.clear();
                self.changes.clear();
                self.selected_log = None;
                self.selected_change = None;
            }
        }
    }

    fn load_command(&mut self, id: &str) {
        let Some(command) = self.command(id).cloned() else {
            return;
        };
        // У связанной команды источник истины — файл на диске.
        let mut script = command.script;
        if let Some(path) = &command.script_path {
            match storage::read_text_file(Path::new(path)) {
                Ok(text) => script = text,
                Err(err) => self.set_error(fill1(self.s().st_file_read_failed, err)),
            }
        }
        self.editor = Editor {
            command_id: Some(command.id.clone()),
            name: command.name,
            script,
            comment: command.comment,
            params: command.params,
        };
        self.logs = self.storage.load_logs(id);
        self.changes = self.storage.load_changes(id);
        self.selected_log = self.logs.len().checked_sub(1);
        self.selected_change = self.changes.len().checked_sub(1);
    }

    pub fn editor_dirty(&self) -> bool {
        let Some(id) = self.editor.command_id.as_deref() else {
            return false;
        };
        let Some(command) = self.command(id) else {
            return false;
        };
        command.name != self.editor.name
            || command.script != self.editor.script
            || command.comment != self.editor.comment
            || command.params != self.editor.params
    }

    pub fn revert_editor(&mut self) {
        if let Some(id) = self.editor.command_id.clone() {
            self.load_command(&id);
            self.set_status(self.s().st_reverted);
        }
    }

    /// Сохраняет команду, записав предыдущую версию в историю изменений.
    pub fn save_editor(&mut self) {
        if !self.editor_dirty() {
            self.set_status(self.s().st_no_changes);
            return;
        }
        if self.editor.name.trim().is_empty() {
            self.set_error(self.s().st_name_empty);
            return;
        }
        let Some(id) = self.editor.command_id.clone() else {
            return;
        };
        let Some(index) = self.commands.iter().position(|c| c.id == id) else {
            return;
        };

        let change = ChangeLog::snapshot(&self.commands[index]);
        let result = self.storage.save_change(&change);
        self.report(result);

        let command = &mut self.commands[index];
        command.name = self.editor.name.trim().to_owned();
        command.script = self.editor.script.clone();
        command.comment = self.editor.comment.clone();
        command.params = self.editor.params.clone();
        let command = command.clone();

        let result = self.storage.save_command(&command);
        self.report(result);

        // Связанная команда пишет скрипт обратно в свой файл.
        if let Some(path) = &command.script_path
            && let Err(err) = storage::write_text_file(Path::new(path), &command.script)
        {
            self.set_error(fill1(self.s().st_file_write_failed, err));
        }

        self.editor.name = command.name.clone();
        self.changes.push(change);
        self.selected_change = self.changes.len().checked_sub(1);
        self.set_status(fill1(self.s().st_saved, &command.name));
    }

    /// Подставляет выбранную версию в редактор — сохранение отдельной кнопкой.
    pub fn restore_version(&mut self, change_index: usize) {
        let Some(change) = self.changes.get(change_index).cloned() else {
            return;
        };
        if !change.old_name.is_empty() {
            self.editor.name = change.old_name;
        }
        self.editor.script = change.old_script;
        self.editor.comment = change.old_comment;
        self.editor.params = change.old_params;
        self.set_status(self.s().st_restored);
    }

    // ---------- операции над деревом ----------

    pub fn create_folder(&mut self, parent_id: Option<String>, name: String) {
        let folder = Folder::new(name.trim(), parent_id.clone());
        let result = self.storage.save_folder(&folder);
        self.report(result);
        if let Some(parent) = parent_id {
            self.expanded.insert(parent);
        }
        let id = folder.id.clone();
        self.folders.push(folder);
        self.select(Selection::Folder(id));
        self.set_status(fill1(self.s().st_created_folder, name.trim()));
    }

    pub fn create_command(&mut self, parent_id: Option<String>, name: String) {
        let command = Command::new(name.trim(), parent_id.clone());
        let result = self.storage.save_command(&command);
        self.report(result);
        if let Some(parent) = parent_id {
            self.expanded.insert(parent);
        }
        let id = command.id.clone();
        self.commands.push(command);
        self.select(Selection::Command(id));
        self.set_status(fill1(self.s().st_created_command, name.trim()));
    }

    pub fn rename(&mut self, target: &Selection, name: String) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            self.set_error(self.s().st_name_empty);
            return;
        }
        match target {
            Selection::Folder(id) => {
                let Some(folder) = self.folders.iter_mut().find(|f| f.id == *id) else {
                    return;
                };
                folder.name = name.clone();
                let folder = folder.clone();
                let result = self.storage.save_folder(&folder);
                self.report(result);
            }
            Selection::Command(id) => {
                let Some(index) = self.commands.iter().position(|c| c.id == *id) else {
                    return;
                };
                // Имя — часть версии команды, поэтому переименование тоже попадает в историю.
                let change = ChangeLog::snapshot(&self.commands[index]);
                let result = self.storage.save_change(&change);
                self.report(result);

                self.commands[index].name = name.clone();
                let command = self.commands[index].clone();
                let result = self.storage.save_command(&command);
                self.report(result);

                if self.editor.command_id.as_deref() == Some(id.as_str()) {
                    self.editor.name = name.clone();
                    self.changes.push(change);
                    self.selected_change = self.changes.len().checked_sub(1);
                }
            }
        }
        self.set_status(fill1(self.s().st_renamed, &name));
    }

    /// Проверяет, что перенос допустим (нельзя вложить папку в саму себя).
    pub fn can_move(&self, target: &Selection, new_parent: Option<&str>) -> bool {
        match target {
            Selection::Command(_) => true,
            Selection::Folder(id) => match new_parent {
                None => true,
                Some(parent) => {
                    let (descendants, _) = self.subtree(id);
                    !descendants.iter().any(|d| d == parent)
                }
            },
        }
    }

    pub fn move_to(&mut self, target: &Selection, new_parent: Option<String>) {
        if !self.can_move(target, new_parent.as_deref()) {
            self.set_error(self.s().st_cant_move_into_self);
            return;
        }
        match target {
            Selection::Folder(id) => {
                let Some(folder) = self.folders.iter_mut().find(|f| f.id == *id) else {
                    return;
                };
                folder.parent_id = new_parent.clone();
                let folder = folder.clone();
                let result = self.storage.save_folder(&folder);
                self.report(result);
            }
            Selection::Command(id) => {
                let Some(command) = self.commands.iter_mut().find(|c| c.id == *id) else {
                    return;
                };
                command.parent_id = new_parent.clone();
                let command = command.clone();
                let result = self.storage.save_command(&command);
                self.report(result);
            }
        }
        if let Some(parent) = new_parent {
            self.expanded.insert(parent);
        }
        self.set_status(fill1(self.s().st_moved, self.display_name(target)));
    }

    /// Текст подтверждения удаления: сколько всего пропадёт.
    pub fn delete_summary(&self, target: &Selection) -> String {
        match target {
            Selection::Command(id) => fill2(
                self.s().dlg_delete_command_info,
                self.storage.load_logs(id).len(),
                self.storage.load_changes(id).len(),
            ),
            Selection::Folder(id) => {
                let (folders, commands) = self.subtree(id);
                fill2(
                    self.s().dlg_delete_folder_info,
                    folders.len() - 1,
                    commands.len(),
                )
            }
        }
    }

    pub fn delete(&mut self, target: &Selection) {
        let name = self.display_name(target);
        match target {
            Selection::Command(id) => {
                let result = self.storage.delete_command(id);
                self.report(result);
                self.commands.retain(|c| c.id != *id);
            }
            Selection::Folder(id) => {
                let (folders, commands) = self.subtree(id);
                for command_id in &commands {
                    let result = self.storage.delete_command(command_id);
                    self.report(result);
                }
                for folder_id in &folders {
                    let result = self.storage.delete_folder(folder_id);
                    self.report(result);
                }
                self.commands.retain(|c| !commands.contains(&c.id));
                self.folders.retain(|f| !folders.contains(&f.id));
            }
        }

        if !self.selection.as_ref().is_some_and(|s| self.exists(s)) {
            self.selection = None;
            self.editor = Editor::default();
            self.logs.clear();
            self.changes.clear();
            self.selected_log = None;
            self.selected_change = None;
            self.persist_ui();
        }
        self.set_status(fill1(self.s().st_deleted, &name));
    }

    // ---------- запуск ----------
    //
    // Разные команды выполняются параллельно — каждая в своём потоке (см. exec::ActiveRun),
    // поэтому долгая сборка одной команды не мешает запустить другую. Одну и ту же команду
    // повторно, пока она выполняется, не запускаем — было бы неясно, какой из двух
    // результатов относится к какому запуску.

    pub fn is_running(&self, command_id: &str) -> bool {
        self.runs.iter().any(|run| run.command_id == command_id)
    }

    fn run_for(&self, command_id: &str) -> Option<&ActiveRun> {
        self.runs.iter().find(|run| run.command_id == command_id)
    }

    fn run_for_mut(&mut self, command_id: &str) -> Option<&mut ActiveRun> {
        self.runs.iter_mut().find(|run| run.command_id == command_id)
    }

    /// Текущий запуск открытой в редакторе команды — то, что видно на её вкладке.
    pub fn current_run(&self) -> Option<&ActiveRun> {
        self.run_for(self.editor.command_id.as_deref()?)
    }

    /// Готовит форму запуска: если параметров нет — запускает сразу.
    ///
    /// Незаписанные правки сохраняются автоматически, поэтому запускается
    /// ровно та версия, что попала в историю изменений.
    pub fn request_run(&mut self) {
        let Some(id) = self.editor.command_id.clone() else {
            return;
        };
        if self.is_running(&id) {
            return;
        }
        if self.editor_dirty() {
            self.save_editor();
        }
        let Some(command) = self.command(&id) else {
            return;
        };
        if command.params.is_empty() {
            self.start_run(&id, Vec::new());
        } else {
            self.dialog = Some(Dialog::Run {
                command_id: id,
                params: command.params.clone(),
            });
        }
    }

    pub fn start_run(&mut self, command_id: &str, params: Vec<Parameter>) {
        if self.is_running(command_id) {
            let name = self.display_name(&Selection::Command(command_id.to_owned()));
            self.set_status(fill1(self.s().st_already_running, name));
            return;
        }
        let Some(command) = self.command(command_id).cloned() else {
            return;
        };
        let logs = if self.editor.command_id.as_deref() == Some(command_id) {
            self.logs.clone()
        } else {
            self.storage.load_logs(command_id)
        };
        let estimate = exec::estimate_secs(&logs);
        self.runs.push(ActiveRun::start(&command, params, estimate));
        // Переключаем вкладку истории только у той команды, что сейчас открыта —
        // запуск другой команды в фоне не должен дёргать текущий экран.
        if self.editor.command_id.as_deref() == Some(command_id) {
            self.bottom_tab = BottomTab::Runs;
            self.selected_log = None;
        }
        self.set_status(fill1(self.s().st_started, &command.name));
    }

    pub fn cancel_run(&mut self, command_id: &str) {
        if let Some(run) = self.run_for_mut(command_id) {
            run.cancel();
            self.set_status(self.s().st_stopping);
        }
    }

    /// Опрашивает все идущие запуски и сохраняет лог по завершении каждого.
    fn poll_run(&mut self, ctx: &egui::Context) {
        let mut still_running = false;
        for run in &mut self.runs {
            run.poll();
            still_running |= !run.finished;
        }
        if still_running {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        let mut index = 0;
        while index < self.runs.len() {
            if !self.runs[index].finished {
                index += 1;
                continue;
            }
            let run = self.runs.remove(index);
            let log = run.to_log();

            let result = self.storage.save_log(&log);
            self.report(result);

            let message = if run.cancelled {
                fill1(self.s().st_stopped, &run.command_name)
            } else {
                fill2(
                    self.s().st_done,
                    ui::fmt_duration(self.lang, log.duration_secs().unwrap_or_default()),
                    log.exit_code
                        .map_or_else(|| "—".to_owned(), |code| code.to_string()),
                )
            };

            if self.editor.command_id.as_deref() == Some(log.command_id.as_str()) {
                self.logs.push(log);
                self.selected_log = self.logs.len().checked_sub(1);
            }
            self.set_status(message);
        }
    }

    // ---------- импорт и экспорт ----------

    /// Экспорт: `.json` — команда целиком, другое расширение — только скрипт.
    pub fn export_command(&mut self) {
        let Some(command) = self
            .editor
            .command_id
            .clone()
            .and_then(|id| self.command(&id).cloned())
        else {
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(format!("{}.json", safe_file_name(&command.name)))
            .add_filter("JSON", &["json"])
            .add_filter("Shell", &["sh"])
            .save_file()
        else {
            return;
        };

        // Экспортируем то, что видит пользователь, включая несохранённые правки.
        let script = self.editor.script.clone();
        let result = if is_json(&path) {
            let mut export = command.to_export();
            export.script = script;
            serde_json::to_string_pretty(&export)
                .map_err(|err| err.to_string())
                .and_then(|text| storage::write_text_file(&path, &text))
        } else {
            storage::write_text_file(&path, &script)
        };

        match result {
            Ok(()) => self.set_status(fill1(self.s().st_exported, path.display())),
            Err(err) => self.set_error(fill1(self.s().st_file_write_failed, err)),
        }
    }

    pub fn open_import_dialog(&mut self) {
        self.dialog = Some(Dialog::Import {
            path: String::new(),
            link: false,
            parent_id: self.target_parent(),
        });
    }

    /// Создаёт команду из файла. `link` — держать скрипт в исходном файле.
    pub fn import_command(&mut self, path: &str, link: bool, parent_id: Option<String>) {
        let file = PathBuf::from(path);
        let text = match storage::read_text_file(&file) {
            Ok(text) => text,
            Err(err) => {
                self.set_error(fill1(self.s().st_file_read_failed, err));
                return;
            }
        };

        let export = parse_export(&file, &text);
        let linked = link && export.is_none();
        let command = Command {
            id: new_id(),
            name: export
                .as_ref()
                .map(|e| e.name.clone())
                .unwrap_or_else(|| file_stem(&file)),
            script: export.as_ref().map_or(text, |e| e.script.clone()),
            comment: export
                .as_ref()
                .map(|e| e.comment.clone())
                .unwrap_or_default(),
            params: export.map(|e| e.params).unwrap_or_default(),
            parent_id: parent_id.clone(),
            script_path: linked.then(|| path.to_owned()),
        };

        let result = self.storage.save_command(&command);
        self.report(result);
        if let Some(parent) = parent_id {
            self.expanded.insert(parent);
        }
        let (id, name) = (command.id.clone(), command.name.clone());
        self.commands.push(command);
        self.select(Selection::Command(id));
        self.set_status(fill1(self.s().st_imported, name));
    }

    /// Убирает связь с файлом: скрипт остаётся в хранилище.
    pub fn unlink_script(&mut self) {
        let Some(id) = self.editor.command_id.clone() else {
            return;
        };
        let Some(command) = self.commands.iter_mut().find(|c| c.id == id) else {
            return;
        };
        command.script_path = None;
        let command = command.clone();
        let result = self.storage.save_command(&command);
        self.report(result);
        self.set_status(self.s().st_unlinked);
    }

    /// Перечитывает скрипт из связанного файла в редактор.
    pub fn reload_linked_script(&mut self) {
        let Some(path) = self.linked_path() else {
            return;
        };
        match storage::read_text_file(Path::new(&path)) {
            Ok(text) => {
                self.editor.script = text;
                self.set_status(fill1(self.s().st_linked, path));
            }
            Err(err) => self.set_error(fill1(self.s().st_file_read_failed, err)),
        }
    }

    pub fn linked_path(&self) -> Option<String> {
        self.editor
            .command_id
            .as_deref()
            .and_then(|id| self.command(id))
            .and_then(|command| command.script_path.clone())
    }

    // ---------- вспомогательное для истории изменений ----------

    /// Версия, которая пришла на смену изменению `index`.
    pub fn version_after(&self, index: usize) -> (String, Vec<Parameter>, String) {
        match self.changes.get(index + 1) {
            Some(next) => (
                next.old_script.clone(),
                next.old_params.clone(),
                next.old_comment.clone(),
            ),
            None => (
                self.command(self.editor.command_id.as_deref().unwrap_or_default())
                    .map(|c| c.script.clone())
                    .unwrap_or_default(),
                self.command(self.editor.command_id.as_deref().unwrap_or_default())
                    .map(|c| c.params.clone())
                    .unwrap_or_default(),
                self.command(self.editor.command_id.as_deref().unwrap_or_default())
                    .map(|c| c.comment.clone())
                    .unwrap_or_default(),
            ),
        }
    }

    /// Diff запущенной версии скрипта с текущей.
    pub fn log_vs_current(&self, log: &ExecutionLog) -> Vec<diff::DiffLine> {
        let current = self
            .command(&log.command_id)
            .map(|c| c.script.clone())
            .unwrap_or_default();
        diff::script_diff(&log.script, &current)
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_run(ui.ctx());

        ui::menu::menu_bar(self, ui);
        ui::tree::side_panel(self, ui);
        ui::status_bar(self, ui);
        ui::details::central_panel(self, ui);
        ui::dialogs::show(self, ui.ctx());
    }
}

/// Имя файла без разделителей пути — для предложенного имени при экспорте.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "command".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn is_json(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "import".to_owned())
}

/// Распознаёт файл экспорта команды: только `.json` с нужным набором полей.
pub fn parse_export(path: &Path, text: &str) -> Option<CommandExport> {
    is_json(path)
        .then(|| serde_json::from_str::<CommandExport>(text).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
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
        assert_eq!(command.script_path.as_deref(), Some(file.display().to_string().as_str()));

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

        assert_eq!(app.runs.len(), 2, "долгая команда не должна блокировать запуск другой");
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
        assert!(app.status.is_some_and(|s| !s.is_error), "должно быть сообщение, а не ошибка");

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

        assert!(!app.is_running(&fast_id), "быстрая команда должна была завершиться");
        assert!(
            app.is_running(&slow_id),
            "завершение одной команды не должно затрагивать другую (сдвиг индексов при удалении из Vec)"
        );

        if let Some(run) = app.runs.first_mut() {
            run.cancel();
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
}
