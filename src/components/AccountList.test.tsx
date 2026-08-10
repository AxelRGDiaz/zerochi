import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { MailAccount } from "../types/mail";
import { AccountList } from "./AccountList";

function accounts(count: number): MailAccount[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `account-${index}`,
    email: `account-${index}@example.com`,
    displayName: `Cuenta ${index + 1}`,
    pendingCount: index,
    active: true,
  }));
}

function renderAccounts(items: MailAccount[]) {
  return renderToStaticMarkup(<AccountList accounts={items} />);
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
});
