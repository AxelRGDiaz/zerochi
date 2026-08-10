# M4 UI Account and Settings Review

Review date: 2026-08-10

Original reviewed range: `0c8e8a78641ec382be63a6c5939ad6ca4a2e04d7..4fc1a5f924a00d9eeb48bdae946626234fce256d`

Corrective re-review range: `4fc1a5f924a00d9eeb48bdae946626234fce256d..09676fdc8f305b7ee471f118085ed253a64dbbb1`

Final corrective range: `09676fdc8f305b7ee471f118085ed253a64dbbb1..9be231fc9a6c4cd1b785b4f2a3d5b17e3127a1d8`

Reviewed worktree: `/Users/alcheco/.herdr/worktrees/zerochi/codex-ui-m1`. During the prior re-review, concurrent UI-owner edits appeared after the `09676fd` commands and were excluded from that verdict. Those edits were subsequently committed by the owner as `9be231f`. The final re-review started and ended with the worktree clean at that exact commit. No UI commit was merged into the QA branch.

## Integration recommendation

**SCOPED GREEN.** Commits `09676fd` and `9be231f` together resolve the contrast blocker from `4fc1a5f`. Active Gmail “Disponible” text now measures 5.4502:1 and 5.5967:1 on the two effective light provider surfaces, exceeding 4.5:1. The prior dark secondary/control, disabled-provider, primary-text, and gear ratios remain unchanged and passing. Routing, command isolation, provider semantics, control sizing, compact layout, secret boundaries, committed palette tests, and the full build/test suite are green. This recommendation covers integration of the reviewed UI commit chain only; it does not accept the native OAuth command or untested visual/native surfaces.

## Findings

### M4-UI-01 — Resolved: light and dark provider/settings palettes meet minimum contrast

SEVERITY: MEDIUM (resolved by `09676fd` and `9be231f`)

AREA: Functional accessibility / light and dark themes

FILE: `src/App.css`

PROBLEM: The original `4fc1a5f` palette failed dark secondary/control and active Gmail light-state contrast. Commit `09676fd` corrected the dark palette but left the enabled Gmail “Disponible” state at 3.6303:1–3.7279:1. Final commit `9be231f` changes only that active light state from `#927d80` to `#795f66` and adds both effective light surfaces to the committed palette tests. No required text or non-text threshold now fails.

