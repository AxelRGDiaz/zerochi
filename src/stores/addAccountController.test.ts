import { describe, expect, it, vi } from "vitest";
import type { AddAccountErrorTag, AddAccountResult, CommandError } from "../types/contracts";
import type { MailAccount } from "../types/mail";
import {
  classifyAddAccountError,
  createAddAccountController,
  type AddAccountPort,
} from "./addAccountController";

const personalAccount: MailAccount = {
  id: "personal",
  email: "personal@gmail.com",
  displayName: "Personal",
  pendingCount: 2,
  active: true,
};

const addedAccount: MailAccount = {
  id: "new-account",
  email: "nueva@example.com",
  displayName: "Nueva",
  pendingCount: 0,
  active: true,
};

function commandError(
  tag: AddAccountErrorTag,
  retryable = false,
  actionRequired = false,
): CommandError<AddAccountErrorTag> {
  return { tag, message: "sanitized", retryable, actionRequired };
}

describe("add account controller", () => {
  it("starts with no accounts when production provides no persisted snapshot", () => {
    const controller = createAddAccountController(vi.fn<AddAccountPort>());

    expect(controller.getSnapshot()).toEqual({
      accounts: [],
      messages: [],
      addAccountState: { status: "idle" },
      addAccountProvider: "gmail",
    });
  });

  it("invokes the injected port once for Gmail by default and prevents duplicate pending attempts", async () => {
    let resolveAttempt!: (result: AddAccountResult) => void;
    const port = vi.fn<AddAccountPort>(() => new Promise((resolve) => {
      resolveAttempt = resolve;
    }));
    const controller = createAddAccountController(port, [personalAccount]);

    const first = controller.addAccount();
    const duplicate = controller.addAccount();

    expect(first).toBe(duplicate);
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "pending" });
    await Promise.resolve();
    expect(port).toHaveBeenCalledTimes(1);
    expect(port).toHaveBeenCalledWith("gmail");

    resolveAttempt({ account: addedAccount });
    await first;
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "success" });
  });

  it("inserts a new account and updates an existing id without duplication", async () => {
    const updatedAccount = { ...addedAccount, displayName: "Actualizada", pendingCount: 3 };
    const port = vi.fn<AddAccountPort>()
      .mockResolvedValueOnce({ account: addedAccount })
      .mockResolvedValueOnce({ account: updatedAccount });
    const controller = createAddAccountController(port, [personalAccount]);

    await controller.addAccount();
    await controller.addAccount();

    expect(controller.getSnapshot().accounts).toHaveLength(2);
    expect(controller.getSnapshot().accounts[1]).toEqual(updatedAccount);
  });

  it("hydrates persisted accounts without changing the add-account presentation state", () => {
    const controller = createAddAccountController(vi.fn<AddAccountPort>());

    controller.hydrateSnapshot({ accounts: [personalAccount], messages: [] });

    expect(controller.getSnapshot()).toEqual({
      accounts: [personalAccount],
      messages: [],
      addAccountState: { status: "idle" },
      addAccountProvider: "gmail",
    });
  });

  it.each([
    [commandError("cancelled"), { status: "cancelled", tag: "cancelled", retryable: false, actionRequired: false }],
    [commandError("network_unavailable", true), { status: "retryable_error", tag: "network_unavailable", retryable: true, actionRequired: false }],
    [commandError("workspace_restricted", false, true), { status: "action_required", tag: "workspace_restricted", retryable: false, actionRequired: true }],
    [commandError("internal"), { status: "error", tag: "internal", retryable: false, actionRequired: false }],
    [new Error("untyped failure"), { status: "error" }],
  ] as const)("classifies command failures into fixed UI states", (error, expected) => {
    expect(classifyAddAccountError(error)).toEqual(expected);
  });

  it.each([
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
  ] satisfies readonly AddAccountErrorTag[])("retains the approved %s tag and sanitized flags", (tag) => {
    const state = classifyAddAccountError(commandError(tag, true, true));

    expect(state).toMatchObject({ tag, retryable: true, actionRequired: true });
    expect(JSON.stringify(state)).not.toContain("sanitized");
  });

  it("drops unapproved runtime tags and provider messages into the generic error state", () => {
    const state = classifyAddAccountError({
      tag: "provider_private_diagnostic",
      message: "refresh_token=secret user@example.com",
      retryable: true,
      actionRequired: true,
    });

    expect(state).toEqual({ status: "error" });
    expect(JSON.stringify(state)).not.toMatch(/provider_private|refresh_token|secret|user@example\.com/i);
  });

  it("retries only a retryable failure and succeeds on the next invocation", async () => {
    const port = vi.fn<AddAccountPort>()
      .mockRejectedValueOnce(commandError("timeout", true))
      .mockResolvedValueOnce({ account: addedAccount });
    const controller = createAddAccountController(port);

    await controller.addAccount();
    expect(controller.getSnapshot().addAccountState).toEqual({
      status: "retryable_error",
      tag: "timeout",
      retryable: true,
      actionRequired: false,
    });

    await controller.retry();
    expect(port).toHaveBeenCalledTimes(2);
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "success" });
    expect(controller.getSnapshot().accounts).toEqual([addedAccount]);
  });

  it("does not retain secret-shaped fields from command results in UI state", async () => {
    const unsafeRuntimeResult = {
      account: {
        ...addedAccount,
        accessToken: "access-secret",
        refreshToken: "refresh-secret",
        clientSecret: "oauth-secret",
        oauth: { verifier: "pkce-secret" },
      },
      authorizationCode: "code-secret",
    } as unknown as AddAccountResult;
    const controller = createAddAccountController(vi.fn<AddAccountPort>().mockResolvedValue(unsafeRuntimeResult));

    await controller.addAccount();
    const serializedState = JSON.stringify(controller.getSnapshot());

    expect(serializedState).not.toMatch(/accessToken|refreshToken|clientSecret|oauth|authorizationCode/i);
    expect(serializedState).not.toContain("secret");
    expect(controller.getSnapshot().accounts).toEqual([addedAccount]);
  });

  it("sends the chosen provider, keeps it on the account, and retries the same provider", async () => {
    const outlookAccount = { ...personalAccount, id: "outlook-1", provider: "outlook" as const };
    const port = vi.fn<AddAccountPort>()
      .mockRejectedValueOnce({ tag: "timeout", message: "", retryable: true, actionRequired: false })
      .mockResolvedValueOnce({ account: outlookAccount });
    const controller = createAddAccountController(port);

    await controller.addAccount("outlook");
    expect(controller.getSnapshot().addAccountProvider).toBe("outlook");
    expect(controller.getSnapshot().addAccountState.status).toBe("retryable_error");

    await controller.retry();

    expect(port.mock.calls).toEqual([["outlook"], ["outlook"]]);
    expect(controller.getSnapshot().accounts).toEqual([outlookAccount]);

    controller.applySnapshot({ accounts: [outlookAccount, personalAccount], messages: [] });
    expect(controller.getSnapshot().accounts.map(({ provider }) => provider)).toEqual(["outlook", undefined]);
  });
});
