# Herdr workflow

Zerochi keeps its repeatable six-agent definition in `.agent/herdr-agents.json` and its bootstrap in `scripts/herdr-zerochi.mjs`. Herdr's persisted runtime remains in Herdr itself; the repository stores only the declarative topology, names, branches, and prompt paths.

## One-command board

The repository must have a clean committed `HEAD`, because five agents receive isolated Git worktrees. Then run:

```bash
npm run herdr
```

The bootstrap is idempotent. It creates missing workspaces/worktrees, starts missing Codex agents, sends onboarding prompts only to newly started agents, and finally opens or reattaches the Herdr TUI. Existing live agents with the configured names are reused.

The six globally unique Herdr names are:

| Name | Role | Checkout |
|---|---|---|
| `zerochi-lead` | Lead / Architect | Main checkout |
| `zerochi-gmail` | Gmail / Google | `codex/gmail-m1` worktree |
| `zerochi-tauri` | Tauri / macOS | `codex/tauri-m1` worktree |
| `zerochi-ui` | UI / UX | `codex/ui-m1` worktree |
| `zerochi-pet` | Pet / Character | `codex/pet-m1` worktree |
| `zerochi-qa` | QA / Security / Performance | `codex/qa-m1` worktree |

Each agent receives its matching `.agent/prompts/*.md` plus a guardrail to read the coordination documents, report `READY <name>`, and wait without editing until the Lead assigns a bounded task. Automatic bootstrap does not automatically start milestone implementation.

## Supporting commands

```bash
# Validate and display the topology without creating anything
npm run herdr:dry-run

# Create/reuse all agents but do not open the TUI
npm run herdr:setup

# Re-send onboarding to existing agents (use only for recovery)
npm run herdr:reprompt
```

Edit `.agent/herdr-agents.json` when a new milestone needs different short-lived `codex/*` branches. Do not edit generated worktree locations manually and do not reuse the same branch concurrently.

## Initial repository prerequisite

Herdr cannot create worktrees before the repository has a commit. After reviewing M0, create the initial green commit once:

```bash
git add .
git commit -m "chore: bootstrap Zerochi"
npm run herdr
```

The bootstrap never commits, stashes, deletes, resets, or overwrites user changes. It stops if the main checkout is dirty.

## Installation and lifecycle

```bash
brew install herdr
herdr integration install codex
herdr integration status
```

Detach with `ctrl+b q`; agents continue running. Run `npm run herdr` to reattach. Herdr's optional global config is TOML at `~/.config/herdr/config.toml`; Zerochi does not replace it.
