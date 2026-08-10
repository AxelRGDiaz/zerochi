import "./App.css";
import { presentPet } from "./pet/PetEngine";
import { mockAccounts } from "./stores/mockState";
import { usePopupVisibility } from "./stores/popupVisibilityStore";
import { PopupView } from "./views/PopupView";

function App() {
  const { isActive } = usePopupVisibility();
  const pendingTotal = mockAccounts.reduce((total, account) => total + account.pendingCount, 0);
  const presentation = presentPet({
    pendingTotal,
    accounts: mockAccounts.map(({ id, pendingCount }) => ({ accountId: id, pendingCount })),
  });

  return (
    <PopupView
      accounts={mockAccounts}
      isActive={isActive}
      pendingTotal={pendingTotal}
      presentation={presentation}
    />
  );
}

export default App;
