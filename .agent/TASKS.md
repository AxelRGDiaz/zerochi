# Tasks

## Active assignment — multi-monitor popup and stale face tests (2026-10-09)

- [x] `tauri`: preserved current tray face artwork and corrected its three stale test expectations. Diagnose and fix repeated tray activation across monitors under `src-tauri/src/{tray,platform}/**`; explicitly handle target-monitor scale, disconnected monitors, and focus/toggle ordering. Lead approves macOS-only synchronous AppKit logical positioning from the clicked tray window/screen, transfer-aware toggle, deferred actual-focus recheck, and coordinated `Cargo.toml`/`Cargo.lock` addition of narrowly scoped `objc2-app-kit` 0.3.2 features. Other platforms may retain Positioner. Avoid provider/UI/secret changes. Add regression coverage and a native manual checklist.
- [x] `ui`: fixed the stale CSS-shadow test under `src/styles/popoverFrame.test.ts` (5px → intentional 14px), production artwork unchanged. Full check passes: 235 frontend tests, 6 protected-release environment tests, TypeScript/Vite build.
- [x] `lead`: approved the internal native positioning/lifecycle changes and left-button Down contract without DTO expansion; owner and QA review passed. Integrated validation: 235 frontend tests, 239 Rust tests, 6 protected-release environment tests, TypeScript/Vite build, and protected signed release/artifact scan all pass. Installed and launched `/Applications/Zerochi.app`; prior app preserved at `/private/tmp/zerochi-before-multimonitor-p8x8dux_/Zerochi.app`. User confirmed the panel opens on the correct monitor and continues responding after A → B → A. Final app/DMG retained under ignored `src-tauri/target/release/bundle/`.

## Active assignment — macOS 27 tray click regression (2026-10-09)

- [x] `lead`: diagnose installed Zerochi on macOS 27.0.1 and integrate the official tray-icon #355/#365 fix. Approved coordinated dependency scope: `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`, requiring Tauri >=2.12.2 and tray-icon >=0.25.1; matching npm API/CLI versions in Lead-owned `package.json`/`package-lock.json` are required by the release tool. Preserve all existing dirty work. No DTO, OAuth, persistence, or frontend implementation change is approved or required.
- [x] `tauri`: explicitly approved dependency integration; `cargo check --locked --offline` and all 7 platform tests pass. 16/19 tray tests pass including all click/lifecycle/anchor checks; three existing face-string expectations fail independently of the dependency update. Native production paths unchanged. Minimum Rust is now 1.90, satisfied by local 1.97.1.
- [x] `lead`: protected release build and OAuth textual-artifact scan pass; signed with the existing Developer ID certificate fingerprint to avoid accented-name encoding failure. Signature verified outside the restricted sandbox. Corrected app installed at `/Applications/Zerochi.app`; prior app preserved at `/private/tmp/zerochi-before-macos27-20ww3dha/Zerochi.app`. Final app/DMG retained under ignored `src-tauri/target/release/bundle/`. Local build is signed but not notarized; redistribution still requires notarization. Physical-click acceptance awaits manual macOS unlock (computer-use reported the Mac locked).

Status markers: `[ ]` ready, `[>]` active, `[x]` complete, `[!]` blocked. Lead assigns a single owner before work begins.

## Milestones

- **M0 Bootstrap & architecture:** [x] scaffold; [x] contracts/docs/ownership; [x] mock UI; [x] pure PetEngine/tests; [x] sprite placeholder contract; [x] minimal tray/native compile; [x] Herdr installed; [x] declarative six-agent bootstrap and onboarding prompts.
- **M1 Menu bar + UI mock:** [>] `ui` extract popup view and visibility state; [>] `tauri` show/hide/focus popup and tray menu; [ ] `qa` lifecycle/accessibility review.
- **M2 PetEngine mock:** [>] `pet` add event queue/idle fallback tests; [ ] `qa` property/boundary tests.
- **M3 Sprite renderer:** [ ] `pet` add placeholder development sheet and visibility/completion tests; no final art.
- **M4 OAuth single account:** [x] `lead` approve environment/secure-store direction and `add_account` DTO/error contract; [x] `gmail` Rust-side env validation and desktop PKCE core; [x] `tauri` Keychain adapter; [>] live OAuth adapters/orchestration and empty-account UI; [ ] final QA token/config audit.
- **M5 OAuth multicuenta:** [ ] `gmail` independent account credentials/revocation; [ ] `ui` account management.
- **M6 Initial sync:** [ ] `gmail` metadata baseline that creates zero historical pending; [ ] `qa` large mailbox tests.
- **M7 Incremental sync:** [ ] `gmail` history pagination, cursor commit, 404 recovery, backoff, poll mutex.
- **M8 Pending mail state:** [ ] `lead` schema/version contract; [ ] `gmail` dedupe/atomic state; [ ] `qa` restart/corruption tests.
- **M9 Notifications:** [>] post-commit notification handoff; [>] `tauri` native notification/sound/activation; [ ] `qa` eligibility, privacy, and duplicate suppression.
- **M10 Open Gmail:** [>] account-row navigation to the matching Gmail identity; exact thread navigation remains separately pending; [>] `qa` multi-login/privacy cases.
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

