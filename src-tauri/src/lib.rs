mod agent;
mod ask;
mod bubble;
mod google;
mod logs;
mod pet;
mod settings;
mod updater;
mod voice;

use agent::confirm::{self, CardConfirmer};
use agent::policy::Gate;
use agent::tools::{self, Registry};
use agent::OpenRouterLlm;
use bubble::BubbleKind;
use std::sync::Mutex;
use pet::{Pet, PetState};
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, PhysicalPosition, WebviewWindow, WindowEvent,
};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use voice::model::{self, ModelState, ModelStatus};
use voice::recorder::Recorder;
use voice::transcriber::Transcriber;

/// Margen respecto al borde de la pantalla al aparecer.
const SCREEN_MARGIN: f64 = 24.0;
/// Tamaño lógico de la ventana (igual que en tauri.conf.json).
const PET_SIZE: f64 = 120.0;
/// El comando que el agente está ejecutando, para poder detenerlo.
#[derive(Default)]
struct AgentTask(Mutex<Option<tauri::async_runtime::JoinHandle<()>>>);

/// La conversación del último comando, para responderle a Brote ("el primero", "sí, ese").
#[derive(Default)]
struct Conversation(Mutex<Option<(std::time::Instant, Vec<agent::openrouter::Message>)>>);

/// Pasado este tiempo, un comando nuevo empieza de cero.
const CONVERSATION_TTL: Duration = Duration::from_secs(180);

#[tauri::command]
fn get_state(pet: tauri::State<Pet>) -> PetState {
    pet.get()
}

#[tauri::command]
fn set_state(app: AppHandle, state: PetState) {
    pet::set(&app, state);
}

/// Mantener apretado a Brote con el mouse: lo mismo que la tecla para hablar.
#[tauri::command]
fn talk_start(app: AppHandle) {
    on_talk_pressed(&app);
}

#[tauri::command]
fn talk_stop(app: AppHandle) {
    on_talk_released(&app);
}

#[tauri::command]
fn hide_pet(app: AppHandle) {
    bubble::hide(&app);
    if let Some(w) = app.get_webview_window("pet") {
        let _ = w.hide();
    }
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    show_settings(&app);
}

#[tauri::command]
fn stop_agent(app: AppHandle) {
    if stop_running_agent(&app) {
        bubble::show(&app, "Listo, me detuve.", BubbleKind::Info, Some(Duration::from_secs(3)));
        pet::set(&app, PetState::Sleeping);
    }
}

fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Detiene el comando en curso, si hay uno. Las confirmaciones pendientes cuentan como "no".
pub(crate) fn stop_running_agent(app: &AppHandle) -> bool {
    app.state::<Gate>().cancel_all();
    confirm::hide(app);
    match app.state::<AgentTask>().0.lock().unwrap().take() {
        Some(task) => {
            log::info!("Agente detenido");
            task.abort();
            true
        }
        None => false,
    }
}

/// Cuánto dejar visible un texto: más largo, más tiempo, con tope.
fn reading_time(text: &str) -> Duration {
    Duration::from_millis((4_000 + 60 * text.chars().count() as u64).min(15_000))
}

/// Traduce errores técnicos a algo que se entienda en un globo.
fn friendly_agent_error(e: &anyhow::Error, model: &str) -> String {
    let raw = format!("{e:#}");
    let lower = raw.to_lowercase();
    if lower.contains("401") || lower.contains("no auth") || lower.contains("invalid api key") {
        "Mi API key de OpenRouter no funciona. Revísala en Ajustes.".into()
    } else if lower.contains("402") || lower.contains("credits") {
        "Tu cuenta de OpenRouter se quedó sin saldo.".into()
    } else if lower.contains("not a valid model") || lower.contains("no endpoints found") || lower.contains("404") {
        format!("No encuentro el modelo {model} en OpenRouter. Cámbialo en Ajustes.")
    } else if lower.contains("conectarme") || lower.contains("timed out") || lower.contains("timeout") {
        "No pude conectarme. Revisa tu internet.".into()
    } else {
        "Algo falló al pensar la respuesta. ¿Lo intentamos de nuevo?".into()
    }
}

