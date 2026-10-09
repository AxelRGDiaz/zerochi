//! Geometry stays in AppKit points (bottom-left origin), never global physical pixels.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Frame {
    pub fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f64::is_finite)
            && self.width > 0.0
            && self.height > 0.0
    }
    fn top(self) -> f64 {
        self.y + self.height
    }
    fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y <= self.top()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Screen {
    pub frame: Frame,
    pub visible: Frame,
    pub scale: f64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Placement {
    pub x: f64,
    pub top: f64,
    pub anchor_x: f64,
}

pub(super) fn place(icon: Frame, screen: Screen, width: f64, height: f64) -> Option<Placement> {
    if !icon.valid()
        || !screen.frame.valid()
        || !screen.visible.valid()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }
    let visible = screen.visible;
    let x = (icon.x + icon.width / 2.0 - width / 2.0)
        .clamp(visible.x, visible.x + (visible.width - width).max(0.0));
    let top = icon
        .y
        .clamp(visible.y + height.min(visible.height), visible.top());
    Some(Placement {
        x,
        top,
        anchor_x: (icon.x + icon.width / 2.0 - x).clamp(0.0, width),
    })
}

/// Reverse tray-icon's macOS event encoding using each LIVE screen's scale.
/// Its Y flip uses CGDisplayPixelsHigh before multiplying by backing scale.
/// The old popup's scale and monitor never participate in this conversion.
pub(super) fn event_anchor(
    encoded: Frame,
    flip_height: f64,
    screens: &[Screen],
    native_width: Option<f64>,
) -> Option<(Frame, Screen)> {
    if !encoded.valid() || !flip_height.is_finite() {
        return None;
    }
    let candidates: Vec<_> = screens
        .iter()
        .copied()
        .filter_map(|screen| {
            if !screen.scale.is_finite() || screen.scale <= 0.0 || !screen.frame.valid() {
                return None;
            }
            let icon = Frame {
                x: encoded.x / screen.scale,
                y: flip_height - (encoded.y + encoded.height) / screen.scale,
                width: encoded.width / screen.scale,
                height: encoded.height / screen.scale,
            };
            // A tray is at the top of its screen. This disambiguates overlapping
            // candidates on mixed-scale desktops without consulting a later cursor.
            (icon.valid()
                && (icon.top() - screen.frame.top()).abs() <= 2.0
                && screen
                    .frame
                    .contains(icon.x + icon.width / 2.0, icon.y + icon.height / 2.0))
            .then_some((icon, screen))
        })
        .collect();
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }
    // Global coordinates multiplied by per-display scale are not injective.
    // Stacked 1x/2x screens can both fit the same rect. Never choose by order.
    let width = native_width.filter(|width| width.is_finite() && *width > 0.0)?;
    let mut matches = candidates.into_iter().filter(|(icon, screen)| {
        // The same status item has the same logical width on scale-only
        // mirrors. Visible-frame menu height adds an independent check where
        // macOS reserves a menu bar; auto-hidden bars do not provide that hint.
        let menu_height = screen.frame.top() - screen.visible.top();
        (icon.width - width).abs() <= 1.0
            && (menu_height <= 0.0 || (icon.height - menu_height).abs() <= 2.0)
    });
    let candidate = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(candidate)
    }
}

pub(super) fn native_anchor_matches(
    encoded: Frame,
    flip_height: f64,
    icon: Frame,
    source: Screen,
) -> bool {
    if !encoded.valid()
        || !icon.valid()
        || !source.scale.is_finite()
        || source.scale <= 0.0
        || !flip_height.is_finite()
    {
        return false;
    }
    let expected = [
        icon.x * source.scale,
        (flip_height - icon.top()) * source.scale,
        icon.width * source.scale,
        icon.height * source.scale,
    ];
    expected
        .into_iter()
        .zip([encoded.x, encoded.y, encoded.width, encoded.height])
        .all(|(expected, actual)| (expected - actual).abs() <= 1.0)
}

#[derive(Default)]
pub(super) struct Lifecycle {
    pub generation: u64,
    pub screen: Option<Frame>,
    pub last_title: Option<String>,
    pub title_changed_at: Option<std::time::Instant>,
}

