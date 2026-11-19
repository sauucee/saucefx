//! Packet 018: the effect value model must keep its identity across `sync`.
//!
//! Slint's `Repeater` resets every instantiated row when the model instance it
//! observes is replaced. `UiBinding::sync` used to build a brand new `ModelRc`
//! from a slice on every call, so each edit destroyed the rows that were holding
//! the pointer grab, the double-click pair and the keyboard focus. These tests
//! pin the identity that makes real drag/key/reset gestures survive, without
//! freezing the values themselves.

use saucefx_ui::AppWindow;
use saucefx_ui::binding::{EditOutcome, UiBinding};
use slint::ComponentHandle;
use slint::Model;

/// Push the bound state into the window exactly like the real callbacks do.
fn sync(binding: &std::rc::Rc<std::cell::RefCell<UiBinding>>, window: &AppWindow) {
    binding.borrow().sync(window);
}

// The winit backend allows one event loop per process, so every window this
// test needs is created inside the single test function (same rule as view.rs).
/// Scenario 1: model identity and value delivery across syncs and two windows.
fn scenario_model_identity() {
    let window = AppWindow::new().expect("window");
    let binding = UiBinding::install(&window);
    sync(&binding, &window);

    let installed = window.get_effect_values();
    assert_eq!(installed.row_count(), 5, "five effect rows");

    // 1. A sync that changes nothing must not replace the model instance.
    sync(&binding, &window);
    assert_eq!(
        window.get_effect_values(),
        installed,
        "an unchanged sync replaced the effect model, which resets every repeater row"
    );

    // 2. A real edit must update the value through the same instance.
    let outcome = binding.borrow_mut().set_effect(1, 5.0);
    assert_eq!(outcome, EditOutcome::Applied, "edit accepted");
    sync(&binding, &window);
    assert_eq!(
        window.get_effect_values(),
        installed,
        "a changed-row sync replaced the effect model instead of notifying the row"
    );
    assert_eq!(
        window.get_effect_values().row_data(1),
        Some(5.0),
        "the changed row reached the model"
    );
    assert_eq!(
        window.get_effect_values().row_data(0),
        Some(0.0),
        "untouched rows keep their value"
    );

    // 3. Unrelated view updates (EQ gain, settings) must not churn the model
    //    either; the effect rows have to outlive them.
    assert_eq!(
        binding.borrow_mut().set_eq_gain(3, -2.0),
        EditOutcome::Applied
    );
    sync(&binding, &window);
    assert_eq!(
        window.get_effect_values(),
        installed,
        "an EQ edit replaced the effect model"
    );
    binding.borrow_mut().open_settings();
    sync(&binding, &window);
    assert_eq!(
        window.get_effect_values(),
        installed,
        "opening Settings replaced the effect model"
    );
    assert_eq!(window.get_effect_values().row_data(1), Some(5.0));

    // 4. Reset keeps the instance as well.
    assert_eq!(binding.borrow_mut().reset_effect(1), EditOutcome::Applied);
    sync(&binding, &window);
    assert_eq!(window.get_effect_values(), installed);
    assert_eq!(window.get_effect_values().row_data(1), Some(0.0));

    // 5. Two windows must not share the model instance, and an edit in one
    //    must not leak into the other.
    let first = AppWindow::new().expect("first window");
    let first_binding = UiBinding::install(&first);
    let second = AppWindow::new().expect("second window");
    let second_binding = UiBinding::install(&second);
    sync(&first_binding, &first);
    sync(&second_binding, &second);

    let first_model = first.get_effect_values();
    let second_model = second.get_effect_values();
    assert_ne!(
        first_model, second_model,
        "two windows must not share one effect model instance"
    );

    assert_eq!(
        first_binding.borrow_mut().set_effect(4, 10.0),
        EditOutcome::Applied
    );
    sync(&first_binding, &first);
    sync(&second_binding, &second);
    assert_eq!(first.get_effect_values().row_data(4), Some(10.0));
    assert_eq!(
        second.get_effect_values().row_data(4),
        Some(0.0),
        "editing one window leaked into another window's model"
    );
    assert_eq!(first.get_effect_values(), first_model);
    assert_eq!(second.get_effect_values(), second_model);
}

// --- real bound-window gestures -------------------------------------------
//
// Dispatched through Slint's own window-event path (same mechanism as
// tests/view.rs), so the pointer grab, the double-click pair and the keyboard
// focus are the ones the real window uses. Coordinates are the protected
// reference geometry: row i centre 353.59 + 35*i, effect rail x=134..247.

const ROW0_Y: f32 = 353.59;
const RAIL_MID: f32 = 190.0;
const RAIL_MAX: f32 = 246.0;

fn pointer_move(window: &AppWindow, x: f32, y: f32) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
}

fn pointer_press(window: &AppWindow, x: f32, y: f32) {
    pointer_move(window, x, y);
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Left,
        });
}

fn pointer_release(window: &AppWindow, x: f32, y: f32) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button: slint::platform::PointerEventButton::Left,
        });
}

fn click_at(window: &AppWindow, x: f32, y: f32) {
    pointer_press(window, x, y);
    pointer_release(window, x, y);
}

fn key(window: &AppWindow, text: char) {
    window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: text.to_string().into(),
        });
}

fn effect(window: &AppWindow, index: usize) -> Option<f32> {
    window.get_effect_values().row_data(index)
}

fn sync_now(binding: &std::rc::Rc<std::cell::RefCell<UiBinding>>, window: &AppWindow) {
    binding.borrow().sync(window);
}

