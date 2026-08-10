# M0 Baseline Security and Performance Audit

Audit date: 2026-08-10
Baseline: `68915397e05659374815f10af8c71b2eea7a5195` (`fix: wait for Herdr shells before starting agents`)
Scope: the committed `HEAD` tree and all commits reachable from this worktree's refs. Generated build output was inspected only to check frontend reachability and is not committed.

## Release recommendation

**GREEN for the M0 baseline gate and integration of this report.** No critical, high, or medium security/performance issue was found. The baseline is not a production-release candidate: OAuth, secure storage, sync/polling, native hide/show and sleep/wake behavior, and packaging are deliberate later-milestone stubs. The three low-severity findings below should be closed before production release or before the affected surfaces carry credentials or long-running timers.

## Findings

### M0-01 — Committed secret and OAuth configuration baseline is clean

SEVERITY: INFO
AREA: Secrets / OAuth configuration
FILE: `.env.example`, `.gitignore`, committed repository history
PROBLEM: No committed credential artifact, secret-shaped value, real OAuth value, or forbidden OAuth `VITE_` identifier was found. The only credential-like filename reachable in history is the intentionally tracked `.env.example`; its OAuth client ID is a documented placeholder, and a desktop client secret is intentionally absent.
REPRODUCTION: `git rev-list --objects --all` filtered for environment files, Google credential JSON, private keys, and certificate/key containers returned only `.env.example`. A per-revision `git grep` for private-key headers and common token/key shapes returned `secret_shape_history_matches=0`; a per-revision search for OAuth-related `VITE_` identifiers returned `vite_oauth_history_matches=0`.
RECOMMENDED FIX: Preserve the ignore rules and Rust-only `ZEROCHI_GOOGLE_CLIENT_ID` contract. Add equivalent secret-pattern and forbidden-variable checks to CI before M4 OAuth work lands.

### M0-02 — OAuth tokens are not reachable from the frontend

SEVERITY: INFO
AREA: Frontend credential boundary
FILE: `src/**`, `src-tauri/src/commands/mod.rs`, `src-tauri/capabilities/default.json`
PROBLEM: No frontend environment read, Tauri invocation, event listener, token DTO, authorization header, or credential-bearing command exists in M0. The production bundle contains no OAuth environment identifier or placeholder OAuth value. The sole TypeScript `SecureStore` interface is an architectural port and is not imported by runtime UI code.
REPRODUCTION: `git grep` over committed `src` and `src-tauri` found no `import.meta.env`, `process.env`, `invoke(`, OAuth token field, authorization header, or credential-returning command. After `npm run build`, targeted `rg` checks of `dist` returned zero matches for both allowed/forbidden OAuth environment identifiers and the placeholder client-ID text.
RECOMMENDED FIX: Keep OAuth commands Rust-owned and return only sanitized non-secret DTOs. Add a production-bundle negative assertion when M4 introduces OAuth commands.

### M0-03 — Webview Content Security Policy is disabled

SEVERITY: LOW
AREA: Frontend hardening
FILE: `src-tauri/tauri.conf.json`
PROBLEM: `app.security.csp` is `null`. The current mock UI has no untrusted HTML and no credential path, so there is no demonstrated exploit in M0, but a disabled CSP increases the impact of a future injection or unintended remote resource once OAuth/navigation features exist.
REPRODUCTION: `git show HEAD:src-tauri/tauri.conf.json` shows `"csp": null`; inspection of committed React sources found no `dangerouslySetInnerHTML`, remote script source, or OAuth value that would raise current severity.
RECOMMENDED FIX: Define and verify a least-privilege CSP before OAuth-enabled builds are release candidates; explicitly allow only resources required by the final local asset and navigation design.

### M0-04 — Hidden-window visibility is not wired to the renderer

SEVERITY: LOW
AREA: Lifecycle / sprite timers
FILE: `src/App.tsx`, `src/pet/PetRenderer.tsx`, `src/pet/SpriteAnimator.tsx`, `src-tauri/src/platform/mod.rs`
PROBLEM: `App` passes `visible` unconditionally, and the native lifecycle boundary is still a stub. The M0 UI renders a timer-free placeholder, while `SpriteAnimator` correctly avoids creating an interval when `visible=false` and clears its interval on completion, dependency change, or unmount. When the animator is integrated, however, hiding a still-mounted webview without updating this prop could leave animation intervals active.
REPRODUCTION: `git grep` finds one application timer: `window.setInterval` in `SpriteAnimator`. Static control-flow inspection confirms its cleanup paths. `App.tsx` contains `<PetRenderer ... visible />`, and no committed native/React visibility event or store exists. No hidden-window runtime test is present.
RECOMMENDED FIX: During M1/M3, connect native window visibility to React and add an owner-approved fake-timer test proving hide immediately removes the interval and show creates at most one interval. Measure the packaged hidden-window process during M16.

### M0-05 — Polling and wake lifecycle cannot yet create idle wakeups

