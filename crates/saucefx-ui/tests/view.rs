//! Bound-view regressions for the confirmed Packet 008/010 modal and EQ
//! defects.
//!
//! These checks dispatch real Slint window events and read the same properties
//! the window paints, so a source-substring change cannot satisfy them.
//!
//! EXPECTED BASELINE FAILURE: the application Slint repairs are owned by
//! free-pi (`app-window.slint`, `settings.slint`, `save-preset.slint`,
//! `header.slint`) and Freebuff (`equalizer.slint`). Until those patches are
//! integrated this test is expected to stay red for the genuine outside-owner
//! defects. Every assertion below is the protected React reference behaviour;
//! none is weakened to manufacture green. The harness records every failing
//! scenario and reports them together, so the exact failing branch/sequence is
//! visible instead of stopping at the first.
//!
//! The winit backend allows one event loop per process, so all view-level
//! checks share a single window in one test.

use std::cell::RefCell;
use std::rc::Rc;

use saucefx_core::settings::ThemeMode;
use saucefx_core::state::{BAND_COUNT, EFFECT_COUNT};
use saucefx_ui::AppWindow;
use saucefx_ui::binding::UiBinding;
use slint::ComponentHandle;
use slint::platform::{PointerEventButton, WindowEvent};

/// Absolute x of the drawn EQ chart, taken from the protected reference DOM
/// (`visual-013/reference-013-rects.json`): the `.eq-chart` box starts at 324
/// and is 655 wide. The panel's 1px border is part of the child inset, so this
/// is a measured source value, not an arithmetic guess from the padding chain.
const EQ_CHART_X: f32 = 324.0;
/// Reference `.eq-chart` width (1002 panel row - 308 panel left - 1px border
/// - 15 pad - 1px border - 26 scale reserve).
const EQ_CHART_WIDTH: f32 = 655.0;
/// `Layout.eq-bands`.
const EQ_BANDS: usize = BAND_COUNT;
/// A y inside the drawn chart (chart spans roughly y=344..530).
const EQ_Y: f32 = 429.0;

fn press(window: &AppWindow, text: &str) {
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn move_to(window: &AppWindow, x: f32, y: f32) {
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    });
}

fn click(window: &AppWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: PointerEventButton::Left,
        });
}

fn report(window: &AppWindow) -> String {
    window.get_focus_report().to_string()
}

fn readout(window: &AppWindow) -> String {
    window.get_eq_readout().to_string()
}

/// Let Slint finish the layout/focus pass that a visibility change schedules.
/// The first-open probe shows the dialog focus is synchronous, but a reopen
/// after a reset needs the queued layout pass before `fs.focus()` sticks.
fn settle() {
    slint::platform::update_timers_and_animations();
}

fn sync(window: &AppWindow, binding: &Rc<RefCell<UiBinding>>) {
    binding.borrow().sync(window);
    settle();
}

fn open_settings(window: &AppWindow, binding: &Rc<RefCell<UiBinding>>) {
    binding.borrow_mut().open_settings();
    sync(window, binding);
}

fn open_save(window: &AppWindow, binding: &Rc<RefCell<UiBinding>>) {
    binding.borrow_mut().open_save();
    sync(window, binding);
}

/// Collects failures instead of panicking on the first one, so one red run
/// reports every confirmed defect that is still present.
struct Checks {
    failures: Vec<String>,
}

impl Checks {
    fn new() -> Self {
        Self {
            failures: Vec::new(),
        }
    }

    fn is(&mut self, label: &str, ok: bool, detail: impl Into<String>) {
        if !ok {
            self.failures.push(format!("{label}: {}", detail.into()));
        }
    }

    fn eq<T: PartialEq + std::fmt::Debug>(&mut self, label: &str, actual: T, expected: T) {
        if actual != expected {
            self.failures
                .push(format!("{label}: expected {expected:?}, got {actual:?}"));
        }
    }

    fn finish(self) {
        if !self.failures.is_empty() {
            panic!(
                "EXPECTED BASELINE FAILURES — {} confirmed Packet 008/010 defect(s) still present \
                 (free-pi/Freebuff Slint patches not integrated):\n{}",
                self.failures.len(),
                self.failures.join("\n")
            );
        }
    }
}

/// Return the window to the clean startup state between scenarios.
fn reset(window: &AppWindow, binding: &Rc<RefCell<UiBinding>>) {
    {
        let mut state = binding.borrow_mut();
        if state.save_open() {
            state.close_save();
        }
        if state.settings_open() {
            state.close_settings();
        }
        if state.preset_menu_open() {
            state.toggle_preset_menu();
        }
        if state.output_menu_open() {
            state.toggle_output_menu();
        }
        if !state.power() {
            state.toggle_power();
        }
        if state.theme() == ThemeMode::Light {
            state.set_light(false);
        }
        state.reset_all();
        state.set_active_eq_band(None);
        state.sync(window);
    }
    settle();
    // Park the pointer outside every hit area so hover state is clean.
    move_to(window, 20.0, 20.0);
    settle();
}

