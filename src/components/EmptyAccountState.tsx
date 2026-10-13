interface EmptyAccountStateProps {
  onOpenAddAccount: () => void;
}

export function EmptyAccountState({ onOpenAddAccount }: EmptyAccountStateProps) {
  return (
    <section className="empty-account-state" aria-labelledby="empty-account-title">
      <h2 id="empty-account-title">Todavía no hay una cuenta conectada</h2>
      <p>Agrega tu cuenta de Gmail u Outlook para que Zerochi pueda acompañarte con el correo nuevo.</p>
      <button className="empty-account-action" type="button" onClick={onOpenAddAccount}>
        Elegir proveedor
      </button>
    </section>
  );
}
