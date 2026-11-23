import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface CardField {
  key: string;
  label: string;
  value: string;
  editable: boolean;
  multiline: boolean;
}

interface Payload {
  id: number;
  card: { title: string; fields: CardField[] };
}

/**
 * Tarjeta de confirmación. Muestra exactamente lo que se va a hacer; nada corre sin "Confirmar".
 * Los campos editables se pueden corregir, y se ejecuta la versión corregida.
 */
export default function Confirm() {
  const [payload, setPayload] = useState<Payload | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [sent, setSent] = useState(false);

  const load = (p: Payload) => {
    setPayload(p);
    setValues(Object.fromEntries(p.card.fields.map((f) => [f.key, f.value])));
    setSent(false);
  };

  useEffect(() => {
    invoke<Payload | null>("get_confirmation").then((p) => p && load(p)).catch(console.error);
    const unlisten = listen<Payload>("confirm", (e) => load(e.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  if (!payload) return null;
  const { card } = payload;

  const answer = (approved: boolean) => {
    if (sent) return;
    setSent(true);
    const edits = Object.fromEntries(
      card.fields.filter((f) => f.editable && values[f.key] !== f.value).map((f) => [f.key, values[f.key]]),
    );
    invoke("resolve_confirmation", { id: payload.id, approved, edits }).catch(console.error);
  };

  const edited = card.fields.some((f) => f.editable && values[f.key] !== f.value);

  return (
    <div className="bubble-stage">
      <div className="bubble card" role="dialog" aria-label={card.title}>
        <span className="bubble-label">¿Lo hago?</span>
        <h2 className="card-title">{card.title}</h2>
        <div className="card-fields">
          {card.fields.map((f) => (
            <label key={f.key} className="card-field">
              <span>{f.label}</span>
              {!f.editable ? (
                <div className="card-value">{f.value}</div>
              ) : f.multiline ? (
                <textarea
                  rows={5}
                  value={values[f.key] ?? ""}
                  disabled={sent}
                  onChange={(e) => setValues({ ...values, [f.key]: e.target.value })}
                />
              ) : (
                <input
                  value={values[f.key] ?? ""}
                  disabled={sent}
                  onChange={(e) => setValues({ ...values, [f.key]: e.target.value })}
                />
              )}
            </label>
          ))}
        </div>
        <div className="card-actions">
          {edited && <span className="card-edited">Con tus cambios</span>}
          <button className="btn btn-secondary" onClick={() => answer(false)} disabled={sent}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={() => answer(true)} disabled={sent}>
            Confirmar
          </button>
        </div>
      </div>
    </div>
  );
}
