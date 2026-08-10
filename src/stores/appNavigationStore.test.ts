import { describe, expect, it, vi } from "vitest";
import { createAppNavigationStore } from "./appNavigationStore";

describe("app navigation store", () => {
  it("routes among home, provider selection, and settings", () => {
    const store = createAppNavigationStore();

    expect(store.getSnapshot()).toEqual({ route: "home" });
    store.navigate("add_account");
    expect(store.getSnapshot()).toEqual({ route: "add_account" });
    store.navigate("settings");
    expect(store.getSnapshot()).toEqual({ route: "settings" });
  });

  it("returns secondary views home and ignores duplicate navigation", () => {
    const store = createAppNavigationStore("settings");
    const listener = vi.fn();
    store.subscribe(listener);

    store.navigate("settings");
    store.back();

    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.getSnapshot()).toEqual({ route: "home" });
  });
});
