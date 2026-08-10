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
      addAccountState: { status: "idle" },
    });
  });

  it("invokes the injected zero-argument port once and prevents duplicate pending attempts", async () => {
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
    expect(port).toHaveBeenCalledWith();

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

  it.each([
    [commandError("cancelled"), "cancelled"],
    [commandError("network_unavailable", true), "retryable_error"],
    [commandError("workspace_restricted", false, true), "action_required"],
    [commandError("internal"), "error"],
    [new Error("untyped failure"), "error"],
  ] as const)("classifies command failures into fixed UI states", (error, status) => {
    expect(classifyAddAccountError(error)).toEqual({ status });
  });

  it("retries only a retryable failure and succeeds on the next invocation", async () => {
    const port = vi.fn<AddAccountPort>()
      .mockRejectedValueOnce(commandError("timeout", true))
      .mockResolvedValueOnce({ account: addedAccount });
    const controller = createAddAccountController(port);

    await controller.addAccount();
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "retryable_error" });

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
});
