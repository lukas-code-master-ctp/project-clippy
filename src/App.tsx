import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";
import petSvg from "./canela.svg?raw";
import { PET_STATES, type PetState } from "./state";

const DRAG_THRESHOLD_PX = 4;
/** Mantener apretado más que esto, sin mover, empieza a escuchar. */
const HOLD_TO_TALK_MS = 400;
/** Ventana para el segundo clic de un doble clic. */
const DOUBLE_CLICK_MS = 260;
const TICKLE_MS = 900;

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


  // Gestos sobre Brote:
  // - arrastrar (mover más de 4 px) lo mueve;
  // - mantener apretado sin mover = hablar mientras se mantiene, igual que Ctrl+Shift+Espacio;
  // - doble clic = caja para escribirle;
  // - un clic = cosquillas.
  const clickTimer = useRef<number | undefined>(undefined);
  const tickleTimer = useRef<number | undefined>(undefined);

  const tickle = () => {
    const svg = hostRef.current?.querySelector("svg");
    if (svg) {
      // Quitar la clase y forzar un reflow reinicia la animación si le hacen cosquillas seguidas.
      // La clase en el <svg> cambia los cuadros; en el contenedor, lo hace saltar.
      const targets = [svg, hostRef.current!];
      targets.forEach((el) => el.classList.remove("tickle"));
      void svg.getBoundingClientRect();
      targets.forEach((el) => el.classList.add("tickle"));
      window.clearTimeout(tickleTimer.current);
      tickleTimer.current = window.setTimeout(() => targets.forEach((el) => el.classList.remove("tickle")), TICKLE_MS);
    }
    invoke("tickle").catch(console.error);
  };

  const onMouseDown = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const startX = e.screenX;
    const startY = e.screenY;
    let mode: "pending" | "drag" | "talk" = "pending";

    const holdTimer = window.setTimeout(() => {
      if (mode !== "pending") return;
      mode = "talk";
      window.clearTimeout(clickTimer.current);
      clickTimer.current = undefined;
      invoke("talk_start").catch(console.error);
    }, HOLD_TO_TALK_MS);

    const onMove = (ev: MouseEvent) => {
      if (mode !== "pending") return;
      if (Math.hypot(ev.screenX - startX, ev.screenY - startY) > DRAG_THRESHOLD_PX) {
        mode = "drag";
        cleanup();
        getCurrentWindow().startDragging().catch(console.error);
      }
    };
    const onUp = () => {
      cleanup();
      if (mode === "talk") {
        invoke("talk_stop").catch(console.error);
      } else if (mode === "pending") {
        if (clickTimer.current !== undefined) {
          // Segundo clic a tiempo: doble clic.
          window.clearTimeout(clickTimer.current);
          clickTimer.current = undefined;
          invoke("open_ask").catch(console.error);
        } else {
          clickTimer.current = window.setTimeout(() => {
            clickTimer.current = undefined;
            tickle();
          }, DOUBLE_CLICK_MS);
        }
      }
    };
    const cleanup = () => {
      window.clearTimeout(holdTimer);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const onContextMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    const menu = await Menu.new({
      items: [
        await MenuItem.new({ id: "ask", text: "Escribirle…", action: () => invoke("open_ask") }),
        await PredefinedMenuItem.new({ item: "Separator" }),
        await MenuItem.new({ id: "stop", text: "Detener", action: () => invoke("stop_agent") }),
        await MenuItem.new({ id: "settings", text: "Ajustes…", action: () => invoke("open_settings") }),
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
      dangerouslySetInnerHTML={{ __html: petSvg }}
    />
  );
}
