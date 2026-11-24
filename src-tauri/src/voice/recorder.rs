//! Captura del micrófono mientras se mantiene la tecla para hablar.
//!
//! El stream de cpal no es `Send` en todas las plataformas, así que vive en un hilo propio:
//! `start` abre el micrófono en ese hilo y `stop` lo cierra y devuelve el audio
//! ya convertido a mono, 16 kHz, f32, que es lo que Whisper espera.

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

/// Grabación en curso: señal para detenerla y el hilo que devuelve el audio.
struct Active {
    stop: mpsc::Sender<()>,
    handle: JoinHandle<Result<Vec<f32>>>,
}

#[derive(Default)]
pub struct Recorder {
    active: Mutex<Option<Active>>,
}

impl Recorder {
    pub fn is_recording(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    /// Abre el micrófono por defecto y empieza a acumular audio.
    /// Espera a que el stream esté abierto para poder informar errores (sin micrófono, sin permiso).
    pub fn start(&self) -> Result<()> {
        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return Ok(());
        }
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

        let handle = std::thread::spawn(move || -> Result<Vec<f32>> {
            let opened = open_stream();
            let (stream, buffer, rate, channels) = match opened {
                Ok(v) => {
                    let _ = ready_tx.send(Ok(()));
                    v
                }
                Err(e) => {
                    let msg = format!("{e:#}");
                    let _ = ready_tx.send(Err(e));
                    return Err(anyhow!(msg));
                }
            };
            // Bloquea hasta que llegue la señal de parar (o se suelte el emisor).
            let _ = stop_rx.recv();
            drop(stream);
            let raw = std::mem::take(&mut *buffer.lock().unwrap());
            Ok(resample_linear(&downmix(&raw, channels), rate, WHISPER_SAMPLE_RATE))
        });

        match ready_rx.recv() {
            Ok(Ok(())) => {
                *active = Some(Active { stop: stop_tx, handle });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => Err(anyhow!("el hilo de audio terminó sin abrir el micrófono")),
        }
    }

    /// Detiene la grabación y devuelve el audio a 16 kHz mono.
    pub fn stop(&self) -> Result<Vec<f32>> {
        let active = self.active.lock().unwrap().take().ok_or_else(|| anyhow!("no había grabación en curso"))?;
        let _ = active.stop.send(());
        active.handle.join().map_err(|_| anyhow!("el hilo de audio falló"))?
    }
}

type Opened = (cpal::Stream, Arc<Mutex<Vec<f32>>>, u32, usize);

fn open_stream() -> Result<Opened> {
    let host = cpal::default_host();
    let device = host.default_input_device().context("no encontré un micrófono")?;
    let config = device.default_input_config().context("no pude leer la configuración del micrófono")?;
    let rate = config.sample_rate();
    let channels = config.channels() as usize;
    let format = config.sample_format();
    let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * channels * 15)));

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config.into(), buffer.clone())?,
        SampleFormat::I16 => build::<i16>(&device, config.into(), buffer.clone())?,
        SampleFormat::U16 => build::<u16>(&device, config.into(), buffer.clone())?,
        SampleFormat::I32 => build::<i32>(&device, config.into(), buffer.clone())?,
        SampleFormat::I8 => build::<i8>(&device, config.into(), buffer.clone())?,
        SampleFormat::U8 => build::<u8>(&device, config.into(), buffer.clone())?,
        other => return Err(anyhow!("formato de audio no soportado: {other:?}")),
    };
    stream.play().context("no pude iniciar la grabación")?;
    log::info!("Micrófono: {} Hz, {channels} canal(es), {format:?}", rate);
    Ok((stream, buffer, rate, channels))
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, buffer: Arc<Mutex<Vec<f32>>>) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &_| {
            if let Ok(mut buf) = buffer.lock() {
                buf.extend(data.iter().map(|s| <f32 as FromSample<T>>::from_sample_(*s)));
            }
        },
        |err| log::error!("Error en el stream de audio: {err}"),
        None,
    )?;
    Ok(stream)
}

/// Promedia los canales intercalados en uno solo.
pub fn downmix(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect()
}

/// Remuestreo lineal. Basta para voz y evita sumar otra dependencia.
pub fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let out_len = ((input.len() as f64) / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx];
            let b = *input.get(idx + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_promedia_canales() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5], 2), vec![0.5, 0.5]);
        assert_eq!(downmix(&[0.1, 0.2], 1), vec![0.1, 0.2]);
    }

    #[test]
    fn resample_48k_a_16k_conserva_duracion() {
        let one_second = vec![0.0f32; 48_000];
        assert_eq!(resample_linear(&one_second, 48_000, 16_000).len(), 16_000);
        let ramp: Vec<f32> = (0..4).map(|i| i as f32).collect();
        assert_eq!(resample_linear(&ramp, 32_000, 16_000), vec![0.0, 2.0]);
    }
}
