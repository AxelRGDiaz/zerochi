import type { AppSnapshot } from "../types/contracts";
import type { AddAccountController } from "./addAccountController";

export type AppSnapshotPort = () => Promise<AppSnapshot>;
export type AppSnapshotListener = (snapshot: AppSnapshot) => void;
export type AppSnapshotListenPort = (listener: AppSnapshotListener) => Promise<() => void>;

export interface AppSnapshotHydrator {
  hydrate(): Promise<void>;
}

export interface AppSnapshotSubscription {
  start(refreshSnapshot?: () => void | Promise<void>): () => void;
}

export interface AppSnapshotSubscriptionOptions {
  maxAttempts?: number;
  retryBaseDelayMs?: number;
  retryMaxDelayMs?: number;
}

export const DEFAULT_SNAPSHOT_LISTEN_MAX_ATTEMPTS = 5;
export const DEFAULT_SNAPSHOT_LISTEN_RETRY_BASE_DELAY_MS = 250;
export const DEFAULT_SNAPSHOT_LISTEN_RETRY_MAX_DELAY_MS = 4_000;

export function snapshotListenRetryDelay(
  failedAttempts: number,
  baseDelayMs = DEFAULT_SNAPSHOT_LISTEN_RETRY_BASE_DELAY_MS,
  maxDelayMs = DEFAULT_SNAPSHOT_LISTEN_RETRY_MAX_DELAY_MS,
): number {
  const exponent = Math.max(0, Math.min(failedAttempts - 1, 30));
  return Math.min(baseDelayMs * (2 ** exponent), maxDelayMs);
}

export function createAppSnapshotHydrator(
  getAppSnapshot: AppSnapshotPort,
  accountController: AddAccountController,
): AppSnapshotHydrator {
  let hydration: Promise<void> | null = null;

  return {
    hydrate() {
      if (hydration) return hydration;

      const expectedLiveRevision = accountController.getLiveSnapshotRevision();
      const attempt = Promise.resolve()
        .then(() => getAppSnapshot())
        .then((snapshot) => accountController.hydrateSnapshot(snapshot, expectedLiveRevision))
        .catch(() => undefined);
      hydration = attempt;
      void attempt.then(() => {
        if (hydration === attempt) hydration = null;
      });

      return attempt;
    },
  };
}

export function createAppSnapshotSubscription(
  listenAppSnapshot: AppSnapshotListenPort,
  accountController: AddAccountController,
  options: AppSnapshotSubscriptionOptions = {},
): AppSnapshotSubscription {
  const maxAttempts = Math.max(1, options.maxAttempts ?? DEFAULT_SNAPSHOT_LISTEN_MAX_ATTEMPTS);
  const retryBaseDelayMs = Math.max(
    0,
    options.retryBaseDelayMs ?? DEFAULT_SNAPSHOT_LISTEN_RETRY_BASE_DELAY_MS,
  );
  const retryMaxDelayMs = Math.max(
    retryBaseDelayMs,
    options.retryMaxDelayMs ?? DEFAULT_SNAPSHOT_LISTEN_RETRY_MAX_DELAY_MS,
  );

  return {
    start(refreshSnapshot) {
      let disposed = false;
      let unlisten: (() => void) | null = null;
      let retryTimer: ReturnType<typeof setTimeout> | null = null;
      let attempts = 0;
      let registering = false;

      const readFreshSnapshot = async () => {
        if (disposed) return;
        try {
          await refreshSnapshot?.();
        } catch {
          // A local snapshot read failure must not stop native listener recovery.
        }
      };

      const scheduleRetry = () => {
        if (disposed || attempts >= maxAttempts || retryTimer !== null) return;
        retryTimer = setTimeout(() => {
          retryTimer = null;
          void register();
        }, snapshotListenRetryDelay(attempts, retryBaseDelayMs, retryMaxDelayMs));
      };

      const register = async (): Promise<void> => {
        if (disposed || registering || unlisten !== null || attempts >= maxAttempts) return;
        registering = true;
        attempts += 1;
        await readFreshSnapshot();
        if (disposed) {
          registering = false;
          return;
        }

        try {
          const stopListening = await listenAppSnapshot((snapshot) => {
            if (!disposed) accountController.applySnapshot(snapshot);
          });
          registering = false;
          if (disposed) {
            stopListening();
            return;
          }
          unlisten = stopListening;
          await readFreshSnapshot();
        } catch {
          registering = false;
          scheduleRetry();
        }
      };

      void register();

      return () => {
        disposed = true;
        if (retryTimer !== null) clearTimeout(retryTimer);
        retryTimer = null;
        unlisten?.();
        unlisten = null;
      };
    },
  };
}
