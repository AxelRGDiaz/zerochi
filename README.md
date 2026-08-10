# Zerochi

**Your inbox pet.** Feed Zerochi. Reach Inbox Zero.

Zerochi is a lightweight macOS menu-bar application built with Tauri 2, Rust, React, and TypeScript. M0 contains a mock UI, a pure tested PetEngine, a declarative sprite contract, a minimal tray, and boundaries for later Gmail integration. It does not access Gmail yet.

## Requirements

- Node.js and npm
- Rust stable (`rustup`, `cargo`, `rustc`)
- macOS Tauri prerequisites

## Run

```bash
npm install
npm run test
npm run tauri dev
```

Frontend-only development works with `npm run dev`; the full quality gate is `npm run check`.

## Google OAuth configuration

Real Gmail access is not implemented yet, but every local/deployed build must use its own Google Cloud OAuth client:

1. Create or select a Google Cloud project and enable the Gmail API.
2. Configure the OAuth consent screen.
3. Create an OAuth 2.0 Client ID with application type **Desktop app**.
4. Copy the environment template and insert only your Client ID:

```bash
cp .env.example .env
```

```dotenv
ZEROCHI_GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
```

`.env`, `credentials.json`, `client_secret*.json`, and `google-oauth*.json` are ignored by Git. Never commit downloaded Google credentials or real values. Do not rename the variable with a `VITE_` prefix: Vite exposes such variables to frontend bundles.

Zerochi is a public installed OAuth client and will use Authorization Code + PKCE. It intentionally does not accept `ZEROCHI_GOOGLE_CLIENT_SECRET`, because a distributed desktop application cannot keep a client secret confidential. During M4, Rust will load the Client ID from the environment; PKCE material and access tokens will remain in memory.

User refresh tokens are different from the Google application Client ID: refresh tokens will be stored per account in macOS Keychain through the Rust `SecureStore`. They must never be written to `.env`, React, web storage, plaintext files, logs, or the repository.

## Open the Herdr board

Install Herdr once if necessary:

```bash
brew install herdr
```

The board definition lives in `.agent/herdr-agents.json`. It defines six named Codex agents, their prompt files, workspaces, and isolated branches. The bootstrap starts them with these Herdr names:

- `zerochi-lead`
- `zerochi-gmail`
- `zerochi-tauri`
- `zerochi-ui`
- `zerochi-pet`
- `zerochi-qa`

Herdr worktrees require a clean repository with at least one commit. After reviewing the bootstrap, create the initial commit once:

```bash
git add .
git commit -m "chore: bootstrap Zerochi"
```

Then one command creates or reuses all six agents, sends each role prompt, and opens or reattaches the board:

```bash
npm run herdr
```

Agents perform onboarding and answer `READY`; they do not start implementation until the Lead assigns work. The command is idempotent and does not duplicate live named agents. Preview it without changing Herdr or Git with `npm run herdr:dry-run`; initialize without opening the TUI with `npm run herdr:setup`.

Herdr keeps its session and panes alive when the terminal client detaches. The optional Codex session-resume integration can be installed once with `herdr integration install codex`. Full lifecycle, recovery, branch, and prompt details are documented in `.agent/HERDR.md`.

Read `.agent/PRODUCT.md`, `.agent/ARCHITECTURE.md`, `.agent/CONTRACTS.md`, `.agent/AGENTS.md`, `.agent/TASKS.md`, and `.agent/STATUS.md` before starting work. Herdr setup is in `.agent/HERDR.md`.

## Security baseline

Refresh tokens remain in the Rust secure-store boundary backed by native macOS Keychain. Signed/provisioned builds use Protected Data; unsigned local development uses the explicitly selected legacy Keychain adapter. The implementation uses `keyring-core` + `apple-native-keyring-store`, never React or Tauri commands that return secrets. See ADR-014 and ADR-015 in `.agent/DECISIONS.md`.
