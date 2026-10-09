import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { focusViewHeading } from "../components/ViewHeading";
import { createAppNavigationStore } from "../stores/appNavigationStore";
import type { CheckMailNowState } from "../stores/checkMailNowController";
import type { RemoveAccountState } from "../stores/removeAccountController";
import type { MailAccount } from "../types/mail";
import { focusRemovalTransition, SettingsView } from "./SettingsView";

const linkedAccounts: MailAccount[] = [
  {
    id: "account-a",
    email: "ana@example.com",
    displayName: "Ana",
    pendingCount: 9,
    inboxUnreadCount: 3,
    active: true,
  },
  {
    id: "account-b",
    email: "equipo@example.com",
    pendingCount: 1,
    active: true,
  },
];

function renderSettings(
  state: RemoveAccountState = { status: "idle" },
  accounts: MailAccount[] = linkedAccounts,
  checkMailNowState: CheckMailNowState = { status: "idle" },
) {
  return renderToStaticMarkup(
    <SettingsView
      accounts={accounts}
      checkMailNowState={checkMailNowState}
      onBack={() => undefined}
      onCheckMailNow={() => undefined}
      onCancelRemove={() => undefined}
      onConfirmRemove={() => undefined}
      onRequestRemove={() => undefined}
      removeAccountState={state}
    />,
  );
}

