# Gmail readonly History refactor — final QA report

Audit date: 2026-08-12  
Workspace: `/Users/alcheco/Developer/zerochi`  
Inspected baseline: `7dc0d77` plus the current integrated, uncommitted Lead/Gmail/Tauri working tree  
Mode: production source read-only; only this QA report and the obsolete scope language in `M4_ADD_ACCOUNT_ACCEPTANCE.md` were edited.

## Decision

**PASS focal for the Gmail readonly/History refactor; RED only for the repository-wide release gate.** Provider, state, sync, command, and lifecycle focal suites pass, and the requested provider surface is implemented fail-closed. GRH-01 is closed by the scheduler’s start-of-iteration event drain plus its two sleep/wake regressions. GRH-03 and GRH-04 are closed by the new provider-status matrix, durable-state failure matrix, and v1-v3 migration round trips. The repository-wide full suites remain red only because of the preexisting out-of-scope pet/tray debt in GRH-02.

No OAuth value, token, provider body, callback value, identity, message content, or Keychain value was read into or copied into this report.

## Closed findings

### GRH-01 — Queued sleep pause priority before due-account dispatch — CLOSED

SEVERITY: RESOLVED

AREA: macOS lifecycle / scheduler ordering

FILE: `src-tauri/src/commands/mod.rs:339-357`; `src-tauri/src/commands/mod.rs:404-480`; `src-tauri/src/commands/mod.rs:1502-1565`; `src-tauri/src/platform/sleep_wake.rs:46-67`

PROBLEM: The previous loop could carry a queued native `Pause` into an iteration and still select due accounts before consuming it.

REPRODUCTION: `queued_sleep_is_drained_before_due_accounts_are_selected` queues `Pause` with an account already due. The scheduler drains all pending events at the beginning of the iteration, sets `paused`, and selects no due account. `sleep_pauses_dispatch_and_wake_makes_idle_accounts_due_now` separately proves idle wake catch-up and one trailing request for an in-flight account.

RECOMMENDED FIX: None. Keep both regressions and the start-of-iteration drain. Physical macOS sleep/wake remains a mandatory artifact gate because unit tests do not instantiate NSWorkspace power notifications.

## Open findings

### GRH-02 — Global full-suite gate is red because preexisting pet/tray face contracts disagree

SEVERITY: MEDIUM

AREA: repository-wide release gate / preexisting UI and tray projection debt outside the Gmail refactor

FILE: `src/pet/PetRenderer.tsx:3-9`; `src/pet/PetRenderer.test.tsx:26-54`; `src-tauri/src/tray/mod.rs:185-215`; `src-tauri/src/tray/mod.rs:555-603`

PROBLEM: Production renders the happy face as `• ᴗ •` and the satisfied tray face as `(• - •)`, while some tests require the prior happy glyph and one tray test requires the happy face for zero unread. The Rust test file also contains contradictory happy expectations. This does not invalidate Gmail History behavior, but it makes both mandatory full test gates fail.

REPRODUCTION: Run the full Rust and frontend suites listed under “Executed checks.” Rust reports 213/215 passing with two tray failures; Vitest reports 223/225 passing with two `PetRenderer` failures.

RECOMMENDED FIX: Lead/UI should select one canonical face mapping, align production and all expectations atomically, and rerun both complete suites. Do not weaken the Gmail/History focal assertions to obtain green.

## Additional closed findings

### GRH-03 — Provider retry classification lacked an explicit 429 regression matrix — CLOSED

SEVERITY: RESOLVED

AREA: Gmail HTTP error coverage

FILE: `src-tauri/src/gmail/mod.rs:701-720`; `src-tauri/src/gmail/mod.rs:1698-1770`; `src-tauri/src/sync/mod.rs:487-496`; `src-tauri/src/sync/mod.rs:1304-1370`

PROBLEM: The shared status mapping already classified Gmail 401/403 as auth-required and 429/5xx as unavailable/retryable, but the endpoint matrix was not explicitly pinned.

