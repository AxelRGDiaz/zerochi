use serde::Serialize;
#[cfg(not(target_os = "macos"))]
use tauri::Emitter;
use tauri::{
    menu::MenuBuilder,
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    App, Manager, Rect, WindowEvent,
};
#[cfg(any(not(target_os = "macos"), test))]
use tauri::{PhysicalPosition, PhysicalSize};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_positioner::{Position, WindowExt};

#[cfg(any(target_os = "macos", test))]
mod geometry;
#[cfg(target_os = "macos")]
mod macos;

use crate::platform;

const MAIN_WINDOW_LABEL: &str = "main";
const QUIT_MENU_ID: &str = "quit";
const TRAY_ID: &str = "zerochi";
const POPOVER_ANCHOR_CHANGED_EVENT: &str = "popover_anchor_changed";

#[derive(Clone, Copy, Debug, PartialEq)]
struct PhysicalFrame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct PopoverAnchorChanged {
    x: f64,
}

fn physical_frame(rect: Rect) -> PhysicalFrame {
    // Positioner stores tray events in physical coordinates using this same
    // conversion. Mirroring it keeps the CSS pointer aligned with its anchor.
    let position = rect.position.to_physical::<f64>(1.0);
    let size = rect.size.to_physical::<f64>(1.0);
    PhysicalFrame {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

#[cfg(any(not(target_os = "macos"), test))]
fn popover_anchor_x(
    tray_rect: Rect,
    window_position: PhysicalPosition<i32>,
    window_size: PhysicalSize<u32>,
    scale_factor: f64,
) -> Option<f64> {
    let icon = physical_frame(tray_rect);
    if !scale_factor.is_finite()
        || scale_factor <= 0.0
        || !icon.x.is_finite()
        || !icon.y.is_finite()
        || !icon.width.is_finite()
        || !icon.height.is_finite()
    {
        return None;
    }

    let window_width_physical = f64::from(window_size.width);
    let anchor_x_physical = icon.x + icon.width / 2.0 - f64::from(window_position.x);
    let anchor_x_logical =
        (anchor_x_physical / scale_factor).clamp(0.0, window_width_physical / scale_factor);
    if !anchor_x_logical.is_finite() {
        return None;
    }

    Some(anchor_x_logical)
}

#[cfg(any(not(target_os = "macos"), test))]
fn open_popover<PositionError, EmitError>(
    position: impl FnOnce() -> Result<(), PositionError>,
    emit_anchor: impl FnOnce() -> Result<(), EmitError>,
    restore: impl FnOnce(),
) {
    if position().is_ok() {
        let _ = emit_anchor();
    }
    restore();
}

#[cfg(not(target_os = "macos"))]
fn emit_popover_anchor(window: &tauri::WebviewWindow, tray_rect: Rect) -> tauri::Result<()> {
    let Some(x) = popover_anchor_x(
        tray_rect,
        window.outer_position()?,
        window.outer_size()?,
        window.scale_factor()?,
    ) else {
        return Ok(());
    };
    window.emit(POPOVER_ANCHOR_CHANGED_EVENT, PopoverAnchorChanged { x })
}

#[cfg(not(target_os = "macos"))]
fn show_anchored_popover(app: &tauri::AppHandle, tray_rect: Rect) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        platform::restore_main_window(app);
        return;
    };
    open_popover(
        || window.move_window_constrained(Position::TrayBottomCenter),
        || emit_popover_anchor(&window, tray_rect),
        || platform::restore_main_window(app),
    );
}

#[cfg(any(not(target_os = "macos"), test))]
fn toggle_popover<VisibilityError, HideError>(
    is_visible: impl FnOnce() -> Result<bool, VisibilityError>,
    hide: impl FnOnce() -> Result<(), HideError>,
    open: impl FnOnce(),
) {
    if matches!(is_visible(), Ok(true)) {
        let _ = hide();
    } else {
        open();
    }
}

#[cfg(not(target_os = "macos"))]
fn toggle_anchored_popover(app: &tauri::AppHandle, tray_rect: Rect) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    toggle_popover(
        || window.is_visible(),
        || window.hide(),
        || show_anchored_popover(app, tray_rect),
    );
}