/// Startup is checked before any reset: a reset-before-startup would hide a
/// real first-paint problem.
fn scenario_startup(window: &AppWindow, binding: &Rc<RefCell<UiBinding>>, checks: &mut Checks) {
    checks.eq("startup readout", readout(window), String::new());
    checks.eq(
        "startup version parity",
        window.get_app_version().to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );
    checks.eq(
        "startup active band",
        binding.borrow().active_eq_band(),
        None,
    );
    checks.is(
        "startup Settings closed",
        !binding.borrow().settings_open(),
        "settings_open() is true at startup",
    );
    checks.is(
        "startup Save closed",
        !binding.borrow().save_open(),
        "save_open() is true at startup",
    );

    // Initial populated readout -> clear -> empty window.
    for (band, expected) in [
        (1, "25 Hz \u{b7} 0 dB"),
        (17, "1 kHz \u{b7} 0 dB"),
        (30, "20 kHz \u{b7} 0 dB"),
    ] {
        binding.borrow_mut().set_active_eq_band(Some(band));
        sync(window, binding);
        checks.eq(
            &format!("readout for band {band}"),
            readout(window),
            expected.to_string(),
        );
    }
    binding.borrow_mut().set_active_eq_band(None);
    sync(window, binding);
    checks.eq("readout clears", readout(window), String::new());
    checks.eq(
        "active band clears",
        binding.borrow().active_eq_band(),
        None,
    );

    // A captured preset keeps fractional EQ values; the readout must format
    // them with the reference half-rounding rule and must not round the state.
    let mut captured = [0.0f32; BAND_COUNT];
    captured[17] = -1.25;
    checks.is(
        "captured preset applies",
        binding
            .borrow_mut()
            .apply_captured_values(&[0.0; EFFECT_COUNT], &captured),
        "apply_captured_values returned false",
    );
    binding.borrow_mut().set_active_eq_band(Some(17));
    sync(window, binding);
    checks.eq(
        "negative half readout",
        readout(window),
        "1 kHz \u{b7} -1.2 dB".to_string(),
    );
    checks.eq(
        "captured gain keeps its precision",
        binding.borrow().eq_gains()[17],
        -1.25f32,
    );
}

fn scenario_settings_keyboard(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    checks.is(
        "Settings opens",
        binding.borrow().settings_open(),
        "settings_open() is false",
    );
    checks.eq(
        "Settings focus starts on Close",
        report(window),
        "settings:0".to_string(),
    );

    press(window, "\t");
    checks.eq(
        "Tab moves to Dark",
        report(window),
        "settings:1".to_string(),
    );
    press(window, "\t");
    checks.eq(
        "Tab moves to Light",
        report(window),
        "settings:2".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Backtab returns to Dark",
        report(window),
        "settings:1".to_string(),
    );

    press(window, "\t");
    press(window, "\n");
    checks.eq(
        "Enter on Light sets the Light theme",
        binding.borrow().theme(),
        ThemeMode::Light,
    );
    checks.is(
        "bound light property follows",
        window.get_light(),
        "get_light() is false",
    );

    press(window, "\u{19}");
    press(window, "\n");
    checks.eq(
        "Enter on Dark sets the Dark theme",
        binding.borrow().theme(),
        ThemeMode::Dark,
    );

    press(window, "\u{1b}");
    checks.is(
        "Escape closes Settings",
        !binding.borrow().settings_open(),
        "settings_open() is still true",
    );
}

fn scenario_save_success(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);

    // Open Save the way a person does: the Settings card's own button.
    click(window, 702.0, 353.0);
    settle();
    checks.is(
        "pointer Settings Save opener opens Save",
        binding.borrow().save_open(),
        "save_open() is false after the pointer opener",
    );

    binding.borrow_mut().set_save_name("");
    sync(window, binding);
    click(window, 500.0, 283.0);
    settle();
    checks.eq(
        "name field is focused",
        report(window),
        "save:input".to_string(),
    );
    press(window, "\t");
    checks.eq(
        "Tab from input reaches Cancel",
        report(window),
        "save:cancel".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Backtab from Cancel returns to input",
        report(window),
        "save:input".to_string(),
    );

    // Invalid name -> actual Enter -> Save remains open.
    press(window, "\u{000a}");
    checks.is(
        "invalid name must not save",
        binding.borrow().save_open(),
        "save_open() is false after Enter on an invalid name",
    );

    binding.borrow_mut().set_save_name("Evening listen");
    sync(window, binding);
    checks.is(
        "valid name is accepted",
        binding.borrow().save_valid(),
        "save_valid() is false",
    );
    press(window, "\u{000a}");
    checks.is(
        "valid Enter closes Save",
        !binding.borrow().save_open(),
        "save_open() is still true",
    );
    checks.is(
        "valid save closes Settings too",
        !binding.borrow().settings_open(),
        "settings_open() is still true",
    );
    checks.is(
        "valid save selects the new preset",
        binding.borrow().preset_name().contains("Evening listen"),
        format!("preset is {}", binding.borrow().preset_name()),
    );
}

