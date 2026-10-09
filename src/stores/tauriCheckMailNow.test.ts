import { describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { invokeCheckMailNow } from "./tauriCheckMailNow";

describe("tauri check mail now adapter", () => {
  it("normalizes Tauri's null unit response after invoking without arguments", async () => {
    invoke.mockResolvedValue(null);

    await expect(invokeCheckMailNow()).resolves.toBeUndefined();

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("check_mail_now");
    expect(invoke.mock.calls[0]).toHaveLength(1);
  });
});
