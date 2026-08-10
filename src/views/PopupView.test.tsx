import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { MailAccount } from "../types/mail";
import type { PetPresentation } from "../pet/PetState";
import { PopupView } from "./PopupView";

const accounts: MailAccount[] = [
  { id: "personal", email: "personal@gmail.com", displayName: "Personal", pendingCount: 2, active: true },
];

const presentation: PetPresentation = {
  animation: "idle_happy",
  message: "Te acompaño con calma.",
  mood: "happy",
};

function renderPopup(isActive: boolean) {
  return renderToStaticMarkup(
    <PopupView
      accounts={accounts}
      isActive={isActive}
      onOpenAddAccount={() => undefined}
      onOpenSettings={() => undefined}
      pendingTotal={2}
      presentation={presentation}
    />,
  );
}

describe("PopupView", () => {
  it("renders the existing popup content and live summary while active", () => {
    const markup = renderPopup(true);

    expect(markup).toContain("YOUR INBOX PET");
    expect(markup).toContain("ZEROCHI");
    expect(markup).toContain("Tengo 2 mails");
    expect(markup).toContain("Te acompaño con calma.");
    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('aria-label="Cuentas conectadas"');
    expect(markup).toContain('aria-label="Zerochi está happy"');
  });

  it("does not render popup or animation content while inactive", () => {
    expect(renderPopup(false)).toBe("");
  });

  it("keeps account and popup actions keyboard-operable with labels", () => {
    const markup = renderPopup(true);

    expect(markup.match(/<button/g)).toHaveLength(3);
    expect(markup.match(/type="button"/g)).toHaveLength(3);
    expect(markup).toContain('aria-label="2 pendientes"');
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('aria-label="Abrir configuración"');
  });
});
