import type { AddAccountState } from "../stores/addAccountController";

interface PopupActionsProps {
  addAccountState: AddAccountState;
  onAddAccount: () => void;
  onRetryAddAccount: () => void;
}

const statusPresentation: Record<Exclude<AddAccountState["status"], "idle">, {
  message: string;
  role: "alert" | "status";
}> = {
  pending: { message: "Abriendo Google para conectar tu cuenta…", role: "status" },
  success: { message: "Cuenta agregada correctamente.", role: "status" },
  cancelled: { message: "Conexión cancelada. No se agregó ninguna cuenta.", role: "status" },
  retryable_error: {
    message: "No pudimos agregar la cuenta. Puedes intentarlo nuevamente.",
    role: "alert",
  },
  action_required: {
    message: "Se necesita una acción para terminar de agregar la cuenta.",
    role: "alert",
  },
  error: { message: "No pudimos agregar la cuenta.", role: "alert" },
};

export function PopupActions({ addAccountState, onAddAccount, onRetryAddAccount }: PopupActionsProps) {
  const isPending = addAccountState.status === "pending";
  const status = addAccountState.status === "idle" ? null : statusPresentation[addAccountState.status];

  return (
    <footer className="app-footer">
      <div className="add-account-control">
        <button type="button" aria-label="Agregar cuenta" disabled={isPending} onClick={onAddAccount}>
          {isPending ? "Agregando cuenta…" : "＋ Agregar cuenta"}
        </button>
        {status && <p className="add-account-status" role={status.role}>{status.message}</p>}
        {addAccountState.status === "retryable_error" && (
          <button type="button" aria-label="Reintentar agregar cuenta" onClick={onRetryAddAccount}>
            Reintentar
          </button>
        )}
      </div>
      <button type="button" aria-label="Abrir configuración">⚙︎</button>
    </footer>
  );
}
