import type { MailAccount } from "../types/mail";

export function AccountList({ accounts }: { accounts: MailAccount[] }) {
  return (
    <ul className="account-list" aria-label="Cuentas conectadas">
      {accounts.map((account) => (
        <li key={account.id}>
          <button className="account-row" type="button">
            <span className="account-dot" aria-hidden="true" />
            <span className="account-name">
              {account.displayName ?? account.email}
              <small>{account.email}</small>
            </span>
            <span className="account-count" aria-label={`${account.pendingCount} pendientes`}>
              {account.pendingCount}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}