fn scenario_save_escape(window: &AppWindow, binding: &Rc<RefCell<UiBinding>>, checks: &mut Checks) {
    reset(window, binding);
    open_settings(window, binding);
    click(window, 702.0, 353.0);
    settle();
    click(window, 500.0, 283.0);
    settle();
    press(window, "\u{1b}");
    checks.is(
        "Escape closes Save",
        !binding.borrow().save_open(),
        "save_open() is still true",
    );
    checks.is(
        "Escape keeps Settings",
        binding.borrow().settings_open(),
        "Settings was dismissed with Save",
    );
    let focus = report(window);
    checks.is(
        "Escape returns focus to Settings",
        focus.starts_with("settings:") && focus != "settings:none",
        format!("got {focus}"),
    );
}

fn scenario_click_scoping(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);

    // A blank part of the Settings card is not a control, but it must keep the
    // dialog open instead of falling through to the backdrop.
    click(window, 500.0, 320.0);
    settle();
    checks.is(
        "blank Settings card click stays open",
        binding.borrow().settings_open(),
        "Settings dismissed on an inside click",
    );

    click(window, 40.0, 480.0);
    settle();
    checks.is(
        "outside click closes Settings",
        !binding.borrow().settings_open(),
        "settings_open() is still true",
    );
    checks.eq(
        "closed Settings reports no modal focus",
        report(window),
        "none".to_string(),
    );
}

fn scenario_background_unreachable(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    let preset = binding.borrow().preset_name().to_string();
    let effects = binding.borrow().effects();
    let eq = binding.borrow().eq_gains();
    click(window, 150.0, 112.0);
    settle();
    checks.is(
        "outside click closes Settings",
        !binding.borrow().settings_open(),
        "settings_open() is still true",
    );
    checks.is(
        "outside click does not open the preset menu",
        !binding.borrow().preset_menu_open(),
        "preset_menu_open() is true",
    );
    checks.eq(
        "outside click does not change the preset",
        binding.borrow().preset_name().to_string(),
        preset,
    );
    checks.eq(
        "outside click does not change effects",
        binding.borrow().effects(),
        effects,
    );
    checks.eq(
        "outside click does not change EQ",
        binding.borrow().eq_gains(),
        eq,
    );
}

fn scenario_save_outside_click(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    open_save(window, binding);
    checks.is(
        "Save opens over Settings",
        binding.borrow().save_open() && binding.borrow().settings_open(),
        "Save/Settings did not both open",
    );

    let preset = binding.borrow().preset_name().to_string();
    let effects = binding.borrow().effects();
    let eq = binding.borrow().eq_gains();
    click(window, 40.0, 480.0);
    settle();

    checks.is(
        "outside click closes Save",
        !binding.borrow().save_open(),
        "save_open() is still true",
    );
    checks.is(
        "outside click keeps Settings",
        binding.borrow().settings_open(),
        "Settings was dismissed with Save",
    );
    let focus = report(window);
    checks.is(
        "focus returns to Settings, not settings:none",
        focus.starts_with("settings:") && focus != "settings:none",
        format!("got {focus}"),
    );
    checks.eq(
        "outside click does not change the preset",
        binding.borrow().preset_name().to_string(),
        preset,
    );
    checks.eq(
        "outside click does not change effects",
        binding.borrow().effects(),
        effects,
    );
    checks.eq(
        "outside click does not change EQ",
        binding.borrow().eq_gains(),
        eq,
    );
    checks.is(
        "outside click does not open the preset menu",
        !binding.borrow().preset_menu_open(),
        "preset_menu_open() is true",
    );
}

fn scenario_save_escape_targets(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    for target in ["save:input", "save:cancel", "save:save", "save:close"] {
        reset(window, binding);
        open_settings(window, binding);
        open_save(window, binding);
        binding.borrow_mut().set_save_name("Escape target");
        sync(window, binding);

        let mut reached = report(window) == target;
        for _ in 0..6 {
            if reached {
                break;
            }
            press(window, "\t");
            reached = report(window) == target;
        }
        checks.is(
            &format!("Save exposes {target} to the keyboard"),
            reached,
            format!("focus sequence ended at {}", report(window)),
        );
        if reached {
            press(window, "\u{1b}");
            checks.is(
                &format!("Escape closes Save from {target}"),
                !binding.borrow().save_open(),
                "save_open() is still true",
            );
            checks.is(
                &format!("Escape from {target} keeps Settings"),
                binding.borrow().settings_open(),
                "Settings was dismissed with Save",
            );
        }
    }
}

