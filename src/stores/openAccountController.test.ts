import { describe, expect, it, vi } from "vitest";
import { createOpenAccountController, type OpenAccountPort } from "./openAccountController";

function deferred() {
  let resolve!: () => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<void>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

describe("open account controller", () => {
  it("prevents duplicate handoffs for the same account while it is pending", async () => {
    const handoff = deferred();
    const port = vi.fn<OpenAccountPort>().mockReturnValue(handoff.promise);
    const controller = createOpenAccountController(port);

    const first = controller.openAccount("account-a");
    const duplicate = controller.openAccount("account-a");
    await Promise.resolve();

    expect(duplicate).toBe(first);
    expect(port).toHaveBeenCalledTimes(1);
    expect(port).toHaveBeenCalledWith("account-a");
    expect(controller.getSnapshot()).toEqual({ byAccountId: { "account-a": "pending" } });

    handoff.resolve();
    await first;
    expect(controller.getSnapshot()).toEqual({ byAccountId: {} });
  });

  it("keeps pending and failure state isolated per account", async () => {
    const accountA = deferred();
    const accountB = deferred();
    const port = vi.fn<OpenAccountPort>((accountId) => (
      accountId === "account-a" ? accountA.promise : accountB.promise
    ));
    const controller = createOpenAccountController(port);

    const first = controller.openAccount("account-a");
    const second = controller.openAccount("account-b");
    await Promise.resolve();
    expect(controller.getSnapshot()).toEqual({
      byAccountId: { "account-a": "pending", "account-b": "pending" },
    });

    accountA.resolve();
    await first;
    expect(controller.getSnapshot()).toEqual({ byAccountId: { "account-b": "pending" } });

    accountB.reject(new Error("private provider output for user@example.com"));
    await second;
    expect(controller.getSnapshot()).toEqual({ byAccountId: { "account-b": "error" } });
    expect(JSON.stringify(controller.getSnapshot())).not.toMatch(/provider|user@example|private/i);
  });

  it("clears a previous fixed failure state when retrying", async () => {
    const port = vi.fn<OpenAccountPort>()
      .mockRejectedValueOnce({ message: "https://mail.google.com/private?token=secret" })
      .mockResolvedValueOnce();
    const controller = createOpenAccountController(port);

    await controller.openAccount("account-a");
    expect(controller.getSnapshot()).toEqual({ byAccountId: { "account-a": "error" } });

    const retry = controller.openAccount("account-a");
    expect(controller.getSnapshot()).toEqual({ byAccountId: { "account-a": "pending" } });
    await retry;
    expect(controller.getSnapshot()).toEqual({ byAccountId: {} });
  });
});