impl Lifecycle {
    pub fn width_hint_is_stable(&self, now: std::time::Instant) -> bool {
        self.title_changed_at.is_none_or(|changed| {
            now.saturating_duration_since(changed) >= std::time::Duration::from_millis(250)
        })
    }
    pub fn reconcile_visible_screen(&mut self, visible: bool, current: Option<Frame>) {
        // OAuth/notification restoration and display disconnection can move a
        // visible popup independently of the last tray intent.
        if visible {
            self.screen = current;
        }
    }
    /// Every intent invalidates a delayed blur. Cross-screen intent always opens.
    pub fn click(&mut self, visible: bool, target: Option<Frame>) -> bool {
        self.generation = self.generation.wrapping_add(1);
        let close = visible && target.is_some() && self.screen == target;
        self.screen = if close { None } else { target };
        !close
    }
    pub fn hide(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.screen = None;
    }
    pub fn should_hide_after_blur(&self, generation: u64, focused: bool) -> bool {
        self.generation == generation && !focused
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn screen(x: f64, y: f64, scale: f64) -> Screen {
        Screen {
            frame: Frame {
                x,
                y,
                width: 1440.0,
                height: 900.0,
            },
            visible: Frame {
                x,
                y,
                width: 1440.0,
                height: 876.0,
            },
            scale,
        }
    }
    fn icon(screen: Screen) -> Frame {
        Frame {
            x: screen.frame.x + 1000.0,
            y: screen.frame.top() - 24.0,
            width: 80.0,
            height: 24.0,
        }
    }
    fn encoded(icon: Frame, scale: f64, flip: f64) -> Frame {
        Frame {
            x: icon.x * scale,
            y: (flip - icon.top()) * scale,
            width: icon.width * scale,
            height: icon.height * scale,
        }
    }
    #[test]
    fn repeated_a_b_a_mixed_scales_never_accumulate_old_scale_or_size() {
        let a = screen(0.0, 0.0, 2.0);
        let b = screen(-1440.0, -300.0, 1.0);
        let mut lifecycle = Lifecycle::default();
        for target in [a, b, a, b, a, b, a] {
            let (anchor, live) = event_anchor(
                encoded(icon(target), target.scale, 1800.0),
                1800.0,
                &[a, b],
                None,
            )
            .unwrap();
            assert_eq!(live.frame, target.frame);
            assert!(lifecycle.click(true, Some(live.frame)));
            let placement = place(anchor, live, 390.0, 600.0).unwrap();
            assert_eq!(placement.x, target.frame.x + 845.0);
            assert_eq!(placement.top, target.frame.y + 876.0);
            assert_eq!(placement.anchor_x, 195.0);
        }
        assert!(!lifecycle.click(true, Some(a.frame)));
    }
    #[test]
    fn disconnected_old_screen_and_invalid_event_leave_next_click_recoverable() {
        let old = screen(-1440.0, 0.0, 1.0);
        let live = screen(0.0, 0.0, 2.0);
        assert!(event_anchor(encoded(icon(old), 1.0, 1800.0), 1800.0, &[live], None).is_none());
        let mut lifecycle = Lifecycle::default();
        lifecycle.click(true, Some(old.frame));
        assert!(lifecycle.click(true, None));
        assert!(lifecycle.click(true, Some(live.frame)));
        assert!(place(icon(live), live, f64::NAN, 600.0).is_none());
        assert!(place(icon(live), live, 390.0, 600.0).is_some());
    }
    #[test]
    fn stacked_mixed_scale_ambiguity_never_silently_selects_the_primary() {
        let a = screen(0.0, 0.0, 2.0);
        let b = Screen {
            frame: Frame {
                x: 0.0,
                y: -1440.0,
                width: 2560.0,
                height: 1440.0,
            },
            visible: Frame {
                x: 0.0,
                y: -1440.0,
                width: 2560.0,
                height: 1416.0,
            },
            scale: 1.0,
        };
        let clicked = Frame {
            x: 1000.0,
            y: -24.0,
            width: 80.0,
            height: 24.0,
        };
        let event = encoded(clicked, b.scale, 1800.0);
        assert!(event_anchor(event, 1800.0, &[a, b], None).is_none());
        assert!(event_anchor(event, 1800.0, &[b, a], None).is_none());
        assert!(native_anchor_matches(event, 1800.0, clicked, b));
        assert!(!native_anchor_matches(event, 1800.0, icon(a), a));
        // Shared status-button window still points at A, but its logical width
        // identifies the original B event independently of native x/y.
        let (_, target) = event_anchor(event, 1800.0, &[a, b], Some(80.0)).unwrap();
        assert_eq!(target.frame, b.frame);
        assert!(event_anchor(event, 1800.0, &[a, b], Some(70.0)).is_none());
        // A false primary interpretation must also fail when only height differs.
        let false_primary = Frame {
            x: 500.0,
            y: 888.0,
            width: 40.0,
            height: 24.0,
        };
        assert!(!native_anchor_matches(event, 1800.0, false_primary, a));
    }
    #[test]
    fn title_halving_cannot_select_wrong_scale_and_width_guard_expires() {
        let a = screen(0.0, 0.0, 2.0);
        let b = Screen {
            frame: Frame {
                x: 0.0,
                y: -1440.0,
                width: 2560.0,
                height: 1440.0,
            },
            visible: Frame {
                x: 0.0,
                y: -1440.0,
                width: 2560.0,
                height: 1416.0,
            },
            scale: 1.0,
        };
        let clicked = Frame {
            x: 1000.0,
            y: -24.0,
            width: 160.0,
            height: 24.0,
        };
        let event = encoded(clicked, b.scale, 1800.0);
        // Current width80 could resemble the wrong2x interpretation, but its
        // decoded height12 disagrees with A's reserved24-point menu bar.
        assert!(event_anchor(event, 1800.0, &[a, b], Some(80.0)).is_none());
        let now = std::time::Instant::now();
        let lifecycle = Lifecycle {
            title_changed_at: Some(now),
            ..Lifecycle::default()
        };
        assert!(!lifecycle.width_hint_is_stable(now + std::time::Duration::from_millis(249)));
        assert!(lifecycle.width_hint_is_stable(now + std::time::Duration::from_millis(250)));
        // Auto-hidden menu bars cannot supply a height check. During the title
        // transition no width hint is supplied; ambiguity stays recoverable.
        let hidden_a = Screen {
            visible: a.frame,
            ..a
        };
        let hidden_b = Screen {
            visible: b.frame,
            ..b
        };
        assert!(event_anchor(event, 1800.0, &[hidden_a, hidden_b], None).is_none());
    }
    #[test]
    fn clamps_negative_origin_edges_and_oversized_popup_without_panicking() {
        let screen = screen(-1440.0, -900.0, 1.0);
        let mut anchor = icon(screen);
        anchor.x = screen.frame.x;
        let left = place(anchor, screen, 390.0, 600.0).unwrap();
        assert_eq!(left.x, -1440.0);
        assert_eq!(left.anchor_x, 40.0);
        anchor.x = -80.0;
        let right = place(anchor, screen, 390.0, 600.0).unwrap();
        assert_eq!(right.x, -390.0);
        assert_eq!(right.anchor_x, 350.0);
        assert!(place(anchor, screen, 2000.0, 2000.0).is_some());
    }
    #[test]
    fn transfer_or_close_invalidates_delayed_blur_but_real_blur_hides() {
        let a = screen(0.0, 0.0, 2.0);
        let b = screen(1440.0, 0.0, 1.0);
        let mut lifecycle = Lifecycle::default();
        lifecycle.click(false, Some(a.frame));
        let pre_transfer = lifecycle.generation;
        assert!(lifecycle.click(true, Some(b.frame)));
        assert!(!lifecycle.should_hide_after_blur(pre_transfer, false));
        assert!(!lifecycle.should_hide_after_blur(lifecycle.generation, true));
        assert!(lifecycle.should_hide_after_blur(lifecycle.generation, false));
        let pre_close = lifecycle.generation;
        assert!(!lifecycle.click(true, Some(b.frame)));
        assert!(!lifecycle.should_hide_after_blur(pre_close, false));
        lifecycle.hide();
        assert!(lifecycle.click(false, Some(a.frame)));
    }
    #[test]
    fn independently_restored_popup_closes_on_its_actual_screen_or_transfers() {
        let a = screen(0.0, 0.0, 2.0);
        let b = screen(1440.0, 0.0, 1.0);
        let mut lifecycle = Lifecycle::default();
        lifecycle.reconcile_visible_screen(true, Some(a.frame));
        assert!(!lifecycle.click(true, Some(a.frame)));
        lifecycle.hide();
        lifecycle.reconcile_visible_screen(true, Some(a.frame));
        assert!(lifecycle.click(true, Some(b.frame)));
        // AppKit relocates the visible window after display B is disconnected.
        lifecycle.reconcile_visible_screen(true, Some(a.frame));
        assert!(!lifecycle.click(true, Some(a.frame)));
    }
}
