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

/// Часть текста строки в разбивке по словам — только для [`LineKind::Replaced`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    /// Совпадает со второй половиной пары — рисуется обычным серым «заменено».
    Common,
    /// Разошлось: на минус-строке — как удалено, на плюс-строке — как добавлено.
    Changed,
}

#[derive(Debug, Clone)]
pub struct Span {
    pub kind: SpanKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
    pub text: String,
    /// Только для [`LineKind::Replaced`], когда старая и новая строки сопоставлены
    /// один к одному: на какие куски разбит [`Self::text`] и что из них общее.
    /// `None` — либо строка не заменена, либо заменённых строк не поровну
    /// (тогда однозначного сопоставления слов нет, и вся строка красится целиком).
    pub spans: Option<Vec<Span>>,
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
/// остаются зелёными и красными. Если в блоке замены ровно одна старая строка
/// на одну новую — считается ещё и словесный diff внутри строки (см. [`word_spans`]),
/// чтобы в UI подсветить не всю строку целиком, а только изменившийся кусок.
pub fn script_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let diff = TextDiff::from_lines(old, new);
    let mut lines = Vec::new();

    for op in diff.ops() {
        if let DiffOp::Replace {
            old_index,
            old_len: 1,
            new_index,
            new_len: 1,
            ..
        } = *op
            && let Some((old_text, new_text)) = replace_pair(&diff, op)
        {
            let (old_spans, new_spans) = word_spans(&old_text, &new_text);
            lines.push(DiffLine {
                kind: LineKind::Replaced,
                old_no: Some(old_index + 1),
                new_no: None,
                text: old_text,
                spans: Some(old_spans),
            });
            lines.push(DiffLine {
                kind: LineKind::Replaced,
                old_no: None,
                new_no: Some(new_index + 1),
                text: new_text,
                spans: Some(new_spans),
            });
            continue;
        }

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
                spans: None,
            });
        }
    }
    lines
}

/// Старая и новая строка блока `Replace {old_len: 1, new_len: 1}` без концевого перевода строки.
///
/// `None`, если у `similar` вдруг не нашлось ровно одного удаления и одной вставки —
/// такого не бывает по определению `Replace`, но лучше отступить, чем ломать рендер.
fn replace_pair(diff: &TextDiff<'_, '_, str>, op: &DiffOp) -> Option<(String, String)> {
    let mut old_text = None;
    let mut new_text = None;
    for change in diff.iter_changes(op) {
        let text = change.value().trim_end_matches(['\n', '\r']).to_owned();
        match change.tag() {
            ChangeTag::Delete => old_text = Some(text),
            ChangeTag::Insert => new_text = Some(text),
            ChangeTag::Equal => return None,
        }
    }
    Some((old_text?, new_text?))
}

/// Словесный diff двух строк: на что разбить старую и что — новую, чтобы показать
/// рядом общие и разошедшиеся куски.
///
/// Делит по границам unicode-слов (буквы/цифры отдельно от пунктуации и пробелов),
/// а не по пробелам и не по символам — иначе, например, общее для обеих строк
/// `netodis_1_12` внутри `"hosts=netodis_1_12"` осталось бы незамеченным: посимвольный
/// diff то и дело находит случайные совпадения одной буквы и дробит строку на трудно
/// читаемые обрывки, а diff по пробельным «словам» не увидел бы `netodis_1_12` вовсе,
/// потому что в старой строке это часть более длинного куска без пробелов.
fn word_spans(old: &str, new: &str) -> (Vec<Span>, Vec<Span>) {
    let diff = TextDiff::from_unicode_words(old, new);
    let mut old_spans = Vec::new();
    let mut new_spans = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                push_span(&mut old_spans, SpanKind::Common, change.value());
                push_span(&mut new_spans, SpanKind::Common, change.value());
            }
            ChangeTag::Delete => push_span(&mut old_spans, SpanKind::Changed, change.value()),
            ChangeTag::Insert => push_span(&mut new_spans, SpanKind::Changed, change.value()),
        }
    }
    (old_spans, new_spans)
}

