# Decisions

| ADR | Decision | Consequence |
|---|---|---|
| 001 | Tauri 2, not Electron | Smaller native utility; Rust toolchain required. |
| 002 | No backend in V1 | State and polling remain local. |
| 003 | Gmail API, not IMAP | Official OAuth/history model. |
| 004 | Gmail read-only | No Gmail mutation; scope review remains a launch risk. |
| 005 | Pending differs from unread | Initial historical unread does not inflate Zerochi. |
| 006 | Opening from Zerochi consumes locally | Idempotent `openedFromZerochiAt`; Gmail unchanged. |
| 007 | Provider-independent PetEngine | Only normalized snapshots enter pet logic. |
| 008 | Declarative PNG sprite sheets | Variable rows/frames/FPS/loop without component hardcoding. |
| 009 | Hidden UI stops animation | No continuous visual timer while popup is closed. |
| 010 | Secrets stay outside frontend | Rust secure-store port; no tokens in React or plaintext. |
| 011 | One modular desktop process | Avoid backend, DI framework, event bus, and heavy database. |
| 012 | Worktrees per specialist wave | Ownership branches reduce collisions; Lead integrates. |
| 013 | M0 bundles macOS `.app` only | DMG/signing/notarization stay in M17; native code still receives a full bundle check. |
| 014 | Native macOS Keychain is the V1 secure store | Rust uses `keyring-core` + `apple-native-keyring-store`; Protected Data for signed builds, legacy adapter only for unsigned development; Stronghold is unnecessary in V1. |
| 015 | Google app configuration is local and Rust-only | `.env` contains only `ZEROCHI_GOOGLE_CLIENT_ID`; Google JSON and `.env` are ignored; no `VITE_` OAuth variables or embedded desktop client secret. |
| 016 | Herdr topology is repository-defined and idempotent | `.agent/herdr-agents.json` defines six globally named agents; a safe bootstrap creates one main workspace plus five isolated short-lived worktrees and only performs onboarding. |
| 017 | `add_account` is a zero-argument Rust-owned OAuth command | UI receives only a normalized `MailAccount` or a sanitized tagged error; OAuth configuration and secrets never cross into React. V1 serializes add attempts. |

## Pending decisions

- Browser navigation reliability for simultaneous Google accounts; spike behind `ExternalNavigationService`.
- Exact normal/locked polling intervals and macOS energy budget.
