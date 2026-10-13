export const accountProviders = [
  { id: "gmail", name: "Gmail", enabled: true },
  { id: "outlook", name: "Outlook", enabled: true },
  { id: "icloud", name: "iCloud", enabled: false },
] as const;

export type AccountProvider = typeof accountProviders[number];

export function getProviderAction(
  provider: AccountProvider,
  onAddGmail: () => void,
  onAddOutlook?: () => void,
): (() => void) | undefined {
  if (!provider.enabled) return undefined;
  return provider.id === "outlook" ? onAddOutlook : onAddGmail;
}

interface ProviderOptionsProps {
  /** One add attempt runs at a time, so a pending attempt disables every provider. */
  gmailPending: boolean;
  onAddGmail: () => void;
  onAddOutlook?: () => void;
}

export function ProviderOptions({ gmailPending, onAddGmail, onAddOutlook }: ProviderOptionsProps) {
  return (
    <ul className="provider-list" aria-label="Proveedores de correo">
      {accountProviders.map((provider) => {
        const disabled = !provider.enabled || gmailPending;
        const state = provider.enabled ? (gmailPending ? "Conectando…" : "Disponible") : "Próximamente";

        return (
          <li key={provider.id}>
            <button
              className="provider-button"
              type="button"
              aria-label={provider.enabled ? `Agregar cuenta de ${provider.name}` : `${provider.name}, Próximamente`}
              disabled={disabled}
              onClick={getProviderAction(provider, onAddGmail, onAddOutlook)}
            >
              <span>{provider.name}</span>
              <span className="provider-state">{state}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
