import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { AddAccountResult } from "../types/contracts";
import {
  accountProviders,
  getProviderAction,
} from "../components/ProviderOptions";
import {
  createAddAccountController,
  type AddAccountPort,
} from "../stores/addAccountController";
import { ProviderSelectionView } from "./ProviderSelectionView";

function renderProviderView(onAddGmail = vi.fn()) {
  return renderToStaticMarkup(
    <ProviderSelectionView
      addAccountState={{ status: "idle" }}
      onAddGmail={onAddGmail}
      onBack={() => undefined}
      onRetry={() => undefined}
    />,
  );
}

describe("ProviderSelectionView", () => {
  it("offers Gmail and Outlook as enabled providers and labels future providers", () => {
    const markup = renderProviderView();

    expect(markup).toContain('<main class="app-shell" aria-labelledby="add-account-title">');
    expect(markup).not.toContain("home-popup");
    expect(accountProviders.filter(({ enabled }) => enabled).map(({ id }) => id)).toEqual(["gmail", "outlook"]);
    expect(markup).toContain('aria-label="Agregar cuenta de Gmail"');
    expect(markup).toContain('aria-label="Agregar cuenta de Outlook"');
    expect(markup).toContain('aria-label="iCloud, Próximamente" disabled=""');
    expect(markup.match(/Próximamente/g)).toHaveLength(2);
  });

  it("does not invoke OAuth merely by entering the provider view", () => {
    const onAddGmail = vi.fn();
    renderProviderView(onAddGmail);
    expect(onAddGmail).not.toHaveBeenCalled();
  });

  it("disables Gmail while an add attempt is pending", () => {
    const markup = renderToStaticMarkup(
      <ProviderSelectionView
        addAccountState={{ status: "pending" }}
        onAddGmail={() => undefined}
        onBack={() => undefined}
        onRetry={() => undefined}
      />,
    );

    expect(markup).toContain('aria-label="Agregar cuenta de Gmail" disabled=""');
    expect(markup).toContain('aria-label="Agregar cuenta de Outlook" disabled=""');
    expect(markup).toContain("Conectando…");
    expect(markup).toContain("Completa el acceso en Google.");
    expect(markup).toContain("Zerochi volverá a mostrarse");
  });

  it("wires each enabled provider to its own action on the duplicate-safe controller", async () => {
    let resolveAttempt!: (result: AddAccountResult) => void;
    const port = vi.fn<AddAccountPort>(() => new Promise((resolve) => {
      resolveAttempt = resolve;
    }));
    const controller = createAddAccountController(port);
    const onAddGmail = () => void controller.addAccount();
    const gmail = accountProviders[0];

    getProviderAction(gmail, onAddGmail)?.();
    getProviderAction(gmail, onAddGmail)?.();
    await Promise.resolve();

    expect(port).toHaveBeenCalledTimes(1);
    const onAddOutlook = vi.fn();
    expect(getProviderAction(accountProviders[1], onAddGmail, onAddOutlook)).toBe(onAddOutlook);
    expect(getProviderAction(accountProviders[1], onAddGmail)).toBeUndefined();
    expect(getProviderAction(accountProviders[2], onAddGmail, onAddOutlook)).toBeUndefined();

    resolveAttempt({
      account: { id: "new", email: "new@example.com", pendingCount: 0, active: true },
    });
    await controller.addAccount();
  });

  it("names Microsoft in the feedback of an Outlook attempt", () => {
    const markup = renderToStaticMarkup(
      <ProviderSelectionView
        addAccountState={{ status: "pending" }}
        addAccountProvider="outlook"
        onAddGmail={() => undefined}
        onBack={() => undefined}
        onRetry={() => undefined}
      />,
    );

    expect(markup).toContain("Completa el acceso en Microsoft.");
    expect(markup).not.toContain("Google");
  });
});
