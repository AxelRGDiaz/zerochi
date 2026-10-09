# M4 Add Account Security Acceptance Plan

Plan date: 2026-08-10

QA branch baseline after merging `main`: `a97bf900d1be10505f5503ce9158fc4b7576625f`.

Normative contract source: commit `063326967bb917d9a2b7d5306cff78f482a51808`, `.agent/CONTRACTS.md`, ADR 017, and `src/types/contracts.ts`.

Design source reviewed: Gmail commit `d6ef64d`, `src-tauri/src/auth/M4_DESIGN.md`. Where that earlier design conflicts with commit `0633269`, the Lead-owned contract controls.

## Acceptance decision

**GREEN to integrate this plan only. M4 `add_account` is not currently accepted or ready to enable.** The implementation security decision remains blocked until the mandatory automated gates and applicable manual macOS gates below pass against one integrated commit. A command that compiles but has absent, ignored, filtered-out, or zero matching acceptance tests does not pass.

## Current findings and gaps

### M4-GAP-01 — Gmail design conflicts with serialized V1 attempts

SEVERITY: HIGH

AREA: OAuth attempt concurrency

FILE: `.agent/CONTRACTS.md`; `src-tauri/src/auth/M4_DESIGN.md` at Gmail commit `d6ef64d`

PROBLEM: The approved contract permits one active add attempt and requires a second request to return `auth_in_progress` without sharing or cancelling the first. The earlier Gmail design says concurrent attempts are permitted when isolated and includes a two-concurrent-attempt test. Implementing the design literally would violate the normative V1 wire behavior.

REPRODUCTION: Compare `.agent/CONTRACTS.md` at merged `HEAD` with the “Authorization Code + PKCE sequence” and PKCE test 10 in `git show d6ef64d:src-tauri/src/auth/M4_DESIGN.md`.

RECOMMENDED FIX: Implement a single application-level add-attempt guard. Make the loser return the exact `auth_in_progress` wire error before listener allocation, browser launch, or transient secret creation. Update the Gmail-owned design when its owner next edits it; do not weaken the approved contract.

### M4-GAP-02 — Earlier design proposes a non-contract add-account tag

SEVERITY: MEDIUM

AREA: Error contract

FILE: `src/types/contracts.ts`; `src-tauri/src/auth/M4_DESIGN.md` at Gmail commit `d6ef64d`

PROBLEM: The design’s proposed auth table includes `credential_revoked`, but that tag is absent from the Lead-approved `AddAccountErrorTag` union. It may be useful for a later refresh contract, but it cannot cross the M4 `add_account` boundary without Lead approval.

REPRODUCTION: Compare the design’s “Tagged sanitized error model” table with `AddAccountErrorTag` in merged `src/types/contracts.ts`.

RECOMMENDED FIX: Keep any refresh-only variant internal or map it to an allowed M4 tag where semantically correct. Snapshot the exact approved add-account union. Adding `credential_revoked` to the wire is an integration blocker unless Lead first changes the shared contract and adds a serialization compatibility test.

### M4-GAP-03 — Integrated baseline has no executable OAuth or Keychain acceptance surface

SEVERITY: MEDIUM

AREA: Implementation and test readiness

FILE: `src-tauri/src/auth/mod.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/secure_store/mod.rs`, `src-tauri/src/accounts/mod.rs`

PROBLEM: The merged baseline still contains M0 stubs and no `add_account` command, callback listener, token exchange, account persistence orchestration, Keychain adapter, or M4 integration tests. The fixed sanitized message catalog and retry/action metadata are also not implemented. This is expected before specialist integration but means no M4 functional gate can currently pass.

REPRODUCTION: `git show HEAD` of the listed modules shows stub documentation only; `cargo test --locked --manifest-path src-tauri/Cargo.toml` currently executes zero Rust tests.

RECOMMENDED FIX: Integrate the Gmail and Tauri foundations in contract-first order, then add the canonical acceptance targets defined below during Lead-owned command integration. Do not enable the UI command until those targets execute nonzero tests and pass.

### M4-GAP-04 — Webview CSP remains disabled before an OAuth-capable UI

SEVERITY: LOW

AREA: Frontend hardening

FILE: `src-tauri/tauri.conf.json`

