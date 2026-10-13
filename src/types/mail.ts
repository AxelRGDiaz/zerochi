export type MailProviderId = "gmail" | "outlook";

export interface MailAccount {
  id: string;
  email: string;
  /** Absent means Gmail: accounts stored before providers existed carry no value. */
  provider?: MailProviderId;
  displayName?: string;
  pendingCount: number;
  inboxUnreadCount?: number;
  active: boolean;
  lastHistoryId?: string;
  lastSyncAt?: string;
}

export interface MailMessage {
  id: string;
  threadId: string;
  accountId: string;
  sender: string;
  subject: string;
  receivedAt: string;
  detectedAt: string;
  openedFromZerochiAt?: string;
}

export type AccountId = MailAccount["id"];
