import type { MailAccount } from "../types/mail";
import type { PetPresentation } from "../pet/PetState";
import { AccountList } from "../components/AccountList";
import { PopupActions } from "../components/PopupActions";
import { PopupSummary } from "../components/PopupSummary";
import { PetRenderer } from "../pet/PetRenderer";

interface PopupViewProps {
  accounts: MailAccount[];
  isActive: boolean;
  pendingTotal: number;
  presentation: PetPresentation;
}

export function PopupView({ accounts, isActive, pendingTotal, presentation }: PopupViewProps) {
  if (!isActive) return null;

  return (
    <main className="app-shell">
      <header className="app-header">
        <p className="eyebrow">YOUR INBOX PET</p>
        <h1>ZEROCHI</h1>
      </header>
      <PetRenderer presentation={presentation} visible={isActive} />
      <PopupSummary message={presentation.message} pendingTotal={pendingTotal} />
      <AccountList accounts={accounts} />
      <PopupActions />
    </main>
  );
}
