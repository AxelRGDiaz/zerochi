import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PopoverFrame, safePopoverAnchorX } from "./PopoverFrame";

describe("PopoverFrame", () => {
  it("renders one presentation-only wrapper without adding a landmark role", () => {
    const markup = renderToStaticMarkup(<PopoverFrame><main>Contenido</main></PopoverFrame>);

    expect(markup).toBe('<div class="popover-frame"><main>Contenido</main></div>');
    expect(markup).not.toContain("role=");
    expect(markup).not.toContain("--popover-anchor-x");
  });

  it("converts window coordinates to frame coordinates and clamps pointer corners", () => {
    expect(safePopoverAnchorX(195, 16, 358)).toBe(179);
    expect(safePopoverAnchorX(-50, 16, 358)).toBe(28);
    expect(safePopoverAnchorX(900, 16, 358)).toBe(330);
    expect(safePopoverAnchorX(20, 8, 40)).toBe(20);
  });

  it("rejects malformed geometry instead of producing an invalid CSS value", () => {
    expect(safePopoverAnchorX(Number.NaN, 8, 374)).toBeNull();
    expect(safePopoverAnchorX(100, Number.POSITIVE_INFINITY, 374)).toBeNull();
    expect(safePopoverAnchorX(100, 8, 0)).toBeNull();
  });
});
