//! Estado de Brote. El núcleo es la fuente de verdad: la UI solo lo dibuja.
//! Más adelante el agente (voz, Gmail, Calendar) cambiará el estado desde aquí.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PetState {
    Sleeping,
    Listening,
    Thinking,
    Working,
    Done,
}

/// Estado compartido. `generation` sube en cada cambio, para que una secuencia
/// automática (la demo de la tecla) sepa si alguien la interrumpió.
pub struct Pet {
    inner: Mutex<(PetState, u64)>,
}

impl Default for Pet {
    fn default() -> Self {
        Self { inner: Mutex::new((PetState::Sleeping, 0)) }
    }
}

impl Pet {
    pub fn get(&self) -> PetState {
        self.inner.lock().unwrap().0
    }

    pub fn generation(&self) -> u64 {
        self.inner.lock().unwrap().1
    }
}

/// Cambia el estado y avisa a la UI. Devuelve la nueva generación.
pub fn set(app: &AppHandle, state: PetState) -> u64 {
    let pet = app.state::<Pet>();
    let generation = {
        let mut g = pet.inner.lock().unwrap();
        g.0 = state;
        g.1 += 1;
        g.1
    };
    let _ = app.emit("pet-state", state);
    generation
}
