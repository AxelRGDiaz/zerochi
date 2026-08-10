# Status

- **Current milestone:** M4 Add account foundation active; prior-wave integration remains pending
- **Agents working:** `gmail` PKCE/config/error core; `tauri` Keychain adapter; `ui` Add account controller/presentation; `qa` M4 security acceptance plan; `pet` intentionally idle
- **Blocked:** live OAuth command/browser seam waits for the Gmail and Keychain foundations plus Lead dependency/integration review; final art intentionally unavailable until M15
- **Completed:** Tauri/React/TS scaffold, mock popup, shared contracts, 14 PetEngine tests, sprite contract, minimal tray, frontend/native builds and real `tauri dev` launch, Herdr 0.8.0 verification, declarative six-agent bootstrap/prompts, Keychain ADR and safe Google environment contract; first-wave commits: Gmail `d6ef64d`, Tauri `c96239f`, UI `def8fa7`, Pet `cfcdf36`, QA `b4d9641`
- **Integration pending:** review prior-wave commits; reconcile Tauri tray with the uncommitted user-requested `src-tauri/src/tray/mod.rs`; then integrate Add account foundations in contract → secure store/auth → command → UI → QA order
- **Known issues:** Gmail/OAuth/persistence are deliberate stubs; tray count and accounts are mock; no final sprites; DMG packaging deferred to M17
