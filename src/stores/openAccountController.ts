import { useSyncExternalStore } from "react";
import type { AccountId } from "../types/mail";

export type OpenAccountPort = (accountId: AccountId) => Promise<void>;
export type OpenAccountStatus = "pending" | "error";

export interface OpenAccountSnapshot {
  byAccountId: Readonly<Record<AccountId, OpenAccountStatus>>;
}

type OpenAccountListener = () => void;

export interface OpenAccountController {
  getSnapshot(): OpenAccountSnapshot;
  subscribe(listener: OpenAccountListener): () => void;
  openAccount(accountId: AccountId): Promise<void>;
}

export function createOpenAccountController(openAccount: OpenAccountPort): OpenAccountController {
  let snapshot: OpenAccountSnapshot = { byAccountId: {} };
  const listeners = new Set<OpenAccountListener>();
  const inFlightByAccountId = new Map<AccountId, Promise<void>>();

  const updateAccount = (accountId: AccountId, status?: OpenAccountStatus) => {
    const next = { ...snapshot.byAccountId };
    if (status === undefined) delete next[accountId];
    else next[accountId] = status;
    snapshot = { byAccountId: next };
    listeners.forEach((listener) => listener());
  };

  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    openAccount(accountId) {
      const activeAttempt = inFlightByAccountId.get(accountId);
      if (activeAttempt) return activeAttempt;

      updateAccount(accountId, "pending");
      const attempt = Promise.resolve()
        .then(() => openAccount(accountId))
        .then(() => updateAccount(accountId))
        .catch(() => updateAccount(accountId, "error"))
        .finally(() => {
          if (inFlightByAccountId.get(accountId) === attempt) {
            inFlightByAccountId.delete(accountId);
          }
        });

      inFlightByAccountId.set(accountId, attempt);
      return attempt;
    },
  };
}

export function useOpenAccountController(controller: OpenAccountController): OpenAccountSnapshot {
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
