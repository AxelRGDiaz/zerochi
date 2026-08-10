import type { PetAnimation } from "./PetState";

export interface SpriteAnimationDefinition {
  id: PetAnimation;
  row: number;
  startFrame: number;
  frameCount: number;
  fps: number;
  loop: boolean;
}

const idle = (id: PetAnimation, row: number): SpriteAnimationDefinition => ({ id, row, startFrame: 0, frameCount: 1, fps: 2, loop: true });
export const SPRITE_MANIFEST: Record<PetAnimation, SpriteAnimationDefinition> = {
  idle_satisfied: idle("idle_satisfied", 0), idle_happy: idle("idle_happy", 1), idle_busy: idle("idle_busy", 2),
  idle_tired: idle("idle_tired", 3), idle_exhausted: idle("idle_exhausted", 4), idle_overwhelmed: idle("idle_overwhelmed", 5),
  new_mail: { id: "new_mail", row: 6, startFrame: 0, frameCount: 1, fps: 8, loop: false },
  look_mail: { id: "look_mail", row: 7, startFrame: 0, frameCount: 1, fps: 8, loop: false },
  eat_mail: { id: "eat_mail", row: 8, startFrame: 0, frameCount: 1, fps: 10, loop: false },
  inbox_reduced: { id: "inbox_reduced", row: 9, startFrame: 0, frameCount: 1, fps: 8, loop: false },
  inbox_zero: { id: "inbox_zero", row: 10, startFrame: 0, frameCount: 1, fps: 10, loop: false },
  sleep: { id: "sleep", row: 11, startFrame: 0, frameCount: 1, fps: 3, loop: true },
};
