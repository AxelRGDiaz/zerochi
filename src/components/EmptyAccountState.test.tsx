import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { createAddAccountController, type AddAccountPort } from "../stores/addAccountController";
import { createAppNavigationStore } from "../stores/appNavigationStore";
import { EmptyAccountState } from "./EmptyAccountState";

describe("EmptyAccountState", () => {
  it("renders friendly accessible onboarding without fake account data", () => {
    const markup = renderToStaticMarkup(<EmptyAccountState onOpenAddAccount={() => undefined} />);

    expect(markup).toContain('aria-labelledby="empty-account-title"');
    expect(markup).toContain("Todavía no hay una cuenta conectada");
    expect(markup).toContain("Agrega tu cuenta de Gmail");
    expect(markup).toContain('<button class="empty-account-action" type="button">Elegir proveedor</button>');
    expect(markup).not.toContain("@");
  });

  it("opens provider selection without invoking add_account", () => {
    const port = vi.fn<AddAccountPort>();
    createAddAccountController(port);
    const navigation = createAppNavigationStore();
    const openProviderSelection = () => navigation.navigate("add_account");

    renderToStaticMarkup(<EmptyAccountState onOpenAddAccount={openProviderSelection} />);
    expect(port).not.toHaveBeenCalled();

    openProviderSelection();
    expect(navigation.getSnapshot()).toEqual({ route: "add_account" });
    expect(port).not.toHaveBeenCalled();
  });
});
