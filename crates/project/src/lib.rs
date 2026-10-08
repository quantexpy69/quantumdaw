//! Formato de proyecto `.qproj` (JSON), proyecto demo y exportación de audio.
mod export;

pub use export::{Format, export, write_wav};

use serde::{Deserialize, Serialize};
use std::{f32::consts::TAU, fs, path::Path};

pub const FILE: &str = "project.qproj";
pub const AUDIO_DIR: &str = "Audio Files";
pub const RENDER_DIR: &str = "Rendered";
pub const VERSION: u32 = 2;
pub const PALETTE: [[u8; 3]; 8] = [[0xF2, 0x6D, 0x6D], [0xF2, 0xB8, 0x5C], [0x6D, 0xD3, 0x9C], [0x6D, 0xB4, 0xF2], [0xB0, 0x8C, 0xF2], [0xF2, 0x8C, 0xD0], [0x5C, 0xD6, 0xD6], [0xC8, 0xD4, 0x5C]];

type Gen<'a> = dyn Fn(usize, f32) -> f32 + 'a;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Project {
    pub version: u32,
    /// Dispositivos de audio por nombre ("" = por defecto, entrada "-" = ninguna).
    pub output_device: String,
    pub input_device: String,
    /// 0 = frecuencia del dispositivo.
    pub sample_rate: u32,
    /// 0 = tamaño de buffer del sistema.
    pub buffer: u32,
    pub rec_bits: u16,
    pub bpm: f32,
    pub signature: (u32, u32),
    pub metronome: bool,
    pub looping: bool,
    pub loop_secs: (f64, f64),
    pub groups: Vec<GroupState>,
    pub tracks: Vec<TrackState>,
    /// Fundido entre segmentos del comp de tomas (ms).
    pub comp_xfade_ms: f32,
    /// Mapa de tempo: (inicio en segundos, BPM). Vacío = tempo único `bpm`.
    pub tempo_map: Vec<(f64, f32)>,
    /// Datos de la canción y derechos de autor.
    pub meta: SongMeta,
    /// Marcas de la línea de tiempo: (segundos, nombre).
    pub markers: Vec<(f64, String)>,
    /// Pista de acordes y pista de arreglo (Intro, Estrofa, Coro…): (inicio, fin en segundos, nombre).
    pub chords: Vec<(f64, f64, String)>,
    pub sections: Vec<(f64, f64, String)>,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SongMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub composer: String,
    pub producer: String,
    pub year: String,
    pub genre: String,
    pub copyright: String,
    pub publisher: String,
    pub isrc: String,
    pub notes: String,
    /// Quién creó la canción, cuándo (fecha y hora) y cuándo se guardó por última vez.
    pub created_by: String,
    pub created_at: String,
    pub saved_at: String,
}

/// Ficha de un proyecto: carpeta que agrupa varias canciones (`proyecto.json`).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Collection {
    pub name: String,
    pub author: String,
    pub created_at: String,
    pub description: String,
}

