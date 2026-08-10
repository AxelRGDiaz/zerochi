import type { PetPresentation } from "./PetState";

export function PetRenderer({ presentation, visible }: { presentation: PetPresentation; visible: boolean }) {
  if (!visible) return null;
  return (
    <section className="pet-stage" aria-label={`Zerochi está ${presentation.mood}`}>
      <div>
        <div className="pet-placeholder" aria-hidden="true">• ᴗ •</div>
        <p className="pet-mood">{presentation.mood}</p>
      </div>
    </section>
  );
}
