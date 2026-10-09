import { renderToStaticMarkup } from "react-dom/server";
import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";
import type { MailAccount } from "../types/mail";
import { AccountList } from "./AccountList";

function accounts(count: number): MailAccount[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `account-${index}`,
    email: `account-${index}@example.com`,
    displayName: `Cuenta ${index + 1}`,
    pendingCount: index,
    inboxUnreadCount: index,
    active: true,
  }));
}

function renderAccounts(items: MailAccount[]) {
  return renderToStaticMarkup(<AccountList accounts={items} onOpenAccount={() => undefined} />);
}

describe("AccountList", () => {
  it("renders an empty, non-focusable list for zero accounts", () => {
    const markup = renderAccounts([]);

    expect(markup).toContain('<ul class="account-list" aria-label="Cuentas conectadas"></ul>');
    expect(markup).not.toContain("tabindex");
    expect(markup).not.toContain("<button");
  });

  it("makes one account reachable through the keyboard-enabled scroll region", () => {
    const markup = renderAccounts(accounts(1));

    expect(markup).toContain('aria-label="Cuentas conectadas" tabindex="0"');
    expect(markup.match(/<button/g)).toHaveLength(1);
    expect(markup).toContain("account-0@example.com");
  });

  it("renders every account in a representative three-account input", () => {
    const representativeAccounts = accounts(3);
    const markup = renderAccounts(representativeAccounts);

    expect(markup.match(/<button/g)).toHaveLength(representativeAccounts.length);
    representativeAccounts.forEach(({ email }) => expect(markup).toContain(email));
  });

  it("keeps every account represented and keyboard-operable in a large list", () => {
    const largeAccountList = accounts(40);
    const markup = renderAccounts(largeAccountList);

    expect(markup.match(/<button/g)).toHaveLength(40);
    expect(markup.match(/type="button"/g)).toHaveLength(40);
    expect(markup).toContain("account-0@example.com");
    expect(markup).toContain("account-39@example.com");
    expect(markup).toContain('tabindex="0"');
  });

  it("renders an accessible action-required indication only on the affected row", () => {
    const markup = renderToStaticMarkup(
      <AccountList
        accounts={accounts(2)}
        onOpenAccount={() => undefined}
        syncStatuses={{ "account-1": { status: "action_required" } }}
      />,
    );

    expect(markup.match(/Requiere atención para sincronizar/g)).toHaveLength(1);
    expect(markup.match(/role="status"/g)).toHaveLength(1);
    expect(markup.indexOf("Requiere atención para sincronizar")).toBeGreaterThan(
      markup.indexOf("account-1@example.com"),
    );
    expect(markup.indexOf("Requiere atención para sincronizar")).toBeLessThan(
      markup.indexOf('aria-label="1 sin leer"'),
    );
  });

  it("renders retrying as a non-alarmist state and clears healthy presentation", () => {
    const retryingMarkup = renderToStaticMarkup(
      <AccountList
        accounts={accounts(1)}
        onOpenAccount={() => undefined}
        syncStatuses={{ "account-0": { status: "retrying", retryAfterSeconds: 30 } }}
      />,
    );
    const healthyMarkup = renderAccounts(accounts(1));

    expect(retryingMarkup).toContain("Reconectando…");
    expect(retryingMarkup).not.toContain('role="alert"');
    expect(healthyMarkup).not.toContain("account-sync-status");
  });

  it("uses Gmail unread for the account badge without substituting Zerochi pending", () => {
    const markup = renderAccounts([{
      ...accounts(1)[0],
      pendingCount: 9,
      inboxUnreadCount: 3,
    }]);

    expect(markup).toContain('aria-label="3 sin leer"');
    expect(markup).toContain('>3</span>');
    expect(markup).not.toContain("9 pendientes");
  });

  it("activates the matching account through the native mouse and keyboard-operable button", () => {
    const opened: string[] = [];
    const list = AccountList({
      accounts: accounts(2),
      onOpenAccount: (accountId) => opened.push(accountId),
    }) as ReactElement<{
      children: ReactElement<{ children: ReactElement<{ onClick: () => void }>[] }>[];
    }>;
    const secondButton = list.props.children[1].props.children[0];

    secondButton.props.onClick();

    expect(opened).toEqual(["account-1"]);
    const markup = renderToStaticMarkup(list);
    expect(markup.match(/type="button"/g)).toHaveLength(2);
    expect(markup).toContain('aria-label="Abrir Gmail de Cuenta 2"');
    expect(markup).not.toContain("tabindex=\"-1\"");
  });

  it("disables only the pending account and shows fixed per-row progress and failure copy", () => {
    const markup = renderToStaticMarkup(
      <AccountList
        accounts={accounts(3)}
        onOpenAccount={() => undefined}
        openAccountStatuses={{ "account-0": "pending", "account-1": "error" }}
      />,
    );

    expect(markup.match(/disabled=""/g)).toHaveLength(1);
    expect(markup.match(/aria-busy="true"/g)).toHaveLength(1);
    expect(markup).toContain("Abriendo Gmail…");
    expect(markup).toContain("No pudimos abrir Gmail. Inténtalo de nuevo.");
    expect(markup.match(/role="alert"/g)).toHaveLength(1);
    expect(markup).not.toContain("provider");
  });
});
