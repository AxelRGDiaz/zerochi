# M9 native new-mail notification acceptance

Audit date: 2026-08-11

Verdict: **GREEN for automated integration; physical macOS acceptance remains pending**

## Scope

This QA-only review covered the approved M9 contract from durable incremental commit through `PollOutcome`, application coordination, the native UserNotifications adapter, bundled custom sound, duplicate suppression, and activation. No production code, shared contracts, or Lead-owned coordination files were changed by QA.

## Automated acceptance

- Eligibility is persistence-defined. `commit_incremental` returns only records inserted after `(accountId,messageId)` deduplication, and a failed cursor compare-and-swap returns no outcome.
- Baseline, expired-history recovery, unread-only changes, startup hydration, and duplicate/replayed provider ids produce an empty `new_messages` batch.
- The coordinator publishes the committed snapshot first, submits one notification per committed message, and emits the healthy status last. A notification error is discarded best-effort and does not block healthy sync or mutate the durable commit.
- `NotificationRequest` contains only an opaque SHA-256-derived local id. Native presentation is fixed to `Nuevo correo` / `Tienes un correo nuevo en Zerochi` / `notification-chime.wav`; sender, subject, email, provider payload/error, credentials, cursor, and browser position cannot enter the request or payload.
- The native adapter suppresses repeated stable ids, requests only alert/sound authorization, ignores denied permission and asynchronous native errors, and accepts activation only for the Zerochi opaque-id namespace.
- Activation delegates to the existing best-effort main-window restore path, which shows before focusing and does not mutate mail state or navigate to Gmail.
- The debug `.app` contains `Contents/Resources/notification-chime.wav`. Source and bundled hashes are both `92f9e1ffd419d19a9d4d4248ada52b78757e9a9cc96fee0af784be969b1e9367`. The bundled resource is RIFF/WAVE PCM, 16-bit, mono, 44.1 kHz.

## Checks

- `cargo test --locked accounts::tests`: 15 passed, 0 failed.
- `cargo test --locked sync::tests`: 12 passed, 0 failed.
- `cargo test --locked notifications::tests`: 5 passed, 0 failed.
- `cargo test --locked commands::tests`: 9 passed, 0 failed.
- `cargo test --locked`: 166 passed, 0 failed; main/doc-test targets had 0 tests.
- `cargo check --locked`: passed.
- `cargo fmt --check`: passed.
- `git diff --check` on the M9 Rust/config files: passed.
- Direct bundle inspection verified the sound path, file type, and source/bundle hash equality.

## Findings

No automated M9 finding.

## Manual acceptance still required

On a physical macOS candidate build, verify each of the following once with notifications allowed and once with permission denied where applicable:

1. A genuinely new incremental message displays exactly one banner and plays the custom sound once.
2. A foreground notification still presents and plays once, without a duplicated sound.
3. Denied permission leaves polling, pending, unread count, and future scheduling healthy.
4. Clicking the delivered notification restores, shows, and focuses the Zerochi popup without opening Gmail.
5. Restart/hydration and a subsequent duplicate provider id do not replay the notification or sound.

These interactive checks are not automatable evidence of Notification Center permission state, audible playback, or real activation/focus behavior. They remain packaging acceptance, not an automated integration blocker.

## Development-launch regression re-audit

Re-audit date: 2026-08-11

Verdict: **GREEN; the unbundled development abort is closed**

The original macOS adapter called `UNUserNotificationCenter::currentNotificationCenter` during startup even when Tauri launched the raw `target/debug/zerochi` executable. macOS raises an Objective-C exception for that unbundled process shape. The owner fix now checks the executable path before touching UserNotifications: only an executable under `<name>.app/Contents/MacOS/` installs the center/delegate and submits native notifications; the raw development binary uses a successful best-effort no-op backend.

QA executed both process shapes with external GUI permission and bounded runtime:

- `src-tauri/target/debug/zerochi` remained alive with no `NSInternalInconsistencyException` or abort and exited only after QA sent Ctrl-C.
- `src-tauri/target/debug/bundle/macos/Zerochi.app/Contents/MacOS/zerochi` also remained alive and exited only after QA sent Ctrl-C, proving that the bundle path still initializes.
- `cargo test --locked notifications::tests`: 6 passed, including bundled/unbundled executable-shape coverage.
- `cargo test --locked`: 167 passed, 0 failed.
- `cargo check --locked` and `cargo fmt --check`: passed.
- The bundled `notification-chime.wav` remained valid PCM and byte-identical to the source hash recorded above.

No regression finding remains. Audible notification delivery and click activation continue to require the physical manual acceptance listed above.
