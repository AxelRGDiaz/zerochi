import { describe, expect, it, vi } from "vitest";
import type { PopoverAnchorChanged } from "../types/contracts";
import {
  createPopoverAnchorSubscription,
  type PopoverAnchorListenPort,
} from "./popoverAnchor";

describe("popover anchor subscription", () => {
  it("applies only finite native anchor values", async () => {
    let listener!: (event: PopoverAnchorChanged) => void;
    const applyAnchor = vi.fn();
    const subscription = createPopoverAnchorSubscription(async (nextListener) => {
      listener = nextListener;
      return vi.fn();
    }, applyAnchor);
    subscription.start();
    await Promise.resolve();

    listener({ x: 120 });
    listener({ x: Number.NaN });
    listener({ x: Number.POSITIVE_INFINITY });
    listener({ x: "private" } as unknown as PopoverAnchorChanged);
    listener(null as unknown as PopoverAnchorChanged);

    expect(applyAnchor).toHaveBeenCalledTimes(1);
    expect(applyAnchor).toHaveBeenCalledWith(120);
  });

  it("stops events and unregisters after cleanup", async () => {
    let listener!: (event: PopoverAnchorChanged) => void;
    const stopListening = vi.fn();
    const applyAnchor = vi.fn();
    const cleanup = createPopoverAnchorSubscription(async (nextListener) => {
      listener = nextListener;
      return stopListening;
    }, applyAnchor).start();
    await Promise.resolve();
    await Promise.resolve();

    listener({ x: 88 });
    cleanup();
    listener({ x: 144 });

    expect(applyAnchor).toHaveBeenCalledTimes(1);
    expect(stopListening).toHaveBeenCalledTimes(1);
  });

  it("cleans up a listener that finishes registering after unmount", async () => {
    let resolveListen!: (unlisten: () => void) => void;
    const stopListening = vi.fn();
    const listenPort = vi.fn<PopoverAnchorListenPort>(() => new Promise((resolve) => {
      resolveListen = resolve;
    }));
    const cleanup = createPopoverAnchorSubscription(listenPort, vi.fn()).start();
    await Promise.resolve();

    cleanup();
    resolveListen(stopListening);
    await Promise.resolve();
    await Promise.resolve();

    expect(stopListening).toHaveBeenCalledTimes(1);
  });

  it("keeps the CSS default when registration throws synchronously", () => {
    const applyAnchor = vi.fn();
    const listenPort = (() => {
      throw new Error("unavailable");
    }) as PopoverAnchorListenPort;

    expect(() => createPopoverAnchorSubscription(listenPort, applyAnchor).start()).not.toThrow();
    expect(applyAnchor).not.toHaveBeenCalled();
  });
});
