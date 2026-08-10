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
  it("offers Gmail as the sole enabled provider and labels future providers", () => {
    const markup = renderProviderView();

    expect(accountProviders.filter(({ enabled }) => enabled).map(({ id }) => id)).toEqual(["gmail"]);
    expect(markup).toContain('aria-label="Agregar cuenta de Gmail"');
    expect(markup).toContain('aria-label="Outlook, Próximamente" disabled=""');
    expect(markup).toContain('aria-label="iCloud, Próximamente" disabled=""');
    expect(markup.match(/Próximamente/g)).toHaveLength(4);
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
    expect(markup).toContain("Conectando…");
  });

  it("wires only Gmail to the existing duplicate-safe controller", async () => {
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
    expect(getProviderAction(accountProviders[1], onAddGmail)).toBeUndefined();
    expect(getProviderAction(accountProviders[2], onAddGmail)).toBeUndefined();

    resolveAttempt({
      account: { id: "new", email: "new@example.com", pendingCount: 0, active: true },
    });
    await controller.addAccount();
  });
});
