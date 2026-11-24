import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

type Status = { kind: "idle" } | { kind: "busy"; text: string } | { kind: "ok"; text: string } | { kind: "error"; text: string };

/** Versión instalada y botón para buscar una nueva. Brote igual revisa solo al arrancar y cada 6 horas. */
export default function Updates() {
  const [version, setVersion] = useState("");
  const [status, setStatus] = useState<Status>({ kind: "idle" });

  useEffect(() => {
    invoke<string>("app_version").then(setVersion).catch(() => {});
  }, []);

  async function check() {
    setStatus({ kind: "busy", text: "Buscando… si hay una versión nueva, Brote se instala y se reinicia." });
    try {
      setStatus({ kind: "ok", text: await invoke<string>("check_updates") });
    } catch (e) {
      setStatus({ kind: "error", text: `No pude actualizar: ${e}` });
    }
  }

  return (
    <>
      <h2>Actualizaciones</h2>
      <section>
        <p className="hint">
          Versión {version || "…"}. Brote busca versiones nuevas al arrancar y cada 6 horas, y se instala solo cuando no lo estás
          usando.
        </p>
        <div className="settings-actions">
          <button className="btn btn-secondary" onClick={check} disabled={status.kind === "busy"}>
            Buscar actualizaciones
          </button>
        </div>
        {status.kind !== "idle" && (
          <p className={`status status-${status.kind}`} role="status">
            {status.text}
          </p>
        )}
      </section>
    </>
  );
}
