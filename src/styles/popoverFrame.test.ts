import { describe, expect, it } from "vitest";
// @ts-expect-error Node types are intentionally absent from the browser bundle config.
import { readFileSync } from "node:fs";

const appCss = readFileSync(new URL("../App.css", import.meta.url), "utf8");

describe("anchored popover CSS contract", () => {
  it("leaves the native canvas transparent and clips document overflow", () => {
    expect(appCss).toContain("html, body, #root { width: 100%; height: 100%; margin: 0; overflow: hidden; background: transparent; }");
    expect(appCss).toMatch(/:root \{[\s\S]*background: transparent;/);
  });

  it("keeps safe outer gutters, rounded frame, pointer, and contained shadows", () => {
    expect(appCss).toContain("width: calc(100% - 32px)");
    expect(appCss).toContain("height: calc(100% - 40px)");
    expect(appCss).toContain("margin: 16px 16px 24px");
    expect(appCss).toContain("border-radius: 28px");
    expect(appCss).toContain("filter: drop-shadow(0 10px 14px var(--popover-shadow-near)) drop-shadow(0 3px 6px var(--popover-shadow-far))");
    expect(appCss).toContain(".popover-frame::before");
    expect(appCss).toContain("left: var(--popover-anchor-x)");
    expect(appCss).toContain('content: ""');
    expect(appCss).toContain("pointer-events: none");
  });

  it("fills the frame with transparent shells and scrolls long subviews internally", () => {
    expect(appCss).toContain(".app-shell { width: 100%; height: 100%; min-height: 0;");
    expect(appCss).toMatch(/\.app-shell \{[^}]*overflow-y: auto;[^}]*border-radius: inherit;[^}]*background: transparent;/);
    expect(appCss).toMatch(/\.home-popup \{[^}]*overflow: hidden;/);
    expect(appCss).toMatch(/\.home-popup \.account-list \{[^}]*overflow-y: auto;/);
    expect(appCss).toMatch(/\.settings-view \{[^}]*height: 100%;[^}]*overflow-y: auto;/);
  });

  it("visually hides only the home header so the pet occupies row one", () => {
    expect(appCss).toMatch(/\.home-popup > \.app-header \{[^}]*position: absolute;[^}]*clip: rect\(0 0 0 0\);[^}]*white-space: nowrap;/);
    expect(appCss).toContain(".home-popup > .pet-stage { grid-row: 1; }");
    expect(appCss).not.toMatch(/\.subview-header[^}]*clip:/);
  });

  it("defines original light and dark surface, highlight, and shadow variables", () => {
    expect(appCss).toContain("--popover-surface: #f4eee6");
    expect(appCss).toContain("--popover-highlight: #fffaf2");
    expect(appCss).toContain("--popover-surface: var(--dark-surface)");
    expect(appCss).toContain("--popover-highlight: var(--dark-raised-surface)");
    expect(appCss).toContain("--popover-shadow-near: rgba(0, 0, 0, .34)");
    expect(appCss).toContain("--popover-shadow-far: rgba(0, 0, 0, .2)");
  });

  it("animates manual sync without forcing motion for reduced-motion users", () => {
    expect(appCss).toContain(".settings-check-mail-spinner");
    expect(appCss).toContain("animation: settings-check-mail-spin .8s linear infinite");
    expect(appCss).toContain("@keyframes settings-check-mail-spin");
    expect(appCss).toContain("animation: settings-check-mail-pulse 1.2s ease-in-out infinite");
    expect(appCss).toContain("@keyframes settings-check-mail-pulse");
    expect(appCss).toMatch(/@media \(prefers-reduced-motion: reduce\) \{[^}]*\.settings-check-mail-spinner[^}]*\.settings-check-mail-label \{ animation: none; \}/);
  });
});
