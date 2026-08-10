import { SubViewHeader } from "../components/SubViewHeader";

interface SettingsViewProps {
  onBack: () => void;
}

export function SettingsView({ onBack }: SettingsViewProps) {
  return (
    <main className="app-shell" aria-labelledby="settings-title">
      <SubViewHeader onBack={onBack} title="Configuración" titleId="settings-title" />
      <div className="settings-copy">
        <section aria-labelledby="access-title">
          <h2 id="access-title">Acceso de solo lectura</h2>
          <p>Zerochi consulta Gmail para detectar correo nuevo. No marca mensajes como leídos, no archiva y no elimina correo.</p>
        </section>
        <section aria-labelledby="preferences-title">
          <h2 id="preferences-title">Preferencias</h2>
          <p>Esta versión todavía no incluye preferencias configurables.</p>
        </section>
      </div>
    </main>
  );
}
