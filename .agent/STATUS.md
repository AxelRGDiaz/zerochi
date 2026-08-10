# Status

- **Current milestone:** M0 complete; first parallel wave active (M1 implementation, M2 pet queue, M4 design only)
- **Agents working:** `gmail` M4 sequence/error design; `tauri` M1 popup/tray lifecycle; `ui` M1 popup/store extraction; `pet` M2 event queue; `qa` M0 security/performance baseline
- **Blocked:** Gmail OAuth implementation remains unauthorized in this wave; final art intentionally unavailable until M15
- **Completed:** Tauri/React/TS scaffold, mock popup, shared contracts, 14 PetEngine tests, sprite contract, minimal tray, frontend/native builds and real `tauri dev` launch, Herdr 0.8.0 verification, declarative six-agent bootstrap/prompts, Keychain ADR and safe Google environment contract
- **Integration pending:** review five green specialist commits before merge; reconcile the Tauri branch with the uncommitted user-requested `src-tauri/src/tray/mod.rs` change currently in `main`
- **Known issues:** Gmail/OAuth/persistence are deliberate stubs; tray count and accounts are mock; no final sprites; DMG packaging deferred to M17