/// Дописывает кусок к последнему span'у того же типа — иначе на каждое слово
/// приходится свой кусок текста, и в UI выходит рябь из мелких перекрашенных букв
/// вместо читаемых связных участков.
fn push_span(spans: &mut Vec<Span>, kind: SpanKind, text: &str) {
    match spans.last_mut() {
        Some(last) if last.kind == kind => last.text.push_str(text),
        _ => spans.push(Span {
            kind,
            text: text.to_owned(),
        }),
    }
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
        spans: None,
    };

    for (index, param) in new.iter().enumerate() {
        match old.iter().find(|p| p.name == param.name) {
            None => lines.push(line(LineKind::Added, format_param(param), None, Some(index + 1))),
            Some(before) if before != param => {
                let old_text = format_param(before);
                let new_text = format_param(param);
                let (old_spans, new_spans) = word_spans(&old_text, &new_text);
                lines.push(DiffLine {
                    kind: LineKind::Replaced,
                    old_no: Some(index + 1),
                    new_no: None,
                    text: old_text,
                    spans: Some(old_spans),
                });
                lines.push(DiffLine {
                    kind: LineKind::Replaced,
                    old_no: None,
                    new_no: Some(index + 1),
                    text: new_text,
                    spans: Some(new_spans),
                });
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

    /// Формат кусков строки для сравнения в тестах: тип и сам текст.
    fn span_tags(spans: &[Span]) -> Vec<(SpanKind, &str)> {
        spans.iter().map(|s| (s.kind, s.text.as_str())).collect()
    }

    #[test]
    fn a_one_to_one_replace_highlights_only_the_changed_word() {
        let lines = script_diff("echo one\n", "echo two\n");
        let old_spans = lines[0]
            .spans
            .as_ref()
            .expect("минус-строка размечена по словам");
        let new_spans = lines[1]
            .spans
            .as_ref()
            .expect("плюс-строка размечена по словам");

        assert_eq!(
            span_tags(old_spans),
            [(SpanKind::Common, "echo "), (SpanKind::Changed, "one")]
        );
        assert_eq!(
            span_tags(new_spans),
            [(SpanKind::Common, "echo "), (SpanKind::Changed, "two")]
        );
    }

    #[test]
    fn a_shared_identifier_glued_to_different_text_on_both_sides_stays_common() {
        // Ровно случай со скриншота пользователя: `-e "hosts=X"` заменили на
        // `--limit X` — значение `netodis_1_12` не тронуто, хотя в старой строке
        // оно приклеено к кавычкам и `hosts=` без единого пробела между ними.
        let old = "ansible-playbook update.yml -e \"hosts=netodis_1_12\"\n";
        let new = "ansible-playbook update.yml --limit netodis_1_12\n";
        let lines = script_diff(old, new);
        assert_eq!(lines.len(), 2);

        let old_spans = lines[0].spans.as_ref().unwrap();
        let new_spans = lines[1].spans.as_ref().unwrap();
        let common = |spans: &[Span]| -> String {
            spans
                .iter()
                .filter(|s| s.kind == SpanKind::Common)
                .map(|s| s.text.as_str())
                .collect()
        };

        assert!(common(old_spans).contains("netodis_1_12"), "{old_spans:?}");
        assert!(common(new_spans).contains("netodis_1_12"), "{new_spans:?}");
        // А то, что реально разошлось, всё ещё помечено, а не потерялось в общем.
        assert!(
            old_spans
                .iter()
                .any(|s| s.kind == SpanKind::Changed && s.text.contains("hosts=")),
            "{old_spans:?}"
        );
        assert!(
            new_spans
                .iter()
                .any(|s| s.kind == SpanKind::Changed && s.text.contains("limit")),
            "{new_spans:?}"
        );
    }

    #[test]
    fn an_uneven_replace_block_falls_back_to_coloring_the_whole_line() {
        // Две строки заменили тремя — сопоставить их слово-в-слово нечем,
        // поэтому словесной разбивки нет и строка красится целиком, как раньше.
        let lines = script_diff("a\nb\n", "x\ny\nz\n");
        assert!(lines.iter().all(|l| l.kind == LineKind::Replaced));
        assert!(lines.iter().all(|l| l.spans.is_none()));
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

        // У заменённого значения параметра тоже есть словесная разбивка —
        // в UI подсветится только "1" / "2", а не вся строка "EDIT: String = …".
        let replaced: Vec<_> = lines
            .iter()
            .filter(|l| l.kind == LineKind::Replaced)
            .collect();
        assert_eq!(replaced.len(), 2);
        assert!(replaced.iter().all(|l| l.spans.is_some()));
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
