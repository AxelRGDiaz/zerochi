import type { PetMood, PetPresentation } from "./PetState";

export const PET_FACES: Readonly<Record<PetMood, string>> = {
  satisfied: "• - •",
  happy: "•‿•",
  busy: "•̀ - •́",
  tired: "ᵕ - ᵕ",
  exhausted: "× - ×",
  overwhelmed: "@ - @",
};

export const PET_MOOD_LABELS: Readonly<Record<PetMood, string>> = {
  satisfied: "satisfecho",
  happy: "feliz",
  busy: "ocupado",
  tired: "cansado",
  exhausted: "agotado",
  overwhelmed: "abrumado",
};

export function faceForPresentation(presentation: PetPresentation): string {
  return presentation.animation === "sleep" ? "— - —" : PET_FACES[presentation.mood];
}

export function PetRenderer({ presentation, visible }: { presentation: PetPresentation; visible: boolean }) {
  if (!visible) return null;
  const isSleeping = presentation.animation === "sleep";
  const status = isSleeping ? "durmiendo" : PET_MOOD_LABELS[presentation.mood];
  const face = faceForPresentation(presentation);

  return (
    <section className="pet-stage" aria-label={`Zerochi está ${status}`}>
      <div>
        {isSleeping ? (
          <div style={{ position: "relative", width: 112, height: 112 }} aria-hidden="true">
            <div className="pet-placeholder">{face}</div>
            <span
              className="pet-sleep-marks"
              style={{ position: "absolute", top: 2, right: -30, color: "#593f46", font: "700 20px/1 monospace" }}
            >
              zZ
            </span>
          </div>
        ) : (
          <div className="pet-placeholder" aria-hidden="true">{face}</div>
        )}
        <p className="pet-mood">{status}</p>
      </div>
    </section>
  );
}
