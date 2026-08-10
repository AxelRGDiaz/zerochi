import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { MailAccount } from "../types/mail";
import type { PetPresentation } from "../pet/PetState";
import { createAddAccountController, type AddAccountPort } from "../stores/addAccountController";
import { PopupView } from "./PopupView";

const accounts: MailAccount[] = [
  { id: "personal", email: "personal@gmail.com", displayName: "Personal", pendingCount: 2, active: true },
];

const presentation: PetPresentation = {
  animation: "idle_happy",
  message: "Te acompaño con calma.",
  mood: "happy",
};

const emptyPresentation: PetPresentation = {
  animation: "idle_satisfied",
  message: "Todo está tranquilo.",
  mood: "satisfied",
};

function renderPopup(isActive: boolean, accountInput = accounts) {
  const pendingTotal = accountInput.reduce((total, account) => total + account.pendingCount, 0);
  return renderToStaticMarkup(
    <PopupView
      accounts={accountInput}
      isActive={isActive}
      onOpenAddAccount={() => undefined}
      onOpenSettings={() => undefined}
      pendingTotal={pendingTotal}
      presentation={accountInput.length === 0 ? emptyPresentation : presentation}
    />,
  );
}

describe("PopupView", () => {
  it("renders the dedicated empty state for a fresh account list", () => {
    const markup = renderPopup(true, []);

    expect(markup).toContain('class="empty-account-state"');
    expect(markup).toContain("Todavía no hay una cuenta conectada");
    expect(markup).toContain("Elegir proveedor");
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('aria-label="Abrir configuración"');
    expect(markup).not.toContain('class="account-list"');
    expect(markup).not.toContain("@");
  });

  it("renders the existing popup content and live summary while active", () => {
    const markup = renderPopup(true);

    expect(markup).toContain('<main class="app-shell home-popup">');
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

  it("replaces the empty state with the scrolling list and persistent footer after success", async () => {
    const connectedAccount: MailAccount = {
      id: "connected",
      email: "connected@example.com",
      displayName: "Conectada",
      pendingCount: 0,
      active: true,
    };
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: connectedAccount }),
    );

    expect(renderPopup(true, controller.getSnapshot().accounts)).toContain("empty-account-state");
    await controller.addAccount();
    const markup = renderPopup(true, controller.getSnapshot().accounts);

    expect(markup).not.toContain("empty-account-state");
    expect(markup).toContain('class="account-list" aria-label="Cuentas conectadas" tabindex="0"');
    expect(markup).toContain("connected@example.com");
    expect(markup.indexOf('class="account-list"')).toBeLessThan(markup.indexOf('class="app-footer"'));
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('aria-label="Abrir configuración"');
  });
});
