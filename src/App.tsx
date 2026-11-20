import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import broteSvg from "./brote.svg?raw";
import { PET_STATES, STATE_LABELS, type PetState } from "./state";

const DRAG_THRESHOLD_PX = 4;

export default function App() {
  const hostRef = useRef<HTMLDivElement>(null);
  const [state, setState] = useState<PetState>("sleeping");

  // El núcleo en Rust es la fuente de verdad del estado: la UI solo lo refleja.
  useEffect(() => {
    invoke<PetState>("get_state").then(setState).catch(console.error);
    const unlisten = listen<PetState>("pet-state", (e) => setState(e.payload));
    return () => {
      unlisten.then((off) => off());
    };
  }, []);

  // Aplica la clase s-<estado> al <svg> de Brote.
  useEffect(() => {
    const svg = hostRef.current?.querySelector("svg");
    if (!svg) return;
    PET_STATES.forEach((s) => svg.classList.remove(`s-${s}`));
    svg.classList.add(`s-${state}`);
  }, [state]);

  // Arrastrar mueve la ventana. Un clic sin arrastrar avanza al siguiente estado (demo del hito 1).
  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const startX = e.screenX;
    const startY = e.screenY;
    let dragging = false;

    const onMove = (ev: MouseEvent) => {
      if (dragging) return;
      if (Math.hypot(ev.screenX - startX, ev.screenY - startY) > DRAG_THRESHOLD_PX) {
        dragging = true;
        cleanup();
        getCurrentWindow().startDragging().catch(console.error);
      }
    };
    const onUp = () => {
      cleanup();
      if (!dragging) invoke("cycle_state").catch(console.error);
    };
    const cleanup = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const onContextMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    const stateItems = await Promise.all(
      PET_STATES.map((s) =>
        MenuItem.new({
          id: `state-${s}`,
          text: (s === state ? "● " : "    ") + STATE_LABELS[s],
          action: () => invoke("set_state", { state: s }),
        }),
      ),
    );
    const menu = await Menu.new({
      items: [
        ...stateItems,
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({ id: "hide", text: "Ocultar", action: () => invoke("hide_pet") }),
        await MenuItem.new({ id: "quit", text: "Salir", action: () => invoke("quit_app") }),
      ],
    });
    await menu.popup();
  };

  return (
    <div
      ref={hostRef}
      className="stage"
      onMouseDown={onMouseDown}
      onContextMenu={onContextMenu}
      dangerouslySetInnerHTML={{ __html: broteSvg }}
    />
  );
}