/// Scenario 2: real bound-window gestures, modal suppression and power off.
fn scenario_gestures() {
    let window = AppWindow::new().expect("window");
    let binding = UiBinding::install(&window);
    sync_now(&binding, &window);
    // Captured before the gestures so the identity assertion below compares
    // against a real earlier model, not against the getter itself.
    let installed = window.get_effect_values();

    // 1. One held drag must follow every move, not just the first step.
    pointer_press(&window, 135.0, ROW0_Y);
    for (x, want) in [
        (157.0, 2.0),
        (RAIL_MID, 5.0),
        (224.0, 8.0),
        (RAIL_MAX, 10.0),
    ] {
        pointer_move(&window, x, ROW0_Y);
        assert_eq!(effect(&window, 0), Some(want), "held drag reached x={x}");
    }
    pointer_release(&window, RAIL_MAX, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        Some(10.0),
        "release keeps the dragged value"
    );

    // 2. Keyboard steps keep working after the value edits that used to
    //    rebuild the row and drop focus.
    click_at(&window, RAIL_MID, ROW0_Y);
    assert_eq!(effect(&window, 0), Some(5.0), "click sets the midpoint");
    for (step, want) in [
        ('\u{f703}', 6.0),
        ('\u{f703}', 7.0),
        ('\u{f702}', 6.0),
        ('\u{f701}', 5.0),
        ('\u{f700}', 6.0),
    ] {
        key(&window, step);
        assert_eq!(
            effect(&window, 0),
            Some(want),
            "arrow step kept focus and applied"
        );
    }
    // The reference range input jumps to its limits: Home/PageDown to 0 and
    // End/PageUp to 10, with focus retained after every jump.
    key(&window, '\u{f729}'); // Home
    assert_eq!(effect(&window, 0), Some(0.0), "Home drops to the minimum");
    key(&window, '\u{f72c}'); // PageUp
    assert_eq!(
        effect(&window, 0),
        Some(10.0),
        "PageUp jumps to the maximum"
    );
    key(&window, '\u{f72d}'); // PageDown
    assert_eq!(
        effect(&window, 0),
        Some(0.0),
        "PageDown drops to the minimum"
    );
    key(&window, '\u{f72b}'); // End
    assert_eq!(effect(&window, 0), Some(10.0), "End jumps to the maximum");
    assert_eq!(
        window.get_effect_values(),
        installed,
        "the model stays a single instance"
    );

    // 3. Double-click resets only its own row. The delays keep the earlier
    //    clicks out of the double-click interval so the pair under test is the
    //    one that fires.
    assert_eq!(
        binding.borrow_mut().set_effect(1, 7.0),
        EditOutcome::Applied
    );
    sync_now(&binding, &window);
    std::thread::sleep(std::time::Duration::from_millis(700));
    click_at(&window, RAIL_MID, ROW0_Y);
    std::thread::sleep(std::time::Duration::from_millis(700));
    click_at(&window, RAIL_MID, ROW0_Y);
    click_at(&window, RAIL_MID, ROW0_Y);
    assert_eq!(effect(&window, 0), Some(0.0), "double-click reset row 0");
    assert_eq!(
        effect(&window, 1),
        Some(7.0),
        "row 1 untouched by the reset"
    );

    // 4. A modal opened while the pointer is down must swallow the rest of the
    //    gesture without changing the value, and editing must return after it
    //    closes.
    pointer_press(&window, RAIL_MID, ROW0_Y);
    pointer_move(&window, 224.0, ROW0_Y);
    let held = effect(&window, 0);
    assert_eq!(held, Some(8.0), "value follows before the modal opens");
    binding.borrow_mut().open_settings();
    sync_now(&binding, &window);
    assert!(window.get_settings_open(), "modal premise: settings open");
    pointer_move(&window, RAIL_MAX, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        held,
        "move while modal open changed the value"
    );
    pointer_release(&window, RAIL_MAX, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        held,
        "release while modal open changed the value"
    );
    binding.borrow_mut().close_settings();
    sync_now(&binding, &window);
    assert!(
        !window.get_settings_open(),
        "modal premise: settings closed"
    );
    pointer_press(&window, 135.0, ROW0_Y);
    pointer_move(&window, RAIL_MID, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        Some(5.0),
        "new gesture works after the modal"
    );
    pointer_release(&window, RAIL_MID, ROW0_Y);

    // 5. Same for processing off: the retained grab must not edit.
    pointer_press(&window, RAIL_MID, ROW0_Y);
    pointer_move(&window, 224.0, ROW0_Y);
    let before_off = effect(&window, 0);
    assert_eq!(before_off, Some(8.0), "value follows before power off");
    binding.borrow_mut().toggle_power();
    sync_now(&binding, &window);
    assert!(!window.get_power(), "power premise: processing off");
    pointer_move(&window, RAIL_MAX, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        before_off,
        "move while powered off changed the value"
    );
    pointer_release(&window, RAIL_MAX, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        before_off,
        "release while powered off changed the value"
    );
    binding.borrow_mut().toggle_power();
    sync_now(&binding, &window);
    assert!(window.get_power(), "power premise: processing on");
    pointer_press(&window, 135.0, ROW0_Y);
    pointer_move(&window, RAIL_MID, ROW0_Y);
    assert_eq!(
        effect(&window, 0),
        Some(5.0),
        "new gesture works after power returns"
    );
    pointer_release(&window, RAIL_MID, ROW0_Y);
}

// The winit backend allows one event loop per process and every `#[test]` gets
// its own thread, so the scenarios run in ONE Slint-thread test. No assertion
// above is relaxed by this: each scenario is the whole test it used to be.
#[test]
fn effects_continuity_scenarios() {
    scenario_model_identity();
    scenario_gestures();
}
