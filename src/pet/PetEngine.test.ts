import { describe, expect, it } from "vitest";
import { eventForTransition, idleCopyForUnread, moodForUnread, presentPet } from "./PetEngine";

describe("moodForUnread", () => {
  it.each([
    [0, "satisfied"], [1, "happy"], [5, "happy"], [6, "busy"], [15, "busy"],
    [16, "tired"], [30, "tired"], [31, "exhausted"], [50, "exhausted"], [51, "overwhelmed"],
  ])("maps %i unread to %s", (unread, mood) => expect(moodForUnread(unread)).toBe(mood));

  it.each([Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY, -3])(
    "normalizes invalid count %s to the zero range",
    (unread) => expect(moodForUnread(unread)).toBe("satisfied"),
  );
});

describe("idleCopyForUnread", () => {
  it.each([
    [0, "Todo tranquilo por aquí ✨"],
    [1, "Hay un sobre esperándonos."],
    [5, "Hay un sobre esperándonos."],
    [6, "Tengo varios sobres para ordenar."],
    [15, "La bandeja está tomando ritmo."],
    [16, "Se juntaron bastantes sobres."],
    [30, "La bandeja viene con energía hoy."],
    [31, "Hay una buena pila de sobres."],
    [50, "Seguimos a nuestro propio ritmo."],
    [51, "Hay muchos sobres, pero no hay apuro."],
  ])("selects bounded copy for %i unread", (unread, copy) => {
    expect(idleCopyForUnread(unread)).toBe(copy);
  });

  it("rotates deterministically within a range", () => {
    const firstPass = [6, 7, 8, 9].map(idleCopyForUnread);
    const secondPass = [6, 7, 8, 9].map(idleCopyForUnread);

    expect(new Set(firstPass).size).toBe(4);
    expect(secondPass).toEqual(firstPass);
    expect(idleCopyForUnread(10)).toBe(idleCopyForUnread(6));
  });
});

describe("eventForTransition", () => {
  it.each([
    [5, 4, "inbox_reduced"], [4, 5, "new_mail"], [1, 0, "inbox_zero"], [0, 0, "idle"],
  ])("maps %i → %i to %s", (previous, current, event) => expect(eventForTransition(previous, current)).toBe(event));
});

describe("presentPet", () => {
  it("sleeps when there are no accounts", () => {
    expect(presentPet({ inboxUnreadTotal: 0, pendingTotal: 0, accounts: [] })).toMatchObject({
      mood: "satisfied",
      animation: "sleep",
    });
  });

  it("keeps sleeping without accounts even when an event is requested", () => {
    expect(presentPet({ inboxUnreadTotal: 0, pendingTotal: 0, accounts: [] }, "new_mail").animation).toBe("sleep");
  });

  it("uses the satisfied idle animation when an account exists with no pending mail", () => {
    expect(
      presentPet({
        inboxUnreadTotal: 0,
        pendingTotal: 0,
        accounts: [{ accountId: "account-1", pendingCount: 0 }],
      }),
    ).toMatchObject({
      animation: "idle_satisfied",
      message: "Todo tranquilo por aquí ✨",
    });
  });

  it("keeps event copy independent from idle count rotation", () => {
    expect(
      presentPet(
        { inboxUnreadTotal: 23, pendingTotal: 18, accounts: [{ accountId: "account-1", pendingCount: 18 }] },
        "new_mail",
      ).message,
    ).toBe("¡Llegó un sobre!");
  });

  it("derives ambient presentation from unread while pending stays separate", () => {
    const account = [{ accountId: "account-1", pendingCount: 0 }];
    const withNoPending = presentPet({ inboxUnreadTotal: 16, pendingTotal: 0, accounts: account });
    const withPending = presentPet({ inboxUnreadTotal: 16, pendingTotal: 42, accounts: account });

    expect(withNoPending).toEqual(withPending);
    expect(withNoPending).toMatchObject({
      mood: "tired",
      animation: "idle_tired",
      message: "Se juntaron bastantes sobres.",
    });
  });

  it("changes ambient presentation when unread changes even if pending does not", () => {
    const account = [{ accountId: "account-1", pendingCount: 3 }];

    expect(presentPet({ inboxUnreadTotal: 3, pendingTotal: 3, accounts: account }).mood).toBe("happy");
    expect(presentPet({ inboxUnreadTotal: 31, pendingTotal: 3, accounts: account }).mood).toBe("exhausted");
  });
});
