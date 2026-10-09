import { useSyncExternalStore } from "react";

export const DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS = 5_000;
export const DEFAULT_CHECK_MAIL_NOW_MINIMUM_PENDING_MS = 800;

export type CheckMailNowPort = () => Promise<void>;

export type CheckMailNowState =
  | { status: "idle" }
  | { status: "pending" }
  | { status: "success" }
  | { status: "error" };

type CheckMailNowListener = () => void;

export interface CheckMailNowController {
  getSnapshot(): CheckMailNowState;
  subscribe(listener: CheckMailNowListener): () => void;
  checkMailNow(): Promise<void>;
  reset(): void;
}

export function createCheckMailNowController(
  checkMailNowPort: CheckMailNowPort,
  timeoutMs = DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS,
  minimumPendingMs = DEFAULT_CHECK_MAIL_NOW_MINIMUM_PENDING_MS,
): CheckMailNowController {
  let state: CheckMailNowState = { status: "idle" };
  let inFlight: Promise<void> | null = null;
  let cancelInFlight: (() => void) | null = null;
  let generation = 0;
  const listeners = new Set<CheckMailNowListener>();

  const update = (next: CheckMailNowState) => {
    state = next;
    listeners.forEach((listener) => listener());
  };

  return {
    getSnapshot: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    checkMailNow() {
      if (inFlight) return inFlight;
      update({ status: "pending" });
      const attemptGeneration = generation;
      const pendingStartedAt = Date.now();

      let timeoutId: ReturnType<typeof setTimeout> | null = null;
      let minimumPendingTimerId: ReturnType<typeof setTimeout> | null = null;
      let cancelMinimumPending: (() => void) | null = null;
      let rejectDeadline!: () => void;
      const deadline = new Promise<void>((_, reject) => {
        rejectDeadline = () => reject(new Error("check_mail_now acknowledgement timeout"));
        timeoutId = setTimeout(() => {
          timeoutId = null;
          rejectDeadline();
        }, timeoutMs);
      });
      const cancelDeadline = () => {
        if (timeoutId === null) return;
        clearTimeout(timeoutId);
        timeoutId = null;
        rejectDeadline();
      };
      const waitForMinimumPending = () => {
        const elapsed = Date.now() - pendingStartedAt;
        const remaining = Math.max(0, minimumPendingMs - elapsed);
        if (remaining === 0) return Promise.resolve();

        return new Promise<void>((resolve) => {
          const finishMinimumPending = () => {
            if (minimumPendingTimerId !== null) clearTimeout(minimumPendingTimerId);
            minimumPendingTimerId = null;
            cancelMinimumPending = null;
            resolve();
          };
          cancelMinimumPending = finishMinimumPending;
          minimumPendingTimerId = setTimeout(finishMinimumPending, remaining);
        });
      };
      cancelInFlight = () => {
        cancelDeadline();
        cancelMinimumPending?.();
      };

      const attempt = Promise.race([
        Promise.resolve().then(() => checkMailNowPort()),
        deadline,
      ])
        .then(async () => {
          await waitForMinimumPending();
          if (generation === attemptGeneration) update({ status: "success" });
        })
        .catch(() => {
          if (generation === attemptGeneration) update({ status: "error" });
        })
        .finally(() => {
          if (timeoutId !== null) clearTimeout(timeoutId);
          timeoutId = null;
          if (minimumPendingTimerId !== null) clearTimeout(minimumPendingTimerId);
          minimumPendingTimerId = null;
          cancelMinimumPending = null;
          if (generation === attemptGeneration && inFlight === attempt) {
            inFlight = null;
            cancelInFlight = null;
          }
        });

      inFlight = attempt;
      return attempt;
    },
    reset() {
      if (state.status === "idle" && inFlight === null) return;
      generation += 1;
      const cancelAttempt = cancelInFlight;
      inFlight = null;
      cancelInFlight = null;
      cancelAttempt?.();
      update({ status: "idle" });
    },
  };
}

export function useCheckMailNowController(
  controller: CheckMailNowController,
): CheckMailNowState {
  return useSyncExternalStore(controller.subscribe, controller.getSnapshot, controller.getSnapshot);
}
