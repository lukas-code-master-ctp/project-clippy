//! Gestos sobre Brote que no son la voz: escribirle una orden y las cosquillas.

use crate::bubble::{self, BubbleKind};
use crate::pet::{Pet, PetState};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Tamaño lógico de la caja (igual que en tauri.conf.json).
pub const ASK_W: f64 = 320.0;
pub const ASK_H: f64 = 64.0;

const TICKLE_LINES: &[&str] = &["¡Jiji, cosquillas!", "¡Ay, para, para!", "Jajaja, ¡ahí no!", "¡Me haces reír!", "Hihihi…"];
static NEXT_LINE: AtomicUsize = AtomicUsize::new(0);

/// Doble clic: muestra la caja para escribir, sobre Brote.
#[tauri::command]
pub fn open_ask(app: AppHandle) {
    let Some(win) = app.get_webview_window("ask") else { return };
    bubble::hide(&app);
    bubble::place_above_pet(&app, "ask", ASK_W, ASK_H);
    let _ = win.show();
    let _ = win.set_focus();
    let _ = app.emit_to("ask", "ask-open", ());
    log::info!("Caja para escribir abierta");
}

#[tauri::command]
pub fn close_ask(app: AppHandle) {
    hide(&app);
}

pub fn hide(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("ask") {
        let _ = win.hide();
    }
}

/// Lo escrito va al agente igual que lo dicho por voz.
#[tauri::command]
pub fn submit_text(app: AppHandle, text: String) {
    hide(&app);
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }
    // Solo el largo: lo escrito no queda en el log.
    log::info!("Orden escrita ({} caracteres)", text.chars().count());
    crate::stop_running_agent(&app);
    crate::start_agent(&app, text);
}

/// Un clic: Brote se ríe. La animación la hace la ventana; acá va la frase,
/// solo si está durmiendo para no tapar lo que esté diciendo.
#[tauri::command]
pub fn tickle(app: AppHandle) {
    if app.state::<Pet>().get() != PetState::Sleeping {
        return;
    }
    let line = TICKLE_LINES[NEXT_LINE.fetch_add(1, Ordering::Relaxed) % TICKLE_LINES.len()];
    bubble::show(&app, line, BubbleKind::Info, Some(Duration::from_millis(1800)));
}
