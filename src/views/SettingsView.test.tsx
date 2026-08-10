import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { focusViewHeading } from "../components/ViewHeading";
import { createAppNavigationStore } from "../stores/appNavigationStore";
import { SettingsView } from "./SettingsView";

describe("SettingsView", () => {
  it("renders an accessible title, back control, and honest read-only information", () => {
    const markup = renderToStaticMarkup(<SettingsView onBack={() => undefined} />);

    expect(markup).toContain('aria-labelledby="settings-title"');
    expect(markup).toContain('<h1 id="settings-title" tabindex="-1">Configuración</h1>');
    expect(markup).toContain('aria-label="Volver al inicio"');
    expect(markup).toContain("Acceso de solo lectura");
    expect(markup).toContain("No marca mensajes como leídos, no archiva y no elimina correo.");
    expect(markup).toContain("todavía no incluye preferencias configurables");
    expect(markup).not.toMatch(/checkbox|select|Guardar/);
  });

  it("navigates to settings, focuses its title, and returns home", () => {
    const navigation = createAppNavigationStore();
    const focus = vi.fn();

    navigation.navigate("settings");
    focusViewHeading({ focus });
    navigation.back();

    expect(focus).toHaveBeenCalledTimes(1);
    expect(navigation.getSnapshot()).toEqual({ route: "home" });
  });
});
