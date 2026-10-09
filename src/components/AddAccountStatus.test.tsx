import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  classifyAddAccountError,
  type AddAccountState,
} from "../stores/addAccountController";
import type { AddAccountErrorTag, CommandError } from "../types/contracts";
import { AddAccountStatus } from "./AddAccountStatus";

function renderStatus(state: AddAccountState) {
  return renderToStaticMarkup(<AddAccountStatus state={state} onRetry={() => undefined} />);
}

describe("AddAccountStatus", () => {
  it.each([
    ["pending", "Completa el acceso en Google.", "status"],
    ["success", "Cuenta agregada correctamente.", "status"],
    ["error", "No pudimos agregar la cuenta.", "alert"],
  ] as const)("preserves accessible Spanish feedback for %s", (status, message, role) => {
    const markup = renderStatus({ status } as AddAccountState);

    expect(markup).toContain(message);
    expect(markup).toContain(`role="${role}"`);
  });

  it("keeps retryable errors assertive and exposes a labeled retry button", () => {
    const markup = renderStatus({
      status: "retryable_error",
      tag: "timeout",
      retryable: true,
      actionRequired: false,
    });

    expect(markup).toContain('role="alert"');
    expect(markup).toContain('aria-live="assertive"');
    expect(markup).toContain('type="button"');
    expect(markup).toContain('aria-label="Reintentar agregar cuenta"');
  });

  it.each([
    ["auth_in_progress", "Ya hay una conexión con Google en curso."],
    ["browser_open_failed", "No pudimos abrir Google en tu navegador."],
    ["callback_invalid", "No pudimos validar la respuesta de Google."],
    ["cancelled", "Conexión cancelada."],
    ["config_invalid", "Zerochi no está configurado para conectarse con Google."],
    ["identity_invalid", "No pudimos verificar la identidad de la cuenta."],
    ["internal", "Zerochi no pudo completar la conexión."],
    ["network_unavailable", "No pudimos comunicarnos con Google."],
    ["provider_denied", "Google no autorizó el acceso."],
    ["refresh_token_missing", "Google no concedió acceso continuo a la cuenta."],
    ["secure_store_failed", "No pudimos guardar la cuenta de forma segura."],
    ["timeout", "Se agotó el tiempo para completar el acceso."],
    ["token_exchange_failed", "No pudimos finalizar el acceso con Google."],
    ["workspace_restricted", "Tu organización de Google bloqueó el acceso."],
  ] satisfies ReadonlyArray<readonly [AddAccountErrorTag, string]>) (
    "maps the approved %s tag to useful sanitized copy",
    (tag, message) => {
      const state = classifyAddAccountError({
        tag,
        message: "provider diagnostic that must stay hidden",
        retryable: false,
        actionRequired: tag === "workspace_restricted",
      } satisfies CommandError<AddAccountErrorTag>);
      const markup = renderStatus(state);

      expect(markup).toContain(message);
      expect(markup).not.toContain("provider diagnostic");
      expect(markup).not.toContain("Reintentar");
    },
  );

  it("shows retry only when the sanitized contract marks the failure retryable", () => {
    const retryable = classifyAddAccountError({
      tag: "network_unavailable",
      message: "hidden",
      retryable: true,
      actionRequired: false,
    });
    const terminal = classifyAddAccountError({
      tag: "network_unavailable",
      message: "hidden",
      retryable: false,
      actionRequired: false,
    });

    expect(renderStatus(retryable)).toContain("Reintentar");
    expect(renderStatus(terminal)).not.toContain("Reintentar");
  });

  it("never renders raw provider details or secret-shaped strings", () => {
    const state = classifyAddAccountError({
      tag: "token_exchange_failed",
      message: "access_token=secret authorization_code=private user@example.com",
      retryable: true,
      actionRequired: false,
    });
    const markup = renderStatus(state);

    expect(markup).toContain("Cierra la pestaña anterior y vuelve a iniciar la conexión desde Zerochi.");
    expect(markup).toContain('aria-label="Reintentar agregar cuenta"');
    expect(markup).not.toMatch(/access_token|authorization_code|secret|private|user@example\.com/i);
  });

  it("explains the browser handoff while Google consent is pending", () => {
    const markup = renderStatus({ status: "pending" });

    expect(markup).toContain("Completa el acceso en Google.");
    expect(markup).toContain("Cuando termines, Zerochi volverá a mostrarse y agregará tu cuenta.");
    expect(markup).toContain('role="status"');
    expect(markup).toContain('aria-live="polite"');
  });
});
