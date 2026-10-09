# Menu-bar-only lifecycle acceptance (macOS)

Use a packaged `.app` for launch checks; `cargo run` does not exercise LaunchServices bundle
metadata. Keep Activity Monitor and Dock visible, but do not capture account or notification data.

## Launch and restoration

- [ ] Quit Zerochi completely, launch the packaged app from Finder ten times, and confirm no Zerochi
      icon appears or flashes in the Dock while the menu-bar item initializes.
- [ ] Launch at login and after a cold reboot; confirm Zerochi remains menu-bar-only throughout the
      slower startup path.
- [ ] Open and close the tray popover repeatedly. Confirm showing/focusing it never adds a Dock icon
      and hiding it leaves the process running in the menu bar.
- [ ] Complete, cancel, deny, and time out OAuth. Confirm every terminal popup restoration stays
      menu-bar-only and preserves the original OAuth result.
- [ ] Activate a current native notification and a stale Notification Center entry. Confirm neither
      path exposes a Dock icon; the stale entry remains a silent no-op.

## Lifecycle edges

- [ ] Put the Mac to sleep with the popup hidden, then wake it. Repeat with the popup visible.
      Confirm wake/catch-up does not create a Dock icon and tray toggling still works.
- [ ] Switch rapidly between Spaces and full-screen apps while opening the popover. Confirm focus
      may follow normal macOS rules but Zerochi never becomes a regular Dock application.
- [ ] Confirm the packaged `Zerochi.app/Contents/Info.plist` contains boolean `LSUIElement = true`.
      This bundle-level declaration prevents the transient launch race before Rust setup executes.

## Multi-monitor popup regression (macOS 27)

Use the freshly packaged application with separate menu bars enabled on both displays. Keep mail
and account data out of screenshots and logs. Automated point-geometry/lifecycle tests cover these
rules, but cannot prove AppKit's mirrored status-item window, Spaces, focus, or animation behavior.

- [ ] On a Retina 2x internal display A and 1x external display B, click A → B → A at least twenty
      times while the popup remains visible. Each different-display click transfers the popup below
      that display's clicked icon, with correct pointer alignment and a 390×600 logical-point size.
- [ ] Repeat with the popup hidden between clicks and with rapid A/B clicks. A second click on the
      same display closes; the following mouse release must not reopen it. A delayed blur from the
      previous display must not hide the newly focused popup.
- [ ] Arrange B to the left (negative X), above, and directly below A (negative Y). Try both tray
      edges and alternate the primary display. Verify the popup remains inside that display's usable
      frame and that no first-display preference appears. Ambiguous stale event coordinates are
      rejected instead of opening on the wrong display; a subsequent valid click remains usable.
- [ ] Disconnect the display containing the popup, then click the remaining menu bar. Repeat after
      reconnecting and after changing scaling. The app must stay alive and subsequent clicks must
      reopen/transfer it; no cached monitor or old backing scale may survive.
- [ ] Open through OAuth completion or notification restoration; a same-display tray click closes
      that visible popup and a different-display tray click transfers it.
- [ ] Click another application after opening, press the normal close shortcut, and repeat after
      switching Spaces/full-screen apps. A genuine blur or close hides the popup; reopening works.
      Confirm there is still no Zerochi Dock icon.