describe("SettingsView", () => {
  it("renders an accessible title, back control, and honest read-only information", () => {
    const markup = renderSettings();

    expect(markup).toContain('<main class="app-shell settings-view" aria-labelledby="settings-title">');
    expect(markup).not.toContain("home-popup");
    expect(markup).toContain('<h1 id="settings-title" tabindex="-1">Configuración</h1>');
    expect(markup).toContain('aria-label="Volver al inicio"');
    expect(markup).toContain("todas las cuentas activas");
    expect(markup).toContain('<span class="settings-check-mail-label">Sincronizar correos nuevos</span>');
  });

  it("lists linked account identity and Gmail unread independently from pending", () => {
    const markup = renderSettings();

    expect(markup).toContain('aria-label="Cuentas vinculadas"');
    expect(markup).toContain("Ana");
    expect(markup).toContain("ana@example.com");
    expect(markup).toContain("equipo@example.com");
    expect(markup).toContain('aria-label="3 sin leer"');
    expect(markup).toContain('aria-label="0 sin leer"');
    expect(markup).not.toContain("9 pendientes");
  });

  it("keeps an inactive account manageable without exposing its stale unread count", () => {
    const markup = renderSettings({ status: "idle" }, [{
      ...linkedAccounts[0],
      active: false,
      inboxUnreadCount: 80,
    }]);

    expect(markup).toContain("ana@example.com");
    expect(markup).toContain('aria-label="Desvincular Ana"');
    expect(markup).toContain('aria-label="Conteo sin leer no disponible para cuenta inactiva"');
    expect(markup).toContain('class="account-count" aria-label="Conteo sin leer no disponible para cuenta inactiva">—</span>');
    expect(markup).not.toContain('aria-label="80 sin leer"');
    expect(markup).not.toMatch(/>80</);
  });

  it("renders an inline keyboard-operable confirmation with explicit cancel", () => {
    const markup = renderSettings({ status: "confirming", accountId: "account-a" });

    expect(markup).toContain('role="group"');
    expect(markup).toContain('aria-labelledby="remove-account-confirmation-0"');
    expect(markup).toContain("¿Desvincular esta cuenta de Zerochi?");
    expect(markup).toContain('<button class="settings-remove-confirm" type="button">Desvincular</button>');
    expect(markup).toContain('<button class="settings-remove-cancel" type="button">Cancelar</button>');
    expect(markup.match(/type="button"/g)).toHaveLength(5);
  });

  it.each([
    ["account_not_found", "La cuenta ya no está vinculada."],
    ["secure_store_failed", "No pudimos borrar las credenciales del llavero. La cuenta sigue vinculada."],
    ["internal", "No pudimos desvincular la cuenta."],
  ] as const)("renders fixed sanitized %s feedback inline", (tag, copy) => {
    const markup = renderSettings({ status: "error", accountId: "account-a", tag });

    expect(markup).toContain('role="alert"');
    expect(markup).toContain(copy);
  });

  it("uses generic copy when no approved error tag is available", () => {
    const markup = renderSettings({ status: "error", accountId: "account-a" });

    expect(markup).toContain("No pudimos desvincular la cuenta.");
    expect(markup).not.toMatch(/token|provider|body/i);
  });

  it("shows an accessible completion status and an honest empty list", () => {
    const markup = renderSettings({ status: "success" }, []);

    expect(markup).toContain('role="status"');
    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('tabindex="-1"');
    expect(markup).toContain("Cuenta desvinculada correctamente.");
    expect(markup).toContain("No hay cuentas vinculadas.");
  });

  it("announces pending removal and marks only its account row busy", () => {
    const markup = renderSettings({ status: "pending", accountId: "account-a" });

    expect(markup.match(/aria-busy="true"/g)).toHaveLength(1);
    expect(markup).toContain('<p class="settings-removal-pending" role="status" aria-live="polite">Desvinculando cuenta…</p>');
    expect(markup).toContain("Desvinculando…");
  });

  it("disables only the mail-check control while its request is pending", () => {
    const markup = renderSettings(
      { status: "confirming", accountId: "account-a" },
      linkedAccounts,
      { status: "pending" },
    );

    expect(markup).toContain('aria-busy="true"');
    expect(markup).toContain('disabled=""');
    expect(markup).toContain('<span class="settings-check-mail-spinner" aria-hidden="true"></span>');
    expect(markup).toContain('<span class="settings-check-mail-label">Sincronizando correos…</span>');
    expect(markup).toContain('id="check-mail-status" class="settings-check-mail-status" role="status" aria-live="polite">Sincronizando correos…</p>');
    expect(markup).toContain('<button class="settings-remove-confirm" type="button">Desvincular</button>');
    expect(markup).toContain('<button class="settings-remove-cancel" type="button">Cancelar</button>');
  });

  it("announces that a search was initiated without claiming completion", () => {
    const markup = renderSettings({ status: "idle" }, linkedAccounts, { status: "success" });

    expect(markup).toContain('role="status" aria-live="polite">Sincronización iniciada.</p>');
    expect(markup).not.toContain("Búsqueda completada");
    expect(markup).not.toContain("Correos actualizados");
    expect(markup).not.toContain("disabled");
  });

  it("renders a fixed local mail-check failure without disabling retry", () => {
    const markup = renderSettings({ status: "idle" }, linkedAccounts, { status: "error" });

    expect(markup).toContain('class="settings-check-mail-error" role="alert">No pudimos iniciar la sincronización de correos.</p>');
    expect(markup).toContain('aria-describedby="check-mail-status"');
    expect(markup).not.toContain("disabled");
  });

  it("moves focus through confirm, cancel, error, and success transitions", () => {
    let activeElement = "none";
    const target = (name: string) => ({ focus: () => { activeElement = name; } });
    const triggers = new Map([
      ["account-a", target("trigger-a")],
      ["account-b", target("trigger-b")],
    ]);
    const targets = {
      confirmation: target("confirmation"),
      success: target("success-status"),
      trigger: (accountId: string) => triggers.get(accountId) ?? null,
    };

    focusRemovalTransition(
      { status: "idle" },
      { status: "confirming", accountId: "account-a" },
      targets,
    );
    expect(activeElement).toBe("confirmation");

    focusRemovalTransition(
      { status: "confirming", accountId: "account-a" },
      { status: "idle" },
      targets,
    );
    expect(activeElement).toBe("trigger-a");

    focusRemovalTransition(
      { status: "pending", accountId: "account-b" },
      { status: "error", accountId: "account-b", tag: "secure_store_failed" },
      targets,
    );
    expect(activeElement).toBe("trigger-b");

    focusRemovalTransition(
      { status: "pending", accountId: "account-a" },
      { status: "success" },
      targets,
    );
    expect(activeElement).toBe("success-status");
  });

  it("navigates to settings, focuses its title, and returns home", () => {
    const navigation = createAppNavigationStore();
    const focus = vi.fn();

    navigation.navigate("settings");
    focusViewHeading({ focus });
    navigation.back();

    expect(focus).toHaveBeenCalledTimes(1);
    expect(navigation.getSnapshot()).toEqual({ route: "home" });
  });
});
