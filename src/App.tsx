import "./App.css";
import { presentPet } from "./pet/PetEngine";
import { createAddAccountController, useAddAccountController } from "./stores/addAccountController";
import { mockAccounts } from "./stores/mockState";
import { usePopupVisibility } from "./stores/popupVisibilityStore";
import { invokeAddAccount } from "./stores/tauriAddAccount";
import { PopupView } from "./views/PopupView";

const addAccountController = createAddAccountController(invokeAddAccount, mockAccounts);

function App() {
  const { isActive } = usePopupVisibility();
  const { accounts, addAccountState } = useAddAccountController(addAccountController);
  const pendingTotal = accounts.reduce((total, account) => total + account.pendingCount, 0);
  const presentation = presentPet({
    pendingTotal,
    accounts: accounts.map(({ id, pendingCount }) => ({ accountId: id, pendingCount })),
  });

  return (
    <PopupView
      accounts={accounts}
      addAccountState={addAccountState}
      isActive={isActive}
      onAddAccount={() => void addAccountController.addAccount()}
      onRetryAddAccount={() => void addAccountController.retry()}
      pendingTotal={pendingTotal}
      presentation={presentation}
    />
  );
}

export default App;
