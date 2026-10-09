import { describe, expect, it } from "vitest";
import type { MailAccount } from "../types/mail";
import {
  inboxUnreadTotal,
  MAX_SAFE_MAIL_COUNT,
  normalizeInboxUnreadCount,
} from "./mailCounters";

function account(id: string, inboxUnreadCount?: number, active = true): MailAccount {
  return {
    id,
    email: `${id}@example.com`,
    pendingCount: 99,
    ...(inboxUnreadCount === undefined ? {} : { inboxUnreadCount }),
    active,
  };
}

describe("Gmail unread counters", () => {
  it("returns zero for no accounts or a missing unread count", () => {
    expect(inboxUnreadTotal([])).toBe(0);
    expect(inboxUnreadTotal([account("missing")])).toBe(0);
  });

  it("projects one active account without substituting pending", () => {
    expect(inboxUnreadTotal([account("one", 3)])).toBe(3);
  });

  it("sums unread across active accounts and ignores inactive accounts", () => {
    expect(inboxUnreadTotal([
      account("one", 3),
      account("two", 4),
      account("inactive", 80, false),
    ])).toBe(7);
  });

  it("normalizes malformed values and saturates unsafe counts and sums", () => {
    expect(normalizeInboxUnreadCount(-4)).toBe(0);
    expect(normalizeInboxUnreadCount(Number.NaN)).toBe(0);
    expect(normalizeInboxUnreadCount(Number.POSITIVE_INFINITY)).toBe(0);
    expect(normalizeInboxUnreadCount(2.9)).toBe(2);
    expect(normalizeInboxUnreadCount(MAX_SAFE_MAIL_COUNT + 1000)).toBe(MAX_SAFE_MAIL_COUNT);
    expect(inboxUnreadTotal([
      account("maximum", MAX_SAFE_MAIL_COUNT),
      account("overflow", 5),
    ])).toBe(MAX_SAFE_MAIL_COUNT);
  });
});
