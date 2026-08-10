# Status

- **Current milestone:** M4 Add account UI navigation active; foundation commits delivered and pending Lead integration
- **Agents working:** `ui` account-provider and settings views; `gmail`, `tauri`, `qa`, and `pet` idle pending integration/review
- **Blocked:** live OAuth command/browser seam waits for Lead integration of delivered Gmail `0e3b8f9` and Keychain `3de02d0`; final art intentionally unavailable until M15
- **Completed:** Tauri/React/TS scaffold, mock popup, shared contracts, PetEngine/sprite foundation, Herdr bootstrap, Keychain ADR and safe Google environment contract; Add account foundations: Gmail `0e3b8f9`, Tauri `3de02d0`, UI `0c8e8a7`, QA `e02bcaf`
- **Integration pending:** review prior-wave commits; reconcile Tauri tray with the uncommitted user-requested `src-tauri/src/tray/mod.rs`; then integrate Add account foundations in contract → secure store/auth → command → UI → QA order
- **Known issues:** Gmail/OAuth/persistence are deliberate stubs; tray count and accounts are mock; no final sprites; DMG packaging deferred to M17