- [x] `gmail`: document the M4 desktop PKCE sequence and sanitized error model under `src-tauri/src/auth/**`; design only, with no OAuth implementation or contract changes. Commit `d6ef64d`.
- [x] `tauri`: implement M1 popup/tray lifecycle under `src-tauri/src/tray/**`: hidden startup, click-to-show/focus, blur/close-to-hide, macOS Dock suppression, and an explicit Quit tray action. Commit `c96239f`.
- [x] `ui`: extract the M1 popup presentation and a testable visibility store using existing DTOs, with the coordinated `src/App.tsx` composition change only. Commit `def8fa7`.
- [x] `pet`: implement a pure deterministic pet-event queue with idle fallback and boundary/replacement tests under `src/pet/**`; no UI wiring or shared-contract changes. Commit `cfcdf36`.
- [x] `qa`: perform the M0 security/performance baseline audit and commit the reproducible findings report at `docs/qa/M0_BASELINE.md`; production fixes remain with owners. Commit `b4d9641`.

## Active assignments — Add account foundation

- [x] `gmail`: implement the M4 Rust-only client-ID validator, sanitized auth error model, PKCE/state generation, authorization request construction, and deterministic injected-port tests under owned auth/account paths; no live token persistence or command wiring yet. Commit `0e3b8f9`.
- [x] `tauri`: implement and test the M4/M13 `SecureStore` macOS Keychain adapter under `src-tauri/src/secure_store/**`, including protected-data versus unsigned-development selection, UUID isolation, redaction, deletion, and zeroization. Commit `3de02d0`.
- [x] `ui`: implement the Add account controller and accessible pending/success/error/cancelled presentation on top of the approved zero-argument command contract, preserving mockability and preventing duplicate clicks. Commit `0c8e8a7`.
- [x] `qa`: review the approved M4 contract/design and commit a reproducible Add account security acceptance plan; production fixes remain with owners. Commit `e02bcaf`.
- [ ] `lead`: after green specialist commits, approve dependency changes, integrate the Rust Tauri command/browser callback seam, run cross-branch contract tests, and dispatch the implementation audit.

## Active assignment — account and settings views

- [x] `ui`: add internal popup navigation, a dedicated Add account provider view with Gmail as the only enabled provider, a dedicated Settings view, accessible back navigation, and a materially larger settings gear/control; preserve the existing Add account controller and contracts. Commits `4fc1a5f`, `09676fd`, `9be231f`; integrated in `main` at `1e24058`.
- [x] `qa`: review keyboard navigation, focus, disabled-provider semantics, compact layout, dark mode, and accessible control sizing. Final green report `cb6735f`; integrated in `main`.

## Active assignments — live Gmail OAuth and empty state

- [x] `gmail`: live single-account PKCE orchestration, bounded loopback callback, TLS token/identity adapters, normalized account state, and compensating Keychain deletion. Commit `55f08cf`; integrated at `8da2985`.
- [x] `tauri`: non-logging browser adapter and explicit Keychain/auth bridge, reconciled to the final typed UUID set/delete contract. Commits `5935d55`, `9b6e99b`; integrated at `b24d833`, `c2932f6`.
- [x] `ui`: production starts with zero accounts and renders accessible onboarding without invoking OAuth before Gmail selection; success restores the internally scrolling account list. Commit `c7ad963`; integrated at `c017f62`.
- [>] `qa`: run the final integrated M4 acceptance plan and block any token/config exposure, callback weakness, storage-order issue, inaccessible empty state, or contract mismatch.
- [x] `lead`: approve the Rustls HTTP dependency, integrate owner commits, and wire the stable zero-argument Tauri `add_account` command/managed state. Commits `810c80d`, `02d6851`; cross-boundary suites green before final QA.

