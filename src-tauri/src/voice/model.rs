//! Descarga y ubicación del modelo de Whisper.
//!
//! El modelo pesa unos 190 MB, así que no va dentro del instalador: se descarga
//! una sola vez al primer arranque y queda en la carpeta de datos de la app.

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;

/// `small` cuantizado: buen equilibrio entre precisión en español y velocidad en CPU.
pub const MODEL_FILE: &str = "ggml-small-q5_1.bin";
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin";
/// Un archivo más chico que esto es una descarga cortada o una página de error.
const MIN_MODEL_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ModelStatus {
    Missing,
    Downloading { percent: u8 },
    Ready,
    Failed { message: String },
}

pub struct ModelState {
    pub status: Mutex<ModelStatus>,
}

impl Default for ModelState {
    fn default() -> Self {
        Self { status: Mutex::new(ModelStatus::Missing) }
    }
}

/// Ruta del modelo. `BROTE_WHISPER_MODEL` permite usar otro archivo (útil para pruebas).
pub fn model_path(app: &AppHandle) -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("BROTE_WHISPER_MODEL") {
        return Ok(PathBuf::from(custom));
    }
    let dir = app.path().app_data_dir().context("no encontré la carpeta de datos de la app")?;
    Ok(dir.join("models").join(MODEL_FILE))
}

fn is_valid(path: &PathBuf) -> bool {
    let min = if std::env::var("BROTE_WHISPER_MODEL").is_ok() { 1 } else { MIN_MODEL_BYTES };
    std::fs::metadata(path).map(|m| m.len() >= min).unwrap_or(false)
}

fn set_status(app: &AppHandle, status: ModelStatus) {
    *app.state::<ModelState>().status.lock().unwrap() = status;
}

pub fn status(app: &AppHandle) -> ModelStatus {
    app.state::<ModelState>().status.lock().unwrap().clone()
}

/// Deja el modelo listo: si ya existe no hace nada; si no, lo descarga.
/// Llama a `on_progress` con cada punto porcentual nuevo.
pub async fn ensure_model(app: AppHandle, on_progress: impl Fn(u8)) -> Result<PathBuf> {
    let path = model_path(&app)?;
    if is_valid(&path) {
        set_status(&app, ModelStatus::Ready);
        return Ok(path);
    }
    set_status(&app, ModelStatus::Downloading { percent: 0 });
    match download(&path, |p| {
        set_status(&app, ModelStatus::Downloading { percent: p });
        on_progress(p);
    })
    .await
    {
        Ok(()) => {
            set_status(&app, ModelStatus::Ready);
            Ok(path)
        }
        Err(e) => {
            set_status(&app, ModelStatus::Failed { message: format!("{e:#}") });
            Err(e)
        }
    }
}

pub(crate) async fn download(path: &PathBuf, on_progress: impl Fn(u8)) -> Result<()> {
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let partial = path.with_extension("part");
    let response = reqwest::get(MODEL_URL).await.context("no pude conectarme para bajar el modelo de voz")?;
    if !response.status().is_success() {
        return Err(anyhow!("la descarga del modelo respondió {}", response.status()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&partial).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_percent = 0u8;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("se cortó la descarga del modelo")?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let p = ((downloaded * 100) / total).min(100) as u8;
            if p != last_percent {
                last_percent = p;
                on_progress(p);
            }
        }
    }
    file.flush().await?;
    drop(file);
    if downloaded < MIN_MODEL_BYTES {
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(anyhow!("el modelo descargado está incompleto ({downloaded} bytes)"));
    }
    tokio::fs::rename(&partial, path).await?;
    Ok(())
}