REPRODUCTION: At committed `9be231f`, independently apply the WCAG relative-luminance formula rather than trusting the new tests. Light active-provider `#795f66` measures 5.4502:1 on effective `#fcf8f2` and 5.5967:1 on effective `#fffbf6`. The retained dark matrix is byte-for-byte unchanged: secondary/control `#c5b4b7` is 8.5216:1 on `#211b20`, 7.0525:1 on effective `#34292d`, and 6.6497:1 on `#392d30`; disabled-provider `#a89498` is 5.9199:1, 4.8994:1, and 4.6195:1; primary `#f4e9df` is 14.1413:1, 11.7034:1, and 11.0350:1. The gear inherits `#c5b4b7`, so its minimum is 6.6497:1. Text exceeds [WCAG 2.2 SC 1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum), and the gear exceeds the 3:1 [non-text contrast requirement](https://www.w3.org/TR/WCAG22/#non-text-contrast).

RECOMMENDED FIX: Complete. Preserve `#795f66` for the enabled light provider state, the current dark variables, and the committed two-surface light plus three-surface dark regression matrix. Any future palette change must rerun the same independent calculations.

### M4-UI-02 — Tests do not exercise real navigation events or heading focus

SEVERITY: LOW

AREA: Accessibility regression coverage

FILE: `src/views/SettingsView.test.tsx`, `src/views/ProviderSelectionView.test.tsx`, `src/components/PopupActions.test.tsx`, `src/components/ViewHeading.tsx`

PROBLEM: The view tests render static server markup, so they do not click the footer/back/provider controls, run the `ViewHeading` effect, or assert `document.activeElement`. The settings test calls `focusViewHeading` directly with a mock, which proves only the helper invocation. Source inspection supports deterministic heading focus on each route mount, but the actual focus and event wiring lack DOM-level regression coverage. Compact 390px coverage remains CSS inspection only. Commit `09676fd` adds useful numeric dark-palette tests but does not close these interaction/visual gaps.

REPRODUCTION: The tests import `renderToStaticMarkup`; repository test scans find no `userEvent`, `fireEvent`, `document.activeElement`, `toHaveFocus`, browser/DOM accessibility runner, or rendered 390px/dark-theme assertion. `SettingsView.test.tsx` manually invokes `focusViewHeading({ focus })` rather than mounting `ViewHeading` and observing focus.

RECOMMENDED FIX: Add an owner-approved DOM interaction test that mounts `App`, clicks Add account and Settings, verifies the correct route heading owns focus, activates Back, verifies the home heading owns focus, and proves footer navigation alone never invokes OAuth. Add a small browser visual matrix at 390px for light/dark and keyboard focus. This coverage gap does not independently block this commit, but should accompany the contrast fix.

## Verified behavior

| Requirement | Result | Evidence |
|---|---|---|
| `home` / `add_account` / `settings` routing | PASS | `AppRoute` is an exact three-value union; `App.tsx` renders the matching view; store tests cover navigation, duplicate navigation, and Back to home. |
| Footer Add account does not invoke OAuth | PASS | `PopupActions` receives only `onOpenAddAccount`; `App.tsx` maps it to `appNavigationStore.navigate("add_account")`. Provider-view rendering test confirms no Gmail invocation on entry. |
| Gmail is sole enabled provider | PASS | `accountProviders` marks only Gmail enabled. Gmail alone resolves to `onAddGmail`, which calls the existing zero-argument duplicate-safe controller. Double controller activation reaches the injected port once. |
| Outlook/iCloud disabled and callback-free | PASS | Both render native `disabled`, include visible and accessible “Próximamente”, and `getProviderAction` returns `undefined`; tests assert both results. React therefore attaches no click handler. |
| Settings is honest/read-only | PASS | It states Gmail read-only behavior and that preferences are not configurable; it renders no input, checkbox, select, save action, preference store, or persistence command. |
| Back navigation | PASS by source/unit review | Both subviews use the labeled native Back button and `appNavigationStore.back`, which deterministically routes home. DOM click coverage remains M4-UI-02. |
| Deterministic heading focus | PASS by source review | Every route mounts `ViewHeading`; its mount effect focuses an `h1` with `tabIndex={-1}`. Back remounts the home view/heading. DOM focus coverage remains M4-UI-02. |
| Accessible names/status roles | PASS | Add, settings, Back, Gmail, future providers, and Retry have explicit Spanish accessible names. Pending/success/cancelled use `role=status`; failures use `role=alert`; subviews use `aria-labelledby`. |
| Gear size and visibility | PASS | Settings control remains `minHeight: 44`, `minWidth: 44`; its hidden decorative glyph remains 23px. It inherits dark secondary `#c5b4b7`, at least 6.6497:1 on requested dark surfaces, exceeding the 3:1 non-text threshold. |
| Compact layout | PASS by source/build review | `.app-shell` remains `width: min(100%, 390px)` with 24px side padding; new provider/settings content uses width-constrained grids and rows. Runtime visual coverage remains untested. |
| Dark theme palette | PASS | Secondary/control text: 6.6497:1–8.5216:1; disabled providers: 4.6195:1–5.9199:1; primary text: 11.0350:1–14.1413:1 across `#211b20`, `#34292d`, and `#392d30`. |
| Active Gmail state in light mode | PASS | Final `#795f66` is 5.4502:1 on `#fcf8f2` and 5.5967:1 on `#fffbf6`, exceeding 4.5:1. |
| UI security/contract boundary | PASS | The final corrective diff changes only `src/App.css` and palette test/module files. No behavior file, native/contract/dependency file, new `invoke`, OAuth config, token/secret field, callback/provider data, or preference state was found. The production bundle scan is clean. |

## Checks and results

| Check | Result |
|---|---|
| Required `.agent` coordination documents | Read before review; QA ownership and zero-argument command contract confirmed. |
| `git status --short --branch` in UI worktree | Clean `codex/ui-m1` at final re-review start and completion, on `9be231f`. |
| `git merge-base --is-ancestor 09676fd 9be231f` | PASS. |
| `git diff --no-ext-diff --no-renames 09676fd..9be231f` | Inspected exact final corrective diff: `src/App.css`, `src/styles/darkPalette.test.ts`, and new `src/styles/lightPalette.ts`; 37 insertions/1 deletion. No merge performed. |
| `git diff --check 09676fd..9be231f` | PASS; no whitespace errors. |
| Behavior-file equality check across corrective range | PASS: routing, navigation, provider enablement/callbacks, controller, settings content, view markup, gear sizing, and compact layout source files are unchanged. |
| `npm test -- src/styles/darkPalette.test.ts` | PASS: 10/10 committed dark/light palette tests. |
| `npm test` in UI worktree | PASS: 10 files, 55/55 tests. |
| `npm run check` in UI worktree | PASS: 55/55 tests plus TypeScript/Vite production build. |
| `npm run build` in UI worktree | PASS: 50 modules; JS 202.29 kB (63.75 kB gzip), CSS 4.47 kB (1.45 kB gzip). |
| Changed-line OAuth/config/token/secret/preference scan | PASS: zero runtime matches. |
| Changed-line native invocation scan | PASS: zero new `invoke` calls. |
| Changed native/contract/dependency file scan | PASS: zero files under `src-tauri`, `src/types`, or package manifests/lockfile. |
| Built `dist` OAuth/config/token/secret scan | PASS: zero matching bundle files. |
| Independent dark WCAG matrix | PASS: secondary/control 6.6497:1–8.5216:1; disabled-provider 4.6195:1–5.9199:1; primary 11.0350:1–14.1413:1. |
| Independent gear non-text calculation | PASS: inherited secondary color is at least 6.6497:1, exceeding 3:1. |
| Independent active Gmail light calculation | PASS: 5.4502:1–5.5967:1, exceeding 4.5:1. |

The npm commands used narrowly approved access for generated Vite cache/build output in the separate UI worktree. Final behavior/security comparisons and bundle output use committed object `9be231f` from a clean worktree.

## Untested visual and native surfaces

- No screenshot or pixel inspection of the actual 390px macOS WebView in light/dark appearance.
- No real keyboard traversal, VoiceOver announcement, or DOM `activeElement` observation; current markup/focus conclusions are source-based.
- No text zoom, long localization, reduced-motion, forced-colors, or high-contrast appearance pass.
- No native popup hide/show, blur, reopen, or route-retention test with the Tauri tray/window lifecycle.
- No live OAuth browser flow or native `add_account` command was invoked; the reviewed UI commit adds no such command.

## Integration needs

Lead may integrate the reviewed UI chain through full commit `9be231fc9a6c4cd1b785b4f2a3d5b17e3127a1d8`. The scoped green verdict depends on retaining both contrast corrections and their committed palette tests. M4-UI-02 remains a non-blocking residual gap: DOM focus/navigation and actual 390px light/dark visual/native testing should be scheduled, but no current source defect was found. No contract, native, OAuth, preference, or migration coordination is otherwise required for this UI diff.
