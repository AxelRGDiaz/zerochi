import type { AddAccountState } from "../stores/addAccountController";
import type { AddAccountErrorTag } from "../types/contracts";
import type { MailProviderId } from "../types/mail";

interface AddAccountStatusProps {
  state: AddAccountState;
  provider?: MailProviderId;
  onRetry: () => void;
}

interface StatusPresentation {
  message: string;
  detail?: string;
  role: "alert" | "status";
}

const lifecyclePresentation: Record<"pending" | "success", StatusPresentation> = {
  pending: {
    message: "Completa el acceso en Google.",
    detail: "Cuando termines, Zerochi volverá a mostrarse y agregará tu cuenta.",
    role: "status",
  },
  success: { message: "Cuenta agregada correctamente.", role: "status" },
};

const errorPresentation: Record<AddAccountErrorTag, Omit<StatusPresentation, "role">> = {
  auth_in_progress: {
    message: "Ya hay una conexión con Google en curso.",
    detail: "Completa esa ventana antes de iniciar otra conexión.",
  },
  browser_open_failed: {
    message: "No pudimos abrir Google en tu navegador.",
    detail: "Comprueba que tengas un navegador disponible e inténtalo nuevamente.",
  },
  callback_invalid: {
    message: "No pudimos validar la respuesta de Google.",
    detail: "Vuelve a iniciar la conexión para recibir una respuesta nueva y segura.",
  },
  cancelled: {
    message: "Conexión cancelada.",
    detail: "No se agregó ninguna cuenta.",
  },
  config_invalid: {
    message: "Zerochi no está configurado para conectarse con Google.",
    detail: "Revisa la configuración de OAuth de esta instalación.",
  },
  identity_invalid: {
    message: "No pudimos verificar la identidad de la cuenta.",
    detail: "Inicia la conexión nuevamente y elige una cuenta de Google válida.",
  },
  internal: {
    message: "Zerochi no pudo completar la conexión.",
    detail: "Inténtalo nuevamente.",
  },
  network_unavailable: {
    message: "No pudimos comunicarnos con Google.",
    detail: "Revisa tu conexión a internet e inténtalo nuevamente.",
  },
  provider_denied: {
    message: "Google no autorizó el acceso.",
    detail: "Para agregar la cuenta, acepta el permiso de solo lectura solicitado por Zerochi.",
  },
  refresh_token_missing: {
    message: "Google no concedió acceso continuo a la cuenta.",
    detail: "Revoca el acceso anterior de Zerochi en Google y vuelve a conectar la cuenta.",
  },
  secure_store_failed: {
    message: "No pudimos guardar la cuenta de forma segura.",
    detail: "Verifica que el llavero de macOS esté disponible e inténtalo nuevamente.",
  },
  timeout: {
    message: "Se agotó el tiempo para completar el acceso.",
    detail: "Inicia nuevamente la conexión y termina el consentimiento en Google.",
  },
  token_exchange_failed: {
    message: "No pudimos finalizar el acceso con Google.",
    detail: "Cierra la pestaña anterior y vuelve a iniciar la conexión desde Zerochi.",
  },
  workspace_restricted: {
    message: "Tu organización de Google bloqueó el acceso.",
    detail: "Solicita autorización a tu administrador de Workspace antes de volver a intentarlo.",
  },
};

function presentationFor(state: Exclude<AddAccountState, { status: "idle" }>): StatusPresentation {
  if (state.status === "pending" || state.status === "success") {
    return lifecyclePresentation[state.status];
  }
  if ("tag" in state) {
    return {
      ...errorPresentation[state.tag],
      role: state.tag === "cancelled" ? "status" : "alert",
    };
  }
  return { message: "No pudimos agregar la cuenta.", role: "alert" };
}

// The copy is written for Google; the sign-in company is the only provider-specific part.
function forProvider(text: string, provider: MailProviderId): string {
  return provider === "outlook" ? text.split("Google").join("Microsoft") : text;
}

export function AddAccountStatus({ state, provider = "gmail", onRetry }: AddAccountStatusProps) {
  if (state.status === "idle") return null;
  const presentation = presentationFor(state);
  const canRetry = "retryable" in state && state.retryable;

  return (
    <div
      className="add-account-feedback"
      role={presentation.role}
      aria-live={presentation.role === "alert" ? "assertive" : "polite"}
    >
      <p className="add-account-status">{forProvider(presentation.message, provider)}</p>
      {presentation.detail && (
        <p className="add-account-detail">{forProvider(presentation.detail, provider)}</p>
      )}
      {canRetry && (
        <button className="retry-button" type="button" aria-label="Reintentar agregar cuenta" onClick={onRetry}>
          Reintentar
        </button>
      )}
    </div>
  );
}
