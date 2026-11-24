//! Ajustes de Brote. La API key vive en el llavero del sistema (Administrador de
//! credenciales en Windows, Llavero en macOS), nunca en un archivo ni en la interfaz.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "Brote";
const KEYRING_USER: &str = "openrouter-api-key";

/// Modelo por defecto: rápido, barato y bueno usando herramientas.
pub const DEFAULT_MODEL: &str = "anthropic/claude-haiku-4.5";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Id del modelo en OpenRouter.
    pub model: String,
    /// Client ID de la app de escritorio en Google Cloud (el secret va al llavero).
    pub google_client_id: String,
    /// Cuenta de Google conectada, para mostrarla en Ajustes.
    pub google_email: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { model: DEFAULT_MODEL.into(), google_client_id: String::new(), google_email: String::new() }
    }
}

/// Lo que ve la interfaz: nunca la key, solo si existe.
#[derive(Serialize)]
pub struct SettingsView {
    pub model: String,
    pub has_key: bool,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    Ok(app.path().app_config_dir().context("sin carpeta de configuración")?.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Cambia algunos campos y guarda.
pub fn update(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<()> {
    let mut settings = load(app);
    change(&mut settings);
    save(app, &settings)
}

fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    let path = settings_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)?)?;
    Ok(())
}

fn entry() -> Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).context("no pude abrir el llavero del sistema")
}

/// Copia en memoria de la key, para no consultar el llavero en cada comando.
/// (En Linux, además, el llavero del kernel puede no verse desde otros hilos.)
static CACHED_KEY: Mutex<Option<String>> = Mutex::new(None);

pub fn api_key() -> Option<String> {
    let mut cached = CACHED_KEY.lock().unwrap();
    if cached.is_none() {
        *cached = entry().ok()?.get_password().ok().filter(|k| !k.trim().is_empty());
    }
    cached.clone()
}

fn set_api_key(key: &str) -> Result<()> {
    entry()?.set_password(key.trim()).context("no pude guardar la key en el llavero")?;
    *CACHED_KEY.lock().unwrap() = Some(key.trim().to_string());
    Ok(())
}

fn clear_api_key() -> Result<()> {
    *CACHED_KEY.lock().unwrap() = None;
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(anyhow!(e)),
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> SettingsView {
    SettingsView { model: load(&app).model, has_key: api_key().is_some() }
}

/// `api_key`: `None` deja la actual, `Some("")` la borra, `Some(k)` la reemplaza.
#[tauri::command]
pub fn save_settings(app: AppHandle, model: String, api_key: Option<String>) -> Result<SettingsView, String> {
    let model = model.trim();
    update(&app, |s| s.model = if model.is_empty() { DEFAULT_MODEL.into() } else { model.into() }).map_err(|e| format!("{e:#}"))?;
    match api_key.as_deref().map(str::trim) {
        None => {}
        Some("") => clear_api_key().map_err(|e| format!("{e:#}"))?,
        Some(k) => set_api_key(k).map_err(|e| format!("{e:#}"))?,
    }
    Ok(get_settings(app))
}

/// Prueba la key y el modelo con una pregunta mínima. Devuelve la respuesta del modelo.
#[tauri::command]
pub async fn test_connection(app: AppHandle) -> Result<String, String> {
    let key = api_key().ok_or("Todavía no guardas una API key.")?;
    let model = load(&app).model;
    log::info!("Probando OpenRouter con el modelo {model}");
    crate::agent::openrouter::ping(&key, &model).await.map_err(|e| {
        log::error!("Prueba de OpenRouter: {e:#}");
        format!("{e:#}")
    })
}
