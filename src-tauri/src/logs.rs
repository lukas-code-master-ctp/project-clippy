//! Registro local para diagnosticar fallas. Nada de esto sale del computador.
//!
//! Se guarda en la carpeta de logs del sistema (en Windows,
//! `%LOCALAPPDATA%\cl.brote.app\logs\brote.log`), con rotación para no crecer sin límite.
//!
//! Privacidad: nunca se registran keys, tokens, lo que dijiste, el contenido de correos
//! o eventos, ni los argumentos de las herramientas. Solo pasos, tiempos y errores.

use tauri::{AppHandle, Manager};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
use tauri_plugin_opener::OpenerExt;

/// Tamaño máximo de cada archivo antes de rotar.
const MAX_FILE_BYTES: u128 = 1_000_000;
/// Archivos viejos que se conservan además del actual.
const KEEP_FILES: usize = 4;

pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(Target::new(TargetKind::Stdout))
        .target(Target::new(TargetKind::LogDir { file_name: Some("brote".into()) }))
        .max_file_size(MAX_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(KEEP_FILES))
        .level(log::LevelFilter::Warn)
        .level_for("brote_lib", log::LevelFilter::Info)
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} {:<5} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                match record.target() {
                    "brote_lib" => "app",
                    t => t.trim_start_matches("brote_lib::"),
                },
                message
            ))
        })
        .build()
}

/// Primeras líneas de cada sesión: versión y sistema, para saber con qué se reportó una falla.
pub fn startup(app: &AppHandle) {
    let dir = app.path().app_log_dir().map(|p| p.display().to_string()).unwrap_or_default();
    log::info!(
        "Brote {} arrancó en {} {} (logs en {dir})",
        app.package_info().version,
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    // Un pánico en cualquier hilo queda registrado antes de que se pierda.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Pánico: {info}");
        previous(info);
    }));
}

/// Ruta de una URL sin la consulta (la consulta puede llevar búsquedas o correos).
pub fn path_only(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(u) => format!("{}{}", u.host_str().unwrap_or_default(), u.path()),
        Err(_) => url.split('?').next().unwrap_or_default().to_string(),
    }
}

#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), String> {
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener().open_path(dir.to_string_lossy(), None::<&str>).map_err(|e| {
        log::warn!("No pude abrir la carpeta de logs: {e}");
        "No pude abrir la carpeta de logs".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ruta_no_lleva_la_consulta() {
        assert_eq!(
            path_only("https://gmail.googleapis.com/gmail/v1/users/me/messages?q=from:ana@x.cl"),
            "gmail.googleapis.com/gmail/v1/users/me/messages"
        );
        assert_eq!(path_only("no es url?q=secreto"), "no es url");
    }
}