## Active assignments — M4 QA remediation

- [x] `gmail`: resolve M4-FINAL-01 and M4-FINAL-02 with durable atomic non-secret account persistence plus strict bound-authority Host validation. Commit `5f08b91`; integrated at `c438a7d`.
- [x] `lead`: inject `app.path().app_data_dir()/accounts.json` into the live command state and rerun cross-boundary suites. Commit `1302348`.
- [x] `qa`: targeted re-audit closes M4-FINAL-01 and M4-FINAL-02 at `f3cc7bd`; integrated at `ead93fd`. M4-FINAL-03 through M4-FINAL-07 remain separately tracked in `docs/qa/M4_FINAL_IMPLEMENTATION_AUDIT.md`.

## Active assignments — dynamic tray and Gmail baseline

- [x] `tauri`: implement the internal normalized tray projection under `src-tauri/src/tray/**`: no accounts sleeps as `(-_-)`, account-plus-zero has no counter, positive counts mirror the approved pet mood boundaries, and the update seam is unit tested. Startup/post-add wiring is integrated; committed incremental-sync wiring remains active.
- [x] `gmail`: extend normalized account persistence with snapshot/cursor updates and implement a real read-only Gmail baseline after durable OAuth commit. Baseline failure keeps the account visible and retriable; historical mail remains zero pending.
- [x] `gmail`: implement the approved M7 one-shot incremental history/metadata reader, OAuth access-token refresh, v1-to-v2 atomic mail-state migration, cursor CAS/dedupe/pending recomputation, 404 recovery, and retry/backoff seam.
- [x] `tauri`: expose secure refresh-token reads to M7 through the approved Rust-only zeroizing port; no secret crosses commands, events, logs, or serialized state.
- [x] `lead`: integrate persisted AppSnapshot v2 hydration, immediate post-add polling, per-account 45-second/backoff scheduling, `snapshot_changed`, and tray refresh from the same committed snapshot.
- [x] `ui`: consume `get_app_snapshot` and `snapshot_changed` with race-safe normalization so persisted/new accounts and pending counts update the pet without losing a concurrent add result.
- [ ] `qa`: verify no-account versus account-zero tray state, mood boundaries, durable restart hydration, baseline-zero invariant, sanitized failures, and whether continuous polling is genuinely wired.

## Active assignment — per-account sync status remediation

- [x] `lead`: emit the approved sanitized `sync_status_changed` event from scheduler success/retry/action-required outcomes and add serialization/projection tests.
- [x] `ui`: consume `sync_status_changed`, retain status by local account UUID, render action-required only on the affected account, and clear it on that account's healthy event.
- [ ] `qa`: verify payload minimization, per-account isolation, retry/healthy transitions, and absence of provider/error/identity data.

## Active assignment — OAuth completion lifecycle

- [x] `lead`: restore the main window for every terminal `add_account` result and, on durable success, publish `snapshot_changed` before launching the immediate poll; both native operations remain best-effort.
- [x] `gmail`: remove the blocking post-commit baseline from `AddAccountService`; the M7 scheduler owns baseline/retry for cursor-less accounts.
- [x] `gmail`: classify allowlisted Google token-endpoint configuration failures separately from retryable authorization failure, fail closed on malformed success, and prove provider descriptions/secrets never cross the command boundary.
- [x] `ui`: make retryable token-exchange failure restartable with fixed Spanish copy while continuing to discard raw command/provider messages.
- [x] `gmail`: add the Lead-approved debug-only static token-exchange diagnostic categories and redaction/release-gating tests for real local OAuth acceptance.
- [x] `gmail`: expand the closed diagnostic/configuration allowlist after a real `provider_error_other` result to cover Google `redirect_uri_mismatch`, `invalid_request`, and `unsupported_grant_type` without provider-controlled output.
- [x] `gmail`: normalize the real `invalid_request` description into a debug-only static parameter category, proving raw descriptions and values never reach stderr or command errors.

## Active assignment — distributable Google Desktop credential