/// Manda el comando al agente. Lo que se oyó queda en el globo mientras piensa.
pub(crate) fn start_agent(app: &AppHandle, text: String) {
    let Some(key) = settings::api_key() else {
        log::warn!("Falta la API key de OpenRouter");
        bubble::show(
            app,
            "Para ayudarte necesito tu API key de OpenRouter. Te abro los ajustes.",
            BubbleKind::Info,
            Some(Duration::from_secs(6)),
        );
        pet::set(app, PetState::Sleeping);
        show_settings(app);
        return;
    };
    let model = settings::load(app).model;
    bubble::show(app, &text, BubbleKind::Heard, None);
    pet::set(app, PetState::Thinking);

    let task_app = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let app = task_app;
        let opener_app = app.clone();
        let registry = Registry::default()
            .with(tools::fecha_y_hora())
            .with(tools::abrir_sitio_web(move |url| {
                opener_app.opener().open_url(url, None::<&str>).map_err(|e| anyhow::anyhow!("no pude abrir el navegador: {e}"))
            }))
            .with_all(google::tools::all(&google::api(&app)));
        let llm = OpenRouterLlm { key, model: model.clone() };
        let confirmer = CardConfirmer { app: app.clone() };
        let started = std::time::Instant::now();
        let history = match app.state::<Conversation>().0.lock().unwrap().take() {
            Some((at, messages)) if at.elapsed() < CONVERSATION_TTL => messages,
            _ => vec![],
        };
        log::info!("Agente: modelo {model}, {} mensajes previos", history.len());
        let result = agent::run(&llm, &registry, &confirmer, history, &text).await;
        log::info!("Agente terminó en {:.2} s", started.elapsed().as_secs_f32());
        app.state::<AgentTask>().0.lock().unwrap().take();
        match result {
            Ok(outcome) if !outcome.reply.is_empty() => {
                let reply = outcome.reply;
                *app.state::<Conversation>().0.lock().unwrap() = Some((std::time::Instant::now(), outcome.history));
                bubble::show(&app, &reply, BubbleKind::Reply, Some(reading_time(&reply)));
                let generation = pet::set(&app, PetState::Done);
                sleep_after(&app, generation, Duration::from_secs(3));
            }
            Ok(_) => {
                log::warn!("El agente no devolvió texto");
                bubble::hide(&app);
                pet::set(&app, PetState::Sleeping);
            }
            Err(e) => {
                log::error!("Agente: {e:#}");
                bubble::show(&app, &friendly_agent_error(&e, &model), BubbleKind::Error, Some(Duration::from_secs(8)));
                pet::set(&app, PetState::Sleeping);
            }
        }
    });
    *app.state::<AgentTask>().0.lock().unwrap() = Some(task);
}

fn toggle_visibility(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("pet") {
        if w.is_visible().unwrap_or(false) {
            bubble::hide(app);
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

/// Vuelve a dormir después de `delay`, salvo que el estado haya cambiado entretanto.
fn sleep_after(app: &AppHandle, generation: u64, delay: Duration) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if app.state::<Pet>().generation() == generation {
            pet::set(&app, PetState::Sleeping);
        }
    });
}

/// Tecla presionada: empieza a escuchar si el modelo está listo.
fn on_talk_pressed(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("pet") {
        let _ = w.show();
    }
    let recorder = app.state::<Recorder>();
    if recorder.is_recording() {
        return;
    }
    // Hablar de nuevo reemplaza lo que estaba haciendo.
    stop_running_agent(app);
    match model::status(app) {
        ModelStatus::Ready => {}
        ModelStatus::Downloading { percent } => {
            log::info!("Tecla presionada pero el modelo va en {percent}%");
            let msg = format!("Todavía estoy bajando mi modelo de voz ({percent}%). Dame un momento.");
            bubble::show(app, &msg, BubbleKind::Info, Some(Duration::from_secs(4)));
            return;
        }
        ModelStatus::Failed { .. } | ModelStatus::Missing => {
            log::warn!("Tecla presionada sin modelo de voz; reintento la descarga");
            bubble::show(app, "No tengo el modelo de voz. Lo intento bajar de nuevo.", BubbleKind::Error, Some(Duration::from_secs(5)));
            start_model_download(app.clone());
            return;
        }
    }
    match recorder.start() {
        Ok(()) => {
            log::info!("Escuchando");
            bubble::hide(app);
            pet::set(app, PetState::Listening);
        }
        Err(e) => {
            log::error!("Micrófono: {e:#}");
            bubble::show(
                app,
                "No pude usar el micrófono. Revisa que esté conectado y que el sistema le dé permiso a Brote.",
                BubbleKind::Error,
                Some(Duration::from_secs(6)),
            );
            pet::set(app, PetState::Sleeping);
        }
    }
}