PROBLEM: `app.security.csp` remains `null`. Secrets must stay Rust-only, limiting direct token impact, but injected frontend code could still trigger or interfere with a zero-argument OAuth command and spoof status presentation.

REPRODUCTION: `git show HEAD:src-tauri/tauri.conf.json` shows `"csp": null`.

RECOMMENDED FIX: Define a least-privilege CSP before shipping an OAuth-enabled build. If CSP work is scheduled after M4 foundation integration, keep the command feature-disabled in release builds and record Lead acceptance of that sequencing.

### M4-GAP-05 — Rust advisory tooling is unavailable locally

SEVERITY: LOW

AREA: Dependency advisories

FILE: `src-tauri/Cargo.lock`

PROBLEM: Locked Rust dependencies compile, but this worktree cannot evaluate them against RustSec because the `cargo-audit` subcommand is not installed. This is an evidence gap, not proof of a vulnerable crate.

REPRODUCTION: `cargo audit --version` exits with `no such command: audit`; `cargo check --locked` passes.

RECOMMENDED FIX: Run a Lead-pinned `cargo-audit` version in CI against the integrated lockfile and attach the advisory result to acceptance. Unreviewed high/critical reachable advisories block integration.

## Required future test topology

The integrated implementation must provide these discoverable targets, or Lead must record equivalent paths before QA execution:

- `src-tauri/tests/m4_add_account_acceptance.rs`: config, request construction, PKCE/state, callback validation, attempt serialization, error wire snapshots, persistence ordering, cancellation/timeouts, identity, Workspace classification, and secret-leak canaries using injected ports.
- `src-tauri/tests/m4_secure_store_acceptance.rs`: deterministic adapter/selector tests for set/get/update/delete, failure mapping, account isolation, zeroization, and backend selection without touching a real Keychain.
- `src-tauri/tests/m4_keychain_macos.rs`: ignored, serial, real-macOS tests using a fresh synthetic account UUID and guaranteed cleanup; test output must never print secret bytes.
- `src/components/AddAccountController.test.tsx`: zero-argument invocation, duplicate-click suppression, terminal-state recovery, cancellation, keyboard/focus behavior, accessible status, and sanitized presentation.

Canonical automated commands:

```sh
npm ci --ignore-scripts
npm test -- src/components/AddAccountController.test.tsx
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_secure_store_acceptance
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
```

Each named test target must be listed first and must execute at least one test:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance -- --list
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_secure_store_acceptance -- --list
npm test -- src/components/AddAccountController.test.tsx --reporter=verbose
```

## Gate G0 — Contract and provenance

Pass conditions:

- Commit `0633269` is an ancestor of the candidate.
- `add_account` takes no arguments. React supplies no configuration, scope, redirect URI, callback data, account identifier, or token material.
- Success serializes exactly `AddAccountResult { account: MailAccount }` and only after secret storage and account commit succeed.
- Failure serializes exactly `{ tag, message, retryable, actionRequired }`, with a tag from the approved union and no extra source/cause/details field.
- One attempt is active at a time; cancellation is idempotent.

Commands:

```sh
git merge-base --is-ancestor 0633269 HEAD
git diff 0633269..HEAD -- .agent/CONTRACTS.md src/types/contracts.ts
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance contract_
npm test -- src/components/AddAccountController.test.tsx -t "zero-argument contract"
```

Any shared-contract diff requires explicit Lead approval before the remaining gates run.

## Gate G1 — No secrets or OAuth configuration in frontend, repository artifacts, or bundles

Pass conditions:

- Only Rust reads `ZEROCHI_GOOGLE_CLIENT_ID`; no OAuth `VITE_` variable exists.
- No frontend runtime field or command argument can carry a client ID, scope, redirect URI, callback, verifier, code, state, token, header, credential document, or Keychain error.
- No `.env`, downloaded credential JSON, private key, real user metadata, or secret-bearing fixture is tracked.
- Production bundles contain no OAuth environment name/value, secret field, credential path, or synthetic leak canary.
- Tests generate synthetic canaries at runtime and check captured serialization/log buffers without printing the canaries.

Static commands, which print filenames rather than matching contents:

```sh
git grep -Il -E '(import\.meta\.env\.[A-Z0-9_]*(OAUTH|GOOGLE|CLIENT|TOKEN|SECRET)|VITE_[A-Z0-9_]*(OAUTH|GOOGLE|CLIENT|TOKEN|SECRET)|ZEROCHI_GOOGLE_CLIENT_(ID|SECRET)|client_secret[[:space:]]*[:=])' HEAD -- src index.html vite.config.ts
git ls-tree -r --name-only HEAD | rg -i '(^|/)(\.env($|\.)|credentials?[^/]*\.json$|client[_-]?secret[^/]*\.json$|google-oauth[^/]*\.json$|.*\.(pem|key|p12|pfx)$)'
npm run build
rg -l -I -e '(VITE_[A-Z0-9_]*(OAUTH|GOOGLE|CLIENT|TOKEN|SECRET)|ZEROCHI_GOOGLE_CLIENT_(ID|SECRET)|client_secret[[:space:]]*[:=]|replace-with-your-desktop-client-id)' dist
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance secret_non_reachability_
```

Expected static output is empty except the tracked-filename scan may report only `.env.example`; any bundle match blocks integration and must be inspected without copying its contents into QA artifacts.

## Gate G2 — Exact scope, Desktop-client request, and no frontend secret exposure

Pass conditions:

- The authorization request’s scope set is exactly `https://www.googleapis.com/auth/gmail.readonly`, with no duplicates, OIDC scopes, or broader Gmail scope.
- Authorization parameters contain the validated client ID, exact loopback redirect URI, `response_type=code`, `code_challenge_method=S256`, challenge, state, offline access, and the approved consent behavior.
- The authorization URL excludes verifier, authorization code, token, and `client_secret`.
- The token request contains code, verifier, client ID, its paired Desktop-client secret, exact redirect URI, and `grant_type=authorization_code`; none of those values crosses the Rust/Tauri boundary.
- Returned scopes are parsed as a set and fail closed unless the set is exactly the one approved Gmail readonly scope.

