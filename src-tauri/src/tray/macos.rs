//! Native macOS placement avoids Positioner's physical coordinates from the previous screen.

use std::sync::Mutex;

use objc2::{rc::Retained, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSScreen, NSWindow};
use objc2_foundation::{NSPoint, NSRect};
use tauri::{tray::TrayIcon, AppHandle, Emitter, Manager, Rect};

use super::{
    geometry::{event_anchor, native_anchor_matches, place, Frame, Lifecycle, Screen},
    physical_frame, PopoverAnchorChanged, MAIN_WINDOW_LABEL, POPOVER_ANCHOR_CHANGED_EVENT,
};
use crate::platform;

// tray-icon uses these same CoreGraphics functions to encode its event rectangle.
// Read the exact flip baseline rather than substitute an NSScreen logical height.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
}

fn frame(rect: NSRect) -> Frame {
    Frame {
        x: rect.origin.x,
        y: rect.origin.y,
        width: rect.size.width,
        height: rect.size.height,
    }
}

fn screen(native: &NSScreen) -> Screen {
    Screen {
        frame: frame(native.frame()),
        visible: frame(native.visibleFrame()),
        scale: native.backingScaleFactor(),
    }
}

fn window(app: &AppHandle) -> Option<(tauri::WebviewWindow, Retained<NSWindow>)> {
    // All callers run on the main thread. Retain the Tauri-owned NSWindow for
    // this operation only; no raw AppKit pointer is passed through async tasks.
    MainThreadMarker::new()?;
    let webview = app.get_webview_window(MAIN_WINDOW_LABEL)?;
    let pointer = webview.ns_window().ok()?.cast::<NSWindow>();
    // SAFETY: Tauri's ns_window API returns its live NSWindow. Main-thread
    // validation and retaining ensure validity until the operation completes.
    let native = unsafe { Retained::retain(pointer) }?;
    Some((webview, native))
}

pub(super) fn initialize(app: &AppHandle) {
    app.manage(Mutex::new(Lifecycle::default()));
}

pub(super) fn title_will_change(app: &AppHandle, title: &str) {
    let state = app.state::<Mutex<Lifecycle>>();
    let mut lifecycle = state.lock().unwrap_or_else(|p| p.into_inner());
    if lifecycle.last_title.as_deref() != Some(title) {
        lifecycle.last_title = Some(title.to_owned());
        lifecycle.title_changed_at = Some(std::time::Instant::now());
    }
}

pub(super) fn click(tray: &TrayIcon, rect: Rect) {
    let app = tray.app_handle().clone();
    // Tauri guarantees with_inner_tray_icon runs on the main thread. The
    // captured immutable click rectangle identifies the original mirror even
    // when the shared status button has already switched screens.
    let _ = tray.with_inner_tray_icon(move |inner| {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Some((webview, popup)) = window(&app) else {
            return;
        };
        let live: Vec<Screen> = NSScreen::screens(mtm).iter().map(|s| screen(&s)).collect();
        // SAFETY: CoreGraphics returns the connected main display's pixel height.
        let flip = unsafe { CGDisplayPixelsHigh(CGMainDisplayID()) as f64 };
        let encoded = physical_frame(rect);
        let encoded = Frame {
            x: encoded.x,
            y: encoded.y,
            width: encoded.width,
            height: encoded.height,
        };
        let native_source = inner
            .ns_status_item()
            .and_then(|item| item.button(mtm))
            .and_then(|button| button.window())
            .and_then(|tray_window| {
                let native_screen = tray_window.screen()?;
                let candidate = screen(&native_screen);
                let native_icon = frame(tray_window.frame());
                live.iter()
                    .any(|s| s.frame == candidate.frame)
                    .then_some((native_icon, candidate))
            });
        let lifecycle_state = app.state::<Mutex<Lifecycle>>();
        let mut lifecycle = lifecycle_state.lock().unwrap_or_else(|p| p.into_inner());
        // Async tray delivery can straddle a title update. In that interval,
        // width cannot disambiguate scales safely (a new title can halve it).
        let stable_width = lifecycle.width_hint_is_stable(std::time::Instant::now());
        let decoded = event_anchor(
            encoded,
            flip,
            &live,
            native_source
                .filter(|_| stable_width)
                .map(|(icon, _)| icon.width),
        );
        // Full native re-encoding is independent of the fallback. Every frame
        // dimension must match; a different mirrored status window is rejected.
        let source = native_source
            .filter(|(icon, candidate)| native_anchor_matches(encoded, flip, *icon, *candidate));
        let anchor = source.or(decoded);
        let visible = popup.isVisible();
        let current = popup
            .screen()
            .map(|s| frame(s.frame()))
            .filter(|frame| live.iter().any(|s| s.frame == *frame));
        lifecycle.reconcile_visible_screen(visible, current);
        let open = lifecycle.click(visible, anchor.map(|(_, screen)| screen.frame));
        drop(lifecycle);
        if !open {
            // Hide the live main-thread NSWindow synchronously so
            // a later click cannot read stale queued visibility.
            popup.orderOut(None);
            return;
        }
        let Some((icon, target)) = anchor else {
            // A disconnected display or invalid event never poisons the next
            // click. Do not resurrect a window at stale offscreen coordinates.
            return;
        };
        let size = popup.frame().size;
        let Some(position) = place(icon, target, size.width, size.height) else {
            return;
        };
        platform::enforce_menu_bar_only(&app);
        popup.setFrameTopLeftPoint(NSPoint::new(position.x, position.top));
        let _ = webview.emit(
            POPOVER_ANCHOR_CHANGED_EVENT,
            PopoverAnchorChanged {
                x: position.anchor_x,
            },
        );
        // Main-thread live AppKit objects. Unlike Tauri's queued
        // set_position/show chain, the move completes before show/focus.
        popup.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    });
}

pub(super) fn hide(app: &AppHandle) {
    app.state::<Mutex<Lifecycle>>()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .hide();
    if let Some((_, popup)) = window(app) {
        popup.orderOut(None);
    }
}

pub(super) fn blur(app: &AppHandle) {
    let generation = app
        .state::<Mutex<Lifecycle>>()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .generation;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // A tray press or monitor transfer can enqueue blur before its click.
        // Defer briefly, then inspect actual native focus on the main thread.
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        let main_app = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some((_, popup)) = window(&main_app) else {
                return;
            };
            let should_hide = main_app
                .state::<Mutex<Lifecycle>>()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .should_hide_after_blur(generation, popup.isKeyWindow());
            if should_hide {
                hide(&main_app);
            }
        });
    });
}