/// Tecla soltada: deja de grabar y transcribe en segundo plano.
fn on_talk_released(app: &AppHandle) {
    let recorder = app.state::<Recorder>();
    if !recorder.is_recording() {
        return;
    }
    let audio = match recorder.stop() {
        Ok(a) => a,
        Err(e) => {
            log::error!("Grabación: {e:#}");
            bubble::show(app, "Se cortó la grabación. ¿Lo intentamos de nuevo?", BubbleKind::Error, Some(Duration::from_secs(6)));
            pet::set(app, PetState::Sleeping);
            return;
        }
    };
    log::info!("Grabación detenida: {:.1} s", audio.len() as f32 / 16_000.0);
    if audio.len() < voice::MIN_SPEECH_SAMPLES {
        log::info!("Muy corta, se ignora");
        pet::set(app, PetState::Sleeping);
        return;
    }
    pet::set(app, PetState::Thinking);
    let app = app.clone();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = model::model_path(&app).and_then(|path| app.state::<Transcriber>().transcribe(&path, &audio));
        log::info!("Transcripción de {:.1} s de audio en {:.2} s", audio.len() as f32 / 16_000.0, started.elapsed().as_secs_f32());
        match result {
            Ok(text) if text.is_empty() => {
                log::info!("Transcripción vacía");
                bubble::show(&app, "No te escuché bien. ¿Me lo repites?", BubbleKind::Info, Some(Duration::from_secs(4)));
                pet::set(&app, PetState::Sleeping);
            }
            Ok(text) => {
                // Solo el largo: lo que dijiste no queda en el log.
                log::info!("Transcripción lista ({} caracteres)", text.chars().count());
                start_agent(&app, text)
            }
            Err(e) => {
                log::error!("Whisper: {e:#}");
                bubble::show(&app, "No pude entender el audio. ¿Me lo repites?", BubbleKind::Error, Some(Duration::from_secs(6)));
                pet::set(&app, PetState::Sleeping);
            }
        }
    });
}

