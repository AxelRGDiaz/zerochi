import { describe, expect, it, vi } from "vitest";
import type { AddAccountErrorTag, CommandError } from "../types/contracts";
import { createAddAccountController, type AddAccountPort } from "./addAccountController";
import { completeAddAccountAttempt } from "./addAccountFlow";
import { createAppNavigationStore } from "./appNavigationStore";

const addedAccount = {
  id: "connected",
  email: "connected@example.com",
  displayName: "Conectada",
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

describe("add account UI flow", () => {
  it("returns home with the normalized account after success", async () => {
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: addedAccount }),
    );
    const navigation = createAppNavigationStore("add_account");

    await completeAddAccountAttempt(controller, controller.addAccount, navigation.back);

    expect(navigation.getSnapshot()).toEqual({ route: "home" });
    expect(controller.getSnapshot().accounts).toEqual([addedAccount]);
  });

  it("keeps cancellation and action-required feedback on provider selection", async () => {
    const cancelledController = createAddAccountController(
      vi.fn<AddAccountPort>().mockRejectedValue(commandError("cancelled")),
    );
    const actionController = createAddAccountController(
      vi.fn<AddAccountPort>().mockRejectedValue(commandError("workspace_restricted", false, true)),
    );
    const cancelledNavigation = createAppNavigationStore("add_account");
    const actionNavigation = createAppNavigationStore("add_account");

    await completeAddAccountAttempt(cancelledController, cancelledController.addAccount, cancelledNavigation.back);
    await completeAddAccountAttempt(actionController, actionController.addAccount, actionNavigation.back);

    expect(cancelledController.getSnapshot().addAccountState).toEqual({ status: "cancelled" });
    expect(actionController.getSnapshot().addAccountState).toEqual({ status: "action_required" });
    expect(cancelledNavigation.getSnapshot()).toEqual({ route: "add_account" });
    expect(actionNavigation.getSnapshot()).toEqual({ route: "add_account" });
  });

  it("stays on a retryable error and returns home when retry succeeds", async () => {
    const port = vi.fn<AddAccountPort>()
      .mockRejectedValueOnce(commandError("timeout", true))
      .mockResolvedValueOnce({ account: addedAccount });
    const controller = createAddAccountController(port);
    const navigation = createAppNavigationStore("add_account");

    await completeAddAccountAttempt(controller, controller.addAccount, navigation.back);
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "retryable_error" });
    expect(navigation.getSnapshot()).toEqual({ route: "add_account" });

    await completeAddAccountAttempt(controller, controller.retry, navigation.back);
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "success" });
    expect(navigation.getSnapshot()).toEqual({ route: "home" });
  });
});
