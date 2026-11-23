//! Tarjeta de confirmación: la cara visible de la capa de políticas.

use super::policy::{self, Approval, Gate};
use super::Confirmer;
use crate::pet::{self, PetState};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Tamaño lógico de la ventana de la tarjeta (igual que en tauri.conf.json).
pub const CARD_W: f64 = 300.0;
pub const CARD_H: f64 = 220.0;
/// Si el usuario no responde en este tiempo, cuenta como "no".
const TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Serialize, Clone)]
pub struct CardPayload {
    pub id: u64,
    pub preview: String,
}

/// Tarjeta visible ahora, para que la ventana la pida si cargó tarde.
static CURRENT: std::sync::Mutex<Option<CardPayload>> = std::sync::Mutex::new(None);

#[tauri::command]
pub fn get_confirmation() -> Option<CardPayload> {
    CURRENT.lock().unwrap().clone()
}

#[tauri::command]
pub fn resolve_confirmation(app: AppHandle, id: u64, approved: bool) {
    app.state::<Gate>().resolve(id, approved);
}

pub fn hide(app: &AppHandle) {
    *CURRENT.lock().unwrap() = None;
    if let Some(w) = app.get_webview_window("confirm") {
        let _ = w.hide();
    }
}

pub struct CardConfirmer {
    pub app: AppHandle,
}

impl Confirmer for CardConfirmer {
    async fn confirm(&self, _tool: &str, preview: &str) -> Option<Approval> {
        let app = &self.app;
        let (id, rx) = app.state::<Gate>().open();
        let payload = CardPayload { id, preview: preview.to_string() };
        *CURRENT.lock().unwrap() = Some(payload.clone());

        crate::bubble::hide(app);
        if let Some(card) = app.get_webview_window("confirm") {
            let _ = card.emit_to("confirm", "confirm", payload);
            crate::bubble::place_above_pet(app, "confirm", CARD_W, CARD_H);
            let _ = card.show();
        }
        // Brote atento mientras espera la respuesta.
        pet::set(app, PetState::Listening);

        let approval = policy::wait(rx, TIMEOUT).await;
        hide(app);
        pet::set(app, PetState::Working);
        approval
    }
}
