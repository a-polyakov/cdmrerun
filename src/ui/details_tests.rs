use egui_kittest::Harness;

use super::*;

/// Рисует редактор размером 280×200 и возвращает прямоугольники
/// поля ввода и всей рамки.
fn layout(script: &str) -> (egui::Rect, egui::Rect) {
    let mut script = script.to_owned();
    let mut edit = egui::Rect::NOTHING;
    let mut outer = egui::Rect::NOTHING;
    let mut harness = Harness::builder()
        .with_size(egui::vec2(300.0, 400.0))
        .build_ui(|ui| {
            let before = ui.cursor().min;
            edit = script_editor(ui, &mut script, egui::vec2(280.0, 200.0)).rect;
            outer = egui::Rect::from_min_max(before, ui.min_rect().max);
        });
    harness.run();
    // Замыкание держит `edit` и `outer`, пока жив harness.
    drop(harness);
    (edit, outer)
}

/// Длинная строка не переносится: поле раздвигается вправо под прокруткой,
/// а рамка остаётся заданного размера.
#[test]
fn long_script_line_is_not_wrapped() {
    let line = "ansible netodis_1_12 -b -a \"shutdown -h now\" ".repeat(10);
    let (edit, outer) = layout(&line);
    // Поле шире строки в 280px — значит, строка не перенесена.
    assert!(edit.width() > 600.0, "поле должно раздвинуться: {edit:?}");
    assert!(
        outer.width() <= 281.0 && outer.height() <= 201.0,
        "рамка не растёт: {outer:?}"
    );
}

/// Короткий скрипт — поле всё равно занимает всё отведённое место.
#[test]
fn short_script_fills_the_given_size() {
    let (edit, outer) = layout("echo hi");
    assert!(
        (outer.width() - 280.0).abs() < 1.0 && (outer.height() - 200.0).abs() < 1.0,
        "{outer:?}"
    );
    // Поле по высоте — целое число строк, так что может не хватать меньше строки.
    let row = 20.0;
    assert!(
        edit.width() >= 270.0 && edit.height() >= 192.0 - row,
        "{edit:?}"
    );
}
