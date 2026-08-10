import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AddAccountState } from "../stores/addAccountController";
import { AddAccountStatus } from "./AddAccountStatus";

function renderStatus(state: AddAccountState) {
  return renderToStaticMarkup(<AddAccountStatus state={state} onRetry={() => undefined} />);
}

describe("AddAccountStatus", () => {
  it.each([
    ["pending", "Abriendo Google para conectar tu cuenta…", "status"],
    ["success", "Cuenta agregada correctamente.", "status"],
    ["cancelled", "Conexión cancelada. No se agregó ninguna cuenta.", "status"],
    ["retryable_error", "No pudimos agregar la cuenta. Puedes intentarlo nuevamente.", "alert"],
    ["action_required", "Se necesita una acción para terminar de agregar la cuenta.", "alert"],
    ["error", "No pudimos agregar la cuenta.", "alert"],
  ] as const)("preserves accessible Spanish feedback for %s", (status, message, role) => {
    const markup = renderStatus({ status });

    expect(markup).toContain(message);
    expect(markup).toContain(`role="${role}"`);
  });

  it("keeps retryable errors assertive and exposes a labeled retry button", () => {
    const markup = renderStatus({ status: "retryable_error" });

    expect(markup).toContain('role="alert"');
    expect(markup).toContain('type="button"');
    expect(markup).toContain('aria-label="Reintentar agregar cuenta"');
  });
});