Command and required test names:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance oauth_request_
```

Required cases: `oauth_request_exact_scopes`, `oauth_request_uses_s256_and_offline_access`, `oauth_request_excludes_verifier_and_client_secret`, `token_request_uses_paired_desktop_client_credentials`, and `token_response_rejects_scope_drift`.

## Gate G3 — PKCE/state entropy and callback validation

Pass conditions:

- Verifier input has at least 32 bytes of cryptographic entropy and encodes to 43–128 RFC 7636 unreserved characters.
- State is independently generated with at least 128 bits of cryptographic entropy. The verifier and state cannot share bytes or a deterministic seed in production.
- Challenge is unpadded base64url of SHA-256(verifier) and matches a fixed RFC test vector.
- Listener binds only `127.0.0.1` on an OS-assigned ephemeral port and accepts one request on one fixed callback path before a monotonic deadline.
- Callback accepts only the expected method/path and a single valid state plus exactly one of code or error. State comparison is constant-time.
- Missing/duplicate/malformed parameters, code-plus-error, wrong method/path/host, oversized request, mismatched state, second callback, and post-timeout callback are rejected before exchange.
- Browser response HTML is static, bounded, and reflects no query/provider text. Every exit closes the listener and zeroizes/drops transient material.

Command and required test groups:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance pkce_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance callback_
```

The entropy gate requires code review of the production RNG source in addition to deterministic tests; collision sampling alone is not proof of cryptographic entropy.

## Gate G4 — Serialized attempts

Pass conditions:

- The first attempt owns the guard until success, failure, timeout, or cancellation cleanup completes.
- A second request returns sanitized `auth_in_progress` before RNG use, listener bind, browser open, provider call, SecureStore access, or state mutation.
- The second request does not observe, share, cancel, or extend the first attempt.
- The guard is released on every terminal path, including injected port failures and cancellation races; a later attempt can start once cleanup finishes.

