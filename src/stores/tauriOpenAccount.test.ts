import { describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { invokeOpenAccount } from "./tauriOpenAccount";

describe("tauri open account adapter", () => {
  it("invokes open_account with exactly the stable local account id", async () => {
    invoke.mockResolvedValue(undefined);

    await invokeOpenAccount("account-a");

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("open_account", { accountId: "account-a" });
    expect(invoke.mock.calls[0]).toHaveLength(2);
  });
});
