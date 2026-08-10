import "./App.css";
import { presentPet } from "./pet/PetEngine";
import { createAddAccountController, useAddAccountController } from "./stores/addAccountController";
import { completeAddAccountAttempt } from "./stores/addAccountFlow";
import { appNavigationStore, useAppNavigation } from "./stores/appNavigationStore";
import { usePopupVisibility } from "./stores/popupVisibilityStore";
import { invokeAddAccount } from "./stores/tauriAddAccount";
import { ProviderSelectionView } from "./views/ProviderSelectionView";
import { PopupView } from "./views/PopupView";
import { SettingsView } from "./views/SettingsView";

const addAccountController = createAddAccountController(invokeAddAccount);

function App() {
  const { isActive } = usePopupVisibility();
  const { route } = useAppNavigation();
  const { accounts, addAccountState } = useAddAccountController(addAccountController);
  const pendingTotal = accounts.reduce((total, account) => total + account.pendingCount, 0);
  const presentation = presentPet({
    pendingTotal,
    accounts: accounts.map(({ id, pendingCount }) => ({ accountId: id, pendingCount })),
  });

  if (!isActive) return null;

  if (route === "add_account") {
    return (
      <ProviderSelectionView
        addAccountState={addAccountState}
        onAddGmail={() => void completeAddAccountAttempt(
          addAccountController,
          addAccountController.addAccount,
          appNavigationStore.back,
        )}
        onBack={appNavigationStore.back}
        onRetry={() => void completeAddAccountAttempt(
          addAccountController,
          addAccountController.retry,
          appNavigationStore.back,
        )}
      />
    );
  }

  if (route === "settings") {
    return <SettingsView onBack={appNavigationStore.back} />;
  }

  return (
    <PopupView
      accounts={accounts}
      isActive
      onOpenAddAccount={() => appNavigationStore.navigate("add_account")}
      onOpenSettings={() => appNavigationStore.navigate("settings")}
      pendingTotal={pendingTotal}
      presentation={presentation}
    />
  );
}

export default App;
