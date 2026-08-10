import { useSyncExternalStore } from "react";

export interface PopupVisibilitySnapshot {
  isActive: boolean;
}

type PopupVisibilityListener = () => void;

export interface PopupVisibilityStore {
  getSnapshot(): PopupVisibilitySnapshot;
  subscribe(listener: PopupVisibilityListener): () => void;
  setActive(isActive: boolean): void;
  show(): void;
  hide(): void;
}

export function createPopupVisibilityStore(initiallyActive = true): PopupVisibilityStore {
  let snapshot: PopupVisibilitySnapshot = { isActive: initiallyActive };
  const listeners = new Set<PopupVisibilityListener>();

  const setActive = (isActive: boolean) => {
    if (snapshot.isActive === isActive) return;
    snapshot = { isActive };
    listeners.forEach((listener) => listener());
  };

  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    setActive,
    show: () => setActive(true),
    hide: () => setActive(false),
  };
}

export const popupVisibilityStore = createPopupVisibilityStore();

export function usePopupVisibility(): PopupVisibilitySnapshot {
  return useSyncExternalStore(
    popupVisibilityStore.subscribe,
    popupVisibilityStore.getSnapshot,
    popupVisibilityStore.getSnapshot,
  );
}
