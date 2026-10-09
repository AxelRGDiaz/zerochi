import type { AccountId, MailAccount, MailMessage } from "./mail";

export interface MailProvider {
  getIdentity(): Promise<Pick<MailAccount, "email" | "displayName">>;
  initialCursor(): Promise<string>;
  changesSince(cursor: string): Promise<{ cursor: string; messages: MailMessage[] }>;
}

export interface SecureStore {
  setSecret(accountId: AccountId, value: Uint8Array): Promise<void>;
  getSecret(accountId: AccountId): Promise<Uint8Array | null>;
  deleteSecret(accountId: AccountId): Promise<void>;
}

export interface NotificationService {
  notifyNewMail(message: MailMessage): Promise<void>;
}

export interface ExternalNavigationService {
  openAccount(accountId: AccountId): Promise<void>;
  openMessage(message: Pick<MailMessage, "accountId" | "id" | "threadId">): Promise<void>;
}

export type OpenAccountErrorTag =
  | "account_not_found"
  | "internal"
  | "navigation_failed";

export type CheckMailNowErrorTag = "internal";

export interface MailSyncService {
  checkMailNow(): Promise<void>;
}

export interface AppSnapshot {
  accounts: MailAccount[];
  messages: MailMessage[];
}

export type SyncStatus = "healthy" | "retrying" | "action_required";

export interface SyncStatusChanged {
  accountId: AccountId;
  status: SyncStatus;
  retryAfterSeconds?: number;
}

export interface PopoverAnchorChanged {
  x: number;
}

export type AddAccountErrorTag =
  | "auth_in_progress"
  | "browser_open_failed"
  | "callback_invalid"
  | "cancelled"
  | "config_invalid"
  | "identity_invalid"
  | "internal"
  | "network_unavailable"
  | "provider_denied"
  | "refresh_token_missing"
  | "secure_store_failed"
  | "timeout"
  | "token_exchange_failed"
  | "workspace_restricted";

export interface CommandError<Tag extends string = string> {
  tag: Tag;
  message: string;
  retryable: boolean;
  actionRequired: boolean;
}

export interface AddAccountResult {
  account: MailAccount;
}

export type RemoveAccountErrorTag =
  | "account_not_found"
  | "internal"
  | "secure_store_failed";

export interface RemoveAccountResult {
  snapshot: AppSnapshot;
}
