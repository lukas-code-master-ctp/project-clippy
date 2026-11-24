//! Google: conexión de la cuenta y herramientas de Gmail, Calendar y Contactos.

pub mod api;
pub mod auth;
pub mod calendar;
pub mod contacts;
pub mod gmail;
pub mod tools;

use auth::{ClientConfig, GoogleAuth};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// Credenciales de Google Cloud del usuario, si ya las configuró.
pub fn client_config(app: &AppHandle) -> Option<ClientConfig> {
    let id = crate::settings::load(app).google_client_id;
    let secret = app.state::<Arc<GoogleAuth>>().client_secret()?;
    (!id.trim().is_empty()).then(|| ClientConfig { client_id: id.trim().to_string(), client_secret: secret })
}

pub fn api(app: &AppHandle) -> api::Api {
    api::Api::new(app.state::<Arc<GoogleAuth>>().inner().clone(), client_config(app))
}

#[derive(Serialize)]
pub struct GoogleStatus {
    pub configured: bool,
    pub connected: bool,
    pub email: String,
    pub client_id: String,
}

#[tauri::command]
pub fn google_status(app: AppHandle) -> GoogleStatus {
    let settings = crate::settings::load(&app);
    GoogleStatus {
        configured: client_config(&app).is_some(),
        connected: app.state::<Arc<GoogleAuth>>().is_connected(),
        email: settings.google_email,
        client_id: settings.google_client_id,
    }
}

/// Guarda el Client ID y, si viene, el Client secret de Google Cloud.
#[tauri::command]
pub fn google_save_client(app: AppHandle, client_id: String, client_secret: Option<String>) -> Result<GoogleStatus, String> {
    let id = client_id.trim();
    if !id.is_empty() && !id.ends_with(".apps.googleusercontent.com") {
        return Err("El Client ID debería terminar en .apps.googleusercontent.com".into());
    }
    crate::settings::update(&app, |s| s.google_client_id = id.to_string()).map_err(|e| format!("{e:#}"))?;
    if let Some(secret) = client_secret.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        app.state::<Arc<GoogleAuth>>().set_client_secret(secret).map_err(|e| format!("{e:#}"))?;
    }
    Ok(google_status(app))
}

#[tauri::command]
pub async fn google_connect(app: AppHandle) -> Result<GoogleStatus, String> {
    let client = client_config(&app).ok_or("Primero guarda el Client ID y el Client secret.")?;
    let auth = app.state::<Arc<GoogleAuth>>().inner().clone();
    let opener = app.clone();
    let result = auth
        .connect(&client, |url| opener.opener().open_url(url, None::<&str>).map_err(|e| anyhow::anyhow!("no pude abrir el navegador: {e}")))
        .await
        .map_err(|e| {
            log::error!("Google: no se pudo conectar: {e:#}");
            format!("{e:#}")
        })?;
    crate::settings::update(&app, |s| s.google_email = result.email.clone()).map_err(|e| format!("{e:#}"))?;
    Ok(google_status(app))
}

#[tauri::command]
pub async fn google_disconnect(app: AppHandle) -> GoogleStatus {
    app.state::<Arc<GoogleAuth>>().inner().clone().disconnect().await;
    let _ = crate::settings::update(&app, |s| s.google_email.clear());
    google_status(app)
}
