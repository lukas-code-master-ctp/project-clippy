// Estados de Brote. Deben coincidir con `PetState` en src-tauri/src/pet.rs
// y con las clases s-* del SVG.
export const PET_STATES = ["sleeping", "listening", "thinking", "working", "done"] as const;
export type PetState = (typeof PET_STATES)[number];

export const STATE_LABELS: Record<PetState, string> = {
  sleeping: "Durmiendo",
  listening: "Escuchando",
  thinking: "Pensando",
  working: "Trabajando",
  done: "Listo",
};
