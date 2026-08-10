import type { AddAccountState } from "../stores/addAccountController";
import { AddAccountStatus } from "../components/AddAccountStatus";
import { ProviderOptions } from "../components/ProviderOptions";
import { SubViewHeader } from "../components/SubViewHeader";

interface ProviderSelectionViewProps {
  addAccountState: AddAccountState;
  onAddGmail: () => void;
  onBack: () => void;
  onRetry: () => void;
}

export function ProviderSelectionView({
  addAccountState,
  onAddGmail,
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
      />
      <AddAccountStatus state={addAccountState} onRetry={onRetry} />
    </main>
  );
}
