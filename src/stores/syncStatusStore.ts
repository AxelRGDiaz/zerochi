import { useSyncExternalStore } from "react";
import type { SyncStatusChanged } from "../types/contracts";

export type AccountSyncStatus = Omit<SyncStatusChanged, "accountId">;

export interface SyncStatusSnapshot {
  byAccountId: Readonly<Record<string, AccountSyncStatus>>;
}

type SyncStatusListener = () => void;

export interface SyncStatusStore {
  getSnapshot(): SyncStatusSnapshot;
  subscribe(listener: SyncStatusListener): () => void;
  applyStatus(event: SyncStatusChanged): void;
  clearAccount(accountId: string): void;
}

export type SyncStatusEventListener = (event: SyncStatusChanged) => void;
export type SyncStatusListenPort = (listener: SyncStatusEventListener) => Promise<() => void>;

export interface SyncStatusSubscription {
  start(): () => void;
}

export function createSyncStatusStore(): SyncStatusStore {
  let snapshot: SyncStatusSnapshot = { byAccountId: {} };
  const listeners = new Set<SyncStatusListener>();

  const update = (byAccountId: Record<string, AccountSyncStatus>) => {
    snapshot = { byAccountId };
    listeners.forEach((listener) => listener());
  };

  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    applyStatus(event) {
      if (event.status === "healthy") {
        if (!(event.accountId in snapshot.byAccountId)) return;
        const { [event.accountId]: _cleared, ...remaining } = snapshot.byAccountId;
        update(remaining);
        return;
      }

      const status: AccountSyncStatus = event.status === "retrying"
        ? { status: "retrying", retryAfterSeconds: event.retryAfterSeconds }
        : { status: "action_required" };
      update({ ...snapshot.byAccountId, [event.accountId]: status });
    },
    clearAccount(accountId) {
      if (!(accountId in snapshot.byAccountId)) return;
      const { [accountId]: _cleared, ...remaining } = snapshot.byAccountId;
      update(remaining);
    },
  };
}

export function createSyncStatusSubscription(
  listenSyncStatus: SyncStatusListenPort,
  store: SyncStatusStore,
): SyncStatusSubscription {
  return {
    start() {
      let disposed = false;
      let unlisten: (() => void) | null = null;

      try {
        void listenSyncStatus((event) => {
          if (!disposed) store.applyStatus(event);
        })
          .then((stopListening) => {
            if (disposed) stopListening();
            else unlisten = stopListening;
          })
          .catch(() => undefined);
      } catch {
        // A failed native listener leaves the current per-account statuses untouched.
      }

      return () => {
        disposed = true;
        unlisten?.();
        unlisten = null;
      };
    },
  };
}

export function useSyncStatuses(store: SyncStatusStore): SyncStatusSnapshot {
  return useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
}
