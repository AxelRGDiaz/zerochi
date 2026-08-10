import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AddAccountState } from "../stores/addAccountController";
import { PopupActions } from "./PopupActions";

function renderActions(addAccountState: AddAccountState) {
  return renderToStaticMarkup(
    <PopupActions
      addAccountState={addAccountState}
      onAddAccount={() => undefined}
      onRetryAddAccount={() => undefined}
    />,
  );
}

describe("PopupActions", () => {
  it("disables the add button and announces pending work", () => {
    const markup = renderActions({ status: "pending" });

    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('disabled=""');
    expect(markup).toContain("Agregando cuenta…");
    expect(markup).toContain('role="status"');
    expect(markup).toContain("Abriendo Google para conectar tu cuenta…");
  });

  it.each([
    ["success", "Cuenta agregada correctamente.", "status"],
    ["cancelled", "Conexión cancelada. No se agregó ninguna cuenta.", "status"],
    ["retryable_error", "No pudimos agregar la cuenta. Puedes intentarlo nuevamente.", "alert"],
    ["action_required", "Se necesita una acción para terminar de agregar la cuenta.", "alert"],
    ["error", "No pudimos agregar la cuenta.", "alert"],
  ] as const)("renders the %s state in Spanish", (status, message, role) => {
    const markup = renderActions({ status });

    expect(markup).toContain(message);
    expect(markup).toContain(`role="${role}"`);
  });

  it("uses native keyboard-operable buttons with accessible labels for add, retry, and settings", () => {
    const markup = renderActions({ status: "retryable_error" });

    expect(markup.match(/<button/g)).toHaveLength(3);
    expect(markup.match(/type="button"/g)).toHaveLength(3);
    expect(markup).toContain('aria-label="Agregar cuenta"');
    expect(markup).toContain('aria-label="Reintentar agregar cuenta"');
    expect(markup).toContain('aria-label="Abrir configuración"');
  });
});
