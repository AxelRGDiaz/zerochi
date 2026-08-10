# Contracts

The TypeScript source of truth for frontend DTOs is `src/types`; Rust command DTOs must serialize to the same field names and meanings. Breaking changes require Lead review and a contract test.

## Domain records

- `MailAccount`: stable local id, email/display name, active flag, pending count, optional history cursor and sync time.
- `MailMessage`: provider message/thread ids, account id, metadata, detection time, optional local-open time. Never store bodies in V1.
- `InboxPetSnapshot`: pending total plus per-account counts; contains no provider-specific fields.
- `PetPresentation`: mood, animation id, and selected copy. Copy remains outside engine logic.

## Ports

- `MailProvider`: identity, initial cursor, changes after cursor. It knows Gmail but not Zerochi.
- `SyncEngine`: serializes per-account sync, deduplicates `(accountId, messageId)`, commits messages and cursor atomically, categorizes recoverable/auth/history-expired errors.
- `AccountState` / `MailState`: normalized non-secret persistence and local pending transitions.
- `SecureStore`: refresh-token secrets keyed by stable local account UUID. The macOS implementation uses Keychain, provides set/get/delete, redacts errors, zeroizes temporary values, and never returns secrets across Tauri commands.
- `NotificationService`: displays already-decided notification content; never chooses mood.
- `ExternalNavigationService`: opens an account or exact message/thread without exposing browser account-position assumptions.
- `PetEngine`: pure snapshot/transition input to presentation output.

## React/Tauri boundary (planned commands)

`get_app_snapshot`, `add_account`, `remove_account`, `open_account`, `open_message`, `get_preferences`, `update_preferences`. Events: `snapshot_changed`, `sync_status_changed`, `pet_event`. Commands return tagged, sanitized errors; logs exclude tokens, authorization codes, headers, and message bodies.

### `add_account` M4 contract

- Request: no arguments. OAuth configuration is read only by Rust from `ZEROCHI_GOOGLE_CLIENT_ID`; React never supplies configuration, scopes, redirect URIs, or token material.
- Success: `AddAccountResult { account: MailAccount }`, emitted only after the refresh token is stored through `SecureStore` and the non-secret account record is committed.
- Failure: `CommandError<AddAccountErrorTag> { tag, message, retryable, actionRequired }`. Messages are fixed and sanitized; no URL, email, provider body, environment value, callback data, code, verifier, state, token, account id, file path, header, or adapter error may cross the command boundary.
- One add attempt may be active at a time in V1. A second request fails safely with `auth_in_progress`; it must not share or cancel the first attempt.
- User cancellation is idempotent and returns `cancelled` without persisting an account or secret.
- The allowed M4 tags are the `AddAccountErrorTag` union in `src/types/contracts.ts`. Adding or renaming a tag is a shared-contract change requiring Lead approval and a serialization test.

## Invariants

Pending equals persisted messages with no `openedFromZerochiAt`, scoped to active observed messages—not Gmail unread. Opening is idempotent. The cursor advances only with successfully committed state. Account deletion removes normal state and invokes secret deletion as separate auditable work.

OAuth application configuration is supplied only through Rust-side `ZEROCHI_GOOGLE_CLIENT_ID`. No OAuth variable may use the `VITE_` prefix. Google credential JSON and `.env` files are local inputs, never repository artifacts. Zerochi does not require a desktop `client_secret`; refresh tokens persist only through `SecureStore`, while access tokens and PKCE material are memory-only.
