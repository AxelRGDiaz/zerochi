import { describe, expect, it } from "vitest";
import { darkPalette } from "./darkPalette";
import { lightPalette } from "./lightPalette";

interface Rgb {
  red: number;
  green: number;
  blue: number;
}

function hexToRgb(hex: string): Rgb {
  const value = Number.parseInt(hex.slice(1), 16);
  return {
    red: value >> 16,
    green: (value >> 8) & 255,
    blue: value & 255,
  };
}

function rgbToHex({ red, green, blue }: Rgb): string {
  return `#${[red, green, blue]
    .map((channel) => Math.round(channel).toString(16).padStart(2, "0"))
    .join("")}`;
}

function composite(foreground: string, background: string, alpha: number): string {
  const foregroundRgb = hexToRgb(foreground);
  const backgroundRgb = hexToRgb(background);
  return rgbToHex({
    red: foregroundRgb.red * alpha + backgroundRgb.red * (1 - alpha),
    green: foregroundRgb.green * alpha + backgroundRgb.green * (1 - alpha),
    blue: foregroundRgb.blue * alpha + backgroundRgb.blue * (1 - alpha),
  });
}

function linearChannel(channel: number): number {
  const normalized = channel / 255;
  return normalized <= 0.04045
    ? normalized / 12.92
    : ((normalized + 0.055) / 1.055) ** 2.4;
}

function luminance(color: string): number {
  const { red, green, blue } = hexToRgb(color);
  return (0.2126 * linearChannel(red))
    + (0.7152 * linearChannel(green))
    + (0.0722 * linearChannel(blue));
}

function contrastRatio(foreground: string, background: string): number {
  const lighter = Math.max(luminance(foreground), luminance(background));
  const darker = Math.min(luminance(foreground), luminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

describe("dark theme contrast", () => {
  const effectivePanelSurface = composite(
    darkPalette.panelLayer,
    darkPalette.surface,
    darkPalette.panelLayerAlpha,
  );

  it("tracks the effective translucent provider/settings panel surface", () => {
    expect(effectivePanelSurface).toBe("#34292d");
  });

  it.each([
    ["app surface", darkPalette.surface],
    ["effective panel surface", effectivePanelSurface],
    ["raised gradient surface", darkPalette.raisedSurface],
  ])("keeps secondary text AA-compliant on the %s", (_name, background) => {
    expect(contrastRatio(darkPalette.secondaryText, background)).toBeGreaterThanOrEqual(4.5);
  });

  it.each([
    ["effective panel surface", effectivePanelSurface],
    ["raised gradient surface", darkPalette.raisedSurface],
  ])("keeps disabled provider text AA-compliant on the %s", (_name, background) => {
    expect(contrastRatio(darkPalette.disabledText, background)).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps primary provider and settings text compliant on the darkest raised surface", () => {
    expect(contrastRatio(darkPalette.primaryText, darkPalette.raisedSurface)).toBeGreaterThanOrEqual(4.5);
  });
});

describe("light theme active provider contrast", () => {
  const effectivePanelOnSurface = composite(
    lightPalette.panelLayer,
    lightPalette.surface,
    lightPalette.panelLayerAlpha,
  );
  const effectivePanelOnRaisedSurface = composite(
    lightPalette.panelLayer,
    lightPalette.raisedSurface,
    lightPalette.panelLayerAlpha,
  );

  it("tracks both effective translucent provider surfaces", () => {
    expect(effectivePanelOnSurface).toBe("#fcf8f2");
    expect(effectivePanelOnRaisedSurface).toBe("#fffbf6");
  });

  it.each([
    ["base shell", effectivePanelOnSurface, 5.4502],
    ["radial highlight", effectivePanelOnRaisedSurface, 5.5967],
  ])("keeps the active Gmail state compliant over the %s panel", (_name, background, expectedRatio) => {
    const ratio = contrastRatio(lightPalette.activeProviderState, background);
    expect(ratio).toBeCloseTo(expectedRatio, 3);
    expect(ratio).toBeGreaterThanOrEqual(4.5);
  });
});
