import { describe, expect, it, vi } from "vitest";
import { createPopupVisibilityStore } from "./popupVisibilityStore";

describe("popup visibility store", () => {
  it("starts active by default and supports an inactive initial state", () => {
    expect(createPopupVisibilityStore().getSnapshot()).toEqual({ isActive: true });
    expect(createPopupVisibilityStore(false).getSnapshot()).toEqual({ isActive: false });
  });

  it("notifies subscribers only when visibility changes", () => {
    const store = createPopupVisibilityStore();
    const listener = vi.fn();
    const unsubscribe = store.subscribe(listener);

    store.show();
    store.hide();
    store.setActive(false);
    store.show();

    expect(listener).toHaveBeenCalledTimes(2);
    expect(store.getSnapshot()).toEqual({ isActive: true });

    unsubscribe();
    store.hide();
    expect(listener).toHaveBeenCalledTimes(2);
  });
});
