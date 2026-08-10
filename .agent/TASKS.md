# Tasks

Status markers: `[ ]` ready, `[>]` active, `[x]` complete, `[!]` blocked. Lead assigns a single owner before work begins.

## Milestones

- **M0 Bootstrap & architecture:** [x] scaffold; [x] contracts/docs/ownership; [x] mock UI; [x] pure PetEngine/tests; [x] sprite placeholder contract; [x] minimal tray/native compile; [x] Herdr installed; [x] declarative six-agent bootstrap and onboarding prompts.
- **M1 Menu bar + UI mock:** [>] `ui` extract popup view and visibility state; [>] `tauri` show/hide/focus popup and tray menu; [ ] `qa` lifecycle/accessibility review.
- **M2 PetEngine mock:** [>] `pet` add event queue/idle fallback tests; [ ] `qa` property/boundary tests.
- **M3 Sprite renderer:** [ ] `pet` add placeholder development sheet and visibility/completion tests; no final art.
- **M4 OAuth single account:** [x] `lead` approve environment/secure-store direction; [ ] `gmail` Rust-side env validation and desktop PKCE flow; [ ] `tauri` Keychain adapter; [ ] `qa` token/config exposure audit.
- **M5 OAuth multicuenta:** [ ] `gmail` independent account credentials/revocation; [ ] `ui` account management.
- **M6 Initial sync:** [ ] `gmail` metadata baseline that creates zero historical pending; [ ] `qa` large mailbox tests.
- **M7 Incremental sync:** [ ] `gmail` history pagination, cursor commit, 404 recovery, backoff, poll mutex.
- **M8 Pending mail state:** [ ] `lead` schema/version contract; [ ] `gmail` dedupe/atomic state; [ ] `qa` restart/corruption tests.
- **M9 Notifications:** [ ] `tauri` native notification/activation; [ ] `qa` duplicate suppression.
- **M10 Open Gmail:** [ ] `tauri` navigation strategy for account and exact thread; [ ] `qa` multi-login cases.
- **M11 Feeding:** [ ] `gmail` idempotent local open; [ ] `pet` event presentation; [ ] `ui` interaction.
- **M12 Persistence:** [ ] schema migration/atomic write/recovery.
- **M13 Secure storage:** [x] choose native macOS Keychain; [ ] implement Protected Data/unsigned-development adapters, explicit deletion, zeroization, and migration tests.
- **M14 Autostart + lifecycle:** [ ] sleep/wake/offline/reconnect and configurable polling.
- **M15 Final sprite sheet:** blocked until approved original art and manifest arrive.
- **M16 Performance/QA:** idle CPU, memory, wakeups, HTTP, timer and regression budgets.
- **M17 macOS packaging:** signing, notarization, permissions, release checklist.

## First parallel wave after Lead unblocks tools

`gmail`: design M4 sequence/error model only. `tauri`: M1 popup/tray lifecycle. `ui`: M1 component/store extraction. `pet`: M2 transition/event tests. `qa`: M0 baseline security/performance review. Worktrees branch from an integrated green commit.

## Active assignments — first parallel wave

- [>] `gmail`: document the M4 desktop PKCE sequence and sanitized error model under `src-tauri/src/auth/**`; design only, with no OAuth implementation or contract changes.
- [>] `tauri`: implement M1 popup/tray lifecycle under `src-tauri/src/tray/**`: hidden startup, click-to-show/focus, blur/close-to-hide, macOS Dock suppression, and an explicit Quit tray action.
- [>] `ui`: extract the M1 popup presentation and a testable visibility store using existing DTOs, with the coordinated `src/App.tsx` composition change only.
- [>] `pet`: implement a pure deterministic pet-event queue with idle fallback and boundary/replacement tests under `src/pet/**`; no UI wiring or shared-contract changes.
- [>] `qa`: perform the M0 security/performance baseline audit and commit the reproducible findings report at `docs/qa/M0_BASELINE.md`; production fixes remain with owners.