Command:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance serialized_attempt_
```

Required cases cover rapid duplicate calls, calls from separate webview tasks, timeout release, cancellation release, browser failure release, store failure release, and commit-cleanup release.

## Gate G5 — Sanitized error wire shape and diagnostics

Pass conditions:

- Every approved `AddAccountErrorTag` has a fixed user message and reviewed retry/action metadata. Unknown internal causes map to `internal` without source detail.
- JSON uses exactly `tag`, `message`, `retryable`, and camel-case `actionRequired`; no fifth field is serialized.
- `Display`, `Debug`, Tauri rejection serialization, structured logs, browser HTML, and UI text exclude URLs, emails/domains, provider bodies/codes unless explicitly allow-listed, environment values, callback data, account IDs, paths, adapter errors, headers, and all transient/credential material.
- Source errors remain internal and are not serialized through transparent wrappers.
- `auth_in_progress` and `cancelled` use the same safe wire shape. No `credential_revoked` value crosses this command.

Commands:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance error_wire_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance diagnostics_redaction_
npm test -- src/components/AddAccountController.test.tsx -t "sanitized errors"
```

Snapshot review must verify the full approved tag union is covered exactly once and that generated synthetic canaries are absent from every captured sink.

## Gate G6 — SecureStore and Keychain behavior

Pass conditions:

- Service name is exactly `com.zerochi.desktop.oauth.refresh-token`; account key is the stable local account UUID.
- `set/get/update/delete` round trips exact opaque bytes without stringifying, logging, serializing, or exposing them to React. Update replaces only the selected account. Delete is auditable and idempotent.
- Distinct account UUIDs cannot read, overwrite, or delete one another, including prefix/similar-ID cases.
- Locked Keychain and denied access fail closed as sanitized `secure_store_failed`; no fallback storage, plaintext persistence, debug value, account commit, or other-account mutation occurs.
- Temporary input/output and rotated-token buffers use a zeroizing secret type. Tests instrument drop/cleanup paths for success, update, adapter error, cancellation, and panic-safe unwinding where supported.
- Signed/provisioned selection uses Protected Data. Explicit unsigned development selection uses legacy Keychain. A signed build never silently downgrades to legacy after Protected Data failure; an unsigned build never selects a mock/plaintext backend.

Commands:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_secure_store_acceptance secure_store_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_secure_store_acceptance backend_selection_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_secure_store_acceptance zeroization_
```

Unit tests must use injected adapters. Real Keychain evidence is collected only through G11 and must use a fresh synthetic UUID with cleanup.

## Gate G7 — Persistence ordering and compensating cleanup

Pass conditions:

- Identity and eligibility validation finish before persistence.
- A new stable UUID is generated; only refresh-token bytes go to SecureStore. Access token, authorization code, verifier, state, token response, and callback never enter account state or SecureStore.
- Ordering is `SecureStore.set` then atomic non-secret account commit then success publication.
- Store failure creates no account. Account commit failure calls `SecureStore.delete` exactly once. Cleanup failure remains a sanitized failure and never becomes success.
- Cancellation at every boundary, including after token exchange and after secret set, leaves neither account nor secret. Post-token cancellation performs bounded best-effort provider revocation without changing the `cancelled` result.
- Success is impossible before both durable operations complete; other accounts and their secrets remain byte-for-byte unaffected in every injected failure.

Command:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance persistence_ordering_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance compensating_cleanup_
```

The injected ports must record calls so tests assert exact ordering and call counts, not only final state.

## Gate G8 — Cancellation, timeouts, provider denial, and Workspace restrictions

Pass conditions:

- Cancellation is idempotent before browser launch, while awaiting callback, during bounded provider work, after token exchange, and during pre-commit cleanup. It closes the listener, releases the attempt guard, and persists nothing.
- Callback wait, token exchange, identity fetch, revocation, and cleanup use explicit bounded deadlines. Timeout maps safely, aborts remaining work where possible, and cannot later commit on a late completion.
- Explicit user refusal maps to `cancelled`; unknown provider denial maps to `provider_denied`; documented organization/Gmail policy denial maps to `workspace_restricted` without preserving raw text.
- Personal and eligible Workspace identities pass. Missing/unverified identity and suffix-only Workspace inference fail closed. Restrictions affect only the attempted account and never erase existing accounts.
- Offline/transport failures map to `network_unavailable` or the reviewed transient exchange form and retain no new durable state.