fn scenario_save_tab_wrap_disabled(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    open_save(window, binding);
    binding.borrow_mut().set_save_name("");
    sync(window, binding);
    click(window, 500.0, 283.0);
    settle();
    checks.eq(
        "name field is focused",
        report(window),
        "save:input".to_string(),
    );

    let gains_before = binding.borrow().eq_gains();

    // Reference DOM order is Close, Input, Cancel, Save. Forward from the
    // field: Cancel, Close (disabled Save skipped), back to the field.
    press(window, "\t");
    checks.eq(
        "Tab 1 from the field reaches Cancel",
        report(window),
        "save:cancel".to_string(),
    );
    checks.eq(
        "Tab 1 does not focus background EQ",
        binding.borrow().active_eq_band(),
        None,
    );
    press(window, "\t");
    checks.eq(
        "Tab 2 reaches Close and skips the disabled Save",
        report(window),
        "save:close".to_string(),
    );
    checks.eq(
        "Tab 2 does not focus background EQ",
        binding.borrow().active_eq_band(),
        None,
    );
    press(window, "\t");
    checks.eq(
        "Tab 3 wraps back to the field",
        report(window),
        "save:input".to_string(),
    );
    checks.eq(
        "Tab 3 does not focus background EQ",
        binding.borrow().active_eq_band(),
        None,
    );

    // Backward from the field: Close, Cancel (disabled Save skipped), field.
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab from the field reaches Close",
        report(window),
        "save:close".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab from Close reaches Cancel and skips the disabled Save",
        report(window),
        "save:cancel".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab wraps back to the field",
        report(window),
        "save:input".to_string(),
    );

    checks.eq(
        "keyboard trapping never changes EQ gains",
        binding.borrow().eq_gains(),
        gains_before,
    );
    checks.is(
        "keyboard trapping never opens the preset menu",
        !binding.borrow().preset_menu_open(),
        "preset_menu_open() is true",
    );
}

fn scenario_save_tab_wrap_enabled(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    open_save(window, binding);
    // A unique name: `scenario_save_success` already saved "Evening listen",
    // so reusing it here would silently exercise the disabled-Save path.
    binding.borrow_mut().set_save_name("Enabled cycle");
    sync(window, binding);
    checks.is(
        "enabled Save cycle has a valid name",
        binding.borrow().save_valid(),
        "save_valid() is false; the enabled branch is not exercised",
    );
    click(window, 500.0, 283.0);
    settle();
    checks.eq(
        "name field is focused",
        report(window),
        "save:input".to_string(),
    );

    let gains_before = binding.borrow().eq_gains();

    // Enabled Save full forward cycle: Input -> Cancel -> Save -> Close -> Input.
    press(window, "\t");
    checks.eq(
        "Tab 1 from the field reaches Cancel",
        report(window),
        "save:cancel".to_string(),
    );
    checks.eq(
        "Tab 1 does not focus background EQ",
        binding.borrow().active_eq_band(),
        None,
    );
    press(window, "\t");
    checks.eq(
        "Tab 2 reaches Save",
        report(window),
        "save:save".to_string(),
    );
    checks.eq(
        "Tab 2 does not focus background EQ",
        binding.borrow().active_eq_band(),
        None,
    );
    press(window, "\t");
    checks.eq(
        "Tab 3 reaches Close",
        report(window),
        "save:close".to_string(),
    );
    checks.eq(
        "Tab 3 does not focus background EQ",
        binding.borrow().active_eq_band(),
        None,
    );
    press(window, "\t");
    checks.eq(
        "Tab 4 wraps back to the field",
        report(window),
        "save:input".to_string(),
    );

    // Enabled Save full reverse cycle: Input -> Close -> Save -> Cancel -> Input.
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab from the field reaches Close",
        report(window),
        "save:close".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab from Close reaches Save",
        report(window),
        "save:save".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab from Save reaches Cancel",
        report(window),
        "save:cancel".to_string(),
    );
    press(window, "\u{19}");
    checks.eq(
        "Shift+Tab wraps back to the field",
        report(window),
        "save:input".to_string(),
    );

    checks.eq(
        "enabled keyboard trapping never changes EQ gains",
        binding.borrow().eq_gains(),
        gains_before,
    );
}

fn scenario_save_padding(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    open_save(window, binding);

    // Save's header/padding is not a control, but it must still block the
    // Settings backdrop underneath.
    click(window, 500.0, 210.0);
    settle();
    checks.is(
        "Save padding click keeps Save open",
        binding.borrow().save_open(),
        "save_open() is false",
    );
    checks.is(
        "Save padding click keeps Settings open",
        binding.borrow().settings_open(),
        "the click fell through to the Settings backdrop",
    );
    checks.is(
        "Save padding click does not open the preset menu",
        !binding.borrow().preset_menu_open(),
        "preset_menu_open() is true",
    );

    press(window, "\u{1b}");
    checks.is(
        "Escape after padding click closes Save",
        !binding.borrow().save_open(),
        "save_open() is still true",
    );
    checks.is(
        "Escape after padding click keeps Settings",
        binding.borrow().settings_open(),
        "Settings was dismissed with Save",
    );
}

