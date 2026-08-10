# Agent ownership and protocol

Exactly six roles collaborate: `lead`, `gmail`, `tauri`, `ui`, `pet`, `qa`.

Herdr launches them under the globally unique names `zerochi-lead`, `zerochi-gmail`, `zerochi-tauri`, `zerochi-ui`, `zerochi-pet`, and `zerochi-qa`. The Lead owns `.agent/herdr-agents.json` and `scripts/herdr-zerochi.mjs`; changes to topology, prompts, or worktree branches require Lead review.

Every agent must read `PRODUCT.md`, `ARCHITECTURE.md`, `CONTRACTS.md`, this file, `TASKS.md`, and `STATUS.md`; identify one assigned task; edit only owned paths; run relevant tests; then report files changed, checks run, risks, and integration needs.

| Agent | Purpose and owned paths | Must not edit |
|---|---|---|
| lead | Architecture, contracts, assignment, review, and integration. `.agent/**`, `src/shared/**`, `src/types/**`, root configuration | All specialist implementation, including small fixes and visual adjustments |
| gmail | OAuth/provider/sync/accounts. `src-tauri/src/{auth,gmail,sync,accounts}/**` | UI, pet, native adapters, shared contracts |
| tauri | macOS/native. `src-tauri/src/{tray,notifications,secure_store,navigation,platform}/**` | Gmail, UI, pet, shared contracts |
| ui | Popup/stores/styles. `src/{components,views,stores,styles}/**`, coordinated `src/App.tsx` | Gmail/Rust provider, pet logic, contracts |
| pet | Pet logic/rendering/assets. `src/pet/**`, `src/assets/pet/**`, `docs/SPRITES.md` | Gmail, native services, general UI |
| qa | Review and tests across the repo; no feature ownership. Test files may be added beside code only after notifying owner | Production feature implementation or contract changes |

## Change protocol

1. Every production adjustment must be assigned to and implemented or explicitly reviewed by the agent that owns the affected path, regardless of how small or urgent it appears.
2. Lead edits only Lead-owned paths, defines contracts, assigns work, coordinates dependencies, and integrates owner commits; Lead never directly hotfixes a specialist-owned production path.
3. Stop before editing another owner's file.
4. Send Lead: task, reason, proposed interface, compatibility/migration impact, and tests.
5. Lead updates `CONTRACTS.md`/types first or records explicit coordination.
6. Owner implements against the approved contract; QA reviews when the assigned risk or milestone requires it.
7. If a change was accidentally made outside ownership, it is not accepted until the owning specialist reviews it, makes any needed correction, runs its checks, and reports explicit approval.

## Command approval protocol

Zerochi keeps project-local command rules in `.codex/rules/zerochi.rules`. Keep Codex in `workspace-write` with approvals `on-request`; never bypass the sandbox or switch approvals to `never`.

When an unlisted command needs approval, propose the smallest reusable action prefix and omit volatile arguments such as commit messages, file lists, branch names, and generated paths. Prefer prefixes such as `["git", "commit", "-m"]` or `["npm", "run", "check"]`, never executable-only rules such as `["git"]`, `["npm"]`, `["cargo"]`, or shell-wrapper rules such as `["bash", "-lc"]` and `["zsh", "-lc"]`. Destructive, history-rewriting, external, installation, credential, and network actions remain individually reviewed even when a related local action is allowed.

Never commit `.env`, downloaded Google credential JSON, client credentials, tokens, real user metadata, or third-party sprite assets. Use only the placeholder names in `.env.example`; never add OAuth variables with a `VITE_` prefix. Desktop OAuth uses PKCE and `ZEROCHI_GOOGLE_CLIENT_ID`, not an embedded client secret. Refresh tokens belong exclusively to the Rust macOS Keychain adapter defined in `ARCHITECTURE.md`. Do not start real Gmail/OAuth work before its milestone is assigned. One active task per agent; one owner per file.

QA findings use: `SEVERITY`, `AREA`, `FILE`, `PROBLEM`, `REPRODUCTION`, `RECOMMENDED FIX`.
