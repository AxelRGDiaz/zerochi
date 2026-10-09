import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { MailAccount } from "../types/mail";
import type { PetPresentation } from "../pet/PetState";
import { createAddAccountController, type AddAccountPort } from "../stores/addAccountController";
import { inboxUnreadTotal } from "../stores/mailCounters";
import { PopupView } from "./PopupView";

const accounts: MailAccount[] = [
  {
    id: "personal",
    email: "personal@gmail.com",
    displayName: "Personal",
    pendingCount: 2,
    inboxUnreadCount: 3,
    active: true,
  },
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
  return renderToStaticMarkup(
    <PopupView
      accounts={accountInput}
      inboxUnreadTotal={inboxUnreadTotal(accountInput)}
      isActive={isActive}
      onOpenAccount={() => undefined}
      onOpenAddAccount={() => undefined}
      onOpenSettings={() => undefined}
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
    expect(markup).not.toContain('class="summary"');
    expect(markup).not.toContain('class="account-count"');
    expect(markup).not.toContain("Tengo 0 mails");
    expect(markup).not.toContain('aria-label="0 pendientes"');
    expect(markup).not.toContain("@");
  });

  it("renders the existing popup content and live summary while active", () => {
    const markup = renderPopup(true);

    expect(markup).toContain('<main class="app-shell home-popup">');
    expect(markup).toContain("TU MASCOTA DE BANDEJA");
    expect(markup).toContain("ZEROCHI");
    expect(markup).toContain("3 correos sin leer");
    expect(markup).toContain('aria-label="3 correos sin leer en total"');
    expect(markup).toContain("Te acompaño con calma.");
    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('aria-label="Cuentas conectadas"');
    expect(markup).toContain('aria-label="Zerochi está feliz"');
  });

  it.each([
    [0, "0 correos sin leer", "0 correos sin leer en total"],
    [1, "1 correo sin leer", "1 correo sin leer en total"],
  ] as const)("renders normalized global copy for %i unread", (unread, copy, accessibleCopy) => {
    const markup = renderPopup(true, [{ ...accounts[0], inboxUnreadCount: unread }]);

    expect(markup).toContain(copy);
    expect(markup).toContain(`aria-label="${unread} sin leer"`);
    expect(markup).toContain(`aria-label="${accessibleCopy}"`);
  });

  it("does not render popup or animation content while inactive", () => {
    expect(renderPopup(false)).toBe("");
  });

  it("keeps account and popup actions keyboard-operable with labels", () => {
    const markup = renderPopup(true);

    expect(markup.match(/<button/g)).toHaveLength(3);
    expect(markup.match(/type="button"/g)).toHaveLength(3);
    expect(markup).toContain('aria-label="3 sin leer"');
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('aria-label="Abrir configuración"');
  });

  it("keeps the global unread total aligned with multiple account badges", () => {
    const accountInput: MailAccount[] = [
      { ...accounts[0], pendingCount: 9, inboxUnreadCount: 3 },
      {
        id: "work",
        email: "work@example.com",
        pendingCount: 0,
        inboxUnreadCount: 4,
        active: true,
      },
    ];
    const markup = renderPopup(true, accountInput);

    expect(markup).toContain("7 correos sin leer");
    expect(markup).toContain('aria-label="3 sin leer"');
    expect(markup).toContain('aria-label="4 sin leer"');
    expect(markup).toContain("Te acompaño con calma.");
    expect(markup).not.toContain("9 correos sin leer");
  });

  it("shows account 3 and global 3 when Zerochi pending is zero", () => {
    const markup = renderPopup(true, [{
      ...accounts[0],
      pendingCount: 0,
      inboxUnreadCount: 3,
    }]);

    expect(markup).toContain('aria-label="3 sin leer"');
    expect(markup).toContain('aria-label="3 correos sin leer en total"');
    expect(markup).toContain("3 correos sin leer");
    expect(markup).not.toContain("0 correos sin leer");
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