REPRODUCTION: `classifies_gmail_and_refresh_statuses_across_all_read_endpoints_without_leakage` now runs 401, 403, 429, and 503 across profile, unread list, History, metadata, and refresh, checking exact internal categories and canary non-disclosure. `auth_and_retryable_provider_failures_preserve_durable_state` runs auth-required and retryable failures at refresh/profile/list/history/metadata stages and proves the durable snapshot is unchanged and sanitized.

RECOMMENDED FIX: None. Keep both matrices in the focal suite.

### GRH-04 — v1 and v3 migration tests did not prove restart after the v4 baseline commit — CLOSED

SEVERITY: RESOLVED

AREA: durable schema migration coverage

FILE: `src-tauri/src/accounts/mod.rs:629-729`; `src-tauri/src/accounts/mod.rs:1294-1367`; `src-tauri/src/accounts/mod.rs:1590-1673`

PROBLEM: The implementation routed v1-v3 through the same safe legacy normalization, but only v2 previously proved the complete restart path.

REPRODUCTION: All three migration tests now use `assert_legacy_migration_round_trip`: load legacy state; assert cursor/unread reset; commit fresh baseline; drop/reopen v4; commit one incremental message; drop/reopen again; assert message, pending, unread, cursor, and schema v4 durability.

RECOMMENDED FIX: None. Keep the shared round-trip harness for v1-v3.

## Functional verification matrix

| Requirement | Result | Evidence |
|---|---|---|
| Exact OAuth scope | PASS | `auth/request.rs` defines only `gmail.readonly`; token/refresh scope validation rejects missing, duplicate, OIDC, extra, and broader scopes. |
| No forbidden Google services | PASS | Production provider source contains OAuth token/revoke plus Gmail `users.getProfile`, `messages.list`, `history.list`, and `messages.get(format=metadata)` only. No OIDC userinfo, `labels.get`, Pub/Sub, `watch`, Gmail mutations, `full`, `raw`, attachment request, body request, or snippet consumption was found. |
| Profile identity | PASS | Account identity and baseline cursor come from Gmail `users.getProfile`; display name is absent and email/history ID are bounded and validated. |
| Baseline/list pagination >32 KiB | PASS | `q=is:unread`, `maxResults=500`, bounded page tokens, deduplication, 100-page/25,000-ID limits, and a two-page >32 KiB regression pass. No `labels.get` request is made. |
| History pagination >32 KiB | PASS | All four event categories are parsed without `historyTypes`, pages are bounded to 1 MiB/100 pages/20,000 events, and the >32 KiB two-page regression passes. |
| Labels add/remove, delete, duplicates | PASS | Reducer applies ordered label changes, deletion wins within one provider record, duplicate additions are idempotent, and cursor/unread/messages commit atomically with CAS. |
| Current metadata UNREAD | PASS | Only newly added candidates still in the reduced unread set are fetched; `messages.get` requests metadata headers only; the returned current `labelIds` must still include `UNREAD` before pending/notification eligibility. |
| History 404 full resync | PASS | A History-list 404 triggers profile + paginated unread baseline replacement. It advances the cursor and unread set with zero imported pending messages and zero `new_messages`, so no notification is emitted. |
| v1-v3 to v4 | PASS — GRH-04 closed | Each legacy version clears unverifiable cursor/unread, persists a fresh v4 baseline, survives restart, accepts an incremental commit, and survives a second restart. |
| Offline/auth/429/5xx | PASS — GRH-03 closed | The HTTP matrix covers 401/403/429/503 at every read/refresh endpoint; the sync matrix proves action/retry classification, bounded delay, unchanged durable state, committed snapshot, and redaction at every stage. |
| Scheduler no-overlap/concurrency | PASS | Per-account engine guard rejects overlap; scheduler tracks `in_flight`; a slow account does not exclude another due account; repeated manual requests coalesce to one trailing poll. |
| Sleep/wake lifecycle | PASS — GRH-01 closed | Native duplicate events are gated; queued lifecycle events drain before eligibility/due selection; idle wake and in-flight trailing catch-up semantics pass without overlap. Physical macOS evidence remains manual. |
| Notification only post-commit | PASS | Incremental state is durably written before `PollOutcome`; command ordering is snapshot publication, notification, then healthy status. Unread-only and full-resync outcomes carry no new messages. |
| Lead-owned commands/lib and Tauri platform | REVIEWED | Central persistent event channel, detached per-account tasks, serialized snapshot reread/publication, partial snapshot before retry status, observer installation before scheduler start, and process-lifetime NSWorkspace registrations were inspected. No provider or account data crosses the native lifecycle callback. |

