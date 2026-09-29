use egui_kittest::Harness;

use super::*;
use crate::i18n::RU;

#[derive(Default)]
struct DialogState {
    name: String,
    confirmed: usize,
}

fn harness() -> Harness<'static, DialogState> {
    let mut harness = Harness::new_ui_state(
        |ui, state: &mut DialogState| {
            if name_dialog(ui.ctx(), &RU, "Новая группа", "Имя группы", &mut state.name)
                == Outcome::Confirm
            {
                state.confirmed += 1;
            }
        },
        DialogState::default(),
    );
    harness.run();
    harness
}

#[test]
fn enter_confirms_the_typed_name() {
    let mut harness = harness();

    // Поле получает фокус само, поэтому просто печатаем.
    harness.event(egui::Event::Text("Деплой".to_owned()));
    harness.run();
    assert_eq!(harness.state().name, "Деплой");
    assert_eq!(harness.state().confirmed, 0);

    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().confirmed, 1, "Enter должен сохранять имя");
}

#[test]
fn enter_on_an_empty_name_keeps_the_dialog_usable() {
    let mut harness = harness();

    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(
        harness.state().confirmed,
        0,
        "пустое имя подтверждать нечем"
    );

    // Фокус должен вернуться в поле, иначе дальше печатать некуда.
    harness.event(egui::Event::Text("Стенды".to_owned()));
    harness.run();
    assert_eq!(harness.state().name, "Стенды");

    harness.key_press(egui::Key::Enter);
    harness.run();
    assert_eq!(harness.state().confirmed, 1);
}
