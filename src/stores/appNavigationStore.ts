import { useSyncExternalStore } from "react";

export type AppRoute = "home" | "add_account" | "settings";

export interface AppNavigationSnapshot {
  route: AppRoute;
}

type AppNavigationListener = () => void;

export interface AppNavigationStore {
  getSnapshot(): AppNavigationSnapshot;
  subscribe(listener: AppNavigationListener): () => void;
  navigate(route: AppRoute): void;
  back(): void;
}

export function createAppNavigationStore(initialRoute: AppRoute = "home"): AppNavigationStore {
  let snapshot: AppNavigationSnapshot = { route: initialRoute };
  const listeners = new Set<AppNavigationListener>();

  const navigate = (route: AppRoute) => {
    if (snapshot.route === route) return;
    snapshot = { route };
    listeners.forEach((listener) => listener());
  };

  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    navigate,
    back: () => navigate("home"),
  };
}

export const appNavigationStore = createAppNavigationStore();

export function useAppNavigation(): AppNavigationSnapshot {
  return useSyncExternalStore(
    appNavigationStore.subscribe,
    appNavigationStore.getSnapshot,
    appNavigationStore.getSnapshot,
  );
}
