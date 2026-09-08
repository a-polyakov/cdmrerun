//! Доменная модель приложения.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Тип параметра (по мотивам параметров сборки Jenkins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamType {
    /// Однострочная строка.
    String,
    /// Многострочный текст.
    Text,
    /// Флаг: "true" / "false".
    Boolean,
    /// Выбор одного значения из списка.
    Choice(Vec<String>),
    /// Строка, скрытая в UI и в истории запусков.
    Password,
}

impl ParamType {
    pub const KINDS: [&'static str; 5] = ["String", "Text", "Boolean", "Choice", "Password"];

    pub fn kind_index(&self) -> usize {
        match self {
            Self::String => 0,
            Self::Text => 1,
            Self::Boolean => 2,
            Self::Choice(_) => 3,
            Self::Password => 4,
        }
    }

    pub fn label(&self) -> &'static str {
        Self::KINDS[self.kind_index()]
    }

    /// Создаёт тип по индексу в [`ParamType::KINDS`], сохраняя варианты выбора.
    pub fn from_kind_index(index: usize, choices: Vec<String>) -> Self {
        match index {
            1 => Self::Text,
            2 => Self::Boolean,
            3 => Self::Choice(choices),
            4 => Self::Password,
            _ => Self::String,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    /// Значение по умолчанию, подставляемое в форму запуска.
    pub value: String,
    #[serde(rename = "type")]
    pub param_type: ParamType,
}

impl Parameter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: String::new(),
            param_type: ParamType::String,
        }
    }

    /// Значение для показа в UI и в истории: пароли не раскрываем.
    pub fn display_value(&self) -> String {
        if self.param_type == ParamType::Password && !self.value.is_empty() {
            "********".to_owned()
        } else {
            self.value.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub name: String,
    pub script: String,
    pub comment: String,
    pub params: Vec<Parameter>,
    /// Идентификатор родительской папки; `None` — корень дерева.
    pub parent_id: Option<String>,
    /// Порядок среди соседей: он же «номер» узла в дереве.
    #[serde(default)]
    pub order: i32,
    /// Связь с файлом на диске: скрипт читается оттуда и туда же сохраняется.
    #[serde(default)]
    pub script_path: Option<String>,
}

impl Command {
    pub fn new(name: impl Into<String>, parent_id: Option<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            script: "#!/bin/sh\necho \"Привет!\"\n".to_owned(),
            comment: String::new(),
            params: Vec::new(),
            parent_id,
            order: 0,
            script_path: None,
        }
    }

    /// Данные для файла экспорта (без идентификаторов и места в дереве).
    pub fn to_export(&self) -> CommandExport {
        CommandExport {
            name: self.name.clone(),
            script: self.script.clone(),
            comment: self.comment.clone(),
            params: self.params.clone(),
        }
    }
}

/// Формат файла экспорта команды.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExport {
    pub name: String,
    pub script: String,
    pub comment: String,
    pub params: Vec<Parameter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    /// Порядок среди соседей: он же «номер» узла в дереве.
    #[serde(default)]
    pub order: i32,
}

impl Folder {
    pub fn new(name: impl Into<String>, parent_id: Option<String>) -> Self {
        Self {
            id: new_id(),
            name: name.into(),
            parent_id,
            order: 0,
        }
    }
}

/// Пометка строки stderr в объединённом выводе.
///
/// Вывод склеен в одну ленту в порядке появления строк — иначе непонятно,
/// на каком шаге команда ругнулась. Чтобы это было видно и в файле истории,
/// а не только в UI, строки ошибок помечены символом.
pub const ERR_MARK: &str = "! ";
/// Пометка обычной строки: тот же отступ, что и у ошибки, но без символа.
pub const OUT_MARK: &str = "  ";

/// Дописывает строку вывода, пометив ошибку символом.
pub fn push_output_line(output: &mut String, line: &str, is_error: bool) {
    output.push_str(if is_error { ERR_MARK } else { OUT_MARK });
    output.push_str(line);
    output.push('\n');
}

/// Разбирает объединённый вывод на строки; `true` — строка пришла из stderr.
pub fn output_lines(output: &str) -> Vec<(bool, &str)> {
    output
        .lines()
        .map(|line| match line.strip_prefix(ERR_MARK) {
            Some(text) => (true, text),
            None => (false, line.strip_prefix(OUT_MARK).unwrap_or(line)),
        })
        .collect()
}

/// Вывод без пометок — то, что уходит в буфер обмена.
pub fn plain_output(output: &str) -> String {
    let mut plain = String::with_capacity(output.len());
    for (_, text) in output_lines(output) {
        plain.push_str(text);
        plain.push('\n');
    }
    plain
}

/// Запись истории запусков: что именно выполнялось и что получилось.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLog {
    pub id: String,
    pub command_id: String,
    /// Скрипт с уже подставленными параметрами — та самая версия, что была запущена.
    pub script: String,
    /// Объединённый вывод stdout и stderr в порядке появления;
    /// строки stderr помечены [`ERR_MARK`].
    #[serde(default)]
    pub output: String,
    pub exit_code: Option<i32>,
    pub start_time: DateTime<Local>,
    pub end_time: Option<DateTime<Local>>,
    /// Значения параметров запуска (пароли замаскированы).
    #[serde(default)]
    pub params: Vec<Parameter>,
}

impl ExecutionLog {
    pub fn duration_secs(&self) -> Option<f64> {
        self.end_time
            .map(|end| (end - self.start_time).num_milliseconds() as f64 / 1000.0)
    }

    pub fn is_success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Запись истории изменений — полный снимок предыдущей версии команды.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeLog {
    pub id: String,
    pub command_id: String,
    pub old_script: String,
    #[serde(alias = "params")]
    pub old_params: Vec<Parameter>,
    #[serde(default)]
    pub old_name: String,
    #[serde(default)]
    pub old_comment: String,
    pub timestamp: DateTime<Local>,
}

impl ChangeLog {
    /// Снимает текущее состояние команды как «предыдущую версию».
    pub fn snapshot(command: &Command) -> Self {
        Self {
            id: new_id(),
            command_id: command.id.clone(),
            old_script: command.script.clone(),
            old_params: command.params.clone(),
            old_name: command.name.clone(),
            old_comment: command.comment.clone(),
            timestamp: Local::now(),
        }
    }
}

/// Что выбрано в дереве слева.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Selection {
    Folder(String),
    Command(String),
}

impl Selection {
    pub fn id(&self) -> &str {
        match self {
            Self::Folder(id) | Self::Command(id) => id,
        }
    }
}

/// Настройки приложения и состояние интерфейса между запусками.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub lang: crate::i18n::Lang,
    /// Что было выбрано в дереве при прошлом выходе.
    #[serde(default)]
    pub selection: Option<Selection>,
    /// Раскрытые группы.
    #[serde(default)]
    pub expanded: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_lines_are_marked_in_the_merged_output() {
        let mut output = String::new();
        push_output_line(&mut output, "собираю", false);
        push_output_line(&mut output, "warning: нет файла", true);
        push_output_line(&mut output, "готово", false);

        // В файле истории строка ошибки помечена символом.
        assert_eq!(output, "  собираю\n! warning: нет файла\n  готово\n");
        assert_eq!(
            output_lines(&output),
            [
                (false, "собираю"),
                (true, "warning: нет файла"),
                (false, "готово")
            ]
        );
        // В буфер обмена уходит вывод без пометок.
        assert_eq!(
            plain_output(&output),
            "собираю\nwarning: нет файла\nготово\n"
        );
    }
}
