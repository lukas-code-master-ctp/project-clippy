//! Globo de texto junto a Brote. Es una segunda ventana transparente que
//! no recibe clics, para no tapar lo que hay debajo.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

/// Cuánto se mete el globo en la ventana de Brote, en píxeles lógicos.
/// La parte de arriba de esa ventana es aire (Brote empieza más abajo), así que
/// sin este solape el globo quedaría flotando lejos de él.
const OVERLAP: f64 = 40.0;
/// Tamaño lógico de la ventana del globo (igual que en tauri.conf.json).
const BUBBLE_W: f64 = 280.0;
const BUBBLE_H: f64 = 200.0;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BubbleKind {
    /// Lo que entendió de la voz del usuario.
    Heard,
    /// Un aviso de Brote (descargas, estado).
    Info,
    /// La respuesta del agente.
    Reply,
    Error,
}

#[derive(Serialize, Clone)]
pub struct BubblePayload {
    text: String,
    kind: BubbleKind,
}

/// Globo visible ahora. La ventana del globo lo pide al cargar, por si un aviso
/// llegó antes de que su página estuviera lista (pasa al arrancar).
static CURRENT: Mutex<Option<BubblePayload>> = Mutex::new(None);

#[tauri::command]
pub fn get_bubble() -> Option<BubblePayload> {
    CURRENT.lock().unwrap().clone()
}

/// Sube con cada globo nuevo, para que un temporizador viejo no oculte uno reciente.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Muestra un globo. Con `ttl` se oculta solo; con `None` queda hasta el próximo.
pub fn show(app: &AppHandle, text: &str, kind: BubbleKind, ttl: Option<Duration>) {
    let Some(bubble) = app.get_webview_window("bubble") else { return };
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let payload = BubblePayload { text: text.to_string(), kind };
    *CURRENT.lock().unwrap() = Some(payload.clone());
    let _ = bubble.emit_to("bubble", "bubble", payload);
    reposition(app);
    let _ = bubble.show();
    // El globo nunca recibe clics: lo de abajo sigue usable.
    // Va después de show() porque en Linux falla sobre una ventana que nunca se mostró.
    let _ = bubble.set_ignore_cursor_events(true);

    if let Some(ttl) = ttl {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(ttl);
            if GENERATION.load(Ordering::SeqCst) == generation {
                hide(&app);
            }
        });
    }
}

pub fn hide(app: &AppHandle) {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    *CURRENT.lock().unwrap() = None;
    if let Some(bubble) = app.get_webview_window("bubble") {
        let _ = bubble.hide();
    }
}

/// Pone el globo sobre Brote. Ver `place_above_pet`.
pub fn reposition(app: &AppHandle) {
    place_above_pet(app, "bubble", BUBBLE_W, BUBBLE_H);
    place_above_pet(app, "confirm", crate::agent::confirm::CARD_W, crate::agent::confirm::CARD_H);
}

/// Pone una ventana (globo o tarjeta) sobre Brote, alineada a su borde derecho.
/// Si no cabe arriba (Brote pegado al borde superior), la pone debajo.
pub fn place_above_pet(app: &AppHandle, label: &str, logical_w: f64, logical_h: f64) {
    let (Some(pet), Some(win)) = (app.get_webview_window("pet"), app.get_webview_window(label)) else { return };
    let (Ok(pet_pos), Ok(pet_size), Ok(size)) = (pet.outer_position(), pet.outer_size(), win.outer_size()) else { return };
    let scale = pet.scale_factor().unwrap_or(1.0);
    let overlap = (OVERLAP * scale) as i32;
    // Una ventana que nunca se mostró puede reportar tamaño 0 en Linux.
    let bw = size.width.max((logical_w * scale) as u32) as i32;
    let bh = size.height.max((logical_h * scale) as u32) as i32;

    let mut x = pet_pos.x + pet_size.width as i32 - bw;
    let mut y = pet_pos.y - bh + overlap;
    if let Ok(Some(monitor)) = pet.current_monitor() {
        let area = monitor.work_area();
        if y < area.position.y {
            y = pet_pos.y + pet_size.height as i32;
        }
        x = x.clamp(area.position.x, area.position.x + area.size.width as i32 - bw);
    }
    let _ = win.set_position(PhysicalPosition::new(x, y));
}
