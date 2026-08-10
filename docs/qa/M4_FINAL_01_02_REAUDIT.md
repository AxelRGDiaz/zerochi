# M4-FINAL-01/02 targeted re-audit

Audit date: 2026-08-10
Audited QA merge: `ace67d39833e6ce4fefa48c0fe998aabe5d52f28`
Required integrated commit: `13023482a3f30b6c8bd19a20597790acf09a1bbd` (verified ancestor)
Verdict: **SCOPED GREEN — close M4-FINAL-01 and M4-FINAL-02**

This re-audit covers only durable account commit and callback Host validation from `docs/qa/M4_FINAL_IMPLEMENTATION_AUDIT.md`. M4-FINAL-03 through M4-FINAL-07, including manual M4 acceptance, remain outside this verdict and open.

## Closure decisions

### M4-FINAL-01 — Durable account commit

SEVERITY: RESOLVED (formerly HIGH)

AREA: Account persistence and Keychain rollback

FILE: `src-tauri/src/lib.rs:19`, `src-tauri/src/commands/mod.rs:22`, `src-tauri/src/accounts/mod.rs:123`, `src-tauri/src/auth/service.rs:85`

PROBLEM: The prior live command used process-local account state, allowing success without a durable account record and risking an orphaned Keychain credential after restart. The live path now resolves `app.path().app_data_dir().join("accounts.json")`, constructs `JsonAccountState`, and injects it into `AddAccountService`.

REPRODUCTION: Run `rg -n 'app_data_dir|accounts\.json|JsonAccountState::new|accounts: Arc::new' src-tauri/src/lib.rs src-tauri/src/commands/mod.rs`. Run `cargo test --locked --manifest-path src-tauri/Cargo.toml accounts::tests` and repeat with `--release`; the tests prove restart reload, case-insensitive email and UUID duplicate rejection, corrupt/version/unknown-secret-field rejection, the 1 MiB bound, target and parent symlink rejection, private file mode, write-failure non-mutation, and token-free versioned JSON. Inspect `src-tauri/src/auth/service.rs:85-108` and its event-recording tests for `store → commit → delete` on commit failure.

RECOMMENDED FIX: None for M4-FINAL-01. Preserve the app-data path injection, strict schema/size/path checks, temporary-file write plus sync/rename sequence, and compensating Keychain deletion in future persistence changes.

### M4-FINAL-02 — Exact callback Host validation

SEVERITY: RESOLVED (formerly MEDIUM)

AREA: OAuth loopback callback validation

FILE: `src-tauri/src/auth/callback.rs:37`, `src-tauri/src/auth/callback.rs:181`, `src-tauri/src/auth/callback.rs:247`

PROBLEM: The prior callback parser did not validate Host. The listener now derives the expected authority from its actual ephemeral IPv4 loopback binding and requires exactly one case-insensitive Host header whose trimmed value exactly equals `127.0.0.1:<bound-port>`. Request targets must use origin form and cannot begin with `//`.

REPRODUCTION: Run `cargo test --locked --manifest-path src-tauri/Cargo.toml auth::callback::tests` and repeat with `--release`. `rejects_missing_wrong_duplicate_or_malformed_host_and_absolute_targets` covers missing Host, wrong port, duplicate mixed-case Host, whitespace in the field name, combined/malformed values, folded headers, absolute-form targets, and network-path targets. `listener_binds_ipv4_loopback_ephemeral_port_and_returns_static_html_once` verifies the real bound authority. `AddAccountService::add_account` awaits callback validation before invoking token exchange, so every rejected request exits before exchange.

RECOMMENDED FIX: None for M4-FINAL-02. Retain the bound-authority value as listener-owned state and keep hostile Host/target cases in both debug and optimized suites.

## Checks and results

- Coordination documents required by `.agent/AGENTS.md` were read before and after merging current `main`.
- `git merge --no-edit main`: PASS; final QA merge `ace67d3`, with `1302348` verified as an ancestor. The later coordination-only main update changed no runtime or test file.
- `cargo fmt --all -- --check`: PASS.
- `cargo check --locked`: PASS.
- `cargo test --locked` with loopback permission: PASS, 71/71 library tests; 0 main and 0 doc tests.
- `cargo test --locked --release` with loopback permission: PASS, 71/71 library tests; 0 main and 0 doc tests.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: PASS.
- `npm run check`: PASS, 13/13 test files and 70/70 tests; production build transformed 51 modules.
- `cargo build --locked --release`: PASS; optimized production executable rebuilt for artifact inspection.
- High-confidence tracked credential scan: no match.
- Frontend source scan: only shared contract property names and the token-stripping test matched; no runtime OAuth configuration or token transport.
- `dist` OAuth/token/VITE configuration scan: no match.
- Production release executable scan: no concrete Google client ID, VITE OAuth/configuration name, or synthetic token canary.
- Runtime diagnostic scan: no console/Rust debug or logging call matched.
- `.env` remains ignored and absent; no credential value was read or printed.

## Residual manual risks

- Restart durability and filesystem rejection behavior were exercised with isolated local paths, not by launching a packaged `.app` and inspecting its resolved app-data directory.
- Atomic write behavior was not power-loss or forced-I/O-fault tested after rename; tests cover deterministic pre-commit write failure and in-memory non-mutation.
- Host enforcement was exercised through parser and real ephemeral-loopback unit paths, not through an interactive Google browser callback.
- Real Keychain behavior and the broader M4 browser/manual matrix remain governed by the still-open findings in the prior report.

## Integration recommendation

Lead may treat commits `c438a7d` and `1302348` as closing M4-FINAL-01 and M4-FINAL-02 and may integrate this QA evidence. Do not interpret this scoped green as full M4 acceptance: M4-FINAL-03 through M4-FINAL-07 and required manual evidence remain open.