impl Collection {
    pub const FILE: &'static str = "proyecto.json";
    pub fn load(dir: &Path) -> Option<Self> {
        serde_json::from_str(&std::fs::read_to_string(dir.join(Self::FILE)).ok()?).ok()
    }
    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        Ok(std::fs::write(dir.join(Self::FILE), serde_json::to_string_pretty(self)?)?)
    }
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: VERSION,
            output_device: String::new(),
            input_device: String::new(),
            sample_rate: 0,
            buffer: 0,
            rec_bits: 24,
            bpm: 120.0,
            signature: (4, 4),
            metronome: false,
            looping: false,
            loop_secs: (0.0, 0.0),
            groups: vec![],
            tracks: vec![],
            comp_xfade_ms: 10.0,
            tempo_map: vec![],
            meta: SongMeta::default(),
            markers: vec![],
            chords: vec![],
            sections: vec![],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum TrackKind {
    AudioMono,
    #[default]
    AudioStereo,
    Midi,
    /// Pista de clic (metrónomo como audio).
    Click,
    /// Video de referencia (cine, publicidad): fotogramas en el timeline y visor sincronizado.
    Video,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct TrackState {
    pub name: String,
    pub color: [u8; 3],
    pub kind: TrackKind,
    pub group: Option<usize>,
    pub gain: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub clips: Vec<ClipState>,
    pub fx: Vec<FxState>,
    /// Primer canal de la interfaz usado como entrada.
    pub input: u16,
    pub in_gain_db: f32,
    pub monitor: bool,
    /// Automatización en (segundos, valor): volumen como posición de fader 0..1, panorama -1..1.
    pub vol_auto: Vec<(f64, f32)>,
    pub pan_auto: Vec<(f64, f32)>,
    /// Parámetros del QUANTUM Synth (pistas MIDI).
    pub synth: Vec<f32>,
    /// Instrumento de la pista MIDI: id del catálogo (instrumento real) o "synth:<preset>".
    pub instrument: String,
    /// Icono de la pista ("" = automático).
    pub icon: String,
    /// Pista de video: archivo (relativo al proyecto) y posición de inicio en segundos.
    pub video: String,
    pub video_start: f64,
    /// Altura del carril en píxeles (0 = por defecto).
    pub height: f32,
    /// Ruteo: envíos (índice de pista destino, ganancia) y si la salida NO va al master.
    pub sends: Vec<(usize, f32)>,
    pub no_master: bool,
    /// Tomas (una lista de regiones por toma) y comp: (inicio, fin, toma) en segundos.
    pub takes: Vec<Vec<ClipState>>,
    pub comp: Vec<(f64, f64, usize)>,
    pub show_takes: bool,
    /// Polaridad invertida.
    pub invert: bool,
    /// Modo de automatización: 0 Lectura, 1 Apagado, 2 Toque, 3 Retención, 4 Escritura.
    pub auto_mode: u8,
    /// Formato v1 (un archivo por pista); se migra a `clips` al cargar.
    #[serde(skip_serializing)]
    file: Option<String>,
    #[serde(skip_serializing)]
    start_secs: f64,
}

/// Tiempos en segundos; `file` es relativo a la carpeta del proyecto (vacío en regiones MIDI).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ClipState {
    pub file: String,
    pub start: f64,
    pub offset: f64,
    pub len: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub gain_db: f32,
    /// Notas MIDI: (inicio, duración, nota, velocidad), relativas al origen de la región.
    pub notes: Vec<(f64, f64, u8, u8)>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FxState {
    pub kind: String,
    pub params: Vec<f32>,
    pub bypass: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct GroupState {
    pub name: String,
    pub color: [u8; 3],
    /// Edición vinculada: seleccionar/mover regiones afecta a todas las pistas del grupo.
    pub edit: bool,
}

impl TrackState {
    pub fn new(name: String, kind: TrackKind, color: [u8; 3]) -> Self {
        Self { name, kind, color, gain: 1.0, ..Default::default() }
    }
}

impl Project {
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let mut p: Self = serde_json::from_str(&fs::read_to_string(dir.join(FILE))?)?;
        anyhow::ensure!(p.version <= VERSION, "proyecto de una versión más nueva ({})", p.version);
        for (i, t) in p.tracks.iter_mut().enumerate() {
            if let Some(file) = t.file.take() {
                t.clips.push(ClipState { file, start: t.start_secs, len: f64::MAX, ..Default::default() });
                t.color = PALETTE[i % PALETTE.len()];
            }
        }
        p.version = VERSION;
        Ok(p)
    }

    pub fn save(&self, dir: &Path) -> anyhow::Result<()> {
        for d in [AUDIO_DIR, RENDER_DIR, "Backups"] {
            fs::create_dir_all(dir.join(d))?;
        }
        Ok(fs::write(dir.join(FILE), serde_json::to_string_pretty(self)?)?)
    }

    /// Carga el proyecto o, si no existe, genera uno de demostración.
    pub fn open_or_demo(dir: &Path) -> anyhow::Result<Self> {
        if dir.join(FILE).exists() {
            return Self::load(dir);
        }
        let rate = 44_100;
        let len = 16 * rate; // 8 compases a 120 BPM
        let noise = |i: usize| (i as u32).wrapping_mul(2_654_435_761) as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let sines = |fs: &[f32], t: f32| fs.iter().map(|f| (TAU * f * t).sin()).sum::<f32>() / fs.len() as f32;
        let bar = |t: f32| (t / 2.0) as usize % 4;
        let gens: [(&str, &Gen<'_>); 4] = [
            ("Kick", &|_, t| {
                let p = t % 0.5;
                (TAU * (45.0 * p + 3.5 * (1.0 - (-30.0 * p).exp()))).sin() * (-8.0 * p).exp() * 0.9
            }),
            ("Hats", &|i, t| noise(i) * (-60.0 * ((t + 0.25) % 0.5)).exp() * 0.25),
            ("Bass", &|_, t| {
                let f = [55.0, 43.65, 65.41, 49.0][bar(t)];
                sines(&[f, f * 2.0, f * 3.0], t) * (-10.0 * (t % 0.25)).exp() * 0.6
            }),
            ("Pad", &|_, t| {
                let chords = [[220.0, 261.63, 329.63], [174.61, 220.0, 261.63], [261.63, 329.63, 392.0], [196.0, 246.94, 293.66]];
                sines(&chords[bar(t)], t) * (t % 2.0 * 2.0).min(1.0) * 0.3
            }),
        ];
        fs::create_dir_all(dir.join(AUDIO_DIR))?;
        let mut p = Self::default();
        for (i, (name, g)) in gens.into_iter().enumerate() {
            let file = format!("{AUDIO_DIR}/{name}.wav");
            let samples: Vec<f32> = (0..len).map(|i| g(i as usize, i as f32 / rate as f32)).collect();
            write_wav(&dir.join(&file), rate, 1, 32, &samples)?;
            let mut t = TrackState::new(name.into(), TrackKind::AudioMono, PALETTE[i]);
            t.gain = 0.5;
            t.clips.push(ClipState { file, len: 16.0, ..Default::default() });
            p.tracks.push(t);
        }
        p.groups.push(GroupState { name: "Ritmo".into(), color: PALETTE[6], edit: true });
        (p.tracks[0].group, p.tracks[1].group) = (Some(0), Some(0));
        p.save(dir)?;
        Ok(p)
    }
}
