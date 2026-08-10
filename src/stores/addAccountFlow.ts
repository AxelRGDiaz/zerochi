import type { AddAccountController } from "./addAccountController";

export async function completeAddAccountAttempt(
  controller: Pick<AddAccountController, "getSnapshot">,
  attempt: () => Promise<void>,
  onSuccess: () => void,
): Promise<void> {
  await attempt();
  if (controller.getSnapshot().addAccountState.status === "success") onSuccess();
}
