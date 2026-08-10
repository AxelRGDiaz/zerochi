interface PopupSummaryProps {
  message: string;
  pendingTotal: number;
}

export function PopupSummary({ message, pendingTotal }: PopupSummaryProps) {
  return (
    <section className="summary" aria-live="polite">
      <strong>Tengo {pendingTotal} mails</strong>
      <span>{message}</span>
    </section>
  );
}
