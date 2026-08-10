export interface MailAccount {
  id: string;
  email: string;
  displayName?: string;
  pendingCount: number;
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
