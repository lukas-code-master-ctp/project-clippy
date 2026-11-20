mod pet;

use pet::{Pet, PetState};
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, PhysicalPosition, WebviewWindow,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Margen respecto al borde de la pantalla al aparecer.
const SCREEN_MARGIN: f64 = 24.0;
/// Tamaño lógico de la ventana (igual que en tauri.conf.json).
const PET_SIZE: f64 = 200.0;

#[tauri::command]
fn get_state(pet: tauri::State<Pet>) -> PetState {
    pet.get()
}

#[tauri::command]
fn set_state(app: AppHandle, state: PetState) {
    pet::set(&app, state);
}

#[tauri::command]
fn cycle_state(app: AppHandle) {
    let next = app.state::<Pet>().get().next();
    pet::set(&app, next);
}

#[tauri::command]
fn hide_pet(app: AppHandle) {
    if let Some(w) = app.get_webview_window("pet") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn toggle_visibility(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("pet") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
        }
    }
}

/// Ubica a Brote en la esquina inferior derecha del monitor principal.
fn place_bottom_right(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else { return };
    let scale = monitor.scale_factor();
    // Una ventana aún oculta puede reportar tamaño 0 en Linux: se usa el mayor entre ese y el configurado.
    let size = window.outer_size().unwrap_or_default();
    let min = (PET_SIZE * scale) as u32;
    let (w, h) = (size.width.max(min) as i32, size.height.max(min) as i32);
    let margin = (SCREEN_MARGIN * scale) as i32;
    // work_area descuenta la barra de tareas o el dock.
    let area = monitor.work_area();
    let x = area.position.x + area.size.width as i32 - w - margin;
    let y = area.position.y + area.size.height as i32 - h - margin;
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Demo del flujo de voz mientras no existe el agente: mantener la tecla = escuchando;
/// al soltar, pasa por pensando, trabajando y listo, y vuelve a dormir.
/// Si el estado cambia por otro lado a mitad de camino, la secuencia se detiene.
fn run_demo_sequence(app: AppHandle) {
    std::thread::spawn(move || {
        let steps = [
            (PetState::Thinking, 1400),
            (PetState::Working, 1800),
            (PetState::Done, 1600),
            (PetState::Sleeping, 0),
        ];
        let mut generation = pet::set(&app, steps[0].0);
        for window in steps.windows(2) {
            std::thread::sleep(Duration::from_millis(window[0].1));
            if app.state::<Pet>().generation() != generation {
                return;
            }
            generation = pet::set(&app, window[1].0);
        }
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    for s in PetState::ALL {
        items.push(Box::new(MenuItem::with_id(app, format!("state-{}", s.id()), s.label(), true, None::<&str>)?));
    }
    items.push(Box::new(PredefinedMenuItem::separator(app)?));
    items.push(Box::new(MenuItem::with_id(app, "toggle", "Mostrar u ocultar", true, None::<&str>)?));
    items.push(Box::new(MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?));
    let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items.iter().map(|b| b.as_ref()).collect();
    let menu = Menu::with_items(app, &refs)?;

    TrayIconBuilder::with_id("brote")
        .icon(app.default_window_icon().cloned().expect("falta el ícono de la app"))
        .tooltip("Brote")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "toggle" => toggle_visibility(app),
                "quit" => app.exit(0),
                _ => {
                    if let Some(s) = id.strip_prefix("state-").and_then(PetState::from_id) {
                        pet::set(app, s);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Tecla para hablar: Ctrl+Shift+Espacio (Ctrl+Espacio choca con el cambio de idioma en varios sistemas).
    let talk_key = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);

    tauri::Builder::default()
        .manage(Pet::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut != &talk_key {
                        return;
                    }
                    match event.state() {
                        ShortcutState::Pressed => {
                            if let Some(w) = app.get_webview_window("pet") {
                                let _ = w.show();
                            }
                            pet::set(app, PetState::Listening);
                        }
                        ShortcutState::Released => run_demo_sequence(app.clone()),
                    }
                })
                .build(),
        )
        .setup(move |app| {
            if let Err(e) = app.global_shortcut().register(talk_key) {
                eprintln!("No se pudo registrar Ctrl+Shift+Espacio: {e}");
            }
            build_tray(app.handle())?;
            if let Some(w) = app.get_webview_window("pet") {
                place_bottom_right(&w);
                // Visible en todos los escritorios virtuales.
                let _ = w.set_visible_on_all_workspaces(true);
                let _ = w.show();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_state, set_state, cycle_state, hide_pet, quit_app])
        .run(tauri::generate_context!())
        .expect("error al iniciar Brote");
}
