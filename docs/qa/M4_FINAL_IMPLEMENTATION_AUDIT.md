# M4 final implementation audit

Audit date: 2026-08-10
Integrated commit: `265b4c7e31dc0ec1dfad9047c17be4fdce52b1f1`
Required Lead commit: `02d6851b7652995efba2ab88d42d24c78de59ea8` (verified ancestor)
Verdict: **BLOCKED for M4 integration/release**

## Scope and provenance

This read-only audit covered the live zero-argument Gmail `add_account` path, Google browser/HTTP adapters, loopback callback, macOS Keychain adapter and selector, persistence/rollback order, frontend secret boundary, and empty-account UI. The normative sources were `.agent/CONTRACTS.md` and the available QA plan, `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md`.

The requested `docs/qa/M4_ADD_ACCOUNT_SECURITY_PLAN.md` is not present in the integrated tree or reachable Git history. No production code, tests, shared contracts, protected coordination files, or credential inputs were changed.

## Findings

### M4-FINAL-01 — Successful accounts are not durably committed

SEVERITY: HIGH

AREA: Account persistence and Keychain lifecycle

FILE: `src-tauri/src/accounts/mod.rs:81`, `src-tauri/src/commands/mod.rs:39`, `.agent/CONTRACTS.md:29`, `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:255`

PROBLEM: The live command injects `InMemoryAccountState`, whose own documentation says it is process-local. The service can therefore store a refresh token in Keychain, append the account only to a `Vec`, and return success. After process restart the account record is gone while the Keychain credential can remain orphaned. This does not satisfy the contract's committed account record or G7's two durable operations.

REPRODUCTION: Run `rg -n 'InMemoryAccountState|process-local|accounts: Arc' src-tauri/src/accounts/mod.rs src-tauri/src/commands/mod.rs`; inspect `AddAccountService::add_account` at `src-tauri/src/auth/service.rs:85-108`. The passing ordering unit test uses an injected fake `AccountStatePort`; it does not prove restart durability.

RECOMMENDED FIX: Route to the account/persistence owner. Keep live add-account disabled until a durable atomic non-secret account adapter is injected, restart recovery is tested, and commit failure still performs verified Keychain deletion before returning a sanitized failure.

### M4-FINAL-02 — Loopback callbacks accept an unvalidated HTTP Host

SEVERITY: MEDIUM

AREA: OAuth callback validation

FILE: `src-tauri/src/auth/callback.rs:160`, `src-tauri/src/auth/callback.rs:302`, `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:185`

PROBLEM: `parse_request` validates method, request target, HTTP version, query cardinality, and state, but never parses or validates the `Host` header. A request carrying the correct path/state and a wrong or duplicate Host is accepted, contrary to the explicit wrong-host rejection gate. The current callback test helper always supplies a valid Host and has no negative Host case.

REPRODUCTION: Run `nl -ba src-tauri/src/auth/callback.rs | sed -n '160,219p'` and `nl -ba src-tauri/src/auth/callback.rs | sed -n '302,342p'`; `rg -n 'Host|host' src-tauri/src/auth/callback.rs` finds only test request construction, not validation.

RECOMMENDED FIX: Route to the Gmail/auth owner. Parse bounded headers, require exactly one syntactically valid Host matching `127.0.0.1:<bound-port>`, reject absolute-form targets and duplicates, and add loopback tests for missing, wrong, malformed, and duplicate Host values before token exchange.

### M4-FINAL-03 — Cancellation and post-token rollback are not live-complete

SEVERITY: MEDIUM

AREA: OAuth cancellation, timeout, and rollback

FILE: `src-tauri/src/auth/service.rs:85`, `src-tauri/src/auth/service.rs:111`, `src-tauri/src/lib.rs:24`, `src/App.tsx:26`, `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:263`

PROBLEM: `AddAccountService::cancel` is only exercised internally and is not reachable from the registered Tauri/UI boundary; the only registered command is `add_account`. The service has no bounded provider-revocation port, and it performs no cancellation check after account commit. Tests do not inject cancellation during token exchange, identity, secret set, cleanup, or commit. Consequently the required late-cancellation/no-durable-state and best-effort post-token revocation behavior is unproven and partly absent.

