import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";

/** Caja para escribirle una orden a Brote (doble clic sobre él). Enter envía, Esc cierra. */
export default function Ask() {
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const focus = () => {
      setText("");
      // La ventana recién mostrada tarda un instante en recibir el foco.
      window.setTimeout(() => inputRef.current?.focus(), 30);
    };
    focus();
    const unlisten = listen("ask-open", focus);
    const onBlur = () => invoke("close_ask").catch(console.error);
    window.addEventListener("blur", onBlur);
    return () => {
      unlisten.then((off) => off());
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    const value = text.trim();
    if (!value) return;
    setText("");
    invoke("submit_text", { text: value }).catch(console.error);
  };

  return (
    <div className="ask-stage">
      <form className="ask-form" onSubmit={submit}>
        <input
          ref={inputRef}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && invoke("close_ask").catch(console.error)}
          placeholder="Escríbele a Brote…"
          aria-label="Orden para Brote"
          maxLength={500}
          spellCheck
        />
        <button type="submit" disabled={!text.trim()}>
          Enviar
        </button>
      </form>
    </div>
  );
}