fn handle_tray_click(button: MouseButton, button_state: MouseButtonState, toggle: impl FnOnce()) {
    // tray-icon maps macOS `mouseDown:` to `Down` and `mouseUp:` to `Up`.
    // Deciding on press captures visibility before the old window can blur.
    if button == MouseButton::Left && button_state == MouseButtonState::Down {
        toggle();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PetTrayState {
    pub(crate) account_count: usize,
    pub(crate) pending_total: u64,
    pub(crate) inbox_unread_total: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PetTrayMood {
    Sleeping,
    Satisfied,
    Happy,
    Busy,
    Tired,
    Exhausted,
    Overwhelmed,
}

#[derive(Debug, PartialEq, Eq)]
struct PetTrayPresentation {
    title: String,
    tooltip: String,
}

impl PetTrayState {
    const fn sleeping() -> Self {
        Self {
            account_count: 0,
            pending_total: 0,
            inbox_unread_total: 0,
        }
    }

    fn mood(self) -> PetTrayMood {
        if self.account_count == 0 {
            return PetTrayMood::Sleeping;
        }

        match self.inbox_unread_total {
            0 => PetTrayMood::Satisfied,
            1..=5 => PetTrayMood::Happy,
            6..=15 => PetTrayMood::Busy,
            16..=30 => PetTrayMood::Tired,
            31..=50 => PetTrayMood::Exhausted,
            _ => PetTrayMood::Overwhelmed,
        }
    }

    fn presentation(self) -> PetTrayPresentation {
        let mood = self.mood();
        let face = match mood {
            PetTrayMood::Sleeping => "(- _ -)",
            PetTrayMood::Satisfied => "(• - •)",
            PetTrayMood::Happy => "(• ᴗ •)",
            PetTrayMood::Busy => "(•̀ - •́)",
            PetTrayMood::Tired => "(- ᴗ -)",
            PetTrayMood::Exhausted => "(× _ ×)",
            PetTrayMood::Overwhelmed => "(@ _ @)",
        };

        if mood == PetTrayMood::Sleeping {
            return PetTrayPresentation {
                title: face.into(),
                tooltip: "Zerochi — durmiendo, sin cuentas conectadas".into(),
            };
        }

        let title = match self.inbox_unread_total {
            0 => face.into(),
            unread => format!("{face} {unread}"),
        };
        let tooltip = match self.inbox_unread_total {
            0 => "Zerochi — sin correos sin leer".into(),
            1 => "Zerochi — 1 correo sin leer".into(),
            unread => format!("Zerochi — {unread} correos sin leer"),
        };

        PetTrayPresentation { title, tooltip }
    }
}

pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    platform::enforce_menu_bar_only(app.handle());
    #[cfg(target_os = "macos")]
    macos::initialize(app.handle());

    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        window.hide()?;

        let popup = window.clone();
        window.on_window_event(move |event| match event {
            WindowEvent::Focused(false) => {
                #[cfg(target_os = "macos")]
                macos::blur(popup.app_handle());
                #[cfg(not(target_os = "macos"))]
                let _ = popup.hide();
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                #[cfg(target_os = "macos")]
                macos::hide(popup.app_handle());
                #[cfg(not(target_os = "macos"))]
                let _ = popup.hide();
            }
            _ => {}
        });
    }

    let menu = MenuBuilder::new(app)
        .text(QUIT_MENU_ID, "Cerrar Zerochi")
        .build()?;

    let initial = PetTrayState::sleeping().presentation();
    TrayIconBuilder::with_id(TRAY_ID)
        .title(initial.title)
        .tooltip(initial.tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == QUIT_MENU_ID {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
            if let TrayIconEvent::Click {
                rect,
                button,
                button_state,
                ..
            } = event
            {
                handle_tray_click(button, button_state, || {
                    #[cfg(target_os = "macos")]
                    macos::click(tray, rect);
                    #[cfg(not(target_os = "macos"))]
                    toggle_anchored_popover(tray.app_handle(), rect);
                });
            }
        })
        .build(app)?;
    Ok(())
}

pub(crate) fn update(app: &tauri::AppHandle, state: PetTrayState) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let presentation = state.presentation();
    #[cfg(target_os = "macos")]
    {
        let title = presentation.title.clone();
        let app = app.clone();
        // Start the layout guard at the actual main-thread title mutation,
        // not while a background update is waiting in the event queue.
        tray.with_inner_tray_icon(move |inner| {
            macos::title_will_change(&app, &title);
            inner.set_title(Some(title));
        })?;
    }
    #[cfg(not(target_os = "macos"))]
    tray.set_title(Some(&presentation.title))?;
    tray.set_tooltip(Some(&presentation.tooltip))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use tauri::{LogicalPosition, LogicalSize, Position as TauriPosition, Size};

    fn frame(x: f64, y: f64, width: f64, height: f64) -> PhysicalFrame {
        PhysicalFrame {
            x,
            y,
            width,
            height,
        }
    }

    fn tray_rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            position: TauriPosition::Physical(PhysicalPosition::new(x, y)),
            size: Size::Physical(PhysicalSize::new(width, height)),
        }
    }

    fn presentation(accounts: usize, pending: u64, unread: u64) -> PetTrayPresentation {
        PetTrayState {
            account_count: accounts,
            pending_total: pending,
            inbox_unread_total: unread,
        }
        .presentation()
    }

    #[test]
    fn tray_rect_conversion_matches_positioner_physical_coordinates() {
        let logical = Rect {
            position: TauriPosition::Logical(LogicalPosition::new(100.0, 10.0)),
            size: Size::Logical(LogicalSize::new(20.0, 12.0)),
        };
        let physical = Rect {
            position: TauriPosition::Physical(PhysicalPosition::new(200, 20)),
            size: Size::Physical(PhysicalSize::new(40, 24)),
        };

        assert_eq!(physical_frame(logical), frame(100.0, 10.0, 20.0, 12.0));
        assert_eq!(physical_frame(physical), frame(200.0, 20.0, 40.0, 24.0));
    }

    #[test]
    fn anchor_uses_the_real_window_position_after_positioner_moves_it() {
        let anchor = popover_anchor_x(
            tray_rect(700, 0, 40, 48),
            PhysicalPosition::new(330, 42),
            PhysicalSize::new(780, 1200),
            2.0,
        )
        .expect("valid anchor");

        assert_eq!(anchor, 195.0);
    }

    #[test]
    fn anchor_stays_local_and_bounded_after_positioner_constrains_edges() {
        let left = popover_anchor_x(
            tray_rect(-1000, 0, 20, 20),
            PhysicalPosition::new(0, 24),
            PhysicalSize::new(390, 600),
            1.0,
        )
        .expect("left anchor");
        let right = popover_anchor_x(
            tray_rect(2000, 0, 20, 20),
            PhysicalPosition::new(1050, 24),
            PhysicalSize::new(390, 600),
            1.0,
        )
        .expect("right anchor");

        assert_eq!(left, 0.0);
        assert_eq!(right, 390.0);
    }

    #[test]
    fn invalid_scale_skips_the_anchor_best_effort() {
        let anchor = popover_anchor_x(
            tray_rect(700, 0, 40, 48),
            PhysicalPosition::new(330, 42),
            PhysicalSize::new(780, 1200),
            0.0,
        );

        assert_eq!(anchor, None);
    }

    #[test]
    fn anchor_payload_contains_only_a_finite_x() {
        let payload = PopoverAnchorChanged { x: 390.0 };
        let serialized = serde_json::to_value(payload).expect("serialize anchor");

        assert!(payload.x.is_finite());
        assert_eq!(serialized.as_object().map(|object| object.len()), Some(1));
        assert_eq!(serialized["x"], 390.0);
    }

    #[test]
    fn popover_positions_emits_and_restores_in_order() {
        let calls = RefCell::new(Vec::new());

        open_popover(
            || {
                calls.borrow_mut().push("position");
                Ok::<_, ()>(())
            },
            || {
                calls.borrow_mut().push("emit");
                Ok::<_, ()>(())
            },
            || calls.borrow_mut().push("restore"),
        );

        assert_eq!(*calls.borrow(), ["position", "emit", "restore"]);
    }

    #[test]
    fn positioning_failure_skips_anchor_but_still_restores() {
        let calls = RefCell::new(Vec::new());

        open_popover(
            || {
                calls.borrow_mut().push("position");
                Err::<(), _>("unavailable monitor")
            },
            || {
                calls.borrow_mut().push("emit");
                Ok::<_, ()>(())
            },
            || calls.borrow_mut().push("restore"),
        );

        assert_eq!(*calls.borrow(), ["position", "restore"]);
    }

    #[test]
    fn anchor_event_failure_still_restores_after_the_emit_attempt() {
        let calls = RefCell::new(Vec::new());

        open_popover(
            || {
                calls.borrow_mut().push("position");
                Ok::<_, ()>(())
            },
            || {
                calls.borrow_mut().push("emit");
                Err::<(), _>("listener unavailable")
            },
            || calls.borrow_mut().push("restore"),
        );

        assert_eq!(*calls.borrow(), ["position", "emit", "restore"]);
    }

    #[test]
    fn second_left_click_hides_without_position_emit_or_focus_side_effects() {
        let visible = Cell::new(false);
        let calls = RefCell::new(Vec::new());

        for _ in 0..2 {
            toggle_popover(
                || {
                    calls.borrow_mut().push("is_visible");
                    Ok::<_, ()>(visible.get())
                },
                || {
                    calls.borrow_mut().push("hide");
                    visible.set(false);
                    Ok::<_, ()>(())
                },
                || {
                    calls.borrow_mut().extend(["position", "emit", "focus"]);
                    visible.set(true);
                },
            );
        }

        assert_eq!(
            *calls.borrow(),
            [
                "is_visible",
                "position",
                "emit",
                "focus",
                "is_visible",
                "hide"
            ]
        );
        assert!(!visible.get());
    }

    #[test]
    fn visibility_failure_attempts_to_open_best_effort() {
        let calls = RefCell::new(Vec::new());

        toggle_popover(
            || Err::<bool, _>("visibility unavailable"),
            || {
                calls.borrow_mut().push("hide");
                Ok::<_, ()>(())
            },
            || calls.borrow_mut().push("open"),
        );

        assert_eq!(*calls.borrow(), ["open"]);
    }

    #[test]
    fn normal_left_click_opens_once_on_down_and_ignores_up() {
        let opens = Cell::new(0);

        handle_tray_click(MouseButton::Left, MouseButtonState::Down, || {
            opens.set(opens.get() + 1)
        });
        handle_tray_click(MouseButton::Left, MouseButtonState::Up, || {
            opens.set(opens.get() + 1)
        });

        assert_eq!(opens.get(), 1);
    }

    #[test]
    fn down_closes_before_blur_and_the_following_up_cannot_reopen() {
        let visible = Cell::new(false);
        let calls = RefCell::new(Vec::new());
        let click = |state| {
            handle_tray_click(MouseButton::Left, state, || {
                toggle_popover(
                    || Ok::<_, ()>(visible.get()),
                    || {
                        calls.borrow_mut().push("down_hide");
                        visible.set(false);
                        Ok::<_, ()>(())
                    },
                    || {
                        calls.borrow_mut().push("open");
                        visible.set(true);
                    },
                );
            });
        };

        click(MouseButtonState::Down);
        click(MouseButtonState::Up);
        click(MouseButtonState::Down);
        calls.borrow_mut().push("blur_hide");
        visible.set(false);
        click(MouseButtonState::Up);

        assert_eq!(*calls.borrow(), ["open", "down_hide", "blur_hide"]);
        assert!(!visible.get());
    }

    #[test]
    fn no_accounts_sleeps_without_a_counter_even_if_counts_are_stale() {
        let sleeping = presentation(0, u64::MAX, 8);

        assert_eq!(sleeping.title, "(- _ -)");
        assert_eq!(
            sleeping.tooltip,
            "Zerochi — durmiendo, sin cuentas conectadas"
        );
        assert!(!sleeping
            .title
            .chars()
            .any(|character| character.is_ascii_digit()));
    }

    #[test]
    fn zero_pending_with_three_unread_uses_the_happy_face_and_counter() {
        let happy = presentation(1, 0, 3);

        assert_eq!(happy.title, "(• ᴗ •) 3");
        assert_eq!(happy.tooltip, "Zerochi — 3 correos sin leer");
    }

    #[test]
    fn positive_pending_with_zero_unread_is_satisfied_without_a_counter() {
        let satisfied = presentation(1, u64::MAX, 0);

        assert_eq!(satisfied.title, "(• - •)");
        assert_eq!(satisfied.tooltip, "Zerochi — sin correos sin leer");
    }

    #[test]
    fn unread_thresholds_select_faces_and_the_matching_badge() {
        let cases = [
            (1, "(• ᴗ •) 1", PetTrayMood::Happy),
            (5, "(• ᴗ •) 5", PetTrayMood::Happy),
            (6, "(•̀ - •́) 6", PetTrayMood::Busy),
            (15, "(•̀ - •́) 15", PetTrayMood::Busy),
            (16, "(- ᴗ -) 16", PetTrayMood::Tired),
            (30, "(- ᴗ -) 30", PetTrayMood::Tired),
            (31, "(× _ ×) 31", PetTrayMood::Exhausted),
            (50, "(× _ ×) 50", PetTrayMood::Exhausted),
            (51, "(@ _ @) 51", PetTrayMood::Overwhelmed),
        ];

        for (unread, title, mood) in cases {
            let state = PetTrayState {
                account_count: 1,
                pending_total: if unread % 2 == 0 { u64::MAX } else { 0 },
                inbox_unread_total: unread,
            };
            assert_eq!(state.mood(), mood);
            assert_eq!(state.presentation().title, title);
        }
    }

    #[test]
    fn divergent_pending_totals_do_not_change_the_ambient_projection() {
        let without_pending = presentation(1, 0, 3);
        let maximum_pending = presentation(1, u64::MAX, 3);

        assert_eq!(without_pending, maximum_pending);
        assert_eq!(without_pending.title, "(• ᴗ •) 3");
    }

    #[test]
    fn tooltip_uses_singular_and_plural_unread_copy() {
        assert_eq!(
            presentation(1, 50, 1).tooltip,
            "Zerochi — 1 correo sin leer"
        );
        assert_eq!(
            presentation(1, 0, 2).tooltip,
            "Zerochi — 2 correos sin leer"
        );
    }

    #[test]
    fn maximum_totals_format_without_arithmetic_or_truncation() {
        let maximum = presentation(usize::MAX, u64::MAX, u64::MAX);

        assert_eq!(maximum.title, format!("(@ _ @) {}", u64::MAX));
        assert_eq!(
            maximum.tooltip,
            format!("Zerochi — {} correos sin leer", u64::MAX)
        );
    }
}
