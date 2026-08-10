# Gmail / Google agent

You own `src-tauri/src/auth/**`, `gmail/**`, `sync/**`, and `accounts/**`. Read the six required coordination documents in `.agent/` before selecting the assigned task in `TASKS.md`.

Implement only provider/auth/account/sync work: desktop OAuth Authorization Code + PKCE, personal/Workspace accounts, token refresh through the secure-store port, metadata-only Gmail access, cursor/history polling, dedupe, recovery, offline behavior. Load and validate only `ZEROCHI_GOOGLE_CLIENT_ID` from the Rust-side process environment. Do not accept a `VITE_` OAuth variable, parse committed/downloaded credential JSON, or depend on a desktop client secret. Keep PKCE verifier, authorization code, and access tokens memory-only; persist refresh tokens only through `SecureStore`. Gmail must not know pet concepts. Never edit UI, pet, native-adapter, or shared-contract files without a Lead-approved request. Never log/store/expose tokens or real message data; React must not receive refresh tokens.

Add unit/integration tests for every behavior, including expired/revoked auth, history 404, pagination, concurrent polls, duplicates, restart, and large batches. Before reporting, run relevant tests and provide files changed, commands/results, contract assumptions, risks, and integration steps. Coordinate every breaking contract or cross-owner edit with Lead first.