Command:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance cancellation_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance timeout_
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_add_account_acceptance workspace_
```

Tests use a paused/injected monotonic clock and deterministic provider fixtures; no test contacts Google.

## Gate G9 — UI duplicate-click and accessibility behavior

Pass conditions:

- UI calls `add_account` with no argument object and treats the returned account/error as untrusted structured data.
- Pointer double-click, rapid keyboard activation, and activation from two UI events result in one invocation while pending. The control is disabled or otherwise guarded, exposes pending state, and cannot cancel the active Rust attempt accidentally.
- All terminal outcomes restore operability. Success is announced without leaking email beyond the approved account presentation; cancellation is neutral; fixed sanitized errors are announced once.
- Add control has an accessible name, visible focus, keyboard activation, correct disabled/busy semantics, and a polite status region. Focus returns to a sensible target after browser completion/cancellation.
- UI never renders provider HTML, raw error/source fields, callback text, or URLs.

Command:

```sh
npm test -- src/components/AddAccountController.test.tsx
npm run build
```

Required test names include `invokes_add_account_without_arguments`, `coalesces_pointer_double_click`, `coalesces_keyboard_repeat`, `announces_pending_and_terminal_status`, `restores_focus_after_completion`, and `renders_only_sanitized_error_fields`.

## Gate G10 — Dependency and build advisories

Pass conditions:

- Lockfiles are unchanged except for Lead-approved dependencies required by the implementation.
- Frontend tests/build and locked Rust tests/check pass.
- npm production and full advisory scans report no unaccepted vulnerability.
- RustSec reports no unaccepted reachable advisory. High/critical advisories always block; lower findings require documented Lead reachability analysis and expiry date.
- Newly introduced crypto, randomness, URL, HTTP, callback, cancellation, and zeroization crates use minimal features and are visible in the reviewed dependency diff.

Commands:

```sh
npm ci --ignore-scripts
npm run check
npm audit --omit=dev --ignore-scripts
npm audit --ignore-scripts
cargo check --locked --manifest-path src-tauri/Cargo.toml
cargo test --locked --manifest-path src-tauri/Cargo.toml --all-targets
cargo tree --locked --manifest-path src-tauri/Cargo.toml -d
cargo audit --file src-tauri/Cargo.lock --deny warnings
git diff 0633269..HEAD -- package.json package-lock.json src-tauri/Cargo.toml src-tauri/Cargo.lock
```

Advisory results are time-sensitive; record tool versions, database timestamp, candidate commit, and result in the implementation audit.

## Gate G11 — Manual macOS browser and Keychain matrix

Run on the exact candidate `.app` on a supported macOS version. Use dedicated test Google identities and synthetic local account UUIDs. Evidence must redact browser URLs, callback queries, emails/domains, Keychain item contents, tokens, codes, state, verifier/challenge, and provider bodies.

### Browser cases

Launch command for an unsigned development candidate with a local ignored `.env` containing only the two approved paired Desktop-client OAuth variable names:

```sh
npm run tauri dev
```

Manually verify:

1. Personal Google approval shows only the approved Gmail readonly scope, returns one account, and leaves no secret/config in the webview inspector or console.
2. Eligible Workspace approval succeeds without inferring policy from email suffix.
3. Workspace admin/Gmail restriction returns only the fixed `workspace_restricted` presentation and leaves existing accounts untouched.
4. User denial returns neutral `cancelled`; closing the browser and waiting returns bounded `timeout`; retry works after each.
5. Rapid mouse/keyboard activation opens one browser attempt; a direct second command request returns `auth_in_progress` without affecting the first.
6. Wrong path, wrong state, duplicate parameters, and a second callback are rejected with static browser content and no token exchange. Use only the acceptance harness’s generated synthetic callback requests; never copy a real callback URL.
7. Offline token/identity phases return sanitized retryable status, create no durable account, and recover on a later fresh attempt.

### Keychain cases

The ignored serial test runner command is:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_keychain_macos -- --ignored --test-threads=1
```

