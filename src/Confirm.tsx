import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Card {
  id: number;
  preview: string;
}

/** Tarjeta de confirmación. Muestra exactamente lo que se va a hacer; nada corre sin "Confirmar". */
export default function Confirm() {
  const [card, setCard] = useState<Card | null>(null);
  const [sent, setSent] = useState(false);

  useEffect(() => {
    invoke<Card | null>("get_confirmation").then((c) => c && setCard(c)).catch(console.error);
    const unlisten = listen<Card>("confirm", (e) => {
      setCard(e.payload);
      setSent(false);
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  if (!card) return null;

  const answer = (approved: boolean) => {
    if (sent) return;
    setSent(true);
    invoke("resolve_confirmation", { id: card.id, approved }).catch(console.error);
  };

  return (
    <div className="bubble-stage">
      <div className="bubble card" role="dialog" aria-label="Confirmar acción">
        <span className="bubble-label">¿Lo hago?</span>
        <p className="card-preview">{card.preview}</p>
        <div className="card-actions">
          <button className="btn btn-secondary" onClick={() => answer(false)} disabled={sent}>
            Cancelar
          </button>
          <button className="btn btn-primary" onClick={() => answer(true)} disabled={sent} autoFocus>
            Confirmar
          </button>
        </div>
      </div>
    </div>
  );
}
