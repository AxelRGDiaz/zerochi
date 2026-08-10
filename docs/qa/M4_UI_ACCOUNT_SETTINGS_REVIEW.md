# M4 UI Account and Settings Review

Review date: 2026-08-10

Reviewed range: `0c8e8a78641ec382be63a6c5939ad6ca4a2e04d7..4fc1a5f924a00d9eeb48bdae946626234fce256d`

Reviewed worktree: `/Users/alcheco/.herdr/worktrees/zerochi/codex-ui-m1`, clean before and after review. The UI commit was inspected and tested in place; it was not merged into the QA branch.

## Integration recommendation

**BLOCKED.** Commit `4fc1a5f` must not integrate as-is because the new provider/settings UI has a functional color-contrast accessibility failure in both themes, most severely in dark mode. Routing, command isolation, provider semantics, control sizing, and the prescribed automated build/tests otherwise pass review. The review report itself is safe to integrate and does not approve the UI commit.

## Findings

### M4-UI-01 — New secondary text and controls fail minimum contrast

SEVERITY: MEDIUM

AREA: Functional accessibility / dark theme

FILE: `src/App.css`

PROBLEM: The new back control, subview introduction, status/retry text, settings copy, gear glyph, and active Gmail provider state reuse muted colors without dark-theme foreground overrides. The 11–13px text is below the large-text exception and requires at least 4.5:1 contrast under WCAG 2.2 SC 1.4.3. Measured ratios are 2.29:1–2.93:1 for `#795f66` in the dark theme and 3.43:1 for the active provider state on the dark radial background. The active Gmail “Disponible” state is also only 3.63:1 in the light theme. The 23px gear glyph drops below the 3:1 [non-text contrast requirement](https://www.w3.org/TR/WCAG22/#non-text-contrast) over parts of the dark background.

REPRODUCTION: At `4fc1a5f`, inspect `.back-button`, `.subview-intro`, `.add-account-status`, `.retry-button`, `.settings-copy p`, `.provider-state`, and `.settings-icon` in `src/App.css`. Apply the WCAG relative-luminance formula to the declared foregrounds and effective backgrounds: `#795f66` on `#392d30` = 2.29:1; `#795f66` on `#211b20` = 2.93:1; `#927d80` on `#392d30` = 3.43:1; `#927d80` on the effective light provider-row background `#fcf8f2` = 3.63:1. The applicable text threshold is documented by [WCAG 2.2, Success Criterion 1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum).

RECOMMENDED FIX: The UI owner should add explicit accessible foreground colors for light and dark subviews, including the active Gmail state, back control, settings copy, status/retry text, and gear glyph. Recalculate against every actual solid/radial/alpha-composited background and require at least 4.5:1 for normal text and at least 3:1 for the visible control glyph/focus boundary. Re-run the full UI suite and capture 390px light/dark visual evidence before re-review.

### M4-UI-02 — Tests do not exercise real navigation events or heading focus

SEVERITY: LOW

AREA: Accessibility regression coverage

FILE: `src/views/SettingsView.test.tsx`, `src/views/ProviderSelectionView.test.tsx`, `src/components/PopupActions.test.tsx`, `src/components/ViewHeading.tsx`

PROBLEM: The new view tests render static server markup, so they do not click the footer/back/provider controls, run the `ViewHeading` effect, or assert `document.activeElement`. The settings test calls `focusViewHeading` directly with a mock, which proves only the helper invocation. Source inspection supports deterministic heading focus on each route mount, but the actual focus and event wiring lack DOM-level regression coverage. Compact 390px and dark-theme coverage is likewise CSS inspection only.

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
| Gear size | PASS | Settings control has `minHeight: 44`, `minWidth: 44`; its hidden decorative glyph is 23px, within the requested 22–24px range. Contrast is blocked separately in M4-UI-01. |
| Compact layout | PASS by source/build review | `.app-shell` remains `width: min(100%, 390px)` with 24px side padding; new provider/settings content uses width-constrained grids and rows. Runtime visual coverage remains untested. |
| Dark theme selectors | PARTIAL / BLOCKED | New provider rows/settings sections receive dark backgrounds, but their foreground colors fail contrast as M4-UI-01 documents. |
| UI security/contract boundary | PASS | No changed native/contract/dependency file, new `invoke`, OAuth config, token/secret field, callback/provider data, or preference state was found. The production bundle scan is clean. |

## Checks and results

| Check | Result |
|---|---|
| Required `.agent` coordination documents | Read before review; QA ownership and zero-argument command contract confirmed. |
| `git status --short --branch` in UI worktree | Clean `codex/ui-m1` before and after tests/build. |
| `git merge-base --is-ancestor 0c8e8a7 4fc1a5f` | PASS. |
| `git diff --no-ext-diff --no-renames 0c8e8a7..4fc1a5f` | Inspected all 19 range files; commit `4fc1a5f` itself changes 17 UI-owned source/test/style files. No merge performed. |
| `git diff --check 0c8e8a7..4fc1a5f` | PASS; no whitespace errors. |
| `npm test` in UI worktree | PASS: 9 files, 45/45 tests. |
| `npm run check` in UI worktree | PASS: 45/45 tests plus TypeScript/Vite production build. |
| `npm run build` in UI worktree | PASS: 50 modules; JS 202.29 kB (63.75 kB gzip), CSS 3.90 kB (1.31 kB gzip). |
| Changed-line OAuth/config/token/secret/preference scan | PASS: zero runtime matches. |
| Changed-line native invocation scan | PASS: zero new `invoke` calls. |
| Changed native/contract/dependency file scan | PASS: zero files under `src-tauri`, `src/types`, or package manifests/lockfile. |
| Built `dist` OAuth/config/token/secret scan | PASS: zero matching bundle files. |
| WCAG relative-luminance calculation | FAIL: active/new UI color pairs measured 2.29:1–3.63:1, below the applicable 4.5:1 text threshold. |

The initial sandboxed npm attempts failed before test/build execution because Vite could not write `node_modules/.vite-temp` in the separate UI worktree (`EPERM`). The same exact commands were rerun with narrowly approved write access for generated Vite cache/build output and produced the passing results above. No tracked UI file changed.

## Untested visual and native surfaces

- No screenshot or pixel inspection of the actual 390px macOS WebView in light/dark appearance.
- No real keyboard traversal, VoiceOver announcement, or DOM `activeElement` observation; current markup/focus conclusions are source-based.
- No text zoom, long localization, reduced-motion, forced-colors, or high-contrast appearance pass.
- No native popup hide/show, blur, reopen, or route-retention test with the Tauri tray/window lifecycle.
- No live OAuth browser flow or native `add_account` command was invoked; the reviewed UI commit adds no such command.

## Integration needs

Lead should return M4-UI-01 to the UI owner and keep `4fc1a5f` out of the integration branch until revised light/dark colors meet the stated thresholds and QA reruns the exact range review on the replacement commit. The DOM focus/navigation and 390px visual matrix in M4-UI-02 should be added with that fix if feasible. No contract, native, OAuth, preference, or migration coordination is otherwise required for this UI diff.
