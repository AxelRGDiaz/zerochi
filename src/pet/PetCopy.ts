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
  inbox_zero: "¡Bandeja en cero! ✨",
  idle: "Aquí estoy.",
};

export const PET_IDLE_COPY_VARIANTS: Readonly<Record<PetMood, readonly string[]>> = {
  satisfied: [
    "Todo tranquilo por aquí ✨",
    "La bandeja está en calma.",
    "Estoy listo para el próximo sobre.",
  ],
  happy: [
    "Hay un sobre esperándonos.",
    "Puedo ayudarte con este correo.",
    "Una visita nueva en la bandeja.",
    "Vamos a revisar lo que llegó.",
  ],
  busy: [
    "Tengo varios sobres para ordenar.",
    "La bandeja está tomando ritmo.",
    "Podemos avanzar correo por correo.",
    "Hay movimiento en el buzón.",
  ],
  tired: [
    "Se juntaron bastantes sobres.",
    "Un correo a la vez estará bien.",
    "La bandeja viene con energía hoy.",
    "Hagamos un poco de espacio juntos.",
  ],
  exhausted: [
    "Hay una buena pila de sobres.",
    "Podemos empezar por el más sencillo.",
    "Esta bandeja pide una pausa y un correo.",
    "Seguimos a nuestro propio ritmo.",
  ],
  overwhelmed: [
    "Hay muchos sobres, pero no hay apuro.",
    "Elijamos solo uno para comenzar.",
    "Aquí sigo, acompañándote con la bandeja.",
    "Podemos ordenar esta montaña paso a paso.",
  ],
};
