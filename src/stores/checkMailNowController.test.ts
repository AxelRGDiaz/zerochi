import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createCheckMailNowController,
  DEFAULT_CHECK_MAIL_NOW_MINIMUM_PENDING_MS,
  DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS,
  type CheckMailNowPort,
} from "./checkMailNowController";

function deferred() {
  let resolve!: () => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<void>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

afterEach(() => vi.useRealTimers());

describe("check mail now controller", () => {
  it("deduplicates activation while the scheduler request is pending", async () => {
    const request = deferred();
    const port = vi.fn<CheckMailNowPort>().mockReturnValue(request.promise);
    const controller = createCheckMailNowController(port, DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS, 0);

    const first = controller.checkMailNow();
    const duplicate = controller.checkMailNow();
    await Promise.resolve();

    expect(duplicate).toBe(first);
    expect(port).toHaveBeenCalledTimes(1);
    expect(port).toHaveBeenCalledWith();
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    request.resolve();
    await first;
    expect(controller.getSnapshot()).toEqual({ status: "success" });
  });

  it("retries after success", async () => {
    const port = vi.fn<CheckMailNowPort>().mockResolvedValue(undefined);
    const controller = createCheckMailNowController(port, DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS, 0);

    await controller.checkMailNow();
    await controller.checkMailNow();

    expect(port).toHaveBeenCalledTimes(2);
    expect(controller.getSnapshot()).toEqual({ status: "success" });
  });

  it("discards command failure text and permits a retry", async () => {
    const port = vi.fn<CheckMailNowPort>()
      .mockRejectedValueOnce({ message: "provider user@example.com token=secret" })
      .mockResolvedValueOnce(undefined);
    const controller = createCheckMailNowController(port, DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS, 0);

    await controller.checkMailNow();
    expect(controller.getSnapshot()).toEqual({ status: "error" });
    expect(JSON.stringify(controller.getSnapshot())).not.toMatch(/provider|example|token|secret/i);

    await controller.checkMailNow();
    expect(controller.getSnapshot()).toEqual({ status: "success" });
  });

  it("times out a missing command acknowledgement after the five-second default", async () => {
    vi.useFakeTimers();
    const controller = createCheckMailNowController(
      vi.fn<CheckMailNowPort>(() => new Promise(() => undefined)),
      DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS,
      0,
    );

    const attempt = controller.checkMailNow();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS - 1);
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    await vi.advanceTimersByTimeAsync(1);
    await attempt;

    expect(controller.getSnapshot()).toEqual({ status: "error" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("uses an injected timeout and permits retry after expiry", async () => {
    vi.useFakeTimers();
    const port = vi.fn<CheckMailNowPort>()
      .mockImplementationOnce(() => new Promise(() => undefined))
      .mockResolvedValueOnce(undefined);
    const controller = createCheckMailNowController(port, 25, 0);

    const expiredAttempt = controller.checkMailNow();
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(25);
    await expiredAttempt;
    expect(controller.getSnapshot()).toEqual({ status: "error" });

    await controller.checkMailNow();

    expect(port).toHaveBeenCalledTimes(2);
    expect(controller.getSnapshot()).toEqual({ status: "success" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("reset cancels the logical timeout and a fresh attempt remains isolated", async () => {
    vi.useFakeTimers();
    const port = vi.fn<CheckMailNowPort>()
      .mockImplementationOnce(() => new Promise(() => undefined))
      .mockResolvedValueOnce(undefined);
    const controller = createCheckMailNowController(port, 25, 0);

    const cancelledAttempt = controller.checkMailNow();
    await Promise.resolve();
    expect(vi.getTimerCount()).toBe(1);

    controller.reset();
    await cancelledAttempt;
    expect(controller.getSnapshot()).toEqual({ status: "idle" });
    expect(vi.getTimerCount()).toBe(0);

    await controller.checkMailNow();
    await vi.advanceTimersByTimeAsync(25);

    expect(controller.getSnapshot()).toEqual({ status: "success" });
    expect(port).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("resets pending state when leaving settings and ignores its late result", async () => {
    const firstRequest = deferred();
    const secondRequest = deferred();
    const port = vi.fn<CheckMailNowPort>()
      .mockReturnValueOnce(firstRequest.promise)
      .mockReturnValueOnce(secondRequest.promise);
    const controller = createCheckMailNowController(port, DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS, 0);

    const first = controller.checkMailNow();
    await Promise.resolve();
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    controller.reset();
    expect(controller.getSnapshot()).toEqual({ status: "idle" });

    const second = controller.checkMailNow();
    await Promise.resolve();
    expect(port).toHaveBeenCalledTimes(2);
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    firstRequest.resolve();
    await first;
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    secondRequest.resolve();
    await second;
    expect(controller.getSnapshot()).toEqual({ status: "success" });
  });

  it.each(["success", "error"] as const)("resets %s feedback before the next settings visit", async (status) => {
    const port = status === "success"
      ? vi.fn<CheckMailNowPort>().mockResolvedValue(undefined)
      : vi.fn<CheckMailNowPort>().mockRejectedValue(new Error("unavailable"));
    const controller = createCheckMailNowController(port, DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS, 0);
    const listener = vi.fn();
    controller.subscribe(listener);
    await controller.checkMailNow();
    listener.mockClear();

    controller.reset();

    expect(controller.getSnapshot()).toEqual({ status: "idle" });
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("keeps a fast successful acknowledgement pending for the visible default minimum", async () => {
    vi.useFakeTimers();
    const controller = createCheckMailNowController(
      vi.fn<CheckMailNowPort>().mockResolvedValue(undefined),
    );

    const attempt = controller.checkMailNow();
    await vi.advanceTimersByTimeAsync(DEFAULT_CHECK_MAIL_NOW_MINIMUM_PENDING_MS - 1);
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    await vi.advanceTimersByTimeAsync(1);
    await attempt;

    expect(controller.getSnapshot()).toEqual({ status: "success" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("counts command latency toward an injected visible minimum", async () => {
    vi.useFakeTimers();
    const request = deferred();
    const controller = createCheckMailNowController(
      vi.fn<CheckMailNowPort>().mockReturnValue(request.promise),
      DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS,
      800,
    );

    const attempt = controller.checkMailNow();
    await vi.advanceTimersByTimeAsync(600);
    request.resolve();
    await vi.advanceTimersByTimeAsync(199);
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    await vi.advanceTimersByTimeAsync(1);
    await attempt;

    expect(controller.getSnapshot()).toEqual({ status: "success" });
  });

  it("shows command errors immediately without waiting for the visible minimum", async () => {
    vi.useFakeTimers();
    const controller = createCheckMailNowController(
      vi.fn<CheckMailNowPort>().mockRejectedValue(new Error("unavailable")),
      DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS,
      800,
    );

    const attempt = controller.checkMailNow();
    await vi.advanceTimersByTimeAsync(0);
    await attempt;

    expect(controller.getSnapshot()).toEqual({ status: "error" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("reset cancels the visible minimum and isolates a subsequent retry", async () => {
    vi.useFakeTimers();
    const port = vi.fn<CheckMailNowPort>()
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce(undefined);
    const controller = createCheckMailNowController(
      port,
      DEFAULT_CHECK_MAIL_NOW_TIMEOUT_MS,
      800,
    );

    const cancelledAttempt = controller.checkMailNow();
    await vi.advanceTimersByTimeAsync(100);
    expect(controller.getSnapshot()).toEqual({ status: "pending" });

    controller.reset();
    await cancelledAttempt;
    expect(controller.getSnapshot()).toEqual({ status: "idle" });
    expect(vi.getTimerCount()).toBe(0);

    const retry = controller.checkMailNow();
    await vi.advanceTimersByTimeAsync(799);
    expect(controller.getSnapshot()).toEqual({ status: "pending" });
    await vi.advanceTimersByTimeAsync(1);
    await retry;

    expect(controller.getSnapshot()).toEqual({ status: "success" });
    expect(port).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(0);
  });
});
