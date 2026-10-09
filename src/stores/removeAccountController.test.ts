import { describe, expect, it, vi } from "vitest";
import type {
  CommandError,
  RemoveAccountErrorTag,
  RemoveAccountResult,
} from "../types/contracts";
import type { MailAccount, MailMessage } from "../types/mail";
import { createAddAccountController, type AddAccountPort } from "./addAccountController";
import {
  createRemoveAccountController,
  type RemoveAccountPort,
} from "./removeAccountController";
import { createSyncStatusStore } from "./syncStatusStore";

const accountA: MailAccount = {
  id: "account-a",
  email: "a@example.com",
  displayName: "Cuenta A",
  pendingCount: 4,
  inboxUnreadCount: 7,
  active: true,
};

const accountB: MailAccount = {
  id: "account-b",
  email: "b@example.com",
  displayName: "Cuenta B",
  pendingCount: 2,
  inboxUnreadCount: 3,
  active: true,
};

const messageB: MailMessage = {
  id: "message-b",
  threadId: "thread-b",
  accountId: accountB.id,
  sender: "sender@example.com",
  subject: "Asunto",
  receivedAt: "2026-08-10T10:00:00Z",
  detectedAt: "2026-08-10T10:01:00Z",
};

function result(accounts: MailAccount[] = [accountB]): RemoveAccountResult {
  return { snapshot: { accounts, messages: [messageB] } };
}

