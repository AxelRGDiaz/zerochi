import { describe, expect, it, vi } from "vitest";
import {
  createSyncStatusStore,
  createSyncStatusSubscription,
  type SyncStatusEventListener,
  type SyncStatusListenPort,
} from "./syncStatusStore";

describe("sync status store", () => {
  it("isolates retry and action-required status by account UUID", () => {
    const store = createSyncStatusStore();

    store.applyStatus({ accountId: "account-a", status: "retrying", retryAfterSeconds: 30 });
    store.applyStatus({ accountId: "account-b", status: "action_required" });

    expect(store.getSnapshot().byAccountId).toEqual({
      "account-a": { status: "retrying", retryAfterSeconds: 30 },
      "account-b": { status: "action_required" },
    });
  });

  it("healthy clears only the matching account and leaves healthy as the implicit default", () => {
    const store = createSyncStatusStore();
    store.applyStatus({ accountId: "account-a", status: "action_required" });
    store.applyStatus({ accountId: "account-b", status: "retrying", retryAfterSeconds: 12 });

    store.applyStatus({ accountId: "account-a", status: "healthy" });

    expect(store.getSnapshot().byAccountId).toEqual({
      "account-b": { status: "retrying", retryAfterSeconds: 12 },
    });
    const unchangedSnapshot = store.getSnapshot();
    store.applyStatus({ accountId: "already-healthy", status: "healthy" });
    expect(store.getSnapshot()).toBe(unchangedSnapshot);
  });

  it("clears only the explicitly removed account", () => {
    const store = createSyncStatusStore();
    store.applyStatus({ accountId: "account-a", status: "action_required" });
    store.applyStatus({ accountId: "account-b", status: "retrying", retryAfterSeconds: 12 });

    store.clearAccount("account-a");

    expect(store.getSnapshot().byAccountId).toEqual({
      "account-b": { status: "retrying", retryAfterSeconds: 12 },
    });
  });

  it("replaces retry metadata when one account transitions to action-required", () => {
    const store = createSyncStatusStore();
    store.applyStatus({ accountId: "account-a", status: "retrying", retryAfterSeconds: 45 });

    store.applyStatus({ accountId: "account-a", status: "action_required" });

    expect(store.getSnapshot().byAccountId["account-a"]).toEqual({ status: "action_required" });
  });

  it("stops applying events and unregisters after cleanup", async () => {
    let listener!: SyncStatusEventListener;
    const stopListening = vi.fn();
    const listenPort = vi.fn<SyncStatusListenPort>(async (nextListener) => {
      listener = nextListener;
      return stopListening;
    });
    const store = createSyncStatusStore();
    const cleanup = createSyncStatusSubscription(listenPort, store).start();
    await Promise.resolve();
    await Promise.resolve();

    listener({ accountId: "account-a", status: "retrying", retryAfterSeconds: 8 });
    cleanup();
    listener({ accountId: "account-a", status: "action_required" });

    expect(stopListening).toHaveBeenCalledTimes(1);
    expect(store.getSnapshot().byAccountId["account-a"]).toEqual({
      status: "retrying",
      retryAfterSeconds: 8,
    });
  });

  it("cleans up a listener that finishes registering after unmount", async () => {
    let resolveListen!: (unlisten: () => void) => void;
    const stopListening = vi.fn();
    const listenPort = vi.fn<SyncStatusListenPort>(() => new Promise((resolve) => {
      resolveListen = resolve;
    }));
    const cleanup = createSyncStatusSubscription(listenPort, createSyncStatusStore()).start();
    await Promise.resolve();

    cleanup();
    resolveListen(stopListening);
    await Promise.resolve();
    await Promise.resolve();

    expect(stopListening).toHaveBeenCalledTimes(1);
  });
});