- [x] `gmail`: add a redacted/zeroizing Rust-only Desktop client-credential port, require it for authorization-code and refresh grants, and cover compile-time/debug sources plus request redaction.
- [x] `lead`: wire the credential through add-account and sync construction, require build-time configuration for distributable builds, and document safe local/CI invocation without secret values in files or shell history.
- [x] `qa`: release wrapper, A/B rotation, artifact scanner, frontend/DTO/log redaction, and OAuth request shape are green; the ignored debug dotenv is governed separately by Decision 030.
- [x] `lead`: replace value-bearing `env!` release injection with masked `build.rs` output, add a secret-aware textual-artifact scanner, and document clean CI/cache rules.
- [x] `gmail`: consume the generated masked native configuration into validated redacted types without raw generated strings or Cargo env dependencies.
- [x] `gmail`: replace the bespoke debug dotenv parser with `dotenvy` for both Desktop credential variables, preserving process precedence and release isolation.
- [x] `qa`: verified debug dotenv loading from supported working directories, process precedence, no `VITE_` exposure, fail-closed parsing, and release dotenv isolation.
- [x] `lead`: make the protected local release wrapper load a complete ignored root `.env` pair when no process/CI pair exists, reject mixed/partial sources, and cover precedence and file guards with Node tests.

## Active assignment — INBOX unread badges and account unlink

- [x] `gmail`: read bounded `users/me/labels/INBOX.messagesUnread`, persist it independently from pending/history, migrate mail state, and prevent removal/poll races; implement bounded token revocation plus local account/message removal ports.
- [x] `lead`: expose `remove_account`, publish its committed snapshot/tray state, and project global INBOX unread to the native numeric badge without changing PetEngine pending.
- [x] `ui`: list associated accounts in Configuración, show unread badges, provide an accessible confirmation flow for desvincular, invoke the typed command, and update local snapshot/status state without exposing provider details.
- [x] `qa`: verified unread/pending separation, label bounds, multi-account sums, removal isolation/races, Keychain deletion, offline revocation semantics, restart persistence, and accessible focus/progress behavior.
- [x] `gmail`: commit every valid INBOX unread observation before history work and attach the committed snapshot to later sanitized poll failures; reduce healthy interval to 30 seconds.
- [x] `lead`: publish committed unread snapshots even when later incremental work fails, serialize/re-read snapshot projection, run due accounts concurrently, and wake the central scheduler after add/remove.
- [x] `ui`: register `snapshot_changed` before hydration and prevent late OAuth results from restoring stale unread/account state.
- [x] `ui`: normalize the manual-check unit result, prevent indefinite acknowledgement loading with a five-second timeout, keep fast-success pending feedback visible for 800 ms, reset state on Settings exit, and render an accessible reduced-motion-aware spinner and label pulse.
- [x] `pet`: render the application happy mood with the exact face `•‿•` while preserving sleep and other mood faces.
- [ ] `qa`: verify success/error/cancel restoration, result preservation, immediate durable account visibility, event-before-poll ordering, and slow/offline Gmail behavior.

## Active assignment — unified unread presentation

- [x] `gmail`: prove provider, baseline/incremental/recovery, persistence, and PollOutcome preserve a per-account unread value of 3 even when Zerochi pending is 0.
- [x] `ui`: compute the active-account unread sum once and use it for the popup global total while keeping per-account badges on the same normalized field.
- [x] `pet`: drive ambient mood/face/idle-copy ranges from unread total, keep pending transitions separate, and provide deterministic phrase variation.
- [x] `tauri`: drive the native tray face and suffix from the same unread total/ranges, retaining no-account sleep.
- [x] `qa`: verified account/global/tray equality, inactive/missing/malformed handling, all range boundaries, phrase variety/stability, accessibility, and pending/unread separation.

## Active assignment — anchored comic popover

- [x] `ui` via Herdr: audited the current UI against the Hand Mirror anchored-popover pattern and provided an implementation-safe specification without copying assets.
- [x] `ui`: implemented transparent outer canvas, rounded bubble/pointer/shadow, visually hidden home heading, visible subview headings, dark mode, and layout/accessibility tests.
- [x] `tauri`: toggles on left-button down to avoid the blur/up race; on open, positions from the click rectangle before show/focus and covers lifecycle/geometry with tests.
- [x] `lead`: configured the main Tauri window as initially hidden, transparent, undecorated, shadowless, and non-resizable; accepted the macOS private-API packaging impact for this iteration.
- [x] `qa`: verified config, pointer alignment/clamping, transparent CSS structure, toggle/blur lifecycle, 390×600 overflow, dark mode, and keyboard/screen-reader behavior; physical Retina/multimonitor appearance remains manual acceptance.

