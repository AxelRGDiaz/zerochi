import type { AddAccountState } from "../stores/addAccountController";
import type { MailProviderId } from "../types/mail";
import { AddAccountStatus } from "../components/AddAccountStatus";
import { ProviderOptions } from "../components/ProviderOptions";
import { SubViewHeader } from "../components/SubViewHeader";

interface ProviderSelectionViewProps {
  addAccountState: AddAccountState;
  addAccountProvider?: MailProviderId;
  onAddGmail: () => void;
  onAddOutlook?: () => void;
  onBack: () => void;
  onRetry: () => void;
}

export function ProviderSelectionView({
  addAccountState,
  addAccountProvider,
  onAddGmail,
  onAddOutlook,
  onBack,
  onRetry,
}: ProviderSelectionViewProps) {
  return (
    <main className="app-shell" aria-labelledby="add-account-title">
      <SubViewHeader onBack={onBack} title="Agregar cuenta" titleId="add-account-title" />
      <p className="subview-intro">Elige el proveedor de la cuenta que quieres conectar.</p>
      <ProviderOptions
        gmailPending={addAccountState.status === "pending"}
        onAddGmail={onAddGmail}
        onAddOutlook={onAddOutlook}
      />
      <AddAccountStatus state={addAccountState} provider={addAccountProvider} onRetry={onRetry} />
    </main>
  );
}
