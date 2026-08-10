# M4 UI Account and Settings Review

Review date: 2026-08-10

Original reviewed range: `0c8e8a78641ec382be63a6c5939ad6ca4a2e04d7..4fc1a5f924a00d9eeb48bdae946626234fce256d`

Corrective re-review range: `4fc1a5f924a00d9eeb48bdae946626234fce256d..09676fdc8f305b7ee471f118085ed253a64dbbb1`

Reviewed worktree: `/Users/alcheco/.herdr/worktrees/zerochi/codex-ui-m1`. It was clean at corrective re-review start with `09676fd` checked out. The prescribed tests/build completed against that commit. Concurrent UI-owner edits appeared afterward at 15:56:39 in `src/App.css`, `src/styles/darkPalette.test.ts`, and a new `src/styles/lightPalette.ts`; QA did not create, modify, discard, test, or use those uncommitted changes as evidence. No UI commit was merged into the QA branch.

## Integration recommendation

**BLOCKED.** Corrective commit `09676fd` resolves the dark-theme contrast failures, including secondary/control text, disabled providers, focus outline, and gear visibility. It still does not meet the requested normal-text threshold for the active Gmail “Disponible” state in light mode: independently measured ratios are 3.6303:1 and 3.7279:1 on the two possible effective provider-row backgrounds, below 4.5:1. Routing, command isolation, provider semantics, control sizing, compact layout, and the prescribed automated build/tests remain green. The review report itself is safe to integrate and does not approve the UI commit.

## Findings

### M4-UI-01 — Dark palette is corrected but active Gmail light text still fails minimum contrast

SEVERITY: MEDIUM

AREA: Functional accessibility / light and dark themes

FILE: `src/App.css`

PROBLEM: Commit `09676fd` adds dark secondary `#c5b4b7` and disabled-provider `#a89498` colors that pass every requested dark surface. The 23px gear inherits the secondary color and now also passes the 3:1 [non-text contrast requirement](https://www.w3.org/TR/WCAG22/#non-text-contrast). However, the active Gmail provider state remains light-theme `#927d80` at 11px/600 weight. It measures only 3.6303:1 over the effective base panel and 3.7279:1 over the effective raised panel, below the 4.5:1 normal-text requirement. Disabled Outlook/iCloud semantics remain correct, but the enabled Gmail state is not an inactive-control exception.

REPRODUCTION: At committed `09676fd`, apply the WCAG relative-luminance formula independently of `darkPalette.test.ts`. Dark secondary `#c5b4b7` ratios are 8.5216:1 on `#211b20`, 7.0525:1 on effective `#34292d` panels, and 6.6497:1 on `#392d30` raised surfaces. Disabled-provider `#a89498` ratios are 5.9199:1, 4.8994:1, and 4.6195:1 on the same surfaces. Primary `#f4e9df` ratios are 14.1413:1, 11.7034:1, and 11.0350:1. The gear’s inherited `#c5b4b7` therefore has at least 6.6497:1. In light mode, alpha-composite provider layer `rgba(255,252,247,.72)` over `#f4eee6` to `#fcf8f2`, and over `#fffaf2` to `#fffbf6`; `#927d80` measures 3.6303:1 and 3.7279:1 respectively. The applicable text threshold is documented by [WCAG 2.2, Success Criterion 1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum).

RECOMMENDED FIX: Preserve the passing dark variables and add a light-theme active-provider foreground that reaches at least 4.5:1 against both `#fcf8f2` and `#fffbf6`. Add committed tests for those two light composites as well as the existing dark matrix, then rerun test/check/build and the independent QA calculation. Do not rely on native `disabled` exemptions for the enabled Gmail row.

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
| Active Gmail state in light mode | BLOCKED | Unchanged `#927d80` is 3.6303:1 on `#fcf8f2` and 3.7279:1 on `#fffbf6`, below 4.5:1. |
| UI security/contract boundary | PASS | The corrective diff changes only `src/App.css` and palette test/module files. No behavior file, native/contract/dependency file, new `invoke`, OAuth config, token/secret field, callback/provider data, or preference state was found. The production bundle scan is clean. |

## Checks and results

| Check | Result |
|---|---|
| Required `.agent` coordination documents | Read before review; QA ownership and zero-argument command contract confirmed. |
| `git status --short --branch` in UI worktree | Clean `codex/ui-m1` at re-review start and during the completed commands. Concurrent uncommitted owner edits appeared afterward and were excluded. |
| `git merge-base --is-ancestor 4fc1a5f 09676fd` | PASS. |
| `git diff --no-ext-diff --no-renames 4fc1a5f..09676fd` | Inspected exact corrective diff: `src/App.css`, `src/styles/darkPalette.ts`, and `src/styles/darkPalette.test.ts`; 112 insertions/2 deletions. No merge performed. |
| `git diff --check 4fc1a5f..09676fd` | PASS; no whitespace errors. |
| Behavior-file equality check across corrective range | PASS: routing, navigation, provider enablement/callbacks, controller, settings content, view markup, gear sizing, and compact layout source files are unchanged. |
| `npm test` in UI worktree | PASS: 10 files, 52/52 tests, including 7 committed dark-palette tests. |
| `npm run check` in UI worktree | PASS: 52/52 tests plus TypeScript/Vite production build. |
| `npm run build` in UI worktree | PASS: 50 modules; JS 202.29 kB (63.75 kB gzip), CSS 4.40 kB (1.43 kB gzip). |
| Changed-line OAuth/config/token/secret/preference scan | PASS: zero runtime matches. |
| Changed-line native invocation scan | PASS: zero new `invoke` calls. |
| Changed native/contract/dependency file scan | PASS: zero files under `src-tauri`, `src/types`, or package manifests/lockfile. |
| Built `dist` OAuth/config/token/secret scan | PASS: zero matching bundle files. |
| Independent dark WCAG matrix | PASS: secondary/control 6.6497:1–8.5216:1; disabled-provider 4.6195:1–5.9199:1; primary 11.0350:1–14.1413:1. |
| Independent gear non-text calculation | PASS: inherited secondary color is at least 6.6497:1, exceeding 3:1. |
| Independent active Gmail light calculation | FAIL: 3.6303:1–3.7279:1, below 4.5:1. |

The npm commands used narrowly approved access for generated Vite cache/build output in the separate UI worktree. File timestamps show the concurrent owner edits appeared at 15:56:39, after the prescribed test/check/build completed. Final behavior/security comparisons use committed object `09676fd`, not the dirty worktree. QA preserved all concurrent edits.

## Untested visual and native surfaces

- No screenshot or pixel inspection of the actual 390px macOS WebView in light/dark appearance.
- No real keyboard traversal, VoiceOver announcement, or DOM `activeElement` observation; current markup/focus conclusions are source-based.
- No text zoom, long localization, reduced-motion, forced-colors, or high-contrast appearance pass.
- No native popup hide/show, blur, reopen, or route-retention test with the Tauri tray/window lifecycle.
- No live OAuth browser flow or native `add_account` command was invoked; the reviewed UI commit adds no such command.

## Integration needs

Lead should keep `09676fd` out of the integration branch until the UI owner commits a light active-provider foreground that passes both effective light backgrounds and adds light-palette regression coverage. The corrected dark palette should be preserved. QA must re-run the new corrective range, independent ratios, and test/check/build on the replacement commit. The DOM focus/navigation and 390px visual matrix in M4-UI-02 remain non-blocking residual gaps. No contract, native, OAuth, preference, or migration coordination is otherwise required.
