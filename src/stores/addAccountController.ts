import { useSyncExternalStore } from "react";
import type {
  AddAccountErrorTag,
  AddAccountResult,
  AppSnapshot,
  CommandError,
} from "../types/contracts";
import type { MailAccount, MailMessage, MailProviderId } from "../types/mail";
import { normalizeInboxUnreadCount } from "./mailCounters";

export type AddAccountPort = (provider: MailProviderId) => Promise<AddAccountResult>;

interface TaggedAddAccountFailure {
  tag: AddAccountErrorTag;
  retryable: boolean;
  actionRequired: boolean;
}

export type AddAccountState =
  | { status: "idle" }
  | { status: "pending" }
  | { status: "success" }
  | ({ status: "cancelled" } & TaggedAddAccountFailure)
  | ({ status: "retryable_error" } & TaggedAddAccountFailure)
  | ({ status: "action_required" } & TaggedAddAccountFailure)
  | ({ status: "error" } & TaggedAddAccountFailure)
  | { status: "error" };

export interface AddAccountSnapshot {
  accounts: MailAccount[];
  messages: MailMessage[];
  addAccountState: AddAccountState;
  /** Provider of the latest attempt, so feedback and retry address the same provider. */
  addAccountProvider: MailProviderId;
}

type AddAccountListener = () => void;

export interface AddAccountController {
  getSnapshot(): AddAccountSnapshot;
  getLiveSnapshotRevision(): number;
  subscribe(listener: AddAccountListener): () => void;
  hydrateSnapshot(snapshot: AppSnapshot, expectedLiveRevision?: number): void;
  applySnapshot(snapshot: AppSnapshot): void;
  applyRemovalSnapshot(snapshot: AppSnapshot, removedAccountId: string): void;
  addAccount(provider?: MailProviderId): Promise<void>;
  retry(): Promise<void>;
}

function normalizedAccount(account: MailAccount): MailAccount {
  const normalized: MailAccount = {
    id: account.id,
    email: account.email,
    pendingCount: account.pendingCount,
    active: account.active,
  };

  if (account.provider === "outlook") normalized.provider = "outlook";
  if (account.displayName !== undefined) normalized.displayName = account.displayName;
  if (account.inboxUnreadCount !== undefined) {
    normalized.inboxUnreadCount = normalizeInboxUnreadCount(account.inboxUnreadCount);
  }
  if (account.lastHistoryId !== undefined) normalized.lastHistoryId = account.lastHistoryId;
  if (account.lastSyncAt !== undefined) normalized.lastSyncAt = account.lastSyncAt;
  return normalized;
}

function normalizedMessage(message: MailMessage): MailMessage {
  const normalized: MailMessage = {
    id: message.id,
    threadId: message.threadId,
    accountId: message.accountId,
    sender: message.sender,
    subject: message.subject,
    receivedAt: message.receivedAt,
    detectedAt: message.detectedAt,
  };

  if (message.openedFromZerochiAt !== undefined) {
    normalized.openedFromZerochiAt = message.openedFromZerochiAt;
  }
  return normalized;
}

const approvedAddAccountErrorTags = new Set<AddAccountErrorTag>([
  "auth_in_progress",
  "browser_open_failed",
  "callback_invalid",
  "cancelled",
  "config_invalid",
  "identity_invalid",
  "internal",
  "network_unavailable",
  "provider_denied",
  "refresh_token_missing",
  "secure_store_failed",
  "timeout",
  "token_exchange_failed",
  "workspace_restricted",
]);

function isAddAccountErrorTag(value: unknown): value is AddAccountErrorTag {
  return typeof value === "string" && approvedAddAccountErrorTags.has(value as AddAccountErrorTag);
}

function isCommandError(error: unknown): error is CommandError<AddAccountErrorTag> {
  if (typeof error !== "object" || error === null) return false;
  const candidate = error as Partial<CommandError<AddAccountErrorTag>>;
  return isAddAccountErrorTag(candidate.tag)
    && typeof candidate.retryable === "boolean"
    && typeof candidate.actionRequired === "boolean";
}

