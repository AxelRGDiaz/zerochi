import type { PetMood } from "./PetState";

export const MOOD_THRESHOLDS: ReadonlyArray<{ minimum: number; mood: PetMood }> = [
  { minimum: 51, mood: "overwhelmed" },
  { minimum: 31, mood: "exhausted" },
  { minimum: 16, mood: "tired" },
  { minimum: 6, mood: "busy" },
  { minimum: 1, mood: "happy" },
  { minimum: 0, mood: "satisfied" },
];
