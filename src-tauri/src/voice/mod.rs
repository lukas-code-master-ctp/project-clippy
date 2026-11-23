//! Voz: grabar con la tecla, transcribir en local con Whisper.

pub mod model;
pub mod recorder;
pub mod transcriber;

/// Menos de esto es un toque accidental de la tecla, no un comando.
pub const MIN_SPEECH_SAMPLES: usize = recorder::WHISPER_SAMPLE_RATE as usize * 3 / 10;

/// Prueba de punta a punta: baja el modelo real y transcribe una frase en español.
/// Es lenta y necesita internet, así que solo corre a pedido (en CI):
///   BROTE_TEST_WAV=frase.wav cargo test --release -- --ignored --nocapture
#[cfg(test)]
mod e2e {
    use super::{model, recorder, transcriber::Transcriber};

    #[test]
    #[ignore]
    fn baja_el_modelo_y_transcribe_en_espanol() {
        let wav = std::env::var("BROTE_TEST_WAV").expect("falta BROTE_TEST_WAV");
        let expected = std::env::var("BROTE_TEST_EXPECT").unwrap_or_else(|_| "reunión".into());

        let dir = std::env::temp_dir().join("brote-e2e");
        let model_path = dir.join(model::MODEL_FILE);
        if !model_path.exists() {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(model::download(&model_path, |p| {
                if p % 20 == 0 {
                    eprintln!("descarga {p}%");
                }
            }))
            .expect("no se pudo bajar el modelo");
        }

        let mut reader = hound::WavReader::open(&wav).expect("wav inválido");
        let spec = reader.spec();
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
                reader.samples::<i32>().map(|s| s.unwrap() as f32 / max).collect()
            }
            hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        };
        let mono = recorder::downmix(&samples, spec.channels as usize);
        let audio = recorder::resample_linear(&mono, spec.sample_rate, recorder::WHISPER_SAMPLE_RATE);

        let t = Transcriber::default();
        let start = std::time::Instant::now();
        t.load(&model_path).unwrap();
        let loaded = start.elapsed();
        let text = t.transcribe(&model_path, &audio).unwrap();
        eprintln!(
            "Modelo cargado en {:.2} s; {:.1} s de audio transcritos en {:.2} s: {text:?}",
            loaded.as_secs_f32(),
            audio.len() as f32 / 16_000.0,
            (start.elapsed() - loaded).as_secs_f32()
        );
        assert!(text.to_lowercase().contains(&expected.to_lowercase()), "esperaba {expected:?} en {text:?}");
    }
}
