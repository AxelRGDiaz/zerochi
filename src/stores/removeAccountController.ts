import { useSyncExternalStore } from "react";
import type {
  AppSnapshot,
  CommandError,
  RemoveAccountErrorTag,
  RemoveAccountResult,
} from "../types/contracts";

export type RemoveAccountPort = (accountId: string) => Promise<RemoveAccountResult>;

export type RemoveAccountState =
  | { status: "idle" }
  | { status: "confirming"; accountId: string }
  | { status: "pending"; accountId: string }
  | { status: "success" }
  | { status: "error"; accountId: string; tag?: RemoveAccountErrorTag };

type RemoveAccountListener = () => void;

export interface RemoveAccountController {
  getSnapshot(): RemoveAccountState;
  subscribe(listener: RemoveAccountListener): () => void;
  requestRemoval(accountId: string): void;
  cancelRemoval(): void;
  confirmRemoval(): Promise<void>;
}

const approvedErrorTags = new Set<RemoveAccountErrorTag>([
  "account_not_found",
  "internal",
  "secure_store_failed",
]);

function isRemoveAccountError(error: unknown): error is CommandError<RemoveAccountErrorTag> {
  if (typeof error !== "object" || error === null) return false;
  const candidate = error as Partial<CommandError<RemoveAccountErrorTag>>;
  return typeof candidate.tag === "string"
    && approvedErrorTags.has(candidate.tag as RemoveAccountErrorTag)
    && typeof candidate.message === "string"
    && typeof candidate.retryable === "boolean"
    && typeof candidate.actionRequired === "boolean";
}

function isOptionalString(value: unknown): boolean {
  return value === undefined || typeof value === "string";
}

function isNonNegativeInteger(value: unknown): boolean {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isMailAccount(value: unknown): boolean {
  if (typeof value !== "object" || value === null) return false;
  const account = value as Record<string, unknown>;
  return typeof account.id === "string"
    && typeof account.email === "string"
    && isOptionalString(account.displayName)
    && isNonNegativeInteger(account.pendingCount)
    && (account.inboxUnreadCount === undefined || isNonNegativeInteger(account.inboxUnreadCount))
    && typeof account.active === "boolean"
    && isOptionalString(account.lastHistoryId)
    && isOptionalString(account.lastSyncAt);
}

function isMailMessage(value: unknown): boolean {
  if (typeof value !== "object" || value === null) return false;
  const message = value as Record<string, unknown>;
  return typeof message.id === "string"
    && typeof message.threadId === "string"
    && typeof message.accountId === "string"
    && typeof message.sender === "string"
    && typeof message.subject === "string"
    && typeof message.receivedAt === "string"
    && typeof message.detectedAt === "string"
    && isOptionalString(message.openedFromZerochiAt);
}

function validRemovalSnapshot(result: unknown, removedAccountId: string): AppSnapshot | null {
  if (typeof result !== "object" || result === null) return null;
  const snapshot = (result as { snapshot?: unknown }).snapshot;
  if (typeof snapshot !== "object" || snapshot === null) return null;
  const candidate = snapshot as { accounts?: unknown; messages?: unknown };
  if (!Array.isArray(candidate.accounts) || !candidate.accounts.every(isMailAccount)) return null;
  if (!Array.isArray(candidate.messages) || !candidate.messages.every(isMailMessage)) return null;
  if (candidate.accounts.some((account) => account.id === removedAccountId)) return null;
  if (candidate.messages.some((message) => message.accountId === removedAccountId)) return null;
  return candidate as AppSnapshot;
}

export function createRemoveAccountController(
  removeAccount: RemoveAccountPort,
  applyRemovalSnapshot: (snapshot: AppSnapshot, removedAccountId: string) => void,
  clearSyncStatus: (accountId: string) => void,
): RemoveAccountController {
  let state: RemoveAccountState = { status: "idle" };
  let inFlight: Promise<void> | null = null;
  const listeners = new Set<RemoveAccountListener>();

  const update = (next: RemoveAccountState) => {
    state = next;
    listeners.forEach((listener) => listener());
  };

  return {
    getSnapshot: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    requestRemoval(accountId) {
      if (state.status === "pending") return;
      update({ status: "confirming", accountId });
    },
    cancelRemoval() {
      if (state.status === "confirming") update({ status: "idle" });
    },
    confirmRemoval() {
      if (inFlight) return inFlight;
      if (state.status !== "confirming") return Promise.resolve();
      const { accountId } = state;
      update({ status: "pending", accountId });

      const attempt = Promise.resolve()
        .then(() => removeAccount(accountId))
        .then((result) => {
          const snapshot = validRemovalSnapshot(result, accountId);
          if (!snapshot) throw new Error("invalid remove_account result");
          applyRemovalSnapshot(snapshot, accountId);
          clearSyncStatus(accountId);
          update({ status: "success" });
        })
        .catch((error: unknown) => {
          update({
            status: "error",
            accountId,
            ...(isRemoveAccountError(error) ? { tag: error.tag } : {}),
          });
        })
        .finally(() => {
          if (inFlight === attempt) inFlight = null;
        });

      inFlight = attempt;
      return attempt;
    },
  };
}

export function useRemoveAccountController(controller: RemoveAccountController): RemoveAccountState {
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
