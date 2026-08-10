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

export interface AppSnapshot {
  accounts: MailAccount[];
  messages: MailMessage[];
}
