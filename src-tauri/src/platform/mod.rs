//! Native application lifecycle helpers.

mod sleep_wake;

#[cfg(target_os = "macos")]
pub(crate) use sleep_wake::install_macos_sleep_wake_observer;
#[allow(unused_imports)]
pub(crate) use sleep_wake::{SchedulerLifecycleRequest, SleepWakeCallback, SleepWakeObserverError};

use tauri::{AppHandle, Manager, WebviewWindow};

const MAIN_WINDOW_LABEL: &str = "main";

fn apply_menu_bar_only_policy<PolicyError, DockError>(
    set_accessory_policy: impl FnOnce() -> Result<(), PolicyError>,
    hide_dock: impl FnOnce() -> Result<(), DockError>,
) {
    // These are deliberately independent best-effort defenses. Accessory is the semantic AppKit
    // policy; hiding the Dock process registration also repairs a process that launched Regular.
    let _ = set_accessory_policy();
    let _ = hide_dock();
}

/// Reasserts Zerochi's menu-bar-only process state without affecting the caller's result.
pub(crate) fn enforce_menu_bar_only(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    apply_menu_bar_only_policy(
        || app.set_activation_policy(tauri::ActivationPolicy::Accessory),
        || app.set_dock_visibility(false),
    );

    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

trait RestorableWindow {
    fn show_best_effort(&self);
    fn focus_best_effort(&self);
}

impl RestorableWindow for WebviewWindow {
    fn show_best_effort(&self) {
        let _ = self.show();
    }

    fn focus_best_effort(&self) {
        let _ = self.set_focus();
    }
}

fn restore_window(window: &impl RestorableWindow) {
    window.show_best_effort();
    window.focus_best_effort();
}

/// Brings Zerochi's main window back after a native interaction such as OAuth.
///
/// This is intentionally best-effort: a missing window or a native show/focus
/// failure must never replace the result of the interaction that just ended.
/// Calling `set_focus` after `show` activates the window on macOS even while
/// the application keeps its menu-bar `ActivationPolicy::Accessory` policy.
pub(crate) fn restore_main_window(app: &AppHandle) {
    // OAuth completion, notification activation, and tray opening all converge here. Reassert the
    // invariant before show/focus so none of those activation paths can expose a stale Dock icon.
    enforce_menu_bar_only(app);

    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    restore_window(&window);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct RecordingWindow {
        calls: RefCell<Vec<&'static str>>,
    }

    impl RestorableWindow for RecordingWindow {
        fn show_best_effort(&self) {
            self.calls.borrow_mut().push("show");
        }

        fn focus_best_effort(&self) {
            self.calls.borrow_mut().push("focus");
        }
    }

    #[test]
    fn restoration_shows_before_focusing() {
        let window = RecordingWindow {
            calls: RefCell::new(Vec::new()),
        };

        restore_window(&window);

        assert_eq!(*window.calls.borrow(), ["show", "focus"]);
    }

    #[test]
    fn menu_bar_policy_sets_accessory_then_hides_dock_even_if_policy_fails() {
        let calls = RefCell::new(Vec::new());

        apply_menu_bar_only_policy(
            || {
                calls.borrow_mut().push("accessory");
                Err::<(), _>("policy unavailable")
            },
            || {
                calls.borrow_mut().push("hide_dock");
                Ok::<_, ()>(())
            },
        );

        assert_eq!(*calls.borrow(), ["accessory", "hide_dock"]);
    }

    #[test]
    fn menu_bar_policy_keeps_dock_failure_best_effort() {
        let calls = RefCell::new(Vec::new());

        apply_menu_bar_only_policy(
            || {
                calls.borrow_mut().push("accessory");
                Ok::<_, ()>(())
            },
            || {
                calls.borrow_mut().push("hide_dock");
                Err::<(), _>("dock unavailable")
            },
        );

        assert_eq!(*calls.borrow(), ["accessory", "hide_dock"]);
    }

    #[test]
    fn bundle_declares_zerochi_as_a_launch_services_ui_element() {
        let compact_plist = include_str!("../../Info.plist")
            .split_whitespace()
            .collect::<String>();

        assert!(compact_plist.contains("<key>LSUIElement</key><true/>"));
        assert!(!compact_plist.contains("<key>LSUIElement</key><false/>"));
    }
}