## Active assignment — official Positioner anchoring

- [x] `tauri`: replaced the manual tray-window geometry with the official `tauri-plugin-positioner` Rust integration (`tray-icon`, plugin registration, `on_tray_event`, and constrained tray-bottom-center movement), preserving the existing CSS pointer event, lifecycle behavior, tray projection, and dirty unrelated work. Lead-approved coordinated scope was limited to `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/lib.rs`, and `src-tauri/src/tray/mod.rs`; 19 focused tray tests and `cargo check` are green, with no contracts change.
- [ ] `qa`: manually verify physical anchoring and pointer alignment on Retina and at left/right edges or secondary monitors after the focused Rust checks pass.

## Active assignment — M9 native new-mail notifications

- [x] `lead`: approved and integrated the post-commit-only handoff in `commands/lib`: inject the native service, publish the committed snapshot, derive an opaque id from each committed `(accountId,messageId)`, notify best-effort, then emit `healthy`; no frontend event or DTO expansion. Ordering/failure-isolation tests, 166 Rust tests, `cargo check`, and 194 frontend tests/build are green.
- [x] `gmail`: changed the atomic incremental commit result and `PollOutcome` under `src-tauri/src/{accounts,sync}/**` to expose exactly the messages newly inserted after dedupe. Baseline, history-expired recovery, unread-only changes, failed CAS, restart hydration, and duplicate provider ids return an empty notification batch. No native APIs or notification presentation entered Gmail-owned code.
- [x] `tauri`: implemented `NotificationService` under `src-tauri/src/notifications/**`, packaged the supplied `notification-chime` as an immutable macOS application resource, requested/used native notification permission without blocking sync, plays the custom sound once per accepted notification, suppresses repeated stable ids defensively, and restores the popup on activation. Presentation is fixed generic Spanish copy and exact Gmail navigation remains M10.
- [ ] `qa`: verify one notification/sound per newly committed message; zero notifications for baseline, history recovery, unread-only changes, restart/hydration, failed commits, permission denial, and duplicate/overlapping polls; audit the built resource and native payload for forbidden metadata/secrets; keep interactive notification permission, sound, and activation as explicit physical macOS acceptance where automation cannot prove them.

## Active assignment — M10 open Gmail account

- [ ] `ui`: make each home account row an accessible account-opening control without interfering with Settings/unlink interactions; add a typed `open_account` invocation that sends exactly `{ accountId }`, prevents accidental duplicate activation while a handoff is pending, and renders only fixed local Spanish failure copy while discarding command/provider text. Scope: `src/{components,views,stores}/**` plus coordinated `src/App.tsx`; tests must cover mouse/keyboard activation, exact invoke payload, pending behavior, account isolation, and sanitized failure presentation.
- [ ] `tauri`: implement the native `ExternalNavigationService` under `src-tauri/src/navigation/**`. Accept an internal resolved account identity, build only an allowlisted HTTPS `mail.google.com` account URL with a percent-encoded `authuser` hint, never use or persist `/u/N/`, invoke `/usr/bin/open` without a shell and with stdio suppressed, and return only an opaque success/failure. Unit tests must cover reserved/non-ASCII email encoding, host/scheme/path invariants, one native handoff, failure redaction, and absence of positional session indices. Update the native manual checklist for default-browser, matching-session, chooser/login, and multi-session behavior.
- [x] `lead`: wired `open_account(accountId)` in `src-tauri/src/{commands/mod.rs,lib.rs}` against the approved contract: canonical UUID validation, persisted `MailAccount` lookup in Rust, injected `ExternalNavigationService`, unit success, and only `account_not_found`, `navigation_failed`, or `internal` fixed sanitized errors. Command-shape, lookup, no-mutation, redaction, and handler-registration tests are green; no email/URL crosses to React and snapshot/events remain unchanged.
- [ ] `qa`: after UI, Tauri, and Lead wiring are integrated, audit the exact `{ accountId }` boundary, URL/log/error redaction, browser-session uncertainty, keyboard/focus semantics, duplicate activation, and state non-mutation. Automate no-session/one-session/multiple-session adapter cases where possible and record physical Chrome/Safari acceptance for matching account, requested account absent, default-browser selection, and Workspace/personal combinations; exact thread navigation is out of this acceptance pass.

