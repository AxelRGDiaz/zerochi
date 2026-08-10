import { useSyncExternalStore } from "react";
import type {
  AddAccountErrorTag,
  AddAccountResult,
  CommandError,
} from "../types/contracts";
import type { MailAccount } from "../types/mail";

export type AddAccountPort = () => Promise<AddAccountResult>;

export type AddAccountState =
  | { status: "idle" }
  | { status: "pending" }
  | { status: "success" }
  | { status: "cancelled" }
  | { status: "retryable_error" }
  | { status: "action_required" }
  | { status: "error" };

export interface AddAccountSnapshot {
  accounts: MailAccount[];
  addAccountState: AddAccountState;
}

type AddAccountListener = () => void;

export interface AddAccountController {
  getSnapshot(): AddAccountSnapshot;
  subscribe(listener: AddAccountListener): () => void;
  addAccount(): Promise<void>;
  retry(): Promise<void>;
}

function normalizedAccount(account: MailAccount): MailAccount {
  const normalized: MailAccount = {
    id: account.id,
    email: account.email,
    pendingCount: account.pendingCount,
    active: account.active,
  };

  if (account.displayName !== undefined) normalized.displayName = account.displayName;
  if (account.lastHistoryId !== undefined) normalized.lastHistoryId = account.lastHistoryId;
  if (account.lastSyncAt !== undefined) normalized.lastSyncAt = account.lastSyncAt;
  return normalized;
}

function isCommandError(error: unknown): error is CommandError<AddAccountErrorTag> {
  if (typeof error !== "object" || error === null) return false;
  const candidate = error as Partial<CommandError<AddAccountErrorTag>>;
  return typeof candidate.tag === "string"
    && typeof candidate.retryable === "boolean"
    && typeof candidate.actionRequired === "boolean";
}

export function classifyAddAccountError(error: unknown): AddAccountState {
  if (!isCommandError(error)) return { status: "error" };
  if (error.tag === "cancelled") return { status: "cancelled" };
  if (error.actionRequired) return { status: "action_required" };
  if (error.retryable) return { status: "retryable_error" };
  return { status: "error" };
}

function upsertAccount(accounts: MailAccount[], account: MailAccount): MailAccount[] {
  const index = accounts.findIndex(({ id }) => id === account.id);
  if (index === -1) return [...accounts, account];
  return accounts.map((current, currentIndex) => currentIndex === index ? account : current);
}

export function createAddAccountController(
  addAccountPort: AddAccountPort,
  initialAccounts: MailAccount[] = [],
): AddAccountController {
  let snapshot: AddAccountSnapshot = {
    accounts: initialAccounts.map(normalizedAccount),
    addAccountState: { status: "idle" },
  };
  let inFlight: Promise<void> | null = null;
  const listeners = new Set<AddAccountListener>();

  const update = (next: AddAccountSnapshot) => {
    snapshot = next;
    listeners.forEach((listener) => listener());
  };

  const addAccount = (): Promise<void> => {
    if (inFlight) return inFlight;
    update({ ...snapshot, addAccountState: { status: "pending" } });

    const attempt = Promise.resolve()
      .then(() => addAccountPort())
      .then(({ account }) => {
        const normalized = normalizedAccount(account);
        update({
          accounts: upsertAccount(snapshot.accounts, normalized),
          addAccountState: { status: "success" },
        });
      })
      .catch((error: unknown) => {
        update({ ...snapshot, addAccountState: classifyAddAccountError(error) });
      })
      .finally(() => {
        if (inFlight === attempt) inFlight = null;
      });

    inFlight = attempt;
    return attempt;
  };

  return {
    getSnapshot: () => snapshot,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    addAccount,
    retry() {
      if (snapshot.addAccountState.status !== "retryable_error") return Promise.resolve();
      return addAccount();
    },
  };
}

export function useAddAccountController(controller: AddAccountController): AddAccountSnapshot {
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