## Executed checks

All Rust commands used `/private/tmp/zerochi-gmail-readonly-qa-target` and `--locked`.

| Check | Result |
|---|---|
| `cargo test ... gmail::tests::` | PASS — 23/23 |
| `cargo test ... sync::tests::` | PASS — 17/17 |
| `cargo test ... accounts::tests::` | PASS — 22/22 |
| `cargo test ... commands::tests::` | PASS — 25/25, including `queued_sleep_is_drained_before_due_accounts_are_selected` |
| `cargo test ... platform::` | PASS — 4/4 |
| Full `cargo test ... -- --test-threads=1` | FAIL — 213/215; both failures are GRH-02 |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS |
| `cargo check --locked` after GRH-03/04 integration | PASS |
| `cargo fmt --check` | PASS |
| Frontend `npm run check` | FAIL during Vitest — 223/225; GRH-02; later stages did not run in that command |
| Standalone `npm run test:release-env` | PASS — 6/6 |
| Standalone `npm run build` | PASS |
| `git diff --check` including final QA-doc edits | PASS |

## Mandatory manual gates

Run against the exact signed candidate `.app` on a supported physical macOS host. Record only boolean/status evidence; redact account identity, OAuth URLs/values, tokens, cursors, message IDs/content, Keychain values, and provider bodies.

- Google Console: the active Desktop client and its paired secret belong to the intended active project; Gmail API is enabled; the consent screen requests only Gmail readonly; no Pub/Sub topic/subscription or Gmail watch is provisioned for Zerochi.
- Consent/identity: a fresh dedicated test identity sees only Gmail readonly, connects successfully, and the stored account identity equals the Gmail profile identity without an OIDC request.
- Baseline: use a mailbox requiring at least two unread-list pages and record only page-count class and final boolean equality with Gmail UI. Confirm historical unread mail creates no pending item or notification.
- Incremental History: after baseline, create one unread message, mark/read/unread it, delete another, and repeat delivery/retry conditions. Confirm final unread equality, one pending/notification only for a genuinely new message still currently unread, and no duplicates.
- 404 recovery: with a controlled expired cursor fixture/harness, verify full resync, zero historical pending/notification, durable v4 state, and normal incremental polling after restart.
- Offline/rate/auth: exercise network loss, 429, 5xx, revoked authorization, locked/denied Keychain, and recovery. Confirm retry/action status is sanitized, prior durable snapshot remains visible, and no duplicate notification appears.
- Sleep/wake: sleep while idle and while a poll is deliberately in flight. Confirm no dispatch occurs after `WillSleep`, no overlap, one catch-up per account on wake, exactly one trailing catch-up for the in-flight account, and duplicate native notifications do not multiply polls.
- Forbidden surface: capture method/path/query-name metadata only and verify zero `labels.get`, Pub/Sub/watch, Gmail mutation, `full`, `raw`, attachment, body, or snippet request. Do not retain authorization headers or query values.
- Notification ordering: force durable-write failure and verify zero notification; on success verify the committed snapshot is observable before the notification and that replay/restart does not notify twice.

## Acceptance criteria

Lead may accept the Gmail readonly/History refactor after the provider/lifecycle manual gates are attached to the exact signed artifact. GRH-01, GRH-03, and GRH-04 are closed with focal automated evidence. GRH-02 does not invalidate the focal refactor result, but it independently blocks a repository-wide release declaration until the full Rust and frontend checks are green.
