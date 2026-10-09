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
4. Copy the environment template and insert the Desktop credential pair:

```bash
cp .env.example .env
```

```dotenv
ZEROCHI_GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
ZEROCHI_GOOGLE_CLIENT_SECRET=your-desktop-client-secret
```

`.env`, `credentials.json`, `client_secret*.json`, and `google-oauth*.json` are ignored by Git. Never commit downloaded Google credentials or real values. Do not rename the variable with a `VITE_` prefix: Vite exposes such variables to frontend bundles.

Zerochi is a public installed OAuth client and uses Authorization Code + PKCE. Google requires the matching Desktop client credential for this project's token exchange. A distributed desktop app cannot keep that shared value confidential, so Zerochi treats it as an extractable build credential rather than a security boundary. In debug, Rust loads both variables through `dotenvy` from the ignored `.env`; variables already present in the process take precedence. Never add a `VITE_` prefix or commit the file.

Run `npm run tauri:release` for production packaging. CI supplies both `ZEROCHI_GOOGLE_CLIENT_ID` and `ZEROCHI_GOOGLE_CLIENT_SECRET` as protected process variables. For a local release, the wrapper uses the ignored root `.env` when neither process variable is set; a complete process pair takes priority and a partial process pair fails instead of mixing sources. The wrapper creates a fresh ephemeral Cargo target, builds the app, and scans textual build outputs for the raw credential without printing it. Runtime release code never reads `.env`. Raw `cargo build --release`/`npm run tauri build` release paths fail closed so a stale credential cannot be reused accidentally. Upload only the final `.app`/installer reported by the wrapper; never cache or upload its temporary Cargo target. The native executable necessarily contains the extractable shared credential. The recipient's Mac requires no OAuth environment variables. Rotating the Desktop credential requires another wrapper build and redistribution.

User refresh tokens are different from the Google application Client ID: refresh tokens will be stored per account in macOS Keychain through the Rust `SecureStore`. They must never be written to `.env`, React, web storage, plaintext files, logs, or the repository.

## Signing, notarization, and sensitive local files

Direct macOS distribution requires local signing material that must not be committed:

- A **Developer ID Application** certificate and its matching private key installed under **login > My Certificates** in macOS Keychain. The certificate identifies the developer; the private key performs the signature and must never be exported into the repository.
- The Developer ID provisioning profile at `src-tauri/signing/Zerochi_Developer_ID_Distribution.provisionprofile`. Tauri embeds this public, signed authorization artifact into `Zerochi.app/Contents/embedded.provisionprofile`, but the downloaded source copy remains local and ignored.
- For notarization, an App Store Connect API private key such as `AuthKey_<KEY_ID>.p8`, stored outside the repository. Supply only its path through `APPLE_API_KEY_PATH`; supply `APPLE_API_KEY` and `APPLE_API_ISSUER` through the process environment or the CI secret manager. Never paste the private-key contents into `.env`, source, logs, issues, or shell commands.
- The local root `.env` containing the Google Desktop OAuth credential pair described above. Only `.env.example` with placeholders is allowed in Git.

The repository ignores provisioning profiles, mobile provisioning profiles, Apple API keys, exported signing identities (`.p12`/`.pfx`), private-key formats (`.p8`/`.pem`/`.key`), certificate and CSR formats (`.cer`/`.crt`/`.der`/`.csr`), keychain databases, Java keystores, downloaded Google credential JSON, and all real dotenv variants. A `.cer` certificate or `.csr` request does not normally contain the private key, but Zerochi still keeps it out of Git because it carries developer identity and signing metadata.

`.gitignore` is only a guard for untracked files: it does not remove a file already committed, and `git add -f` can bypass it. Before pushing, inspect filenames without printing secret contents:

```bash
git status --short --ignored
git ls-files | rg -i '(\.env($|\.)|credentials?[^/]*\.json$|client[_-]?secret|google-oauth|authkey_|\.(provisionprofile|mobileprovision|p8|p12|pfx|pem|key|cer|crt|der|csr|keychain-db|jks|keystore)$)'
```

The second command should report only the intentional placeholder `.env.example`. If a private key, exported identity, OAuth credential, token, or real dotenv value was ever committed or pushed, ignoring or deleting the file is insufficient: revoke/rotate the credential and remove it from repository history through the repository owner.

The Team ID, bundle identifier, certificate name, Key ID, and Issuer ID are identifiers rather than private keys. They may be used as configuration, but should never be confused with the `.p8`/`.p12` contents or passwords protecting exported identities. Recipient Macs need only the final signed and notarized `.dmg`; they do not receive or install the developer certificate, private key, API key, or local provisioning source file.

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

The board opens all six agents together in a persistent 2×3 grid and focuses `zerochi-lead`: Lead/Gmail on the first row, Tauri/UI on the second, and Pet/QA on the third. Use `ctrl+b z` to zoom the focused pane and repeat it to return to the grid.

Specialists perform onboarding and answer `READY`; they start only after Lead assigns one bounded task. On a new Lead session, Lead inspects the project coordination state and dispatches the next safe parallel wave automatically. Reattaching an existing live board does not resend work, so the command remains idempotent and does not duplicate assignments or agents. Preview it without changing Herdr or Git with `npm run herdr:dry-run`; initialize without opening the TUI with `npm run herdr:setup`.

Herdr keeps its session and panes alive when the terminal client detaches. The optional Codex session-resume integration can be installed once with `herdr integration install codex`. Full lifecycle, recovery, branch, and prompt details are documented in `.agent/HERDR.md`.

Read `.agent/PRODUCT.md`, `.agent/ARCHITECTURE.md`, `.agent/CONTRACTS.md`, `.agent/AGENTS.md`, `.agent/TASKS.md`, and `.agent/STATUS.md` before starting work. Herdr setup is in `.agent/HERDR.md`.

## Security baseline

Refresh tokens remain in the Rust secure-store boundary backed by native macOS Keychain. Signed/provisioned builds use Protected Data; unsigned local development uses the explicitly selected legacy Keychain adapter. The implementation uses `keyring-core` + `apple-native-keyring-store`, never React or Tauri commands that return secrets. See ADR-014 and ADR-015 in `.agent/DECISIONS.md`.
