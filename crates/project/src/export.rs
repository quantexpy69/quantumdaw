//! Codificadores de exportación: WAV, FLAC, MP3 (LAME) y OGG Vorbis.
use anyhow::anyhow;
use std::{
    fs,
    num::{NonZeroU8, NonZeroU32},
    path::Path,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    Wav16,
    Wav24,
    Wav32,
    Flac16,
    Flac24,
    Mp3,
    Ogg,
}

impl Format {
    pub const ALL: [Self; 7] = [Self::Wav16, Self::Wav24, Self::Wav32, Self::Flac16, Self::Flac24, Self::Mp3, Self::Ogg];

    pub fn label(self) -> &'static str {
        match self {
            Self::Wav16 => "WAV 16 bits",
            Self::Wav24 => "WAV 24 bits",
            Self::Wav32 => "WAV 32 bits float",
            Self::Flac16 => "FLAC 16 bits",
            Self::Flac24 => "FLAC 24 bits",
            Self::Mp3 => "MP3 320 kbps",
            Self::Ogg => "OGG Vorbis",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Self::Wav16 | Self::Wav24 | Self::Wav32 => "wav",
            Self::Flac16 | Self::Flac24 => "flac",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
        }
    }
}

/// Exporta audio estéreo intercalado. MP3 admite como máximo 48 kHz.
pub fn export(path: &Path, format: Format, rate: u32, samples: &[f32]) -> anyhow::Result<()> {
    match format {
        Format::Wav16 => write_wav(path, rate, 2, 16, samples),
        Format::Wav24 => write_wav(path, rate, 2, 24, samples),
        Format::Wav32 => write_wav(path, rate, 2, 32, samples),
        Format::Flac16 | Format::Flac24 => {
            use flacenc::{component::BitRepr, error::Verify};
            let bits = if format == Format::Flac16 { 16 } else { 24 };
            let config = flacenc::config::Encoder::default().into_verified().map_err(|e| anyhow!("{e:?}"))?;
            let source = flacenc::source::MemSource::from_samples(&to_int(samples, bits), 2, bits as usize, rate as usize);
            let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).map_err(|e| anyhow!("{e:?}"))?;
            let mut sink = flacenc::bitsink::ByteSink::new();
            stream.write(&mut sink).map_err(|e| anyhow!("{e:?}"))?;
            // flacenc anota el bloque final (más corto) como tamaño mínimo; la especificación lo
            // excluye y decodificadores estrictos rechazan el archivo. Mínimo = máximo (bloque fijo).
            let mut bytes = sink.as_slice().to_vec();
            bytes.copy_within(10..12, 8);
            Ok(fs::write(path, bytes)?)
        }
        Format::Mp3 => {
            use mp3lame_encoder::{Bitrate, Builder, FlushNoGap, InterleavedPcm, Quality};
            let err = |e: &dyn std::fmt::Debug| anyhow!("MP3: {e:?}");
            let mut b = Builder::new().ok_or_else(|| anyhow!("no se pudo iniciar LAME"))?;
            b.set_num_channels(2).map_err(|e| err(&e))?;
            b.set_sample_rate(rate).map_err(|e| err(&e))?;
            b.set_brate(Bitrate::Kbps320).map_err(|e| err(&e))?;
            b.set_quality(Quality::Best).map_err(|e| err(&e))?;
            let mut enc = b.build().map_err(|e| err(&e))?;
            // LAME interpreta un buffer de salida de tamaño 0 como "ilimitado": hay que reservar antes.
            let mut out = Vec::with_capacity(mp3lame_encoder::max_required_buffer_size(samples.len() / 2) + 7200);
            enc.encode_to_vec(InterleavedPcm(samples), &mut out).map_err(|e| err(&e))?;
            enc.flush_to_vec::<FlushNoGap>(&mut out).map_err(|e| err(&e))?;
            Ok(fs::write(path, out)?)
        }
        Format::Ogg => {
            let rate = NonZeroU32::new(rate).ok_or_else(|| anyhow!("frecuencia inválida"))?;
            let mut enc = vorbis_rs::VorbisEncoderBuilder::new(rate, NonZeroU8::new(2).unwrap(), fs::File::create(path)?)?.build()?;
            let (l, r): (Vec<f32>, Vec<f32>) = samples.chunks(2).map(|c| (c[0], c[1])).unzip();
            for (a, b) in l.chunks(4096).zip(r.chunks(4096)) {
                enc.encode_audio_block([a, b])?;
            }
            enc.finish()?;
            Ok(())
        }
    }
}

fn to_int(samples: &[f32], bits: u16) -> Vec<i32> {
    let max = ((1i64 << (bits - 1)) - 1) as f32;
    samples.iter().map(|s| (s.clamp(-1.0, 1.0) * max) as i32).collect()
}

/// Escribe un WAV PCM entero (16/24) o float (32) con muestras intercaladas.
pub fn write_wav(path: &Path, rate: u32, channels: u16, bits: u16, samples: &[f32]) -> anyhow::Result<()> {
    let float = bits == 32;
    let sample_format = if float { hound::SampleFormat::Float } else { hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, hound::WavSpec { channels, sample_rate: rate, bits_per_sample: bits, sample_format })?;
    if float {
        samples.iter().try_for_each(|&s| w.write_sample(s))?;
    } else {
        to_int(samples, bits).into_iter().try_for_each(|s| w.write_sample(s))?;
    }
    Ok(w.finalize()?)
}