describe("remove account controller", () => {
  it("requires inline confirmation and cancellation performs no command", async () => {
    const port = vi.fn<RemoveAccountPort>();
    const controller = createRemoveAccountController(port, vi.fn(), vi.fn());

    controller.requestRemoval(accountA.id);
    expect(controller.getSnapshot()).toEqual({ status: "confirming", accountId: accountA.id });

    controller.cancelRemoval();
    expect(controller.getSnapshot()).toEqual({ status: "idle" });
    await controller.confirmRemoval();
    expect(port).not.toHaveBeenCalled();
  });

  it("applies the returned snapshot and clears only the removed account sync status", async () => {
    const accountController = createAddAccountController(vi.fn<AddAccountPort>(), [accountA, accountB]);
    const syncStatuses = createSyncStatusStore();
    syncStatuses.applyStatus({ accountId: accountA.id, status: "action_required" });
    syncStatuses.applyStatus({ accountId: accountB.id, status: "retrying", retryAfterSeconds: 15 });
    const port = vi.fn<RemoveAccountPort>().mockResolvedValue(result());
    const controller = createRemoveAccountController(
      port,
      accountController.applyRemovalSnapshot,
      syncStatuses.clearAccount,
    );

    controller.requestRemoval(accountA.id);
    await controller.confirmRemoval();

    expect(port).toHaveBeenCalledWith(accountA.id);
    expect(accountController.getSnapshot()).toMatchObject({
      accounts: [accountB],
      messages: [messageB],
    });
    expect(syncStatuses.getSnapshot().byAccountId).toEqual({
      [accountB.id]: { status: "retrying", retryAfterSeconds: 15 },
    });
    expect(controller.getSnapshot()).toEqual({ status: "success" });
  });

  it("removes an account added before any live snapshot acknowledges it", async () => {
    const accountController = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: accountA }),
    );
    await accountController.addAccount();
    expect(accountController.getSnapshot().accounts).toEqual([accountA]);

    const controller = createRemoveAccountController(
      vi.fn<RemoveAccountPort>().mockResolvedValue({
        snapshot: { accounts: [], messages: [] },
      }),
      accountController.applyRemovalSnapshot,
      vi.fn(),
    );
    controller.requestRemoval(accountA.id);

    await controller.confirmRemoval();

    expect(controller.getSnapshot()).toEqual({ status: "success" });
    expect(accountController.getSnapshot().accounts).toEqual([]);
  });

  it("authoritatively removes only the requested immediate account", async () => {
    const accountController = createAddAccountController(
      vi.fn<AddAccountPort>()
        .mockResolvedValueOnce({ account: accountA })
        .mockResolvedValueOnce({ account: accountB }),
    );
    await accountController.addAccount();
    await accountController.addAccount();
    expect(accountController.getSnapshot().accounts).toEqual([accountA, accountB]);

    const controller = createRemoveAccountController(
      vi.fn<RemoveAccountPort>().mockResolvedValue({
        snapshot: { accounts: [], messages: [] },
      }),
      accountController.applyRemovalSnapshot,
      vi.fn(),
    );
    controller.requestRemoval(accountA.id);

    await controller.confirmRemoval();

    expect(controller.getSnapshot()).toEqual({ status: "success" });
    expect(accountController.getSnapshot().accounts).toEqual([accountB]);

    accountController.applySnapshot({
      accounts: [accountB],
      messages: [],
    });
    accountController.applySnapshot({
      accounts: [],
      messages: [],
    });

    expect(accountController.getSnapshot().accounts).toEqual([]);
  });

  it.each([
    "account_not_found",
    "secure_store_failed",
    "internal",
  ] satisfies readonly RemoveAccountErrorTag[])("retains only the approved %s tag", async (tag) => {
    const error: CommandError<RemoveAccountErrorTag> = {
      tag,
      message: "refresh_token=secret provider body user@example.com",
      retryable: false,
      actionRequired: tag === "secure_store_failed",
    };
    const controller = createRemoveAccountController(
      vi.fn<RemoveAccountPort>().mockRejectedValue(error),
      vi.fn(),
      vi.fn(),
    );

    controller.requestRemoval(accountA.id);
    await controller.confirmRemoval();

    expect(controller.getSnapshot()).toEqual({ status: "error", accountId: accountA.id, tag });
    expect(JSON.stringify(controller.getSnapshot())).not.toMatch(/refresh_token|secret|provider body|user@example\.com/i);
  });

  it("collapses unknown or malformed error fields to a generic sanitized state", async () => {
    const controller = createRemoveAccountController(
      vi.fn<RemoveAccountPort>().mockRejectedValue({
        tag: "provider_private_error",
        message: { body: "secret" },
        retryable: "yes",
        accessToken: "secret",
      }),
      vi.fn(),
      vi.fn(),
    );

    controller.requestRemoval(accountA.id);
    await controller.confirmRemoval();

    expect(controller.getSnapshot()).toEqual({ status: "error", accountId: accountA.id });
    expect(JSON.stringify(controller.getSnapshot())).not.toMatch(/provider|body|token|secret/i);
  });

  it("rejects malformed or non-removing snapshots without changing accounts or statuses", async () => {
    const applySnapshot = vi.fn();
    const clearSyncStatus = vi.fn();
    const malformedResults = [
      { snapshot: { accounts: "not-an-array", messages: [] } },
      { snapshot: { accounts: [{ ...accountA, pendingCount: "four" }], messages: [] } },
      { snapshot: { accounts: [accountA, accountB], messages: [] } },
      { snapshot: { accounts: [accountB], messages: [{ ...messageB, accountId: accountA.id }] } },
    ];

    for (const malformed of malformedResults) {
      const controller = createRemoveAccountController(
        vi.fn<RemoveAccountPort>().mockResolvedValue(malformed as unknown as RemoveAccountResult),
        applySnapshot,
        clearSyncStatus,
      );
      controller.requestRemoval(accountA.id);
      await controller.confirmRemoval();
      expect(controller.getSnapshot()).toEqual({ status: "error", accountId: accountA.id });
    }

    expect(applySnapshot).not.toHaveBeenCalled();
    expect(clearSyncStatus).not.toHaveBeenCalled();
  });

  it("allows unknown result fields but normalized snapshot state drops them", async () => {
    const accountController = createAddAccountController(vi.fn<AddAccountPort>(), [accountA, accountB]);
    const unsafeAccount = {
      ...accountB,
      refreshToken: "secret-refresh",
      providerBody: "secret-body",
    } as unknown as MailAccount;
    const unsafeResult = {
      ...result([unsafeAccount]),
      accessToken: "secret-access",
    } as unknown as RemoveAccountResult;
    const controller = createRemoveAccountController(
      vi.fn<RemoveAccountPort>().mockResolvedValue(unsafeResult),
      accountController.applyRemovalSnapshot,
      vi.fn(),
    );

    controller.requestRemoval(accountA.id);
    await controller.confirmRemoval();

    expect(accountController.getSnapshot().accounts).toEqual([accountB]);
    expect(JSON.stringify(accountController.getSnapshot())).not.toMatch(/refreshToken|providerBody|secret/i);
  });
});