Run and record these cases individually when operator setup is required:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_keychain_macos macos_keychain_unlocked_round_trip -- --ignored --exact --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_keychain_macos macos_keychain_locked_fails_closed -- --ignored --exact --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_keychain_macos macos_keychain_denied_fails_closed -- --ignored --exact --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --test m4_keychain_macos macos_keychain_account_isolation -- --ignored --exact --test-threads=1
```

Operator matrix:

1. With the login Keychain unlocked, verify set/get/update/delete and repeated delete. Inspect metadata only in Keychain Access: exact service and synthetic UUID account; never reveal the value. Confirm cleanup removes the item.
2. Lock the login Keychain in Keychain Access, run the locked case, and verify sanitized failure, no normal account, and no plaintext fallback. Unlock afterward and confirm no orphan item.
3. In a clean test login/session, deny the application’s Keychain access prompt, run the denied case, and verify the same safe boundary behavior without adapter text crossing to UI/logs.
4. Create two synthetic UUID entries, update/delete one, and verify the other remains accessible and unchanged through in-process assertions with no value output.
5. Unsigned development build selects only the explicit legacy backend. A signed/provisioned acceptance build selects Protected Data. Build with `npm run tauri build`, inspect code-signing status with `codesign --display --verbose=4 src-tauri/target/release/bundle/macos/Zerochi.app`, then run the same round-trip suite. Protected Data failure must not downgrade to legacy.
6. Force account commit failure after a real Keychain set through the acceptance harness and verify compensating deletion leaves no orphan item.

If a restricted Workspace tenant or signed/provisioned test build is unavailable, Lead may merge foundations only with an explicit recorded deferral; `add_account` must remain disabled and M4 security acceptance remains blocked until the corresponding manual evidence exists.

## Baseline commands and results

Executed against merged commit `a97bf900d1be10505f5503ce9158fc4b7576625f` before creating this plan:

| Command | Result |
|---|---|
| Read `.agent/AGENTS.md`, `PRODUCT.md`, `ARCHITECTURE.md`, `CONTRACTS.md`, `TASKS.md`, `STATUS.md` | PASS; QA M4 acceptance-plan assignment and ownership confirmed. |
| `git merge --no-edit main` | PASS; merge commit contains Lead contract commit `0633269`. |
| `git merge-base --is-ancestor 0633269 HEAD` | PASS. |
| `git show d6ef64d:src-tauri/src/auth/M4_DESIGN.md` | Reviewed; contract drifts recorded in M4-GAP-01 and M4-GAP-02. |
| Filename-only committed frontend config scan | PASS; no runtime frontend OAuth config/client-secret match. |
| Reachable-history secret-shape scan | PASS; zero secret-shaped matches. |
| Tracked credential-artifact scan | PASS; only `.env.example`. |
| `npm run build` plus filename-only `dist` OAuth/config scan | PASS; build succeeded and scan returned no match. |
| `npm audit --omit=dev --ignore-scripts` | PASS; zero known production vulnerabilities. |
| `npm audit --ignore-scripts` | PASS; zero known production/development vulnerabilities. |
| `cargo check --locked` | PASS. |
| `cargo audit --version` | NOT RUN; subcommand unavailable, recorded in M4-GAP-05. |

## Integration-blocking criteria

Enabling or declaring M4 `add_account` accepted is blocked by any of the following:

- Contract `0633269` absent, modified without Lead approval, or implemented with arguments, concurrent attempts, extra error fields/tags, or pre-commit success.
- Any OAuth config, credential/token material, real user metadata, callback/provider detail, or synthetic canary reaches frontend source, bundle, command result, browser HTML, logs, fixtures, or plaintext persistence.
- Any scope beyond the exact approved set; any client credential reaching frontend/logs/plaintext persistence or not belonging to the same active Desktop client; non-S256 PKCE; insufficient/linked entropy; non-loopback/fixed-port listener; state/callback validation failure.
- Missing, filtered-out, ignored-only, zero-count, flaky, or failing mandatory acceptance tests.
- Keychain plaintext/mock fallback, incorrect backend selection, cross-account access, non-idempotent deletion, unredacted locked/denied failures, or unverified cleanup/zeroization.
- Incorrect store/commit ordering, orphan secret/account after failure or cancellation, late commit after timeout, or mutation of another account.
- UI duplicate invocation, raw error rendering, inaccessible pending/terminal state, or inability to retry safely.
- An unreviewed high/critical dependency advisory, an unexplained lockfile/dependency change, or a failing locked build/test.
- Missing applicable manual browser/Keychain evidence. The only permitted deferral is the explicitly recorded Lead exception above, with the feature disabled and acceptance still blocked.

No production code, tests, shared contract, dependency, or coordination document is changed by this plan.
