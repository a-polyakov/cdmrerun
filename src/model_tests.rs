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