/// Baja el modelo de Whisper si falta y lo deja cargado en memoria.
fn start_model_download(app: AppHandle) {
    if matches!(model::status(&app), ModelStatus::Downloading { .. }) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let was_missing = !matches!(model::model_path(&app).map(|p| p.exists()), Ok(true));
        if was_missing {
            log::info!("Bajando el modelo de voz");
            bubble::show(&app, "Estoy bajando mi modelo de voz (unos 190 MB). Es solo la primera vez.", BubbleKind::Info, Some(Duration::from_secs(6)));
        }
        let progress_app = app.clone();
        let result = model::ensure_model(app.clone(), move |p| {
            if p % 25 == 0 && p < 100 {
                log::info!("Modelo de voz: {p}%");
                bubble::show(&progress_app, &format!("Bajando modelo de voz: {p}%"), BubbleKind::Info, Some(Duration::from_secs(3)));
            }
        })
        .await;
        match result {
            Ok(path) => {
                log::info!("Modelo de voz listo");
                // Cargarlo ahora evita que la primera transcripción tarde más.
                let load_app = app.clone();
                std::thread::spawn(move || {
                    if let Err(e) = load_app.state::<Transcriber>().load(&path) {
                        log::error!("Whisper al cargar: {e:#}");
                    }
                });
                if was_missing {
                    bubble::show(&app, "Listo. Mantén Ctrl+Shift+Espacio y háblame.", BubbleKind::Info, Some(Duration::from_secs(6)));
                }
            }
            Err(e) => {
                log::error!("Modelo de voz: {e:#}");
                bubble::show(
                    &app,
                    "No pude bajar mi modelo de voz. Revisa tu conexión; lo intento de nuevo cuando me hables.",
                    BubbleKind::Error,
                    Some(Duration::from_secs(8)),
                );
            }
        }
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let mut items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = Vec::new();
    items.push(Box::new(MenuItem::with_id(app, "settings", "Ajustes…", true, None::<&str>)?));
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
                "settings" => show_settings(app),
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

/// Brote no está haciendo nada: buen momento para actualizarse.
pub(crate) fn is_idle(app: &AppHandle) -> bool {
    app.state::<Pet>().get() == PetState::Sleeping
        && !app.state::<Recorder>().is_recording()
        && app.state::<AgentTask>().0.lock().unwrap().is_none()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Tecla para hablar: Ctrl+Shift+Espacio (Ctrl+Espacio choca con el cambio de idioma en varios sistemas).
    let talk_key = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);

    tauri::Builder::default()
        .manage(Pet::default())
        .manage(Recorder::default())
        .manage(Transcriber::default())
        .manage(ModelState::default())
        .manage(Gate::default())
        .manage(AgentTask::default())
        .manage(Conversation::default())
        .manage(std::sync::Arc::new(google::auth::GoogleAuth::default()))
        // Abrir Brote de nuevo muestra el que ya está corriendo, en vez de lanzar otro
        // (dos copias pelean por Ctrl+Shift+Espacio).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("Se intentó abrir otra copia; muestro la que ya corre");
            if let Some(w) = app.get_webview_window("pet") {
                let _ = w.show();
            }
        }))
        .plugin(logs::plugin())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut != &talk_key {
                        return;
                    }
                    match event.state() {
                        ShortcutState::Pressed => on_talk_pressed(app),
                        ShortcutState::Released => on_talk_released(app),
                    }
                })
                .build(),
        )
        .setup(move |app| {
            logs::startup(app.handle());
            if let Err(e) = app.global_shortcut().register(talk_key) {
                log::error!("No se pudo registrar Ctrl+Shift+Espacio: {e}");
            }
            build_tray(app.handle())?;
            // Lee las credenciales una vez al arrancar, en el hilo principal.
            let _ = settings::api_key();
            app.state::<std::sync::Arc<google::auth::GoogleAuth>>().load();
            if let Some(bubble) = app.get_webview_window("bubble") {
                let _ = bubble.set_visible_on_all_workspaces(true);
            }
            if let Some(w) = app.get_webview_window("pet") {
                place_bottom_right(&w);
                // Visible en todos los escritorios virtuales.
                let _ = w.set_visible_on_all_workspaces(true);
                let _ = w.show();
                // El globo sigue a Brote cuando lo arrastran.
                let handle = app.handle().clone();
                w.on_window_event(move |event| {
                    if let WindowEvent::Moved(_) = event {
                        bubble::reposition(&handle);
                    }
                });
            }
            if let Some(w) = app.get_webview_window("settings") {
                // Cerrar los ajustes solo los oculta; Brote sigue corriendo.
                let win = w.clone();
                w.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }
            start_model_download(app.handle().clone());
            updater::start(app.handle().clone());
            // Solo en desarrollo: ejecuta un comando escrito, sin micrófono, para probar el agente.
            #[cfg(debug_assertions)]
            if let Ok(command) = std::env::var("BROTE_DEMO_COMMAND") {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    start_agent(&handle, command);
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bubble::get_bubble,
            confirm::get_confirmation,
            confirm::resolve_confirmation,
            settings::get_settings,
            settings::save_settings,
            settings::test_connection,
            google::google_status,
            google::google_save_client,
            google::google_connect,
            google::google_disconnect,
            logs::open_log_dir,
            updater::app_version,
            updater::check_updates,
            get_state,
            set_state,
            talk_start,
            talk_stop,
            ask::open_ask,
            ask::close_ask,
            ask::submit_text,
            ask::tickle,
            hide_pet,
            quit_app,
            open_settings,
            stop_agent
        ])
        .run(tauri::generate_context!())
        .expect("error al iniciar Brote");
}
