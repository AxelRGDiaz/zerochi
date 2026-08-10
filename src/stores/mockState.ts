import type { MailAccount } from "../types/mail";

export const mockAccounts: MailAccount[] = [
  { id: "personal", email: "personal@gmail.com", displayName: "Personal", pendingCount: 2, active: true },
  { id: "work", email: "trabajo@empresa.cl", displayName: "Trabajo", pendingCount: 5, active: true },
  { id: "support", email: "soporte@empresa.cl", displayName: "Soporte", pendingCount: 1, active: true },
];