REPRODUCTION: Run `rg -n 'invoke_handler|add_account|cancel' src-tauri/src/lib.rs src-tauri/src/commands/mod.rs src/App.tsx src/stores/tauriAddAccount.ts` and `nl -ba src-tauri/src/auth/service.rs | sed -n '54,121p'`. `cargo test --locked -- --list | rg 'cancel|revok'` lists guard/callback tests but no phase-race or revocation tests.

RECOMMENDED FIX: Route the behavior and any contract-visible cancellation seam through Lead approval. Add bounded cancellation at every async boundary, define the commit/cancel race outcome, delete any stored secret before returning `cancelled`, perform bounded best-effort provider revocation after token issuance, and cover every phase with deterministic injected-port tests.

### M4-FINAL-04 — Mandatory acceptance and real macOS evidence are absent while OAuth is live

SEVERITY: HIGH

AREA: Release acceptance coverage

FILE: `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:87`, `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:340`, `.agent/STATUS.md:5`, `src-tauri/src/lib.rs:24`

PROBLEM: The plan's mandatory cross-boundary test targets and real macOS Keychain target do not exist, and the UI/Rust integration test named by the plan is absent. Interactive browser, signed Protected Data, unsigned legacy Keychain, locked/denied, Workspace, and real cleanup evidence remains explicitly outstanding, yet the live command is registered and Gmail is enabled. The plan states that missing applicable tests/manual evidence blocks acceptance and permits deferral only with the feature disabled.

REPRODUCTION: `cargo test --locked --test m4_add_account_acceptance -- --list` fails with `no test target named m4_add_account_acceptance`. File checks report absent `src-tauri/tests/m4_add_account_acceptance.rs`, `m4_secure_store_acceptance.rs`, `m4_keychain_macos.rs`, and `src/components/AddAccountController.test.tsx`. Read `.agent/STATUS.md:5-8` and `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:388-421`.

RECOMMENDED FIX: Disable the production invocation until the owner-approved acceptance harness passes on the integrated commit and the G11 browser/Keychain matrix is recorded against the exact candidate `.app`; alternatively obtain the plan's explicit Lead deferral while keeping the feature disabled.

### M4-FINAL-05 — Webview CSP remains disabled at the OAuth command boundary

SEVERITY: LOW

AREA: Frontend defense in depth

FILE: `src-tauri/tauri.conf.json:23`, `docs/qa/M0_BASELINE.md`

PROBLEM: Tauri still sets `"csp": null` while the webview can invoke the live native OAuth command. Source and built-bundle scans found no secret/config reachability or diagnostic logging, so this is not evidence of present token exposure, but it leaves a preventable webview hardening gap at a security-sensitive native boundary.

REPRODUCTION: Run `rg -n '"csp"' src-tauri/tauri.conf.json` and review the existing M0 finding. `rg -l -a -i '(VITE_[A-Z0-9_]*(OAUTH|GOOGLE|CLIENT|SECRET|TOKEN)|client[_-]?secret|refresh[_-]?token|access[_-]?token|code[_-]?verifier|apps\.googleusercontent\.com)' dist` returns no filenames.

RECOMMENDED FIX: Route CSP changes through Lead/Tauri review. Add a restrictive production CSP compatible with bundled assets and verify that the zero-argument command and popup behavior still work without enabling remote script/content sources.

### M4-FINAL-06 — The requested plan artifact has no auditable repository identity

SEVERITY: LOW

AREA: QA provenance

FILE: `docs/qa/M4_ADD_ACCOUNT_SECURITY_PLAN.md` (absent), `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md`

PROBLEM: The assignment names `M4_ADD_ACCOUNT_SECURITY_PLAN.md`, but that path is absent from both the integrated tree and reachable history. `M4_ADD_ACCOUNT_ACCEPTANCE.md` contains the relevant security gates and was used as the only available equivalent, leaving ambiguity about whether a distinct approved plan was expected.

REPRODUCTION: Run `git ls-files docs/qa/M4_ADD_ACCOUNT_SECURITY_PLAN.md docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md` and `git rev-list --objects --all | rg 'M4_ADD_ACCOUNT_SECURITY_PLAN\.md'`.

RECOMMENDED FIX: Lead should confirm that `M4_ADD_ACCOUNT_ACCEPTANCE.md` is the intended normative plan or restore the named approved artifact before final sign-off; do not duplicate or rename shared QA contracts without approval.

### M4-FINAL-07 — Rust dependency advisories were not evaluated

SEVERITY: LOW

AREA: Dependency assurance