fn scenario_settings_opener_return(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    for _ in 0..12 {
        press(window, "\t");
    }
    checks.eq(
        "Tab 12 reaches the Save opener",
        report(window),
        "settings:13".to_string(),
    );
    press(window, "\n");
    checks.is(
        "Enter on the Save opener opens Save",
        binding.borrow().save_open(),
        "save_open() is false",
    );
    press(window, "\u{1b}");
    checks.is(
        "Escape closes Save",
        !binding.borrow().save_open(),
        "save_open() is still true",
    );
    checks.is(
        "Escape keeps Settings",
        binding.borrow().settings_open(),
        "Settings was dismissed with Save",
    );
    checks.eq(
        "Escape returns focus to the Save opener",
        report(window),
        "settings:13".to_string(),
    );
    press(window, "\n");
    checks.is(
        "Enter after Escape re-opens Save",
        binding.borrow().save_open(),
        "Enter closed Settings instead of re-opening Save",
    );
}

fn scenario_settings_focus_order(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    checks.eq(
        "Settings focus starts on Close",
        report(window),
        "settings:0".to_string(),
    );

    // Source order is 0..9,11,12,13,10,14: the Start-at-login switch (10)
    // follows the Save opener (13), not the custom-accent button (9).
    let mut order = vec![0usize];
    for _ in 0..14 {
        press(window, "\t");
        let focus = report(window);
        let index = focus
            .strip_prefix("settings:")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(999);
        order.push(index);
    }
    checks.eq(
        "Settings Tab order follows the source 0..9,11,12,13,10,14",
        order,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 10, 14],
    );

    // After a cancelled Save restores the Save opener (13), Tab must select
    // Start (10), then Quit (14). A direct 13 -> 14 jump is the defect.
    reset(window, binding);
    open_settings(window, binding);
    for _ in 0..12 {
        press(window, "\t");
    }
    checks.eq(
        "Tab 12 reaches the Save opener",
        report(window),
        "settings:13".to_string(),
    );
    press(window, "\n");
    checks.is(
        "Enter opens Save",
        binding.borrow().save_open(),
        "save_open() is false",
    );
    press(window, "\u{1b}");
    checks.is(
        "Escape closes Save",
        !binding.borrow().save_open(),
        "save_open() is still true",
    );
    checks.eq(
        "Escape restores the Save opener",
        report(window),
        "settings:13".to_string(),
    );
    press(window, "\t");
    checks.eq(
        "Tab after cancelled Save selects Start",
        report(window),
        "settings:10".to_string(),
    );
    press(window, "\t");
    checks.eq(
        "Tab after Start selects Quit",
        report(window),
        "settings:14".to_string(),
    );
}

fn scenario_eq_hit_areas(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    let gains_before = binding.borrow().eq_gains();
    let pitch = EQ_CHART_WIDTH / EQ_BANDS as f32;

    // Every band centre.
    for band in 0..EQ_BANDS {
        let centre = EQ_CHART_X + (band as f32 + 0.5) * pitch;
        move_to(window, centre, EQ_Y);
        settle();
        checks.eq(
            &format!("band {band} centre x={centre:.2}"),
            binding.borrow().active_eq_band(),
            Some(band),
        );
    }

    // Every internal boundary, one pixel either side: the source band cells
    // abut, so there are no dead gaps between centres.
    for boundary in 1..EQ_BANDS {
        let x = EQ_CHART_X + boundary as f32 * pitch;
        move_to(window, x - 1.0, EQ_Y);
        settle();
        checks.eq(
            &format!("gap left of boundary {boundary} x={:.2}", x - 1.0),
            binding.borrow().active_eq_band(),
            Some(boundary - 1),
        );
        move_to(window, x + 1.0, EQ_Y);
        settle();
        checks.eq(
            &format!("gap right of boundary {boundary} x={:.2}", x + 1.0),
            binding.borrow().active_eq_band(),
            Some(boundary),
        );
    }

    // Outer edges.
    move_to(window, EQ_CHART_X + 1.0, EQ_Y);
    settle();
    checks.eq(
        "left edge is band 0",
        binding.borrow().active_eq_band(),
        Some(0),
    );
    move_to(window, EQ_CHART_X + EQ_CHART_WIDTH - 1.0, EQ_Y);
    settle();
    checks.eq(
        "right edge is band 30",
        binding.borrow().active_eq_band(),
        Some(EQ_BANDS - 1),
    );

    // Quarter-pixel probes. The coarse centre/edge samples above still pass on
    // a chart drawn at 323/657, because every internal boundary shifts by less
    // than a pixel; these pin the exact reference box instead. The panel border
    // (1px, box-sizing: border-box in the source) must inset the child, so no
    // band may be reachable in the [323,324) or [979,980) strips.
    for (x, expected, why) in [
        (323.25, None, "outer strip left of the inset chart"),
        (323.75, None, "outer strip left of the inset chart"),
        (324.25, Some(0), "first quarter-pixel inside the chart"),
        (
            978.75,
            Some(EQ_BANDS - 1),
            "last quarter-pixel inside the chart",
        ),
        (979.25, None, "outer strip right of the inset chart"),
        (979.75, None, "outer strip right of the inset chart"),
    ] {
        move_to(window, x, EQ_Y);
        settle();
        checks.eq(
            &format!("strict x={x:.2} ({why})"),
            binding.borrow().active_eq_band(),
            expected,
        );
    }

    // Internal boundaries shift by up to a pixel when the chart box is wrong:
    // 324 + 655/31 = 345.13 and 324 + 30*655/31 = 957.87, versus 344.19 and
    // 958.81 for the old 323/657 box.
    let strict_pitch = EQ_CHART_WIDTH / EQ_BANDS as f32;
    for (x, expected, why) in [
        (
            EQ_CHART_X + strict_pitch - 0.5,
            Some(0),
            "left of the band 0/1 boundary",
        ),
        (
            EQ_CHART_X + strict_pitch + 0.5,
            Some(1),
            "right of the band 0/1 boundary",
        ),
        (
            EQ_CHART_X + 30.0 * strict_pitch - 0.5,
            Some(EQ_BANDS - 2),
            "left of the band 29/30 boundary",
        ),
        (
            EQ_CHART_X + 30.0 * strict_pitch + 0.5,
            Some(EQ_BANDS - 1),
            "right of the band 29/30 boundary",
        ),
    ] {
        move_to(window, x, EQ_Y);
        settle();
        checks.eq(
            &format!("strict boundary x={x:.2} ({why})"),
            binding.borrow().active_eq_band(),
            expected,
        );
    }

    checks.eq(
        "hovering never changes EQ gains",
        binding.borrow().eq_gains(),
        gains_before,
    );
    move_to(window, 20.0, 20.0);
    settle();
    checks.eq(
        "pointer leave clears the readout",
        binding.borrow().active_eq_band(),
        None,
    );
}

