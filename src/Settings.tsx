import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import GoogleSettings from "./GoogleSettings";

interface SettingsView {
  model: string;
  has_key: boolean;
}

type Status = { kind: "idle" } | { kind: "busy"; text: string } | { kind: "ok"; text: string } | { kind: "error"; text: string };

const SUGGESTED_MODELS = [
  { id: "anthropic/claude-haiku-4.5", label: "Claude Haiku 4.5 (rápido y barato, recomendado)" },
  { id: "anthropic/claude-sonnet-4.5", label: "Claude Sonnet 4.5 (más capaz, más caro)" },
  { id: "google/gemini-2.5-flash", label: "Gemini 2.5 Flash (muy barato)" },
];

/** Ajustes: la API key va directo al llavero del sistema; la interfaz nunca la vuelve a ver. */
export default function Settings() {
  const [view, setView] = useState<SettingsView | null>(null);
  const [model, setModel] = useState("");
  const [key, setKey] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  useEffect(() => {
    invoke<SettingsView>("get_settings")
      .then((v) => {
        setView(v);
        setModel(v.model);
      })
      .catch((e) => setStatus({ kind: "error", text: String(e) }));
  }, []);

  const save = async (apiKey: string | null) => {
    setStatus({ kind: "busy", text: "Guardando…" });
    try {
      const v = await invoke<SettingsView>("save_settings", { model, apiKey });
      setView(v);
      setKey("");
      if (!v.has_key) {
        setStatus({ kind: "ok", text: "Guardado. Falta la API key." });
        return;
      }
      setStatus({ kind: "busy", text: "Probando la conexión…" });
      const reply = await invoke<string>("test_connection");
      setStatus({ kind: "ok", text: `Todo listo. El modelo respondió: “${reply}”` });
    } catch (e) {
      setStatus({ kind: "error", text: String(e) });
    }
  };

  if (!view) return <main className="settings">Cargando…</main>;

  return (
    <main className="settings">
      <h1>Ajustes de Brote</h1>
      <h2>Modelo de IA (OpenRouter)</h2>

      <section>
        <label htmlFor="key">API key de OpenRouter</label>
        <input
          id="key"
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={view.has_key ? "Guardada. Escribe otra para reemplazarla." : "sk-or-…"}
          value={key}
          onChange={(e) => setKey(e.target.value)}
        />
        <p className="hint">
          Se guarda en el Administrador de credenciales de Windows, no en un archivo.{" "}
          <a href="#" onClick={(e) => (e.preventDefault(), openUrl("https://openrouter.ai/settings/keys"))}>
            Crear una key
          </a>
        </p>
      </section>

      <section>
        <label htmlFor="model">Modelo</label>
        <input id="model" list="models" spellCheck={false} value={model} onChange={(e) => setModel(e.target.value)} />
        <datalist id="models">
          {SUGGESTED_MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </datalist>
        <p className="hint">Cualquier id de modelo de OpenRouter que soporte herramientas.</p>
      </section>

      <div className="settings-actions">
        {view.has_key && (
          <button className="btn btn-secondary" onClick={() => save("")} disabled={status.kind === "busy"}>
            Borrar key
          </button>
        )}
        <button className="btn btn-primary" onClick={() => save(key.trim() ? key : null)} disabled={status.kind === "busy"}>
          Guardar y probar
        </button>
      </div>

      {status.kind !== "idle" && (
        <p className={`status status-${status.kind}`} role="status">
          {status.text}
        </p>
      )}

      <GoogleSettings />

      <h2>Diagnóstico</h2>
      <section>
        <p className="hint">
          Brote guarda un registro de lo que hace (pasos, tiempos y errores) solo en este computador. No incluye tus
          keys, lo que dices ni el contenido de tus correos. Si algo falla, este archivo ayuda a encontrar la causa.
        </p>
        <div className="settings-actions">
          <button className="btn btn-secondary" onClick={() => invoke("open_log_dir").catch(() => {})}>
            Abrir carpeta de logs
          </button>
        </div>
      </section>
    </main>
  );
}
