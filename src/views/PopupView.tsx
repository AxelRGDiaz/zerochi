import type { MailAccount } from "../types/mail";
import type { PetPresentation } from "../pet/PetState";
import { AccountList } from "../components/AccountList";
import { EmptyAccountState } from "../components/EmptyAccountState";
import { PopupActions } from "../components/PopupActions";
import { PopupSummary } from "../components/PopupSummary";
import { ViewHeading } from "../components/ViewHeading";
import { PetRenderer } from "../pet/PetRenderer";
import type { AccountSyncStatus } from "../stores/syncStatusStore";
import type { OpenAccountStatus } from "../stores/openAccountController";

interface PopupViewProps {
  accounts: MailAccount[];
  inboxUnreadTotal: number;
  isActive: boolean;
  onOpenAccount: (accountId: string) => void;
  onOpenAddAccount: () => void;
  onOpenSettings: () => void;
  presentation: PetPresentation;
  openAccountStatuses?: Readonly<Record<string, OpenAccountStatus>>;
  syncStatuses?: Readonly<Record<string, AccountSyncStatus>>;
}

export function PopupView({
  accounts,
  inboxUnreadTotal,
  isActive,
  onOpenAccount,
  onOpenAddAccount,
  onOpenSettings,
  presentation,
  openAccountStatuses,
  syncStatuses,
}: PopupViewProps) {
  if (!isActive) return null;

  return (
    <main className="app-shell home-popup">
      <header className="app-header">
        <p className="eyebrow">TU MASCOTA DE BANDEJA</p>
        <ViewHeading>ZEROCHI</ViewHeading>
      </header>
      <PetRenderer presentation={presentation} visible={isActive} />
      {accounts.length === 0 ? (
        <EmptyAccountState onOpenAddAccount={onOpenAddAccount} />
      ) : (
        <>
          <PopupSummary inboxUnreadTotal={inboxUnreadTotal} message={presentation.message} />
          <AccountList
            accounts={accounts}
            onOpenAccount={onOpenAccount}
            openAccountStatuses={openAccountStatuses}
            syncStatuses={syncStatuses}
          />
        </>
      )}
      <PopupActions onOpenAddAccount={onOpenAddAccount} onOpenSettings={onOpenSettings} />
    </main>
  );
}
