import { MOOD_THRESHOLDS } from "./PetConfig";
import { PET_COPY, PET_IDLE_COPY_VARIANTS } from "./PetCopy";
import type { InboxPetSnapshot, PetAnimation, PetEvent, PetMood, PetPresentation } from "./PetState";

function normalizePending(pendingTotal: number): number {
  return Number.isFinite(pendingTotal) ? Math.max(0, Math.floor(pendingTotal)) : 0;
}

export function moodForUnread(inboxUnreadTotal: number): PetMood {
  const safeUnread = normalizePending(inboxUnreadTotal);
  return MOOD_THRESHOLDS.find(({ minimum }) => safeUnread >= minimum)!.mood;
}

export function idleCopyForUnread(inboxUnreadTotal: number): string {
  const safeUnread = normalizePending(inboxUnreadTotal);
  const threshold = MOOD_THRESHOLDS.find(({ minimum }) => safeUnread >= minimum)!;
  const variants = PET_IDLE_COPY_VARIANTS[threshold.mood];
  return variants[(safeUnread - threshold.minimum) % variants.length];
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
  const mood = moodForUnread(snapshot.inboxUnreadTotal);
  if (snapshot.accounts.length === 0) {
    return { mood, animation: "sleep", message: PET_COPY.idle };
  }
  const animation: PetAnimation = event === "idle" ? `idle_${mood}` : event === "mail_opened" ? "look_mail" : event === "mail_eaten" ? "eat_mail" : event;
  return { mood, animation, message: event === "idle" ? idleCopyForUnread(snapshot.inboxUnreadTotal) : PET_COPY[event] };
}
