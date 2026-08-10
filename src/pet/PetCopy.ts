import type { PetEvent, PetMood } from "./PetState";

export const PET_COPY: Record<PetMood | PetEvent, string> = {
  satisfied: "Estoy satisfecho ✨",
  happy: "Hay algo para comer.",
  busy: "Tengo varios sobres.",
  tired: "Se está acumulando el correo...",
  exhausted: "Necesito ayuda con estos sobres.",
  overwhelmed: "Creo que estoy debajo del correo.",
  new_mail: "¡Llegó un sobre!",
  mail_opened: "Veamos ese correo.",
  mail_eaten: "Delicioso.",
  inbox_reduced: "Mucho mejor.",
  inbox_zero: "¡Inbox Zero! ✨",
  idle: "Aquí estoy.",
};
