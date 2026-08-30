//! Построение построчного diff'а версий команды — как в git.

use egui::Color32;
use similar::{ChangeTag, DiffOp, TextDiff};

use crate::model::Parameter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Без изменений.
    Equal,
    /// Добавлено — зелёное.
    Added,
    /// Удалено — красное.
    Removed,
    /// Заменено — серое.
    Replaced,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
}

impl DiffLine {
    pub fn sign(&self) -> char {
        match self.kind {
            LineKind::Equal => ' ',
            LineKind::Added => '+',
            LineKind::Removed => '-',
            LineKind::Replaced => {
                if self.new_no.is_some() {
                    '+'
                } else {
                    '-'
                }
            }
        }
    }
}

/// Цвета текста и фона строки diff'а для текущей темы.
pub fn line_colors(kind: LineKind, dark_mode: bool) -> (Color32, Color32) {
    match (kind, dark_mode) {
        (LineKind::Equal, true) => (Color32::from_rgb(0xB0, 0xB6, 0xBE), Color32::TRANSPARENT),
        (LineKind::Equal, false) => (Color32::from_rgb(0x3A, 0x3F, 0x45), Color32::TRANSPARENT),
        (LineKind::Added, true) => (
            Color32::from_rgb(0x7E, 0xE7, 0x87),
            Color32::from_rgb(0x14, 0x33, 0x1D),
        ),
        (LineKind::Added, false) => (
            Color32::from_rgb(0x0F, 0x5D, 0x28),
            Color32::from_rgb(0xDA, 0xFB, 0xE1),
        ),
        (LineKind::Removed, true) => (
            Color32::from_rgb(0xFF, 0x93, 0x8C),
            Color32::from_rgb(0x3C, 0x16, 0x16),
        ),
        (LineKind::Removed, false) => (
            Color32::from_rgb(0x99, 0x14, 0x1B),
            Color32::from_rgb(0xFF, 0xEB, 0xE9),
        ),
        (LineKind::Replaced, true) => (
            Color32::from_rgb(0xA8, 0xB0, 0xB9),
            Color32::from_rgb(0x2A, 0x2E, 0x33),
        ),
        (LineKind::Replaced, false) => (
            Color32::from_rgb(0x5A, 0x62, 0x6B),
            Color32::from_rgb(0xEB, 0xEE, 0xF2),
        ),
    }
}

/// Построчный diff двух версий скрипта.
///
/// Блоки, которые `similar` считает заменой (`DiffOp::Replace`), помечаются
/// как [`LineKind::Replaced`] — в UI они серые, а чистые вставки и удаления
/// остаются зелёными и красными.
pub fn script_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();

    for op in diff.ops() {
        let replaced = matches!(op, DiffOp::Replace { .. });
        for change in diff.iter_changes(op) {
            let kind = match (change.tag(), replaced) {
                (ChangeTag::Equal, _) => LineKind::Equal,
                (_, true) => LineKind::Replaced,
                (ChangeTag::Insert, false) => LineKind::Added,
                (ChangeTag::Delete, false) => LineKind::Removed,
            };
            lines.push(DiffLine {
                kind,
                old_no: change.old_index().map(|i| i + 1),
                new_no: change.new_index().map(|i| i + 1),
                text: change.value().trim_end_matches(['\n', '\r']).to_owned(),
            });
        }
    }
    lines
}

/// Оставляет только изменённые участки с `context` строками контекста вокруг.
pub fn collapse_context(lines: &[DiffLine], context: usize) -> Vec<Option<DiffLine>> {
    let keep: Vec<bool> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if line.kind != LineKind::Equal {
                return true;
            }
            let from = i.saturating_sub(context);
            let to = (i + context).min(lines.len() - 1);
            lines[from..=to].iter().any(|l| l.kind != LineKind::Equal)
        })
        .collect();

    let mut out = Vec::new();
    let mut skipping = false;
    for (line, keep) in lines.iter().zip(keep) {
        if keep {
            skipping = false;
            out.push(Some(line.clone()));
        } else if !skipping {
            skipping = true;
            out.push(None); // разрыв «…»
        }
    }
    out
}

pub fn has_changes(lines: &[DiffLine]) -> bool {
    lines.iter().any(|l| l.kind != LineKind::Equal)
}

fn format_param(param: &Parameter) -> String {
    format!(
        "{}: {} = {}",
        param.name,
        param.param_type.label(),
        param.display_value()
    )
}

/// Diff набора параметров: сопоставление по имени.
pub fn params_diff(old: &[Parameter], new: &[Parameter]) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let line = |kind: LineKind, text: String, old_no, new_no| DiffLine {
        kind,
        old_no,
        new_no,
        text,
    };

    for (index, param) in new.iter().enumerate() {
        match old.iter().find(|p| p.name == param.name) {
            None => lines.push(line(LineKind::Added, format_param(param), None, Some(index + 1))),
            Some(before) if before != param => {
                lines.push(line(LineKind::Replaced, format_param(before), Some(index + 1), None));
                lines.push(line(LineKind::Replaced, format_param(param), None, Some(index + 1)));
            }
            Some(_) => lines.push(line(
                LineKind::Equal,
                format_param(param),
                Some(index + 1),
                Some(index + 1),
            )),
        }
    }

    for (index, param) in old.iter().enumerate() {
        if !new.iter().any(|p| p.name == param.name) {
            lines.push(line(
                LineKind::Removed,
                format_param(param),
                Some(index + 1),
                None,
            ));
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ParamType;

    #[test]
    fn insertion_is_green_and_deletion_is_red() {
        let lines = script_diff("a\nb\n", "a\nb\nc\n");
        assert_eq!(lines.last().unwrap().kind, LineKind::Added);

        let lines = script_diff("a\nb\nc\n", "a\nc\n");
        assert!(lines.iter().any(|l| l.kind == LineKind::Removed && l.text == "b"));
    }

    #[test]
    fn modified_line_is_gray() {
        let lines = script_diff("echo one\n", "echo two\n");
        assert!(lines.iter().all(|l| l.kind == LineKind::Replaced));
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn params_diff_detects_add_remove_and_change() {
        let old = vec![
            Parameter::new("KEEP"),
            Parameter::new("DROP"),
            Parameter {
                name: "EDIT".into(),
                value: "1".into(),
                param_type: ParamType::String,
            },
        ];
        let new = vec![
            Parameter::new("KEEP"),
            Parameter {
                name: "EDIT".into(),
                value: "2".into(),
                param_type: ParamType::String,
            },
            Parameter::new("ADD"),
        ];
        let lines = params_diff(&old, &new);
        let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
        assert!(kinds.contains(&LineKind::Added));
        assert!(kinds.contains(&LineKind::Removed));
        assert!(kinds.contains(&LineKind::Replaced));
    }

    #[test]
    fn context_collapses_long_equal_runs() {
        let old = (0..30).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let new = old.replace("line 15", "line 15 изменена");
        let lines = script_diff(&old, &new);
        let collapsed = collapse_context(&lines, 3);
        assert!(collapsed.len() < lines.len());
        assert!(collapsed.iter().any(Option::is_none));
    }
}
