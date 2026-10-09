import { afterEach, describe, expect, it, vi } from "vitest";
import type { AppSnapshot } from "../types/contracts";
import type { MailAccount } from "../types/mail";
import { createAddAccountController, type AddAccountPort } from "./addAccountController";
import {
  createAppSnapshotHydrator,
  createAppSnapshotSubscription,
  type AppSnapshotListenPort,
  type AppSnapshotListener,
  type AppSnapshotPort,
} from "./appSnapshotHydration";

const persistedAccount: MailAccount = {
  id: "persisted",
  email: "persisted@example.com",
  displayName: "Persistida",
  pendingCount: 4,
  inboxUnreadCount: 12,
  active: true,
};

const addedAccount: MailAccount = {
  id: "added",
  email: "added@example.com",
  displayName: "Recién agregada",
  pendingCount: 0,
  active: true,
};

function appSnapshot(accounts: MailAccount[]): AppSnapshot {
  return { accounts, messages: [] };
}

async function flushMicrotasks(count = 6): Promise<void> {
  for (let index = 0; index < count; index += 1) await Promise.resolve();
}

afterEach(() => vi.useRealTimers());

describe("app snapshot hydration", () => {
  it("loads persisted normalized accounts once", async () => {
    const snapshotPort = vi.fn<AppSnapshotPort>().mockResolvedValue(appSnapshot([persistedAccount]));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydrator = createAppSnapshotHydrator(snapshotPort, controller);

    await Promise.all([hydrator.hydrate(), hydrator.hydrate()]);

    expect(snapshotPort).toHaveBeenCalledTimes(1);
    expect(snapshotPort).toHaveBeenCalledWith();
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount]);
  });

  it("applies live account, pending, and message updates from the injected listener", async () => {
    let listener!: AppSnapshotListener;
    const stopListening = vi.fn();
    const listenPort = vi.fn<AppSnapshotListenPort>(async (nextListener) => {
      listener = nextListener;
      return stopListening;
    });
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const cleanup = createAppSnapshotSubscription(listenPort, controller).start();
    await flushMicrotasks();
    const liveAccount = { ...persistedAccount, pendingCount: 7 };
    const liveMessage = {
      id: "message-1",
      threadId: "thread-1",
      accountId: liveAccount.id,
      sender: "sender@example.com",
      subject: "Actualización",
      receivedAt: "2026-08-10T20:00:00Z",
      detectedAt: "2026-08-10T20:01:00Z",
    };

    listener({ accounts: [liveAccount], messages: [liveMessage] });

    expect(controller.getSnapshot()).toMatchObject({
      accounts: [liveAccount],
      messages: [liveMessage],
    });
    cleanup();
    expect(stopListening).toHaveBeenCalledTimes(1);
  });

  it("reads a fresh snapshot before and after listener registration", async () => {
    let resolveListen!: (unlisten: () => void) => void;
    const listenPort = vi.fn<AppSnapshotListenPort>(() => new Promise((resolve) => {
      resolveListen = resolve;
    }));
    const snapshotPort = vi.fn<AppSnapshotPort>().mockResolvedValue(appSnapshot([persistedAccount]));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydrator = createAppSnapshotHydrator(snapshotPort, controller);
    let hydration: Promise<void> | undefined;
    const cleanup = createAppSnapshotSubscription(listenPort, controller).start(() => {
      hydration = hydrator.hydrate();
    });
    await flushMicrotasks();

    expect(snapshotPort).toHaveBeenCalledTimes(1);

    resolveListen(vi.fn());
    await flushMicrotasks();
    expect(hydration).toBeDefined();
    await hydration;

    expect(snapshotPort).toHaveBeenCalledTimes(2);
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount]);
    cleanup();
  });

  it("performs the fresh hydration after asynchronous listener registration fails", async () => {
    const snapshotPort = vi.fn<AppSnapshotPort>().mockResolvedValue(appSnapshot([persistedAccount]));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydrator = createAppSnapshotHydrator(snapshotPort, controller);
    let hydration: Promise<void> | undefined;
    const cleanup = createAppSnapshotSubscription(
      vi.fn<AppSnapshotListenPort>().mockRejectedValue(new Error("listener unavailable")),
      controller,
    ).start(() => {
      hydration = hydrator.hydrate();
    });

    await Promise.resolve();
    expect(hydration).toBeDefined();
    await hydration;

    expect(snapshotPort).toHaveBeenCalledTimes(1);
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount]);
    cleanup();
  });

  it("performs the fresh hydration after synchronous listener registration throws", async () => {
    const snapshotPort = vi.fn<AppSnapshotPort>().mockResolvedValue(appSnapshot([persistedAccount]));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydrator = createAppSnapshotHydrator(snapshotPort, controller);
    const listenPort = vi.fn<AppSnapshotListenPort>(() => {
      throw new Error("listener unavailable");
    });
    let hydration: Promise<void> | undefined;

    const cleanup = createAppSnapshotSubscription(listenPort, controller).start(() => {
      hydration = hydrator.hydrate();
    });

    expect(hydration).toBeDefined();
    await hydration;
    expect(snapshotPort).toHaveBeenCalledTimes(1);
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount]);
    cleanup();
  });

  it("does not retry or refresh again when a listener failure settles after cleanup", async () => {
    let rejectListen!: (reason?: unknown) => void;
    const afterRegistrationAttempt = vi.fn();
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const cleanup = createAppSnapshotSubscription(
      () => new Promise((_, reject) => {
        rejectListen = reject;
      }),
      controller,
    ).start(afterRegistrationAttempt);
    await flushMicrotasks();

    cleanup();
    rejectListen(new Error("late failure"));
    await Promise.resolve();
    await Promise.resolve();

    expect(afterRegistrationAttempt).toHaveBeenCalledTimes(1);
  });

  it("preserves an immediate add omitted by a live snapshot, then accepts its acknowledged update", async () => {
    let listener!: AppSnapshotListener;
    const listenPort = vi.fn<AppSnapshotListenPort>(async (nextListener) => {
      listener = nextListener;
      return vi.fn();
    });
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: addedAccount }),
    );
    createAppSnapshotSubscription(listenPort, controller).start();
    await Promise.resolve();
    await Promise.resolve();
    await controller.addAccount();

    listener(appSnapshot([persistedAccount]));
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount, addedAccount]);

    const updatedAddedAccount = { ...addedAccount, pendingCount: 2 };
    listener(appSnapshot([persistedAccount, updatedAddedAccount]));
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount, updatedAddedAccount]);
  });

  it("keeps event state over a later add result and does not resurrect it after removal", async () => {
    let listener!: AppSnapshotListener;
    let resolveAdd!: (result: { account: MailAccount }) => void;
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>(() => new Promise((resolve) => {
        resolveAdd = resolve;
      })),
    );
    createAppSnapshotSubscription(async (nextListener) => {
      listener = nextListener;
      return vi.fn();
    }, controller).start();
    await Promise.resolve();
    await Promise.resolve();

    const addAttempt = controller.addAccount();
    await Promise.resolve();
    const liveAddedAccount = {
      ...addedAccount,
      pendingCount: 3,
      inboxUnreadCount: 9,
    };
    listener(appSnapshot([liveAddedAccount]));
    resolveAdd({ account: addedAccount });
    await addAttempt;

    expect(controller.getSnapshot().accounts).toEqual([liveAddedAccount]);
    expect(controller.getSnapshot().addAccountState).toEqual({ status: "success" });

    listener(appSnapshot([]));

    expect(controller.getSnapshot().accounts).toEqual([]);
  });

  it("ignores late startup hydration after a newer live snapshot", async () => {
    let resolveHydration!: (snapshot: AppSnapshot) => void;
    let listener!: AppSnapshotListener;
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydration = createAppSnapshotHydrator(
      vi.fn<AppSnapshotPort>(() => new Promise((resolve) => {
        resolveHydration = resolve;
      })),
      controller,
    ).hydrate();
    createAppSnapshotSubscription(async (nextListener) => {
      listener = nextListener;
      return vi.fn();
    }, controller).start();
    await Promise.resolve();
    await Promise.resolve();
    const liveAccount = { ...persistedAccount, pendingCount: 9 };

    listener(appSnapshot([liveAccount]));
    resolveHydration(appSnapshot([persistedAccount]));
    await hydration;

    expect(controller.getSnapshot().accounts).toEqual([liveAccount]);
  });

  it("cleans up a listener that finishes registering after unmount", async () => {
    let resolveListen!: (unlisten: () => void) => void;
    const stopListening = vi.fn();
    const listenPort = vi.fn<AppSnapshotListenPort>(() => new Promise((resolve) => {
      resolveListen = resolve;
    }));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const cleanup = createAppSnapshotSubscription(listenPort, controller).start();
    await flushMicrotasks();

    cleanup();
    resolveListen(stopListening);
    await Promise.resolve();
    await Promise.resolve();

    expect(stopListening).toHaveBeenCalledTimes(1);
  });

  it("retries one failed registration and then applies unread events from one listener", async () => {
    vi.useFakeTimers();
    let liveListener!: AppSnapshotListener;
    const stopListening = vi.fn();
    const listenPort = vi.fn<AppSnapshotListenPort>()
      .mockRejectedValueOnce(new Error("temporarily unavailable"))
      .mockImplementationOnce(async (listener) => {
        liveListener = listener;
        return stopListening;
      });
    const refreshedAccount = { ...persistedAccount, inboxUnreadCount: 4 };
    const snapshotPort = vi.fn<AppSnapshotPort>()
      .mockResolvedValueOnce(appSnapshot([persistedAccount]))
      .mockResolvedValue(appSnapshot([refreshedAccount]));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydrator = createAppSnapshotHydrator(snapshotPort, controller);
    const cleanup = createAppSnapshotSubscription(listenPort, controller, {
      retryBaseDelayMs: 10,
      retryMaxDelayMs: 10,
    }).start(() => hydrator.hydrate());

    await vi.advanceTimersByTimeAsync(10);
    await flushMicrotasks();

    expect(listenPort).toHaveBeenCalledTimes(2);
    expect(snapshotPort).toHaveBeenCalledTimes(3);
    expect(controller.getSnapshot().accounts[0].inboxUnreadCount).toBe(4);

    liveListener(appSnapshot([{ ...refreshedAccount, inboxUnreadCount: 7 }]));
    expect(controller.getSnapshot().accounts[0].inboxUnreadCount).toBe(7);

    cleanup();
    expect(stopListening).toHaveBeenCalledTimes(1);
  });

  it("keeps a live unread event over the post-registration refresh", async () => {
    let liveListener!: AppSnapshotListener;
    let resolvePostRegistrationRead!: (snapshot: AppSnapshot) => void;
    const stopListening = vi.fn();
    const listenPort = vi.fn<AppSnapshotListenPort>(async (listener) => {
      liveListener = listener;
      return stopListening;
    });
    const snapshotPort = vi.fn<AppSnapshotPort>()
      .mockResolvedValueOnce(appSnapshot([{ ...persistedAccount, inboxUnreadCount: 2 }]))
      .mockImplementationOnce(() => new Promise((resolve) => {
        resolvePostRegistrationRead = resolve;
      }));
    const controller = createAddAccountController(vi.fn<AddAccountPort>());
    const hydrator = createAppSnapshotHydrator(snapshotPort, controller);
    const cleanup = createAppSnapshotSubscription(listenPort, controller)
      .start(() => hydrator.hydrate());
    await flushMicrotasks(20);
    expect(snapshotPort).toHaveBeenCalledTimes(2);

    liveListener(appSnapshot([{ ...persistedAccount, inboxUnreadCount: 9 }]));
    resolvePostRegistrationRead(appSnapshot([{ ...persistedAccount, inboxUnreadCount: 3 }]));
    await flushMicrotasks();

    expect(controller.getSnapshot().accounts[0].inboxUnreadCount).toBe(9);
    cleanup();
    expect(stopListening).toHaveBeenCalledTimes(1);
  });

  it("bounds repeated registration failures with exponential retry delays", async () => {
    vi.useFakeTimers();
    const listenPort = vi.fn<AppSnapshotListenPort>().mockRejectedValue(new Error("unavailable"));
    const refreshSnapshot = vi.fn().mockResolvedValue(undefined);
    const cleanup = createAppSnapshotSubscription(
      listenPort,
      createAddAccountController(vi.fn<AddAccountPort>()),
      { maxAttempts: 3, retryBaseDelayMs: 10, retryMaxDelayMs: 20 },
    ).start(refreshSnapshot);

    await vi.advanceTimersByTimeAsync(100);
    await flushMicrotasks();

    expect(listenPort).toHaveBeenCalledTimes(3);
    expect(refreshSnapshot).toHaveBeenCalledTimes(3);
    expect(vi.getTimerCount()).toBe(0);
    cleanup();
  });

  it("cancels a scheduled retry on unmount", async () => {
    vi.useFakeTimers();
    const listenPort = vi.fn<AppSnapshotListenPort>().mockRejectedValue(new Error("unavailable"));
    const cleanup = createAppSnapshotSubscription(
      listenPort,
      createAddAccountController(vi.fn<AddAccountPort>()),
      { retryBaseDelayMs: 50, retryMaxDelayMs: 50 },
    ).start();
    await flushMicrotasks();
    expect(listenPort).toHaveBeenCalledTimes(1);

    cleanup();
    await vi.advanceTimersByTimeAsync(100);

    expect(listenPort).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("keeps an immediately added account when startup hydration resolves late", async () => {
    let resolveSnapshot!: (snapshot: AppSnapshot) => void;
    const snapshotPort = vi.fn<AppSnapshotPort>(() => new Promise((resolve) => {
      resolveSnapshot = resolve;
    }));
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: addedAccount }),
    );
    const hydration = createAppSnapshotHydrator(snapshotPort, controller).hydrate();

    await Promise.resolve();
    await controller.addAccount();
    resolveSnapshot(appSnapshot([persistedAccount]));
    await hydration;

    expect(controller.getSnapshot().accounts).toEqual([persistedAccount, addedAccount]);
  });

  it("lets the immediate add result win when a stale hydrated account has the same id", async () => {
    let resolveSnapshot!: (snapshot: AppSnapshot) => void;
    const staleAccount = { ...addedAccount, displayName: "Anterior", pendingCount: 8 };
    const snapshotPort = vi.fn<AppSnapshotPort>(() => new Promise((resolve) => {
      resolveSnapshot = resolve;
    }));
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: addedAccount }),
    );
    const hydration = createAppSnapshotHydrator(snapshotPort, controller).hydrate();

    await Promise.resolve();
    await controller.addAccount();
    resolveSnapshot(appSnapshot([staleAccount]));
    await hydration;

    expect(controller.getSnapshot().accounts).toEqual([addedAccount]);
  });

  it("leaves current UI accounts untouched when hydration fails", async () => {
    const snapshotPort = vi.fn<AppSnapshotPort>().mockRejectedValue(new Error("unavailable"));
    const controller = createAddAccountController(
      vi.fn<AddAccountPort>().mockResolvedValue({ account: addedAccount }),
    );
    await controller.addAccount();

    await createAppSnapshotHydrator(snapshotPort, controller).hydrate();

    expect(controller.getSnapshot().accounts).toEqual([addedAccount]);
  });

  it("drops secret-shaped runtime fields while hydrating", async () => {
    const unsafeSnapshot = appSnapshot([{
      ...persistedAccount,
      refreshToken: "secret-token",
      accessToken: "secret-access",
    } as unknown as MailAccount]);
    const controller = createAddAccountController(vi.fn<AddAccountPort>());

    await createAppSnapshotHydrator(
      vi.fn<AppSnapshotPort>().mockResolvedValue(unsafeSnapshot),
      controller,
    ).hydrate();

    expect(JSON.stringify(controller.getSnapshot())).not.toMatch(/secret|token/i);
    expect(controller.getSnapshot().accounts).toEqual([persistedAccount]);
  });
});
