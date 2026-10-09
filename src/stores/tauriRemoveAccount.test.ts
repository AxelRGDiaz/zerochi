import { describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { invokeRemoveAccount } from "./tauriRemoveAccount";

describe("tauri remove account adapter", () => {
  it("invokes the approved command with only the local account id", async () => {
    invoke.mockResolvedValue({ snapshot: { accounts: [], messages: [] } });

    await invokeRemoveAccount("account-a");

    expect(invoke).toHaveBeenCalledWith("remove_account", { accountId: "account-a" });
  });
});
