interface PopupActionsProps {
  onOpenAddAccount: () => void;
  onOpenSettings: () => void;
}

export function PopupActions({ onOpenAddAccount, onOpenSettings }: PopupActionsProps) {
  return (
    <footer className="app-footer">
      <button
        className="add-account-button"
        type="button"
        aria-label="Agregar cuenta"
        onClick={onOpenAddAccount}
        style={{ minHeight: 44 }}
      >
        ＋ Agregar cuenta
      </button>
      <button
        className="settings-button"
        type="button"
        aria-label="Abrir configuración"
        onClick={onOpenSettings}
        style={{ minHeight: 44, minWidth: 44 }}
      >
        <span className="settings-icon" aria-hidden="true" style={{ fontSize: 23 }}>⚙︎</span>
      </button>
    </footer>
  );
}
