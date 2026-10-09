import type { MailAccount } from "../types/mail";
import type { AccountSyncStatus } from "../stores/syncStatusStore";
import { normalizeInboxUnreadCount } from "../stores/mailCounters";
import type { OpenAccountStatus } from "../stores/openAccountController";

interface AccountListProps {
  accounts: MailAccount[];
  onOpenAccount: (accountId: string) => void;
  openAccountStatuses?: Readonly<Record<string, OpenAccountStatus>>;
  syncStatuses?: Readonly<Record<string, AccountSyncStatus>>;
}

export function AccountList({
  accounts,
  onOpenAccount,
  openAccountStatuses = {},
  syncStatuses = {},
}: AccountListProps) {
  return (
    <ul className="account-list" aria-label="Cuentas conectadas" tabIndex={accounts.length > 0 ? 0 : undefined}>
      {accounts.map((account) => {
        const syncStatus = syncStatuses[account.id];
        const openStatus = openAccountStatuses[account.id];
        const opening = openStatus === "pending";
        const inboxUnreadCount = normalizeInboxUnreadCount(account.inboxUnreadCount);
        return (
          <li key={account.id} aria-busy={opening || undefined}>
            <button
              className="account-row"
              type="button"
              aria-label={`Abrir Gmail de ${account.displayName ?? account.email}`}
              disabled={opening}
              onClick={() => onOpenAccount(account.id)}
            >
              <span className="account-dot" aria-hidden="true" />
              <span className="account-name">
                {account.displayName ?? account.email}
                <small>{account.email}</small>
                {syncStatus?.status === "action_required" && (
                  <small className="account-sync-status account-sync-action-required" role="status">
                    Requiere atención para sincronizar
                  </small>
                )}
                {syncStatus?.status === "retrying" && (
                  <small className="account-sync-status account-sync-retrying">
                    Reconectando…
                  </small>
                )}
              </span>
              <span className="account-count" aria-label={`${inboxUnreadCount} sin leer`}>
                {inboxUnreadCount}
              </span>
            </button>
            {opening && (
              <span className="account-open-status" role="status" aria-live="polite">
                Abriendo Gmail…
              </span>
            )}
            {openStatus === "error" && (
              <span className="account-open-error" role="alert">
                No pudimos abrir Gmail. Inténtalo de nuevo.
              </span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