fn scenario_eq_click_arrows(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    let pitch = EQ_CHART_WIDTH / EQ_BANDS as f32;
    let centre17 = EQ_CHART_X + 17.5 * pitch;
    click(window, centre17, EQ_Y);
    settle();
    checks.eq(
        "click lands on band 17",
        binding.borrow().active_eq_band(),
        Some(17),
    );
    let before = binding.borrow().eq_gains();
    let mut up = before;
    up[17] = (before[17] + 1.0).min(12.0);

    press(window, "\u{f700}"); // UpArrow
    checks.eq(
        "Up steps only the clicked band",
        binding.borrow().eq_gains(),
        up,
    );
    press(window, "\u{f701}"); // DownArrow
    checks.eq(
        "Down restores the clicked band",
        binding.borrow().eq_gains(),
        before,
    );
    press(window, "\u{f703}"); // RightArrow
    checks.eq(
        "Right steps only the clicked band",
        binding.borrow().eq_gains(),
        up,
    );
    press(window, "\u{f702}"); // LeftArrow
    checks.eq(
        "Left restores the clicked band",
        binding.borrow().eq_gains(),
        before,
    );
}

fn scenario_eq_focus_after_leave(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    let pitch = EQ_CHART_WIDTH / EQ_BANDS as f32;

    // Mouse click then leave: Equalizer.tsx:151 clears the hover readout, but
    // the arrow must still change the clicked band, proving focus survived.
    let centre17 = EQ_CHART_X + 17.5 * pitch;
    click(window, centre17, EQ_Y);
    settle();
    checks.eq(
        "click lands on band 17",
        binding.borrow().active_eq_band(),
        Some(17),
    );
    move_to(window, 20.0, 20.0);
    settle();
    checks.eq(
        "mouse leave clears the hover readout",
        binding.borrow().active_eq_band(),
        None,
    );
    checks.eq(
        "mouse leave clears the readout text",
        readout(window),
        String::new(),
    );
    let before = binding.borrow().eq_gains();
    let mut expected = before;
    expected[17] = (before[17] + 1.0).min(12.0);
    press(window, "\u{f700}"); // UpArrow
    checks.eq(
        "Up after mouse leave changes the clicked band",
        binding.borrow().eq_gains(),
        expected,
    );
    press(window, "\u{f701}"); // DownArrow
    checks.eq(
        "Down after mouse leave restores the clicked band",
        binding.borrow().eq_gains(),
        before,
    );

    // Keyboard focus: Tab into a band, hover it, then leave. The hover readout
    // clears, but the arrow must still change the focused band.
    reset(window, binding);
    let mut focused = None;
    for _ in 0..80 {
        press(window, "\t");
        if let Some(band) = binding.borrow().active_eq_band() {
            focused = Some(band);
            break;
        }
    }
    checks.is(
        "Tab reaches an EQ band",
        focused.is_some(),
        "no band was focused after 80 Tabs",
    );
    if let Some(band) = focused {
        let centre = EQ_CHART_X + (band as f32 + 0.5) * pitch;
        move_to(window, centre, EQ_Y);
        settle();
        move_to(window, 20.0, 20.0);
        settle();
        checks.eq(
            "keyboard focus leave clears the hover readout",
            binding.borrow().active_eq_band(),
            None,
        );
        checks.eq(
            "keyboard focus leave clears the readout text",
            readout(window),
            String::new(),
        );

        let before = binding.borrow().eq_gains();
        let mut expected = before;
        expected[band] = (before[band] + 1.0).min(12.0);
        press(window, "\u{f700}"); // UpArrow
        checks.eq(
            "Up after keyboard leave changes the focused band",
            binding.borrow().eq_gains(),
            expected,
        );
        press(window, "\u{f701}"); // DownArrow
        checks.eq(
            "Down after keyboard leave restores the focused band",
            binding.borrow().eq_gains(),
            before,
        );
    }
}

