import { describe, expect, it, vi } from "vitest";

const { listen } = vi.hoisted(() => ({ listen: vi.fn() }));

vi.mock("@tauri-apps/api/event", () => ({ listen }));

import { listenPopoverAnchor } from "./tauriPopoverAnchor";

describe("tauri popover anchor adapter", () => {
  it("listens to the approved event and forwards only its payload", async () => {
    const stopListening = vi.fn();
    let nativeListener!: (event: { payload: { x: number } }) => void;
    listen.mockImplementation(async (_eventName, listener) => {
      nativeListener = listener;
      return stopListening;
    });
    const listener = vi.fn();

    const unlisten = await listenPopoverAnchor(listener);
    nativeListener({ payload: { x: 140 } });

    expect(listen).toHaveBeenCalledWith("popover_anchor_changed", expect.any(Function));
    expect(listener).toHaveBeenCalledWith({ x: 140 });
    expect(unlisten).toBe(stopListening);
  });
});
