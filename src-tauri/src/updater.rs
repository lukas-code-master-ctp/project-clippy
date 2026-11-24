//! Actualizaciones automáticas. Cada merge a main publica una versión firmada
//! (ver .github/workflows/release.yml) y Brote la instala solo, sin que bajes nada.
//!
//! La firma se verifica con la llave pública de tauri.conf.json: una descarga
//! alterada o de otro origen se rechaza antes de instalar.

use crate::bubble::{self, BubbleKind};
use anyhow::{Context, Result};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// Espera antes de la primera revisión, para no competir con el arranque.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// Cada cuánto vuelve a revisar mientras Brote está abierto.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Si Brote está ocupado, cada cuánto mira si ya se desocupó para instalar.
const IDLE_POLL: Duration = Duration::from_secs(60);

/// Revisa al arrancar y luego cada 6 horas. En desarrollo no hace nada.
pub fn start(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if let Err(e) = check_and_install(&app, false).await {
                log::warn!("Actualización: {e:#}");
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Busca una versión nueva y, si hay, la baja y la instala.
/// Automático: espera a que Brote esté desocupado. Manual (desde Ajustes): instala de inmediato.
async fn check_and_install(app: &AppHandle, manual: bool) -> Result<String> {
    let current = app.package_info().version.to_string();
    let update = app.updater().context("no pude preparar el actualizador")?.check().await.context("no pude revisar si hay versiones nuevas")?;
    let Some(update) = update else {
        log::info!("Actualización: {current} es la última versión");
        return Ok(format!("Tienes la última versión ({current})."));
    };
    let version = update.version.clone();
    log::info!("Actualización: hay una versión nueva {version} (tengo {current}); bajándola");
    let bytes = update.download(|_, _| {}, || {}).await.context("no pude bajar la versión nueva")?;
    if !manual {
        while !crate::is_idle(app) {
            tokio::time::sleep(IDLE_POLL).await;
        }
    }
    log::info!(
        "Actualización: instalando {version} (instalado como {:?}, ejecutable {:?})",
        tauri::utils::platform::bundle_type(),
        std::env::current_exe().ok()
    );
    bubble::show(app, &format!("Me actualizo a la versión {version}. Vuelvo en un momento."), BubbleKind::Info, None);
    tokio::time::sleep(Duration::from_secs(2)).await;
    // En Windows el instalador cierra Brote, lo actualiza y lo vuelve a abrir.
    update.install(bytes).context("no pude instalar la versión nueva")?;
    app.restart();
}

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<String, String> {
    if cfg!(debug_assertions) {
        return Ok("En modo desarrollo no se buscan actualizaciones.".into());
    }
    check_and_install(&app, true).await.map_err(|e| {
        log::warn!("Actualización manual: {e:#}");
        format!("{e:#}")
    })
}
