import { useEffect, useRef } from "react";
import { SubViewHeader } from "../components/SubViewHeader";
import type { CheckMailNowState } from "../stores/checkMailNowController";
import type { RemoveAccountState } from "../stores/removeAccountController";
import { normalizeInboxUnreadCount } from "../stores/mailCounters";
import type { RemoveAccountErrorTag } from "../types/contracts";
import type { MailAccount } from "../types/mail";

interface SettingsViewProps {
  accounts: MailAccount[];
  checkMailNowState: CheckMailNowState;
  onBack: () => void;
  onCheckMailNow: () => void;
  onCancelRemove: () => void;
  onConfirmRemove: () => void;
  onRequestRemove: (accountId: string) => void;
  removeAccountState: RemoveAccountState;
}

interface FocusTarget {
  focus(): void;
}

interface RemovalFocusTargets {
  confirmation: FocusTarget | null;
  success: FocusTarget | null;
  trigger(accountId: string): FocusTarget | null;
}

export function focusRemovalTransition(
  previous: RemoveAccountState,
  current: RemoveAccountState,
  targets: RemovalFocusTargets,
): void {
  if (current.status === "confirming") {
    targets.confirmation?.focus();
    return;
  }
  if (current.status === "success") {
    targets.success?.focus();
    return;
  }
  if (current.status === "error") {
    targets.trigger(current.accountId)?.focus();
    return;
  }
  if (current.status === "idle" && previous.status === "confirming") {
    targets.trigger(previous.accountId)?.focus();
  }
}

const removalErrorCopy: Record<RemoveAccountErrorTag, string> = {
  account_not_found: "La cuenta ya no está vinculada.",
  secure_store_failed: "No pudimos borrar las credenciales del llavero. La cuenta sigue vinculada.",
  internal: "No pudimos desvincular la cuenta.",
};

function removalErrorMessage(state: RemoveAccountState): string | null {
  if (state.status !== "error") return null;
  return state.tag ? removalErrorCopy[state.tag] : "No pudimos desvincular la cuenta.";
}

export function SettingsView({
  accounts,
  checkMailNowState,
  onBack,
  onCheckMailNow,
  onCancelRemove,
  onConfirmRemove,
  onRequestRemove,
  removeAccountState,
}: SettingsViewProps) {
  const previousRemovalState = useRef(removeAccountState);
  const confirmationButtonRef = useRef<HTMLButtonElement | null>(null);
  const successStatusRef = useRef<HTMLParagraphElement | null>(null);
  const removeButtonRefs = useRef(new Map<string, HTMLButtonElement>());
  const errorMessage = removalErrorMessage(removeAccountState);
  const removalPending = removeAccountState.status === "pending";
  const checkMailPending = checkMailNowState.status === "pending";

  useEffect(() => {
    focusRemovalTransition(previousRemovalState.current, removeAccountState, {
      confirmation: confirmationButtonRef.current,
      success: successStatusRef.current,
      trigger: (accountId) => removeButtonRefs.current.get(accountId) ?? null,
    });
    previousRemovalState.current = removeAccountState;
  }, [removeAccountState]);

  return (
    <main className="app-shell settings-view" aria-labelledby="settings-title">
      <SubViewHeader onBack={onBack} title="Configuración" titleId="settings-title" />
      <div className="settings-copy">
        <section aria-labelledby="accounts-title">
          <h2 id="accounts-title">Cuentas vinculadas</h2>
          {removeAccountState.status === "success" && (
            <p
              className="settings-removal-success"
              role="status"
              aria-live="polite"
              tabIndex={-1}
              ref={successStatusRef}
            >
              Cuenta desvinculada correctamente.
            </p>
          )}
          {accounts.length === 0 ? (
            <p>No hay cuentas vinculadas.</p>
          ) : (
            <ul className="settings-account-list" aria-label="Cuentas vinculadas">
              {accounts.map((account, index) => {
                const name = account.displayName ?? account.email;
                const inboxUnreadCount = account.active
                  ? normalizeInboxUnreadCount(account.inboxUnreadCount)
                  : null;
                const isConfirming = removeAccountState.status === "confirming"
                  && removeAccountState.accountId === account.id;
                const isPending = removalPending && removeAccountState.accountId === account.id;
                const hasError = removeAccountState.status === "error"
                  && removeAccountState.accountId === account.id;
                const confirmationId = `remove-account-confirmation-${index}`;

                return (
                  <li className="settings-account-row" key={account.id} aria-busy={isPending || undefined}>
                    <div className="settings-account-summary">
                      <span className="settings-account-identity">
                        <strong>{name}</strong>
                        <small>{account.email}</small>
                      </span>
                      <span
                        className="account-count"
                        aria-label={inboxUnreadCount === null
                          ? "Conteo sin leer no disponible para cuenta inactiva"
                          : `${inboxUnreadCount} sin leer`}
                      >
                        {inboxUnreadCount ?? "—"}
                      </span>
                    </div>
                    {isConfirming ? (
                      <div className="settings-remove-confirmation" role="group" aria-labelledby={confirmationId}>
                        <p id={confirmationId}>¿Desvincular esta cuenta de Zerochi?</p>
                        <div className="settings-remove-actions">
                          <button
                            className="settings-remove-confirm"
                            type="button"
                            ref={confirmationButtonRef}
                            onClick={onConfirmRemove}
                          >
                            Desvincular
                          </button>
                          <button className="settings-remove-cancel" type="button" onClick={onCancelRemove}>
                            Cancelar
                          </button>
                        </div>
                      </div>
                    ) : (
                      <button
                        className="settings-remove-button"
                        type="button"
                        ref={(element) => {
                          if (element) removeButtonRefs.current.set(account.id, element);
                          else removeButtonRefs.current.delete(account.id);
                        }}
                        aria-label={`Desvincular ${name}`}
                        disabled={removalPending}
                        onClick={() => onRequestRemove(account.id)}
                      >
                        {isPending ? "Desvinculando…" : "Desvincular"}
                      </button>
                    )}
                    {isPending && (
                      <p className="settings-removal-pending" role="status" aria-live="polite">
                        Desvinculando cuenta…
                      </p>
                    )}
                    {hasError && <p className="settings-removal-error" role="alert">{errorMessage}</p>}
                  </li>
                );
              })}
            </ul>
          )}
        </section>
        <section aria-labelledby="preferences-title">
          <h2 id="preferences-title">Sincronización manual</h2>
          <p>Busca correos nuevos en todas las cuentas activas.</p>
          <button
            className="settings-check-mail-button"
            type="button"
            aria-busy={checkMailPending || undefined}
            disabled={checkMailPending}
            aria-describedby={checkMailNowState.status === "idle" ? undefined : "check-mail-status"}
            onClick={onCheckMailNow}
          >
            {checkMailPending && (
              <span className="settings-check-mail-spinner" aria-hidden="true" />
            )}
            <span className="settings-check-mail-label">
              {checkMailPending ? "Sincronizando correos…" : "Sincronizar correos nuevos"}
            </span>
          </button>
          {checkMailNowState.status === "pending" && (
            <p id="check-mail-status" className="settings-check-mail-status" role="status" aria-live="polite">
              Sincronizando correos…
            </p>
          )}
          {checkMailNowState.status === "success" && (
            <p id="check-mail-status" className="settings-check-mail-status" role="status" aria-live="polite">
              Sincronización iniciada.
            </p>
          )}
          {checkMailNowState.status === "error" && (
            <p id="check-mail-status" className="settings-check-mail-error" role="alert">
              No pudimos iniciar la sincronización de correos.
            </p>
          )}
        </section>
      </div>
    </main>
  );
}
