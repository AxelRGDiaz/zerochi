export type PetMood = "satisfied" | "happy" | "busy" | "tired" | "exhausted" | "overwhelmed";
export type PetEvent = "new_mail" | "mail_opened" | "mail_eaten" | "inbox_reduced" | "inbox_zero" | "idle";
export type PetAnimation = `idle_${PetMood}` | "new_mail" | "look_mail" | "eat_mail" | "inbox_reduced" | "inbox_zero" | "sleep";

export interface InboxPetSnapshot {
  inboxUnreadTotal: number;
  pendingTotal: number;
  accounts: Array<{ accountId: string; pendingCount: number }>;
}

export interface PetPresentation {
  mood: PetMood;
  animation: PetAnimation;
  message: string;
}
