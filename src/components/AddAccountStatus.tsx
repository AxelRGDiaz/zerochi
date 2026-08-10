import type { AddAccountState } from "../stores/addAccountController";

interface AddAccountStatusProps {
  state: AddAccountState;
  onRetry: () => void;
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

export function AddAccountStatus({ state, onRetry }: AddAccountStatusProps) {
  if (state.status === "idle") return null;
  const presentation = statusPresentation[state.status];

  return (
    <div>
      <p className="add-account-status" role={presentation.role}>{presentation.message}</p>
      {state.status === "retryable_error" && (
        <button className="retry-button" type="button" aria-label="Reintentar agregar cuenta" onClick={onRetry}>
          Reintentar
        </button>
      )}
    </div>
  );
}