fn scenario_modal_containment(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    let gains = binding.borrow().eq_gains();

    // Check containment after EACH Tab/Backtab, not only at the end.
    for step in 0..20 {
        press(window, "\t");
        let focus = report(window);
        checks.is(
            &format!("Settings Tab {step} keeps focus in Settings"),
            focus.starts_with("settings:") && focus != "settings:none",
            format!("got {focus}"),
        );
        checks.eq(
            &format!("Settings Tab {step} does not focus background EQ"),
            binding.borrow().active_eq_band(),
            None,
        );
        checks.eq(
            &format!("Settings Tab {step} does not change EQ gains"),
            binding.borrow().eq_gains(),
            gains,
        );
    }
    for step in 0..20 {
        press(window, "\u{19}");
        let focus = report(window);
        checks.is(
            &format!("Settings Backtab {step} keeps focus in Settings"),
            focus.starts_with("settings:") && focus != "settings:none",
            format!("got {focus}"),
        );
        checks.eq(
            &format!("Settings Backtab {step} does not focus background EQ"),
            binding.borrow().active_eq_band(),
            None,
        );
        checks.eq(
            &format!("Settings Backtab {step} does not change EQ gains"),
            binding.borrow().eq_gains(),
            gains,
        );
    }
    for (arrow, label) in [
        ("\u{f700}", "Up"),
        ("\u{f701}", "Down"),
        ("\u{f702}", "Left"),
        ("\u{f703}", "Right"),
    ] {
        press(window, arrow);
        checks.eq(
            &format!("Settings {label} arrow does not change EQ gains"),
            binding.borrow().eq_gains(),
            gains,
        );
    }

    open_save(window, binding);
    for step in 0..20 {
        press(window, "\t");
        let focus = report(window);
        checks.is(
            &format!("Save Tab {step} keeps focus in Save"),
            focus.starts_with("save:") && focus != "save:none",
            format!("got {focus}"),
        );
        checks.eq(
            &format!("Save Tab {step} does not focus background EQ"),
            binding.borrow().active_eq_band(),
            None,
        );
        checks.eq(
            &format!("Save Tab {step} does not change EQ gains"),
            binding.borrow().eq_gains(),
            gains,
        );
    }
    for step in 0..20 {
        press(window, "\u{19}");
        let focus = report(window);
        checks.is(
            &format!("Save Backtab {step} keeps focus in Save"),
            focus.starts_with("save:") && focus != "save:none",
            format!("got {focus}"),
        );
        checks.eq(
            &format!("Save Backtab {step} does not focus background EQ"),
            binding.borrow().active_eq_band(),
            None,
        );
        checks.eq(
            &format!("Save Backtab {step} does not change EQ gains"),
            binding.borrow().eq_gains(),
            gains,
        );
    }
    for (arrow, label) in [
        ("\u{f700}", "Up"),
        ("\u{f701}", "Down"),
        ("\u{f702}", "Left"),
        ("\u{f703}", "Right"),
    ] {
        press(window, arrow);
        checks.eq(
            &format!("Save {label} arrow does not change EQ gains"),
            binding.borrow().eq_gains(),
            gains,
        );
    }
}

fn scenario_preset_menu_keyboard(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    // Home + Return selects the first selectable row (Default), whatever the
    // currently selected preset is.
    reset(window, binding);
    binding.borrow_mut().toggle_preset_menu();
    sync(window, binding);
    press(window, "\u{f729}"); // Home
    settle();
    press(window, "\u{000a}"); // Return
    settle();
    checks.eq(
        "Home + Return selects Default",
        binding.borrow().preset_name().to_string(),
        "Default".to_string(),
    );

    // ArrowDown walks the factory rows from the highlighted one: Default ->
    // General -> Music.
    reset(window, binding);
    binding.borrow_mut().toggle_preset_menu();
    sync(window, binding);
    press(window, "\u{f729}"); // Home
    settle();
    for _ in 0..2 {
        press(window, "\u{f701}"); // DownArrow
        settle();
    }
    press(window, "\u{000a}");
    settle();
    checks.eq(
        "Home + Down x2 + Return selects Music",
        binding.borrow().preset_name().to_string(),
        "Music".to_string(),
    );

    // End selects the last selectable output row (System default), like the
    // reference select jumping to the end of its list.
    reset(window, binding);
    binding.borrow_mut().toggle_output_menu();
    sync(window, binding);
    press(window, "\u{f72b}"); // End
    settle();
    press(window, "\u{000a}");
    settle();
    // End lands on the last selectable row: Direct playback in the
    // reference contract.
    checks.eq(
        "End + Return selects Direct playback",
        binding.borrow().output_name().to_string(),
        "Direct playback".to_string(),
    );

    // Escape dismisses and returns focus to the trigger.
    reset(window, binding);
    binding.borrow_mut().toggle_output_menu();
    sync(window, binding);
    press(window, "\u{1b}"); // Escape
    settle();
    checks.is(
        "Escape closes the output menu",
        !binding.borrow().output_menu_open(),
        "output_menu_open() is still true",
    );
    checks.is(
        "Escape refocuses the output trigger",
        window.get_output_selector_focused(),
        "output_selector_focused is false",
    );
}

