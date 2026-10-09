import type { MailAccount } from "../types/mail";

export const MAX_SAFE_MAIL_COUNT = Number.MAX_SAFE_INTEGER;

export function normalizeInboxUnreadCount(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) return 0;
  return Math.min(Math.trunc(value), MAX_SAFE_MAIL_COUNT);
}

export function inboxUnreadTotal(accounts: readonly MailAccount[]): number {
  return accounts.reduce((total, account) => {
    if (!account.active) return total;
    const unread = normalizeInboxUnreadCount(account.inboxUnreadCount);
    return Math.min(total + unread, MAX_SAFE_MAIL_COUNT);
  }, 0);
}