export function classifyAddAccountError(error: unknown): AddAccountState {
  if (!isCommandError(error)) return { status: "error" };
  const details: TaggedAddAccountFailure = {
    tag: error.tag,
    retryable: error.retryable,
    actionRequired: error.actionRequired,
  };
  if (error.tag === "cancelled") return { status: "cancelled", ...details };
  if (error.actionRequired) return { status: "action_required", ...details };
  if (error.retryable) return { status: "retryable_error", ...details };
  return { status: "error", ...details };
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
    messages: [],
    addAccountState: { status: "idle" },
    addAccountProvider: "gmail",
  };
  let inFlight: Promise<void> | null = null;
  let liveSnapshotRevision = 0;
  const immediatelyAddedAccountIds = new Set<string>();
  const liveAcknowledgedAccountIds = new Set<string>();
  const listeners = new Set<AddAccountListener>();

  const update = (next: AddAccountSnapshot) => {
    snapshot = next;
    listeners.forEach((listener) => listener());
  };

  const addAccount = (provider: MailProviderId = "gmail"): Promise<void> => {
    if (inFlight) return inFlight;
    update({ ...snapshot, addAccountState: { status: "pending" }, addAccountProvider: provider });

    const attempt = Promise.resolve()
      .then(() => addAccountPort(provider))
      .then(({ account }) => {
        const normalized = normalizedAccount(account);
        const acknowledgedByLiveSnapshot = liveAcknowledgedAccountIds.has(normalized.id);
        if (!acknowledgedByLiveSnapshot) immediatelyAddedAccountIds.add(normalized.id);
        update({
          ...snapshot,
          accounts: acknowledgedByLiveSnapshot
            ? snapshot.accounts
            : upsertAccount(snapshot.accounts, normalized),
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

  const applyLiveSnapshot = (
    appSnapshot: AppSnapshot,
    authoritativelyRemovedAccountId?: string,
  ) => {
    liveSnapshotRevision += 1;
    if (authoritativelyRemovedAccountId !== undefined) {
      immediatelyAddedAccountIds.delete(authoritativelyRemovedAccountId);
    }
    const liveAccounts = appSnapshot.accounts.map(normalizedAccount);
    const liveAccountIds = new Set(liveAccounts.map(({ id }) => id));
    const unacknowledgedImmediateAccounts = snapshot.accounts.filter(
      ({ id }) => immediatelyAddedAccountIds.has(id) && !liveAccountIds.has(id),
    );
    liveAccountIds.forEach((id) => {
      immediatelyAddedAccountIds.delete(id);
      liveAcknowledgedAccountIds.add(id);
    });
    update({
      ...snapshot,
      accounts: unacknowledgedImmediateAccounts.reduce(upsertAccount, liveAccounts),
      messages: appSnapshot.messages.map(normalizedMessage),
    });
  };

  return {
    getSnapshot: () => snapshot,
    getLiveSnapshotRevision: () => liveSnapshotRevision,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    hydrateSnapshot(appSnapshot, expectedLiveRevision = 0) {
      if (liveSnapshotRevision !== expectedLiveRevision) return;
      const hydratedAccounts = appSnapshot.accounts.map(normalizedAccount);
      const immediateAccounts = snapshot.accounts.filter(({ id }) => immediatelyAddedAccountIds.has(id));
      update({
        ...snapshot,
        accounts: immediateAccounts.reduce(upsertAccount, hydratedAccounts),
        messages: appSnapshot.messages.map(normalizedMessage),
      });
    },
    applySnapshot(appSnapshot) {
      applyLiveSnapshot(appSnapshot);
    },
    applyRemovalSnapshot(appSnapshot, removedAccountId) {
      applyLiveSnapshot(appSnapshot, removedAccountId);
    },
    addAccount,
    retry() {
      if (!("retryable" in snapshot.addAccountState) || !snapshot.addAccountState.retryable) {
        return Promise.resolve();
      }
      return addAccount(snapshot.addAccountProvider);
    },
  };
}

export function useAddAccountController(controller: AddAccountController): AddAccountSnapshot {
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