FILE: `src-tauri/Cargo.lock`, `docs/qa/M4_ADD_ACCOUNT_ACCEPTANCE.md:315`

PROBLEM: Locked compilation, release tests, and strict Clippy pass, but `cargo-audit` is unavailable in the configured environment. The npm production and full dependency audits report zero known vulnerabilities; there is no equivalent Rust advisory result for this candidate.

REPRODUCTION: Run `cargo audit --version`; Cargo returns `no such command: audit`. Run `npm audit --omit=dev --ignore-scripts` and `npm audit --ignore-scripts`; both return `found 0 vulnerabilities`.

RECOMMENDED FIX: Run an approved RustSec advisory scan or review the locked Rust dependency graph with an equivalent pinned scanner before release, then record tool/database versions and disposition any finding without installing tooling as part of this QA-only commit.

## Checks and results

- Coordination: all documents required by `.agent/AGENTS.md` were read before work and again after `git merge --no-edit main`. The merge fast-forwarded to `265b4c7`; `02d6851` is an ancestor.
- Frontend: `npm run check` passed 13 files and 70 tests, then built 51 modules; `npm run build` passed independently with the same production bundle. `git diff --check` passed before report creation.
- Rust debug: `cargo fmt --all -- --check`, `cargo check --locked`, and strict `cargo clippy --locked --all-targets --all-features -- -D warnings` passed.
- Rust tests: `cargo test --locked` passed 63 unit tests, 0 failures, with loopback permission. Main and doc-test targets contained 0 tests.
- Rust release: `cargo test --locked --release` passed the same 63 tests with loopback permission. `cargo build --locked --release` produced an optimized arm64 Mach-O executable.
- Dependency checks: both npm advisory scans passed with zero reported vulnerabilities. RustSec status is unknown because `cargo-audit` is unavailable (M4-FINAL-07).
- Credential/config boundary: `.env` is ignored and absent. `ZEROCHI_GOOGLE_CLIENT_ID` is absent from both the local `.env` and process environment. Only presence/absence was inspected. The sole tracked credential-shaped filename is `.env.example`, containing one classified placeholder client-ID value. Concrete-shaped Rust client-ID literals are confined to `#[cfg(test)]` modules.
- Artifact scans: frontend production source has no OAuth configuration/token transport; the only source matches are contract property names and a stripping test. `dist` has no sensitive-boundary match. The release executable has no `VITE_` OAuth/config name, concrete-shaped Google client ID, or synthetic secret canary. Runtime diagnostic-call scan returned no match.
- OAuth positives: request construction uses the exact four approved scopes, S256 PKCE, independent OS randomness for state/verifier, an ephemeral IPv4 loopback redirect, offline consent, no `client_secret`, and redirect-disabled HTTPS clients with bounded bodies/timeouts. Returned scope drift fails closed.
- Boundary positives: `add_account` has zero arguments; errors serialize to the exact four sanitized fields; one Rust attempt and one UI promise are serialized; refresh-token set precedes account commit; commit failure invokes deletion; release selection uses Protected Data and debug selection explicitly uses legacy Keychain; no plaintext/mock production backend was found.
- UI positives: production starts with an empty account list and no fake account; onboarding and footer only navigate to provider selection; Gmail alone is enabled; duplicate activation is coalesced; status semantics, labeled buttons, heading focus, back navigation, 44 px actions, compact layout CSS, and dark palette tests remain present.

## Untested and manual surfaces

No real credential was available or needed, and this audit intentionally did not open a browser, mutate Keychain, sign/package an app, or print credential contents. Still required are the complete G11 personal/Workspace consent-denial-timeout matrix; direct duplicate command and hostile callback cases; unsigned default-browser behavior; Keychain set/get/update/delete/isolation/locked/denied and cleanup; signed Protected Data entitlement/backend verification; forced real commit-cleanup; memory inspection with synthetic material; and interactive 390 px light/dark keyboard, focus-return, and accessibility verification.

## Integration recommendation

Do not integrate or release the live M4 feature. M4-FINAL-01 and M4-FINAL-04 independently violate explicit acceptance gates; M4-FINAL-02 and M4-FINAL-03 require owner fixes and targeted regression evidence. The frontend/bundle secret boundary, exact OAuth request, sanitization, deterministic unit suites, build, formatting, and lint baselines are green, but they do not override the durable-state, callback, cancellation, and mandatory manual-evidence blockers.
