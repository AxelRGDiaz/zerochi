import "./App.css";
import { useEffect, type ReactNode } from "react";
import { PopoverFrame } from "./components/PopoverFrame";
import { presentPet } from "./pet/PetEngine";
import { createAddAccountController, useAddAccountController } from "./stores/addAccountController";
import { completeAddAccountAttempt } from "./stores/addAccountFlow";
import { createAppSnapshotHydrator, createAppSnapshotSubscription } from "./stores/appSnapshotHydration";
import { appNavigationStore, useAppNavigation } from "./stores/appNavigationStore";
import { usePopupVisibility } from "./stores/popupVisibilityStore";
import { createRemoveAccountController, useRemoveAccountController } from "./stores/removeAccountController";
import { createOpenAccountController, useOpenAccountController } from "./stores/openAccountController";
import { createCheckMailNowController, useCheckMailNowController } from "./stores/checkMailNowController";
import { inboxUnreadTotal } from "./stores/mailCounters";
import { invokeAddAccount } from "./stores/tauriAddAccount";
import { invokeAppSnapshot, listenAppSnapshot } from "./stores/tauriAppSnapshot";
import { invokeRemoveAccount } from "./stores/tauriRemoveAccount";
import { invokeOpenAccount } from "./stores/tauriOpenAccount";
import { invokeCheckMailNow } from "./stores/tauriCheckMailNow";
import { createSyncStatusStore, createSyncStatusSubscription, useSyncStatuses } from "./stores/syncStatusStore";
import { listenSyncStatus } from "./stores/tauriSyncStatus";
import { ProviderSelectionView } from "./views/ProviderSelectionView";
import { PopupView } from "./views/PopupView";
import { SettingsView } from "./views/SettingsView";

const addAccountController = createAddAccountController(invokeAddAccount);
const appSnapshotHydrator = createAppSnapshotHydrator(invokeAppSnapshot, addAccountController);
const appSnapshotSubscription = createAppSnapshotSubscription(listenAppSnapshot, addAccountController);
const syncStatusStore = createSyncStatusStore();
const syncStatusSubscription = createSyncStatusSubscription(listenSyncStatus, syncStatusStore);
const removeAccountController = createRemoveAccountController(
  invokeRemoveAccount,
  (snapshot, accountId) => addAccountController.applyRemovalSnapshot(snapshot, accountId),
  (accountId) => syncStatusStore.clearAccount(accountId),
);
const openAccountController = createOpenAccountController(invokeOpenAccount);
const checkMailNowController = createCheckMailNowController(invokeCheckMailNow);

function App() {
  useEffect(() => {
    const stopSnapshotListening = appSnapshotSubscription.start(() => appSnapshotHydrator.hydrate());
    const stopSyncStatusListening = syncStatusSubscription.start();
    return () => {
      stopSnapshotListening();
      stopSyncStatusListening();
    };
  }, []);

  const { isActive } = usePopupVisibility();
  const { route } = useAppNavigation();

  useEffect(() => {
    if (route !== "settings") return;
    return () => checkMailNowController.reset();
  }, [route]);

  const { accounts, addAccountState } = useAddAccountController(addAccountController);
  const removeAccountState = useRemoveAccountController(removeAccountController);
  const { byAccountId: openAccountStatuses } = useOpenAccountController(openAccountController);
  const checkMailNowState = useCheckMailNowController(checkMailNowController);
  const { byAccountId: syncStatuses } = useSyncStatuses(syncStatusStore);
  const activeAccounts = accounts.filter((account) => account.active);
  const pendingTotal = activeAccounts.reduce((total, account) => total + account.pendingCount, 0);
  const unreadTotal = inboxUnreadTotal(accounts);
  const presentation = presentPet({
    inboxUnreadTotal: unreadTotal,
    pendingTotal,
    accounts: activeAccounts.map(({ id, pendingCount }) => ({ accountId: id, pendingCount })),
  });

  if (!isActive) return null;

  let content: ReactNode;
  if (route === "add_account") {
    content = (
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
  } else if (route === "settings") {
    content = (
      <SettingsView
        accounts={accounts}
        checkMailNowState={checkMailNowState}
        onBack={appNavigationStore.back}
        onCheckMailNow={() => void checkMailNowController.checkMailNow()}
        onCancelRemove={removeAccountController.cancelRemoval}
        onConfirmRemove={() => void removeAccountController.confirmRemoval()}
        onRequestRemove={removeAccountController.requestRemoval}
        removeAccountState={removeAccountState}
      />
    );
  } else {
    content = (
      <PopupView
        accounts={activeAccounts}
        inboxUnreadTotal={unreadTotal}
        isActive
        onOpenAccount={(accountId) => void openAccountController.openAccount(accountId)}
        onOpenAddAccount={() => appNavigationStore.navigate("add_account")}
        onOpenSettings={() => appNavigationStore.navigate("settings")}
        presentation={presentation}
        openAccountStatuses={openAccountStatuses}
        syncStatuses={syncStatuses}
      />
    );
  }

  return <PopoverFrame>{content}</PopoverFrame>;
}

export default App;
