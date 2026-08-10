import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PopupActions } from "./PopupActions";

function renderActions() {
  return renderToStaticMarkup(
    <PopupActions onOpenAddAccount={() => undefined} onOpenSettings={() => undefined} />,
  );
}

describe("PopupActions", () => {
  it("renders native keyboard-operable navigation controls with accessible names", () => {
    const markup = renderActions();

    expect(markup.match(/<button/g)).toHaveLength(2);
    expect(markup.match(/type="button"/g)).toHaveLength(2);
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('aria-label="Abrir configuración"');
    expect(markup).toContain('class="settings-icon" aria-hidden="true"');
  });

  it("gives the settings control a 44px hit target and a clearly visible glyph", () => {
    const markup = renderActions();

    expect(markup).toContain('style="min-height:44px;min-width:44px"');
    expect(markup).toContain('style="font-size:23px"');
  });

  it("identifies both footer controls for comfortable target sizing", () => {
    const markup = renderActions();

    expect(markup).toContain('class="add-account-button"');
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('style="min-height:44px"');
    expect(markup).toContain('class="settings-button"');
  });
});
