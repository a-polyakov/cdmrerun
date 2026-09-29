use super::*;
use crate::model::ParamType;

#[test]
fn insertion_is_green_and_deletion_is_red() {
    let lines = script_diff("a\nb\n", "a\nb\nc\n");
    assert_eq!(lines.last().unwrap().kind, LineKind::Added);

    let lines = script_diff("a\nb\nc\n", "a\nc\n");
    assert!(
        lines
            .iter()
            .any(|l| l.kind == LineKind::Removed && l.text == "b")
    );
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
    let old = (0..30)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let new = old.replace("line 15", "line 15 изменена");
    let lines = script_diff(&old, &new);
    let collapsed = collapse_context(&lines, 3);
    assert!(collapsed.len() < lines.len());
    assert!(collapsed.iter().any(Option::is_none));
}
