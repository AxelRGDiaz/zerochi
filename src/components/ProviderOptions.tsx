export const accountProviders = [
  { id: "gmail", name: "Gmail", enabled: true },
  { id: "outlook", name: "Outlook", enabled: false },
  { id: "icloud", name: "iCloud", enabled: false },
] as const;

export type AccountProvider = typeof accountProviders[number];

export function getProviderAction(
  provider: AccountProvider,
  onAddGmail: () => void,
): (() => void) | undefined {
  return provider.enabled ? onAddGmail : undefined;
}

interface ProviderOptionsProps {
  gmailPending: boolean;
  onAddGmail: () => void;
}

export function ProviderOptions({ gmailPending, onAddGmail }: ProviderOptionsProps) {
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
              aria-label={provider.enabled ? "Agregar cuenta de Gmail" : `${provider.name}, Próximamente`}
              disabled={disabled}
              onClick={getProviderAction(provider, onAddGmail)}
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