## Active assignment — contextual notification activation and forced mail checks

- [x] `lead`: replaced the M9 generic-presentation handoff with the approved internal account/subject/sender projection; extended the central scheduler with coalescing immediate-account and immediate-all events, including one trailing immediate poll when manual intent arrives in flight; made `open_account` enqueue its resolved account without awaiting Gmail; exposed and registered zero-argument `check_mail_now` returning unit with only sanitized `internal` failure. Integrated tests cover enqueue ordering, navigation independence, all-active selection, removed/inactive exclusion without late reinsertion, in-flight coalescing, command registration, fixed errors, and unchanged snapshot/status/notification semantics.
- [x] `gmail`: exposed read-only accessors for the already persisted normalized `MailMessage.sender` and `MailMessage.subject`; no provider calls, bodies/snippets, command DTOs, or frontend events were added. Empty/maximum metadata, rejection bounds, and redacted diagnostics are covered.
- [x] `tauri`: evolved the native notification request/adapter to account title, subject subtitle, sender body, fixed fallbacks, Unicode-safe 512-byte bounds, UUID-only activation correlation, existing sound, and duplicate suppression. Automated adapter/activation/redaction tests and the manual macOS checklist are updated.
- [x] `tauri`/`lead`: evolved native activation correlation to a bounded opaque `accountId + messageId` target, revalidated it against durable state, and added best-effort conversation navigation with one account fallback; no React event/DTO, tab automation, positional session index, or face change.
- [x] `tauri`/`lead`: eliminated the intermittent Dock race with bundle-time `LSUIElement=true` plus runtime Accessory/no-Dock reassertion in setup and restoration paths.
- [x] `ui`: added the typed zero-argument adapter/controller and accessible `Buscar correos ahora` Preferences button with pending/queued/failure local copy, duplicate prevention, backend-text discard, and removal-flow isolation.
- [>] `qa`: audit the integrated wave for native-preview privacy, absence of bodies/snippets/log leakage, exact title/subtitle/body hierarchy, notification-to-matching-account activation, no `/u/N/`, zero-argument manual command, no overlapping polls, no duplicate notifications, and truthful queued-versus-complete UI. Record physical macOS Notification Center/lock-screen preview plus Chrome/Safari multi-session acceptance separately from automated results.

## Active assignment — Gmail readonly History refactor

- [x] `gmail`: replaced OpenID identity and extra scopes with `users.getProfile` plus exact `gmail.readonly`; replaced periodic `labels.get` with paginated authoritative `messages.list(labelIds=INBOX,q=is:unread)` and retained History for bounded candidate/cursor processing; persisted the bounded unread-id set in schema v4 and fetches only `From`, `To`, `Subject` metadata for eligible newly added messages.
- [x] `gmail`/`lead`: made paginated `messages.list(labelIds=INBOX,q=is:unread)` authoritative on every poll, using History only for candidates/cursor, so read/re-unread/archive/Spam/Trash/delete/restart converge without a backend and failures remain atomic.
- [x] `ui`: recover transient `snapshot_changed` registration failures with five cancelable bounded retries, fresh reads around successful registration, monotonic live-revision protection, and no duplicate listener or frontend Gmail polling.
- [x] `tauri`: added explicit macOS sleep/wake lifecycle integration so the single scheduler pauses while asleep and requests one guarded catch-up on wake; existing coalescing and secret boundaries are preserved.
- [x] `ui`: inspection confirms React already consumes only normalized snapshots and sanitized sync status; no Gmail payload or new frontend event is required.
- [x] `pet`: inspection confirms PetEngine already consumes normalized totals/transitions and must not receive Gmail label/history concepts.
- [x] `qa`: focal automated acceptance is green for exact scope/endpoints, paginated baseline/history, label add/remove, deletion, duplicate replay, 404 resync, restart migration, offline/auth/rate-limit failures, sleep/wake, notification eligibility, and forbidden-content scans. Physical Google/macOS gates remain documented.
