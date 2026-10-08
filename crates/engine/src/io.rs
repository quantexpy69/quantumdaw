//! Decodificación de archivos (symphonia) y remuestreo.
use std::{fs::File, path::Path};
use symphonia::core::{codecs::audio::AudioDecoderOptions, formats::TrackType, io::MediaSourceStream};

/// Decodifica WAV, FLAC, OGG, MP3, AAC/M4A o ALAC a estéreo, remuestreado a `rate`.
pub fn decode(path: &Path, rate: u32) -> anyhow::Result<Vec<[f32; 2]>> {
    let mss = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = symphonia::core::formats::probe::Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe().probe(&hint, mss, Default::default(), Default::default())?;
    let track = format.default_track(TrackType::Audio).ok_or_else(|| anyhow::anyhow!("sin pista de audio"))?;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or_else(|| anyhow::anyhow!("códec no soportado"))?;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default())?;
    let (id, mut src_rate) = (track.id, params.sample_rate.unwrap_or(rate));
    let (mut frames, mut tmp) = (Vec::new(), Vec::<f32>::new());
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            // Algunos contenedores señalan el final con un EOF en lugar de `None`.
            Ok(None) | Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id != id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(e.into()),
        };
        let ch = buf.spec().channels().count().max(1);
        src_rate = buf.spec().rate();
        tmp.resize(buf.samples_interleaved(), 0.0);
        buf.copy_to_slice_interleaved(&mut tmp);
        frames.extend(tmp.chunks(ch).map(|c| [c[0], *c.get(1).unwrap_or(&c[0])]));
    }
    Ok(resample(frames, src_rate, rate))
}

/// Remuestreo lineal de `from` a `to` Hz (suficiente para esta etapa del proyecto).
pub fn resample(frames: Vec<[f32; 2]>, from: u32, to: u32) -> Vec<[f32; 2]> {
    if from == to || frames.is_empty() {
        return frames;
    }
    resample_by(&frames, from as f64 / to as f64)
}

/// Remuestreo lineal avanzando `step` muestras de entrada por cada muestra de salida.
pub fn resample_by(frames: &[[f32; 2]], step: f64) -> Vec<[f32; 2]> {
    if frames.is_empty() {
        return vec![];
    }
    (0..(frames.len() as f64 / step) as usize)
        .map(|i| {
            let x = i as f64 * step;
            let (a, b, f) = (frames[(x as usize).min(frames.len() - 1)], frames[(x as usize + 1).min(frames.len() - 1)], x.fract() as f32);
            [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f]
        })
        .collect()
}