SEVERITY: INFO
AREA: Polling / idle performance
FILE: `src-tauri/src/sync/mod.rs`, `src-tauri/src/platform/mod.rs`, `src-tauri/src/lib.rs`
PROBLEM: Sync polling and sleep/wake handling are intentionally unimplemented, so the M0 process has no polling loop, backoff timer, wake handler, or overlap risk to regress. This also means the required 30–60 second guarded polling and single catch-up-on-wake behavior cannot yet be validated.
REPRODUCTION: The sync and platform modules contain only M0 stub documentation. Repository-wide source searches found no polling scheduler, sleep/wake listener, timeout, or native run-event handler. The only recurring application timer is the visibility-gated sprite interval described in M0-04.
RECOMMENDED FIX: In M7/M14, add a per-account poll mutex, cancellable sleep state, jittered backoff, and one guarded wake sync. In M16, measure hidden idle CPU and wakeups over multiple poll periods and a sleep/wake cycle.

### M0-06 — Current diagnostics do not expose sensitive state

SEVERITY: INFO
AREA: Logs / errors
FILE: `src-tauri/src/lib.rs`, `src/**`
PROBLEM: No application logging calls, debug dumps, frontend console diagnostics, provider errors, headers, message bodies, or tokens are present. The only runtime failure text is a generic Tauri startup expectation and contains no dynamic credential content.
REPRODUCTION: Committed-source searches for `console.log/error/warn`, `println!`, `eprintln!`, `dbg!`, authorization headers, bearer values, token fields, and authorization-code fields found no runtime diagnostic sink containing sensitive material.
RECOMMENDED FIX: Preserve structured, redacted error translation when M4 and later provider code is added; add negative tests for token/header/code leakage in command errors and logs.

### M0-07 — Rust dependencies lack an advisory-database check in this worktree

SEVERITY: LOW
AREA: Dependency security
FILE: `src-tauri/Cargo.lock`
PROBLEM: Locked Rust dependencies compile successfully, but `cargo-audit` is not installed, so this audit cannot claim that `Cargo.lock` is clear against the current RustSec advisory database. This is a coverage gap, not evidence of a vulnerable dependency.
REPRODUCTION: `cargo check --locked` and `cargo test --locked` pass. `cargo audit --version` exits 101 with `no such command: audit`. `cargo tree --locked --depth 1` resolves direct runtime dependencies to `serde 1.0.229`, `serde_json 1.0.151`, and `tauri 2.11.5`, plus build dependency `tauri-build 2.6.3`.
RECOMMENDED FIX: Add a pinned RustSec audit step to CI and run it on `Cargo.lock` before any production release; triage advisories for actual target/configuration reachability.

## Commands and results

All commands ran from the clean QA worktree unless a subdirectory is shown.

| Command | Result |
|---|---|
| `git status --short --branch` | Clean `codex/qa-m1` before report creation. |
| `git log -1 --format=...` | Resolved the audited baseline commit shown above. |
| `git ls-tree`, `git rev-list --objects --all`, per-revision `git grep` secret/config scans | No credential artifact beyond `.env.example`; zero secret-shape and forbidden OAuth `VITE_` matches. |
| Targeted committed-source `git grep` scans | No frontend env access, credential commands, unsafe sensitive log sink, polling loop, or lifecycle listener; one sprite interval found. |
| `npm ci --ignore-scripts` | Installed 106 locked packages; no tracked-file change. |
| `npm run check` | PASS: 1 Vitest file, 14/14 tests; TypeScript and Vite production build pass. Bundle: 196.04 kB JS (61.77 kB gzip), 2.36 kB CSS (1.00 kB gzip). |
| `npm audit --omit=dev --ignore-scripts` | PASS: 0 known production dependency vulnerabilities. |
| `npm audit --ignore-scripts` | PASS: 0 known production/development dependency vulnerabilities. |
| Targeted `rg` scans of generated `dist` | Zero OAuth environment, secret, or placeholder client-ID matches. |
| `cargo check --locked` (`src-tauri`) | PASS. |
| `cargo test --locked` (`src-tauri`) | PASS: 0 tests executed; no failures. |
| `cargo audit --version` (`src-tauri`) | NOT RUN: cargo subcommand unavailable. |

The first `npm run check` attempt failed because `node_modules` was absent (`vitest: command not found`). The recorded pass followed the reproducible lockfile install above. The first sandboxed npm audit attempt could not resolve the registry; the successful results used approved read-only registry access.

## Untested surfaces

- Real Google OAuth/PKCE, token refresh, sanitized provider failures, and Rust-side environment validation: M4 stubs.
- Keychain persistence, deletion, zeroization, and command-boundary non-reachability: M13 stub.
- Gmail metadata sync, first-sync baseline, history recovery, deduplication, backoff, and poll overlap: M6–M8 stubs.
- Native tray hide/show/focus/close, React visibility propagation, sleep/wake, offline/reconnect, autostart, and real idle CPU/wakeups: M1/M14/M16 work.
- Sprite timer behavior under DOM mount, hide/show, prop changes, and completion: no component/fake-timer tests yet; recommended for owner-coordinated M3 work.
- Rust unit/integration coverage: the native crate currently contains zero tests.
- RustSec advisory status: `cargo-audit` unavailable, as captured in M0-07.
- Signed/notarized packaging, entitlements, Protected Data Keychain mode, and release bundle behavior: M17 work.

## Integration needs and residual risk

This report changes no production code, tests, contracts, coordination files, or dependencies. No targeted test was added; proposed tests are recommendations for the owning agents and Lead to schedule under the change protocol. Residual M0 risk is limited to the disabled CSP, unwired future visibility signal, missing Rust advisory scan, and broad untested surfaces caused by deliberate stubs. Re-run this audit after M1 visibility work and perform the dedicated M4 token/config exposure audit when OAuth code exists.
