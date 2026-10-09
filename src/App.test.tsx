import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App";
import { appNavigationStore, type AppRoute } from "./stores/appNavigationStore";

function renderRoute(route: AppRoute): string {
  appNavigationStore.navigate(route);
  return renderToStaticMarkup(<App />);
}

afterEach(() => appNavigationStore.back());

describe("anchored popover composition", () => {
  it.each([
    ["home", "home-popup"],
    ["add_account", "add-account-title"],
    ["settings", "settings-title"],
  ] as const)("wraps the %s route in the same presentation-only frame", (route, contentMarker) => {
    const markup = renderRoute(route);

    expect(markup.startsWith('<div class="popover-frame"><main')).toBe(true);
    expect(markup.endsWith("</main></div>")).toBe(true);
    expect(markup).toContain(contentMarker);
    expect(markup.match(/class="popover-frame"/g)).toHaveLength(1);
    expect(markup.match(/<main/g)).toHaveLength(1);
    expect(markup).not.toMatch(/<div class="popover-frame"[^>]*role=/);
  });

  it("keeps exactly one accessible home h1 inside the visually-hidden home header", () => {
    const markup = renderRoute("home");

    expect(markup.match(/<h1/g)).toHaveLength(1);
    expect(markup).toContain('<header class="app-header">');
    expect(markup).toContain('<h1 tabindex="-1">ZEROCHI</h1>');
    expect(markup.indexOf('class="pet-stage"')).toBeGreaterThan(markup.indexOf("</header>"));
  });

  it.each([
    ["add_account", "Agregar cuenta"],
    ["settings", "Configuración"],
  ] as const)("keeps the %s subview heading visible and unique", (route, title) => {
    const markup = renderRoute(route);

    expect(markup.match(/<h1/g)).toHaveLength(1);
    expect(markup).toContain(`>${title}</h1>`);
    expect(markup).toContain('class="subview-header"');
    expect(markup).not.toContain('class="app-header"');
  });
});
