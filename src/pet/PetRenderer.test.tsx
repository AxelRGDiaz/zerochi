import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { PetPresentation } from "./PetState";
import { faceForPresentation, PET_FACES, PET_MOOD_LABELS, PetRenderer } from "./PetRenderer";

function renderPet(presentation: PetPresentation) {
  return renderToStaticMarkup(<PetRenderer presentation={presentation} visible />);
}

describe("PetRenderer", () => {
  it("renders and announces a sleeping pet", () => {
    const markup = renderPet({
      animation: "sleep",
      message: "Aquí estoy.",
      mood: "satisfied",
    });

    expect(markup).toContain('aria-label="Zerochi está durmiendo"');
    expect(markup).toContain('<div class="pet-placeholder">— - —</div>');
    expect(markup).toContain('<span class="pet-sleep-marks"');
    expect(markup).toContain(">zZ</span>");
    expect(markup).not.toContain('class="pet-placeholder">— - — zZ');
    expect(markup).toContain('<p class="pet-mood">durmiendo</p>');
  });

  it("keeps the awake presentation for other animations", () => {
    const markup = renderPet({
      animation: "idle_happy",
      message: "Hay algo para comer.",
      mood: "happy",
    });

    expect(markup).toContain('aria-label="Zerochi está feliz"');
    expect(markup).toContain('<p class="pet-mood">feliz</p>');
    expect(markup).toContain("•‿•");
    expect(markup).not.toContain("zZ");
  });

  it.each([
    ["satisfied", "• - •"],
    ["happy", "•‿•"],
    ["busy", "•̀ - •́"],
    ["tired", "ᵕ - ᵕ"],
    ["exhausted", "× - ×"],
    ["overwhelmed", "@ - @"],
  ] as const)("uses the %s face without a smiling mouth", (mood, face) => {
    const presentation: PetPresentation = {
      animation: `idle_${mood}`,
      message: "Estado",
      mood,
    };

    expect(PET_FACES[mood]).toBe(face);
    expect(faceForPresentation(presentation)).toBe(face);
    expect(renderPet(presentation)).toContain(face);
  });

  it.each([
    ["satisfied", "satisfecho"],
    ["happy", "feliz"],
    ["busy", "ocupado"],
    ["tired", "cansado"],
    ["exhausted", "agotado"],
    ["overwhelmed", "abrumado"],
  ] as const)("renders the %s mood in Spanish", (mood, label) => {
    const markup = renderPet({
      animation: `idle_${mood}`,
      message: "Estado",
      mood,
    });

    expect(PET_MOOD_LABELS[mood]).toBe(label);
    expect(markup).toContain(`aria-label="Zerochi está ${label}"`);
    expect(markup).toContain(`<p class="pet-mood">${label}</p>`);
  });

  it("keeps sleep marks outside the sleeping face", () => {
    const presentation: PetPresentation = {
      animation: "sleep",
      message: "Aquí estoy.",
      mood: "satisfied",
    };

    expect(faceForPresentation(presentation)).toBe("— - —");
    expect(faceForPresentation(presentation)).not.toContain("zZ");
  });
});