fn scenario_power_keyboard(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    // Earlier scenarios leave focus mid-chain (31 EQ bands alone sit after
    // the header), so allow a full traversal cycle like the EQ focus test.
    let mut reached = window.get_power_focused();
    for _ in 0..60 {
        if reached {
            break;
        }
        press(window, "\t");
        settle();
        reached = window.get_power_focused();
    }
    checks.is(
        "Tab reaches the power pill",
        reached,
        "power_focused stayed false after 12 Tabs",
    );
    if reached {
        press(window, "\u{000a}"); // Return
        checks.is(
            "Enter toggles processing from the power pill",
            !binding.borrow().power(),
            "power is still on",
        );
    }
}

fn scenario_eq_range_keys(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    let pitch = EQ_CHART_WIDTH / EQ_BANDS as f32;
    let centre17 = EQ_CHART_X + 17.5 * pitch;
    click(window, centre17, EQ_Y);
    settle();
    checks.eq(
        "click lands on band 17",
        binding.borrow().active_eq_band(),
        Some(17),
    );

    // Clicking a band sets its gain from the pointer (click-to-set), so the
    // range keys run from the post-click baseline. The reference range input
    // jumps: Home to -12 dB, End to +12 dB, and Page keys step 10 dB.
    press(window, "\u{f729}"); // Home
    settle();
    checks.eq(
        "EQ Home drops the band to -12 dB",
        binding.borrow().eq_gains()[17],
        -12.0,
    );
    press(window, "\u{f72b}"); // End
    settle();
    checks.eq(
        "EQ End jumps to +12 dB",
        binding.borrow().eq_gains()[17],
        12.0,
    );
    press(window, "\u{f72d}"); // PageDown
    settle();
    checks.eq(
        "EQ PageDown steps -10 dB",
        binding.borrow().eq_gains()[17],
        2.0,
    );
    press(window, "\u{f72c}"); // PageUp
    settle();
    checks.eq(
        "EQ PageUp steps +10 dB",
        binding.borrow().eq_gains()[17],
        12.0,
    );
}

fn scenario_settings_close_opener(
    window: &AppWindow,
    binding: &Rc<RefCell<UiBinding>>,
    checks: &mut Checks,
) {
    reset(window, binding);
    open_settings(window, binding);
    press(window, "\u{1b}");
    checks.is(
        "Escape closes Settings",
        !binding.borrow().settings_open(),
        "settings_open() is still true",
    );
    checks.eq(
        "closed Settings reports no modal focus",
        report(window),
        "none".to_string(),
    );
    press(window, "\u{000a}");
    checks.is(
        "Enter on the opener reopens Settings",
        binding.borrow().settings_open(),
        "the header opener did not take focus",
    );
}

#[test]
fn bound_view_modal_and_eq_regressions() {
    let window = AppWindow::new().expect("create the Slint window");
    let binding = UiBinding::install(&window);
    window.show().expect("show the window");

    let mut checks = Checks::new();
    scenario_startup(&window, &binding, &mut checks);
    scenario_settings_keyboard(&window, &binding, &mut checks);
    scenario_save_success(&window, &binding, &mut checks);
    scenario_save_escape(&window, &binding, &mut checks);
    scenario_click_scoping(&window, &binding, &mut checks);
    scenario_background_unreachable(&window, &binding, &mut checks);
    scenario_save_outside_click(&window, &binding, &mut checks);
    scenario_save_escape_targets(&window, &binding, &mut checks);
    scenario_save_tab_wrap_disabled(&window, &binding, &mut checks);
    scenario_save_tab_wrap_enabled(&window, &binding, &mut checks);
    scenario_save_padding(&window, &binding, &mut checks);
    scenario_settings_opener_return(&window, &binding, &mut checks);
    scenario_settings_focus_order(&window, &binding, &mut checks);
    scenario_eq_hit_areas(&window, &binding, &mut checks);
    scenario_eq_click_arrows(&window, &binding, &mut checks);
    scenario_eq_focus_after_leave(&window, &binding, &mut checks);
    scenario_modal_containment(&window, &binding, &mut checks);
    scenario_settings_close_opener(&window, &binding, &mut checks);
    scenario_preset_menu_keyboard(&window, &binding, &mut checks);
    scenario_power_keyboard(&window, &binding, &mut checks);
    scenario_eq_range_keys(&window, &binding, &mut checks);
    checks.finish();
}
