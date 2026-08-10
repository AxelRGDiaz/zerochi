import { MOOD_THRESHOLDS } from "./PetConfig";
import { PET_COPY } from "./PetCopy";
import type { InboxPetSnapshot, PetAnimation, PetEvent, PetMood, PetPresentation } from "./PetState";

export function moodForPending(pendingTotal: number): PetMood {
  const safePending = Math.max(0, Math.floor(pendingTotal));
  return MOOD_THRESHOLDS.find(({ minimum }) => safePending >= minimum)!.mood;
}

export function eventForTransition(previous: number, current: number): PetEvent {
  const before = Math.max(0, previous);
  const after = Math.max(0, current);
  if (before > 0 && after === 0) return "inbox_zero";
  if (after > before) return "new_mail";
  if (after < before) return "inbox_reduced";
  return "idle";
}

export function presentPet(snapshot: InboxPetSnapshot, event: PetEvent = "idle"): PetPresentation {
  const mood = moodForPending(snapshot.pendingTotal);
  const animation: PetAnimation = event === "idle" ? `idle_${mood}` : event === "mail_opened" ? "look_mail" : event === "mail_eaten" ? "eat_mail" : event;
  return { mood, animation, message: event === "idle" ? PET_COPY[mood] : PET_COPY[event] };
}
