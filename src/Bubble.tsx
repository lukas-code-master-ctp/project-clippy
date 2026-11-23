import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type BubbleKind = "heard" | "info" | "reply" | "error";
interface BubblePayload {
  text: string;
  kind: BubbleKind;
}

/** Globo de texto sobre Brote. El núcleo decide qué decir, dónde y cuándo ocultarlo. */
export default function Bubble() {
  const [bubble, setBubble] = useState<BubblePayload | null>(null);

  useEffect(() => {
    // Por si el núcleo mostró un globo antes de que esta página cargara.
    invoke<BubblePayload | null>("get_bubble").then((b) => b && setBubble(b)).catch(console.error);
    const unlisten = listen<BubblePayload>("bubble", (e) => setBubble(e.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  if (!bubble) return null;
  return (
    <div className="bubble-stage">
      <div className={`bubble bubble-${bubble.kind}`}>
        {bubble.kind === "heard" && <span className="bubble-label">Te escuché</span>}
        <p>{bubble.kind === "heard" ? `“${bubble.text}”` : bubble.text}</p>
      </div>
    </div>
  );
}
