interface PopupSummaryProps {
  inboxUnreadTotal: number;
  message: string;
}

export function PopupSummary({ inboxUnreadTotal, message }: PopupSummaryProps) {
  const unreadCopy = inboxUnreadTotal === 1
    ? "1 correo sin leer"
    : `${inboxUnreadTotal} correos sin leer`;
  const unreadLabel = inboxUnreadTotal === 1
    ? "1 correo sin leer en total"
    : `${inboxUnreadTotal} correos sin leer en total`;

  return (
    <section className="summary" aria-live="polite">
      <strong aria-label={unreadLabel}>{unreadCopy}</strong>
      <span>{message}</span>
    </section>
  );
}
