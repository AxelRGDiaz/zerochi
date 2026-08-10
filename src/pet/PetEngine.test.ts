import { describe, expect, it } from "vitest";
import { eventForTransition, moodForPending } from "./PetEngine";

describe("moodForPending", () => {
  it.each([
    [0, "satisfied"], [1, "happy"], [5, "happy"], [6, "busy"], [15, "busy"],
    [16, "tired"], [30, "tired"], [31, "exhausted"], [50, "exhausted"], [51, "overwhelmed"],
  ])("maps %i pending to %s", (pending, mood) => expect(moodForPending(pending)).toBe(mood));
});

describe("eventForTransition", () => {
  it.each([
    [5, 4, "inbox_reduced"], [4, 5, "new_mail"], [1, 0, "inbox_zero"], [0, 0, "idle"],
  ])("maps %i → %i to %s", (previous, current, event) => expect(eventForTransition(previous, current)).toBe(event));
});
