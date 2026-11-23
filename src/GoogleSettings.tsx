import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

interface GoogleStatus {
  configured: boolean;
  connected: boolean;
  email: string;
  client_id: string;
}

const GUIDE_URL = "https://github.com/lukas-code-master-ctp/project-clippy/blob/main/docs/configurar-google.md";

/** Sección de Google en Ajustes: credenciales de Google Cloud y conexión de la cuenta. */
export default function GoogleSettings() {
  const [status, setStatus] = useState<GoogleStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [secret, setSecret] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<GoogleStatus>("google_status")
      .then((s) => {
        setStatus(s);
        setClientId(s.client_id);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const run = async (label: string, action: () => Promise<GoogleStatus>) => {
    setBusy(label);
    setError(null);
    try {
      setStatus(await action());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  if (!status) return null;

  const saveClient = () =>
    run("Guardando…", async () => {
      const s = await invoke<GoogleStatus>("google_save_client", { clientId, clientSecret: secret || null });
      setSecret("");
      return s;
    });

  return (
    <section className="google">
      <h2>Google (Gmail y Calendar)</h2>

      {status.connected ? (
        <>
          <p className="connected">Conectado{status.email ? ` como ${status.email}` : ""}.</p>
          <div className="settings-actions">
            <button className="btn btn-secondary" disabled={!!busy} onClick={() => run("Desconectando…", () => invoke("google_disconnect"))}>
              Desconectar
            </button>
          </div>
        </>
      ) : (
        <>
          <p className="hint">
            Necesitas un proyecto en Google Cloud (una sola vez, unos 10 minutos).{" "}
            <a href="#" onClick={(e) => (e.preventDefault(), openUrl(GUIDE_URL))}>
              Ver la guía paso a paso
            </a>
          </p>
          <label htmlFor="gid">Client ID</label>
          <input id="gid" spellCheck={false} placeholder="…apps.googleusercontent.com" value={clientId} onChange={(e) => setClientId(e.target.value)} />
          <label htmlFor="gsecret">Client secret</label>
          <input
            id="gsecret"
            type="password"
            autoComplete="off"
            spellCheck={false}
            placeholder={status.configured ? "Guardado. Escribe otro para reemplazarlo." : "GOCSPX-…"}
            value={secret}
            onChange={(e) => setSecret(e.target.value)}
          />
          <div className="settings-actions">
            <button className="btn btn-secondary" disabled={!!busy} onClick={saveClient}>
              Guardar credenciales
            </button>
            <button
              className="btn btn-primary"
              disabled={!!busy || !status.configured}
              onClick={() => run("Esperando que aceptes en el navegador…", () => invoke("google_connect"))}
            >
              Conectar Google
            </button>
          </div>
        </>
      )}

      {busy && <p className="status status-busy">{busy}</p>}
      {error && <p className="status status-error">{error}</p>}
    </section>
  );
}
