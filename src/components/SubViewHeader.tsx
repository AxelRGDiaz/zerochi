import { ViewHeading } from "./ViewHeading";

interface SubViewHeaderProps {
  onBack: () => void;
  title: string;
  titleId: string;
}

export function SubViewHeader({ onBack, title, titleId }: SubViewHeaderProps) {
  return (
    <header className="subview-header">
      <button className="back-button" type="button" aria-label="Volver al inicio" onClick={onBack}>← Volver</button>
      <ViewHeading id={titleId}>{title}</ViewHeading>
      <span aria-hidden="true" />
    </header>
  );
}
