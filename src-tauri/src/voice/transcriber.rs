//! Transcripción local con whisper.cpp. El audio nunca sale del equipo.

use anyhow::{anyhow, Context, Result};
use std::path::Path;
use std::sync::Mutex;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// El contexto (el modelo cargado en memoria) se crea una vez y se reutiliza.
#[derive(Default)]
pub struct Transcriber {
    ctx: Mutex<Option<WhisperContext>>,
}

impl Transcriber {
    /// Carga el modelo si aún no está en memoria. Tarda uno o dos segundos.
    pub fn load(&self, model: &Path) -> Result<()> {
        let mut guard = self.ctx.lock().unwrap();
        if guard.is_none() {
            let path = model.to_str().ok_or_else(|| anyhow!("ruta del modelo inválida"))?;
            let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
                .context("no pude cargar el modelo de voz")?;
            *guard = Some(ctx);
        }
        Ok(())
    }

    /// Transcribe audio mono de 16 kHz en español.
    pub fn transcribe(&self, model: &Path, audio: &[f32]) -> Result<String> {
        self.load(model)?;
        let guard = self.ctx.lock().unwrap();
        let ctx = guard.as_ref().expect("modelo cargado");
        let mut state = ctx.create_state().context("no pude preparar la transcripción")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("es"));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_single_segment(false);
        params.set_suppress_blank(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
        params.set_n_threads(threads as i32);

        // Whisper trabaja en ventanas de 30 s; un audio de menos de 1 s rinde peor, así que se rellena con silencio.
        let mut padded;
        let input = if audio.len() < 16_000 {
            padded = audio.to_vec();
            padded.resize(16_000 + 1_600, 0.0);
            &padded[..]
        } else {
            audio
        };

        state.full(params, input).context("falló la transcripción")?;
        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_string());
        }
        Ok(clean(&text))
    }
}

/// Quita espacios sobrantes y las etiquetas que Whisper inventa en el silencio.
pub fn clean(raw: &str) -> String {
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = text.to_lowercase();
    let hallucinations = ["[blank_audio]", "(música)", "[música]", "subtítulos realizados por la comunidad de amara.org"];
    if hallucinations.iter().any(|h| lower == *h) {
        return String::new();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn clean_normaliza_espacios_y_descarta_silencio() {
        assert_eq!(clean("  Hola   mundo \n"), "Hola mundo");
        assert_eq!(clean(" [BLANK_AUDIO] "), "");
    }
}
