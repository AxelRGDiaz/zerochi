# Herdr workflow

Zerochi keeps its repeatable six-agent definition in `.agent/herdr-agents.json` and its bootstrap in `scripts/herdr-zerochi.mjs`. Herdr's persisted runtime remains in Herdr itself; the repository stores only the declarative topology, names, branches, and prompt paths.

## One-command board

The repository must have a clean committed `HEAD`, because five agents receive isolated Git worktrees. Then run:

```bash
npm run herdr
```

The bootstrap is idempotent. It creates missing workspaces/worktrees, starts missing Codex agents, sends onboarding prompts only to newly started agents, and finally opens or reattaches the Herdr TUI. Existing live agents with the configured names are reused.

Before opening the TUI, the bootstrap gathers the six live agent panes into the Lead tab and applies this persistent 2×3 board:

```text
┌──────────────────┬──────────────────┐
│ zerochi-lead     │ zerochi-gmail    │
├──────────────────┼──────────────────┤
│ zerochi-tauri    │ zerochi-ui       │
├──────────────────┼──────────────────┤
│ zerochi-pet      │ zerochi-qa       │
└──────────────────┴──────────────────┘
```

`zerochi-lead` is focused when the board opens. Each agent keeps its own checkout and foreground process after its pane moves. Herdr persists the resulting layout across detach/reattach. The bootstrap also restores a missing configured agent name when exactly one unnamed Codex process is running from that role's expected checkout.

The six globally unique Herdr names are:

| Name | Role | Checkout |
|---|---|---|
| `zerochi-lead` | Lead / Architect | Main checkout |
| `zerochi-gmail` | Gmail / Google | `codex/gmail-m1` worktree |
| `zerochi-tauri` | Tauri / macOS | `codex/tauri-m1` worktree |
| `zerochi-ui` | UI / UX | `codex/ui-m1` worktree |
| `zerochi-pet` | Pet / Character | `codex/pet-m1` worktree |
| `zerochi-qa` | QA / Security / Performance | `codex/qa-m1` worktree |

Each agent receives its matching `.agent/prompts/*.md`. Specialists read the coordination documents, report `READY <name>`, and wait without editing until the Lead assigns one bounded task. On a newly created Lead session, Lead inspects Git plus `TASKS.md`/`STATUS.md`, activates the next contract-safe parallel wave, and dispatches the needed specialists. Reattaching live agents does not resend or duplicate assignments.

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

The bootstrap never commits, stashes, deletes, resets, or overwrites user changes. It stops on a dirty main checkout only when an agent is missing and initialization could require creating or opening a workspace/worktree. If all six agents are already live, it may safely reconcile names, restore the board layout, and reattach while the checkout has changes.

## Troubleshooting

New Herdr workspaces can report `agent_pane_busy` for a fraction of a second while their login shell initializes. The bootstrap treats only that exact error as transient and retries every 500 ms for up to 20 seconds. Other start errors fail immediately. Re-running `npm run herdr` is safe: existing workspaces and named live agents are reused.

## Installation and lifecycle

```bash
brew install herdr
herdr integration install codex
herdr integration status
```

Detach with `ctrl+b q`; agents continue running. Run `npm run herdr` to reattach. Herdr's optional global config is TOML at `~/.config/herdr/config.toml`; Zerochi does not replace it.
