//! Motor de audio en tiempo real. El hilo de audio solo lee atómicos, un `ArcSwap`
//! con el grafo de pistas y ring buffers sin locks: no bloquea ni reserva memoria.
mod devices;
mod dsp;
mod fx;
mod io;
mod sampler;
mod synth;
mod tempo;

pub use devices::{DeviceInfo, MidiConnections, audio_devices, connect_midi, midi_ports};
pub use dsp::{detect_bpm, find_peaks, find_silences, peak, pitch_shift, stretch};
pub use fx::{Fx, FxKind, eq_response};
pub use io::{decode, resample};
pub use sampler::Sampler;
pub use synth::{SYNTH_PARAMS, Synth};
pub use tempo::{beat_len, beats_at, bpm_at, frame_at_beat};

use arc_swap::{ArcSwap, ArcSwapOption};
use cpal::traits::{DeviceTrait, StreamTrait};
use std::{
    f32::consts::FRAC_PI_2,
    f64::consts::TAU,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed},
    },
};

pub const BLOCK: usize = 1024;
pub const PEAK_BLOCK: usize = 256;
const FADE: u64 = 64;
const MAX_EVENTS: usize = 256;
pub type MidiEvent = [u8; 3];

#[derive(Default)]
pub struct AtomicF32(AtomicU32);
impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Relaxed))
    }
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Relaxed)
    }
    /// Válido para valores >= 0, cuyo orden de bits coincide con el numérico.
    fn max(&self, v: f32) {
        self.0.fetch_max(v.to_bits(), Relaxed);
    }
    /// Lee el pico acumulado y lo reinicia (usado por los medidores de la UI).
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Relaxed))
    }
}

/// Posición del fader (0..1, curva cuadrática sobre -60..+6 dB) a ganancia lineal.
pub fn fader_gain(pos: f32) -> f32 {
    if pos <= 1e-4 { 0.0 } else { 10f32.powf((pos.min(1.0).sqrt() * 66.0 - 60.0) / 20.0) }
}
pub fn fader_pos(gain: f32) -> f32 {
    if gain <= 0.0 { 0.0 } else { ((20.0 * gain.log10() + 60.0) / 66.0).clamp(0.0, 1.0).powi(2) }
}

/// Valor interpolado de una curva de automatización en el frame `f`.
pub fn automation_at(points: &[(u64, f32)], f: u64) -> Option<f32> {
    let i = points.partition_point(|p| p.0 <= f);
    match (i.checked_sub(1).map(|j| points[j]), points.get(i).copied()) {
        (Some(a), Some(b)) => Some(a.1 + (b.1 - a.1) * (f - a.0) as f32 / (b.0 - a.0).max(1) as f32),
        (Some(p), None) | (None, Some(p)) => Some(p.1),
        (None, None) => None,
    }
}

/// Audio decodificado de un archivo, compartido entre todos sus clips.
pub struct AudioBuf {
    pub file: String,
    pub frames: Vec<[f32; 2]>,
    /// Pico absoluto por bloque de `PEAK_BLOCK` frames, para dibujar la onda.
    pub peaks: Vec<f32>,
}

impl AudioBuf {
    pub fn new(file: String, frames: Vec<[f32; 2]>) -> Arc<Self> {
        let peaks = frames.chunks(PEAK_BLOCK).map(|c| c.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()))).collect();
        Arc::new(Self { file, frames, peaks })
    }
}

/// Nota MIDI; `start` es relativo al origen del material (inicio del clip menos `offset`).
#[derive(Clone, Copy, PartialEq)]
pub struct Note {
    pub start: u64,
    pub len: u64,
    pub key: u8,
    pub vel: u8,
}

/// Material de un clip: audio decodificado o notas MIDI (inmutables, compartidos).
#[derive(Clone)]
pub enum Source {
    Audio(Arc<AudioBuf>),
    Midi(Arc<Vec<Note>>),
}

/// Región no destructiva colocada en el timeline (en frames).
#[derive(Clone)]
pub struct Clip {
    pub src: Source,
    pub start: u64,
    pub offset: u64,
    pub len: u64,
    pub fade_in: u64,
    pub fade_out: u64,
    pub gain: f32,
    /// Estado de selección de la UI.
    pub selected: bool,
}

impl Clip {
    pub fn new(src: Source, start: u64, len: u64) -> Self {
        Self { src, start, offset: 0, len, fade_in: 0, fade_out: 0, gain: 1.0, selected: false }
    }
    pub fn audio(buf: Arc<AudioBuf>, start: u64) -> Self {
        Self::new(Source::Audio(buf.clone()), start, buf.frames.len() as u64)
    }
    pub fn end(&self) -> u64 {
        self.start + self.len
    }
    pub fn buf(&self) -> Option<&Arc<AudioBuf>> {
        if let Source::Audio(b) = &self.src { Some(b) } else { None }
    }
    pub fn notes(&self) -> Option<&Arc<Vec<Note>>> {
        if let Source::Midi(n) = &self.src { Some(n) } else { None }
    }
    /// Frame absoluto donde estaría el inicio del material.
    pub fn origin(&self) -> i64 {
        self.start as i64 - self.offset as i64
    }

    /// Recorta el clip a `[a, b)`; `None` si no se solapan. Los fades solo se conservan en los bordes originales.
    pub fn trim(&self, a: u64, b: u64) -> Option<Self> {
        let (s, e) = (self.start.max(a), self.end().min(b));
        (s < e).then(|| Self {
            start: s,
            offset: self.offset + s - self.start,
            len: e - s,
            fade_in: if s == self.start { self.fade_in } else { 0 },
            fade_out: if e == self.end() { self.fade_out } else { 0 },
            ..self.clone()
        })
    }

    fn gain_at(&self, t: u64) -> f32 {
        let (rel, to_end) = (t - self.start, self.end() - t);
        // Micro-fundido en los bordes (evita clics al cortar) y fades con curva de igual potencia.
        let mut g = self.gain * ((rel.min(to_end - 1)) as f32 / FADE as f32).min(1.0);
        if rel < self.fade_in {
            g *= (rel as f32 / self.fade_in as f32 * FRAC_PI_2).sin();
        }
        if to_end <= self.fade_out {
            g *= (to_end as f32 / self.fade_out as f32 * FRAC_PI_2).sin();
        }
        g
    }

    fn read(&self, pos: u64, out: &mut [[f32; 2]]) {
        let Source::Audio(buf) = &self.src else {
            return;
        };
        for t in pos.max(self.start)..(pos + out.len() as u64).min(self.end()) {
            let Some(s) = buf.frames.get((self.offset + t - self.start) as usize) else {
                break;
            };
            let g = self.gain_at(t);
            let o = &mut out[(t - pos) as usize];
            o[0] += s[0] * g;
            o[1] += s[1] * g;
        }
    }

    /// Añade a `ev` los note on/off que caen en `[pos, end)` (offset dentro del bloque).
    fn events(&self, pos: u64, end: u64, ev: &mut Events) {
        let Source::Midi(notes) = &self.src else {
            return;
        };
        let origin = self.origin();
        for n in notes.iter() {
            let on = origin + n.start as i64;
            let off = (on + n.len as i64).min(self.end() as i64);
            if on < self.start as i64 || on >= self.end() as i64 {
                continue;
            }
            if (pos as i64..end as i64).contains(&on) {
                ev.push((on - pos as i64) as usize, [0x90, n.key, n.vel.max(1)]);
            }
            if (pos as i64..end as i64).contains(&off) {
                ev.push((off - pos as i64) as usize, [0x80, n.key, 0]);
            }
        }
    }
}

/// Lista fija de eventos MIDI de un bloque (sin reservas en el hilo de audio).
pub struct Events {
    pub items: [(usize, MidiEvent); MAX_EVENTS],
    pub len: usize,
}

impl Events {
    fn new() -> Self {
        Self { items: [(0, [0; 3]); MAX_EVENTS], len: 0 }
    }
    fn push(&mut self, at: usize, e: MidiEvent) {
        if self.len < MAX_EVENTS {
            self.items[self.len] = (at, e);
            self.len += 1;
        }
    }
    pub fn as_slice(&self) -> &[(usize, MidiEvent)] {
        &self.items[..self.len]
    }
    fn sort(&mut self) {
        self.items[..self.len].sort_unstable_by_key(|e| (e.0, e.1[0] & 0xF0 == 0x90));
    }
}

/// Parámetros de canal compartidos entre la UI y el hilo de audio.
pub struct Params {
    pub gain: AtomicF32,
    pub pan: AtomicF32,
    pub mute: AtomicBool,
    pub solo: AtomicBool,
    pub arm: AtomicBool,
    /// Monitoreo de entrada (botón I).
    pub monitor: AtomicBool,
    pub in_gain: AtomicF32,
    pub in_meter: AtomicF32,
    pub meter: AtomicF32,
    /// Polaridad invertida (botón Ø).
    pub invert: AtomicBool,
    /// La automatización mueve fader y panorama (falso en modo Off o mientras se escribe).
    pub auto_read: AtomicBool,
    /// Ganancias efectivas L/R y de monitoreo del bloque anterior (suavizado sin clics).
    cur: [AtomicF32; 3],
}

impl Default for Params {
    fn default() -> Self {
        let f = || AtomicBool::new(false);
        Self {
            gain: AtomicF32::new(1.0),
            pan: Default::default(),
            mute: f(),
            solo: f(),
            arm: f(),
            monitor: f(),
            in_gain: AtomicF32::new(1.0),
            invert: f(),
            auto_read: AtomicBool::new(true),
            in_meter: Default::default(),
            meter: Default::default(),
            cur: Default::default(),
        }
    }
}

/// Rampa lineal de `from` a `to` a lo largo de `n` muestras.
fn ramp(from: f32, to: f32, n: usize) -> impl Iterator<Item = f32> {
    (0..n).map(move |i| from + (to - from) * (i + 1) as f32 / n as f32)
}

impl Params {
    /// Aplica volumen y panorama con rampa desde el bloque anterior; devuelve false si quedó en silencio.
    fn apply(&self, buf: &mut [[f32; 2]], gain: f32, audible: bool) -> bool {
        let p = self.pan.get();
        let gain = if self.invert.load(Relaxed) { -gain } else { gain };
        let (tl, tr) = if audible { (gain * (1.0 - p).min(1.0), gain * (1.0 + p).min(1.0)) } else { (0.0, 0.0) };
        let (l0, r0) = (self.cur[0].get(), self.cur[1].get());
        self.cur[0].set(tl);
        self.cur[1].set(tr);
        if l0 == 0.0 && r0 == 0.0 && tl == 0.0 && tr == 0.0 {
            return false;
        }
        let (mut peak, n) = (0f32, buf.len());
        for ((s, gl), gr) in buf.iter_mut().zip(ramp(l0, tl, n)).zip(ramp(r0, tr, n)) {
            *s = [s[0] * gl, s[1] * gr];
            peak = peak.max(s[0].abs()).max(s[1].abs());
        }
        if audible {
            self.meter.max(peak);
        }
        true
    }
}

/// Pista tal como la ve el hilo de audio.
pub struct Node {
    pub params: Arc<Params>,
    pub clips: Vec<Clip>,
    pub fx: Vec<Arc<Fx>>,
    /// Instrumento de las pistas MIDI: sampler (instrumento real) si lo hay, si no el sintetizador.
    pub synth: Option<Arc<Synth>>,
    pub sampler: Option<Arc<Sampler>>,
    /// Canales de la interfaz (izquierdo, derecho) que alimentan la pista.
    pub input: Option<(usize, usize)>,
    /// Automatización en (frame, valor): volumen como posición de fader 0..1, panorama -1..1.
    pub vol_auto: Vec<(u64, f32)>,
    pub pan_auto: Vec<(u64, f32)>,
    /// Envíos post-fader: (posición del destino en el grafo, ganancia). El grafo está ordenado
    /// topológicamente, así que el destino siempre se procesa después del origen.
    pub sends: Vec<(usize, f32)>,
    pub to_master: bool,
    /// Recibe envíos de otros canales (bus): sigue sonando aunque haya pistas en solo.
    pub bus: bool,
    /// Pista de clic: genera el metrónomo como audio (exportable como stem).
    pub click: bool,
    /// Señal recibida de otros canales en el bloque actual (solo la toca el hilo de audio).
    pub recv: Mutex<Vec<[f32; 2]>>,
}

impl Node {
    pub fn recv_buffer() -> Mutex<Vec<[f32; 2]>> {
        Mutex::new(vec![[0.0; 2]; BLOCK])
    }
}

/// Dispositivos y formato de audio. Los dispositivos son nodos de PipeWire ("" = por defecto, entrada "-" = ninguna).
#[derive(Clone, Default, PartialEq)]
pub struct AudioConfig {
    pub output: String,
    pub input: String,
    pub rate: u32,
    pub buffer: u32,
}

/// Entradas del bloque para el render: audio de la interfaz y eventos MIDI en vivo.
#[derive(Clone, Copy)]
pub struct Live<'a> {
    pub audio: &'a [f32],
    pub midi: &'a [MidiEvent],
}

pub struct Engine {
    pub graph: ArcSwap<Vec<Node>>,
    pub master: Params,
    pub playing: AtomicBool,
    pub pos: AtomicU64,
    pub looping: AtomicBool,
    pub loop_start: AtomicU64,
    pub loop_end: AtomicU64,
    pub metronome: AtomicBool,
    pub bpm: AtomicF32,
    /// Compás: `beats` / `unit` (p. ej. 6/8).
    pub beats: AtomicU32,
    pub unit: AtomicU32,
    pub sample_rate: u32,
    pub in_channels: usize,
    pub input_error: Option<String>,
    /// Grabación: el hilo de audio copia la entrada a `rec_rx` (y el MIDI a `midi_rec_rx`) desde `rec_start`.
    pub recording: AtomicBool,
    pub rec_start: AtomicU64,
    pub rec_rx: Mutex<rtrb::Consumer<f32>>,
    pub midi_rec_rx: Mutex<rtrb::Consumer<(u64, MidiEvent)>>,
    /// MIDI en vivo (teclados USB y teclado del piano roll) hacia el hilo de audio.
    pub midi_tx: Arc<Mutex<rtrb::Producer<MidiEvent>>>,
    /// Salida del master (mono) para el analizador, activa solo si `scope_on`.
    pub scope_on: AtomicBool,
    pub scope_rx: Mutex<rtrb::Consumer<f32>>,
    /// Mapa de tempo (frame, BPM) que sigue el metrónomo.
    pub tempo: ArcSwap<Vec<(u64, f32)>>,
    /// Previsualización de la biblioteca: audio, posición, loop y volumen (independiente del transporte).
    pub preview: ArcSwapOption<Vec<[f32; 2]>>,
    pub preview_pos: AtomicU64,
    pub preview_loop: AtomicBool,
    pub preview_gain: AtomicF32,
    /// Render offline previo al fader (proxys y MIDI a audio): instrumento + efectos de la pista.
    prefader: AtomicBool,
}

/// Windows y macOS: el dispositivo con ese id de cpal, o el predeterminado del sistema.
#[cfg(not(target_os = "linux"))]
fn device(input: bool, node: &str) -> Option<cpal::Device> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    node.parse().ok().and_then(|id| host.device_by_id(&id)).or_else(|| if input { host.default_input_device() } else { host.default_output_device() })
}

/// Linux: abre el dispositivo "pipewire" de ALSA apuntando al nodo pedido (o el dispositivo por defecto sin PipeWire).
#[cfg(target_os = "linux")]
fn device(input: bool, node: &str) -> Option<cpal::Device> {
    use cpal::traits::HostTrait;
    let host = cpal::default_host();
    match "alsa:pipewire".parse().ok().and_then(|id| host.device_by_id(&id)) {
        Some(d) => {
            // SAFETY: se llama desde el hilo de la UI al (re)abrir el motor; el plugin ALSA de
            // PipeWire lee PIPEWIRE_NODE al abrir el PCM para elegir el dispositivo destino.
            unsafe {
                match node {
                    "" => std::env::remove_var("PIPEWIRE_NODE"),
                    n => std::env::set_var("PIPEWIRE_NODE", n),
                }
            }
            Some(d)
        }
        None if input => host.default_input_device(),
        None => host.default_output_device(),
    }
}

impl Engine {
    /// Abre salida y entrada con la misma frecuencia y prioridad de tiempo real.
    pub fn start(cfg: &AudioConfig) -> anyhow::Result<(Arc<Self>, Vec<cpal::Stream>)> {
        let out_dev = device(false, &cfg.output).ok_or_else(|| anyhow::anyhow!("sin dispositivo de salida"))?;
        let mut config = out_dev.default_output_config()?.config();
        if cfg.rate > 0 {
            config.sample_rate = cfg.rate;
        }
        if cfg.buffer > 0 {
            config.buffer_size = cpal::BufferSize::Fixed(cfg.buffer);
        }
        let mut streams = vec![];
        let in_dev = if cfg.input == "-" { None } else { device(true, &cfg.input) };
        let in_cfg = in_dev.as_ref().and_then(|d| d.default_input_config().ok()).map(|c| cpal::StreamConfig { channels: c.channels(), ..config });
        let mut in_ch = in_cfg.map_or(0, |c| c.channels as usize);
        let sr = config.sample_rate as usize;
        let (mut in_tx, in_rx) = rtrb::RingBuffer::new((sr * in_ch).max(1));
        let (rec_tx, rec_rx) = rtrb::RingBuffer::new((sr * in_ch * 4).max(1));
        let (midi_rec_tx, midi_rec_rx) = rtrb::RingBuffer::new(4096);
        let (midi_tx, midi_rx) = rtrb::RingBuffer::new(1024);
        let (scope_tx, scope_rx) = rtrb::RingBuffer::new(sr);
        let mut input_error = None;
        if let (Some(dev), Some(in_cfg)) = (in_dev, in_cfg) {
            let cb = move |d: &[f32], _: &_| d.iter().for_each(|&s| _ = in_tx.push(s));
            match dev.build_input_stream(in_cfg, cb, |err| eprintln!("error de entrada: {err}"), None).map_err(anyhow::Error::from).and_then(|s| Ok(s.play().map(|_| s)?)) {
                Ok(s) => streams.push(s),
                Err(e) => (input_error, in_ch) = (Some(e.to_string()), 0),
            }
        }
        let engine = Arc::new(Self {
            graph: Default::default(),
            master: Default::default(),
            playing: false.into(),
            pos: 0.into(),
            looping: false.into(),
            loop_start: 0.into(),
            loop_end: 0.into(),
            metronome: false.into(),
            bpm: AtomicF32::new(120.0),
            beats: 4.into(),
            unit: 4.into(),
            sample_rate: config.sample_rate,
            in_channels: in_ch,
            input_error,
            recording: false.into(),
            rec_start: 0.into(),
            rec_rx: Mutex::new(rec_rx),
            midi_rec_rx: Mutex::new(midi_rec_rx),
            midi_tx: Arc::new(Mutex::new(midi_tx)),
            scope_on: false.into(),
            scope_rx: Mutex::new(scope_rx),
            tempo: ArcSwap::from_pointee(vec![(0, 120.0)]),
            preview: ArcSwapOption::empty(),
            preview_pos: 0.into(),
            preview_loop: true.into(),
            preview_gain: AtomicF32::new(0.8),
            prefader: AtomicBool::new(false),
        });
        let out_dev = device(false, &cfg.output).unwrap_or(out_dev);
        let cb = callback(engine.clone(), config.channels as usize, Rings { in_rx, rec_tx, midi_rx, midi_rec_tx, scope_tx });
        match out_dev.build_output_stream(config, cb, |err| eprintln!("error de audio: {err}"), None) {
            Ok(s) => {
                s.play()?;
                streams.push(s);
                Ok((engine, streams))
            }
            // Si el dispositivo no acepta el tamaño de buffer pedido, se usa el del sistema.
            Err(_) if cfg.buffer > 0 => Self::start(&AudioConfig { buffer: 0, ..cfg.clone() }),
            Err(e) => Err(e.into()),
        }
    }

    pub fn beat_frames(&self) -> f64 {
        self.sample_rate as f64 * 60.0 / self.bpm.get() as f64 * 4.0 / self.unit.load(Relaxed) as f64
    }

    /// Envía un evento MIDI en vivo (p. ej. tocar una tecla del piano roll).
    pub fn send_midi(&self, e: MidiEvent) {
        if let Ok(mut tx) = self.midi_tx.lock() {
            let _ = tx.push(e);
        }
    }

    /// Mezcla en `out` desde el frame `pos`. `only`: renderiza solo esa pista (exportación por canal).
    pub fn render(&self, out: &mut [[f32; 2]], pos: u64, playing: bool, offline: bool, live: Live, only: Option<&Arc<Params>>) {
        out.fill([0.0; 2]);
        let mut tmp = [[0f32; 2]; BLOCK];
        let tmp = &mut tmp[..out.len()];
        let ich = live.audio.len() / out.len().max(1);
        let end = pos + out.len() as u64;
        let sr = self.sample_rate as f32;
        let graph = self.graph.load();
        let any_solo = graph.iter().any(|n| n.params.solo.load(Relaxed));
        let bpm_now = bpm_at(&self.tempo.load(), pos as f64);
        for node in graph.iter() {
            let p = &node.params;
            tmp.fill([0.0; 2]);
            // Envíos recibidos de otros canales en este bloque.
            if let Ok(mut recv) = node.recv.try_lock() {
                for (t, r) in tmp.iter_mut().zip(recv.iter_mut()) {
                    *t = [t[0] + r[0], t[1] + r[1]];
                    *r = [0.0; 2];
                }
            }
            if node.click && playing {
                self.click(tmp, pos);
            }
            if playing {
                node.clips.iter().for_each(|c| c.read(pos, tmp));
            }
            let listening = p.monitor.load(Relaxed) || p.arm.load(Relaxed);
            if node.synth.is_some() || node.sampler.is_some() {
                let mut ev = Events::new();
                if playing {
                    node.clips.iter().for_each(|c| c.events(pos, end, &mut ev));
                }
                if listening {
                    live.midi.iter().for_each(|&e| ev.push(0, e));
                }
                ev.sort();
                match (&node.sampler, &node.synth) {
                    (Some(s), _) => s.render(tmp, ev.as_slice(), sr, offline, pos, playing),
                    (None, Some(s)) => s.render(tmp, ev.as_slice(), sr, offline, pos, playing),
                    _ => {}
                }
            } else if let Some((l, r)) = node.input.filter(|&(l, r)| listening && l.max(r) < ich) {
                // Entrada de la interfaz: medidor si la pista está armada o monitoreando; sonido solo con I.
                let (g, mon0) = (p.in_gain.get(), p.cur[2].get());
                let mon1 = if p.monitor.load(Relaxed) { 1.0 } else { 0.0 };
                p.cur[2].set(mon1);
                let mut peak = 0f32;
                for ((i, s), m) in tmp.iter_mut().enumerate().zip(ramp(mon0, mon1, out.len())) {
                    let (a, b) = (live.audio[i * ich + l] * g, live.audio[i * ich + r] * g);
                    peak = peak.max(a.abs()).max(b.abs());
                    *s = [s[0] + a * m, s[1] + b * m];
                }
                p.in_meter.max(peak);
            }
            for fx in &node.fx {
                fx.process(tmp, sr, bpm_now, offline);
            }
            // Render previo al fader de una pista: sin fader, panorama, automatización ni envíos.
            if offline && self.prefader.load(Relaxed) && only.is_some_and(|o| Arc::ptr_eq(o, &node.params)) {
                out.iter_mut().zip(tmp.iter()).for_each(|(o, s)| *o = [o[0] + s[0], o[1] + s[1]]);
                continue;
            }
            // La automatización (modo lectura) mueve el fader y el panorama.
            let read = playing && p.auto_read.load(Relaxed);
            if let Some(v) = automation_at(&node.vol_auto, end).filter(|_| read) {
                p.gain.set(fader_gain(v));
            }
            if let Some(v) = automation_at(&node.pan_auto, pos).filter(|_| read) {
                p.pan.set(v);
            }
            // Exportando una sola pista, solo ella va a la salida (sus envíos se siguen calculando).
            let to_out = only.map_or(node.to_master, |o| Arc::ptr_eq(o, &node.params));
            let audible = !p.mute.load(Relaxed) && (!any_solo || p.solo.load(Relaxed) || node.bus || only.is_some());
            if p.apply(tmp, p.gain.get(), audible) {
                if to_out {
                    out.iter_mut().zip(tmp.iter()).for_each(|(o, s)| *o = [o[0] + s[0], o[1] + s[1]]);
                }
                for &(target, g) in &node.sends {
                    if let Some(mut recv) = graph.get(target).and_then(|t| t.recv.try_lock().ok()) {
                        recv.iter_mut().zip(tmp.iter()).for_each(|(r, s)| *r = [r[0] + s[0] * g, r[1] + s[1] * g]);
                    }
                }
            }
        }
        if !(offline && self.prefader.load(Relaxed)) {
            self.master.apply(out, self.master.gain.get(), true);
        }
        if playing && !offline && self.metronome.load(Relaxed) {
            self.click(out, pos);
        }
        if !offline && let Some(p) = self.preview.load().as_ref().filter(|p| !p.is_empty()) {
            let (mut k, g, looped) = (self.preview_pos.load(Relaxed) as usize, self.preview_gain.get(), self.preview_loop.load(Relaxed));
            for o in out.iter_mut() {
                if k >= p.len() {
                    if !looped {
                        break;
                    }
                    k = 0;
                }
                *o = [o[0] + p[k][0] * g, o[1] + p[k][1] * g];
                k += 1;
            }
            self.preview_pos.store(k as u64, Relaxed);
        }
    }

    /// Clic del metrónomo siguiendo el mapa de tempo; acento en el primer tiempo del compás.
    fn click(&self, out: &mut [[f32; 2]], pos: u64) {
        let (sr, unit, beats) = (self.sample_rate as f64, self.unit.load(Relaxed), self.beats.load(Relaxed).max(1) as u64);
        let map = self.tempo.load();
        for (i, o) in out.iter_mut().enumerate() {
            let f = (pos + i as u64) as f64;
            let b = beats_at(&map, sr, unit, f);
            let t = b.fract() * beat_len(sr, bpm_at(&map, f), unit) / sr;
            if t < 0.05 {
                let freq = if (b as u64).is_multiple_of(beats) { 1760.0 } else { 1320.0 };
                let v = ((TAU * freq * t).sin() * (-t * 90.0).exp() * 0.5) as f32;
                *o = [o[0] + v, o[1] + v];
            }
        }
    }

    /// Render offline de una pista antes del fader (instrumento + efectos), estéreo intercalado.
    pub fn bounce_prefader(&self, from: u64, to: u64, track: &Arc<Params>) -> Vec<f32> {
        self.prefader.store(true, Relaxed);
        let out = self.bounce(from, to, Some(track));
        self.prefader.store(false, Relaxed);
        out
    }

    /// Mezcla offline de `[from, to)` como estéreo intercalado (de una pista o de todo el proyecto).
    pub fn bounce(&self, from: u64, to: u64, only: Option<&Arc<Params>>) -> Vec<f32> {
        let mut buf = [[0f32; 2]; BLOCK];
        let mut out = Vec::with_capacity((to.saturating_sub(from) * 2) as usize);
        for pos in (from..to).step_by(BLOCK) {
            self.render(&mut buf, pos, true, true, Live { audio: &[], midi: &[] }, only);
            out.extend(buf.iter().take((to - pos) as usize).flatten());
        }
        out
    }
}

struct Rings {
    in_rx: rtrb::Consumer<f32>,
    rec_tx: rtrb::Producer<f32>,
    midi_rx: rtrb::Consumer<MidiEvent>,
    midi_rec_tx: rtrb::Producer<(u64, MidiEvent)>,
    scope_tx: rtrb::Producer<f32>,
}

fn callback(e: Arc<Engine>, ch: usize, mut r: Rings) -> impl FnMut(&mut [f32], &cpal::OutputCallbackInfo) + Send + 'static {
    let mut buf = [[0f32; 2]; BLOCK];
    let ich = e.in_channels;
    let mut input = vec![0f32; BLOCK * ich];
    let mut midi = [[0u8; 3]; 128];
    let (mut was_recording, mut primed) = (false, false);
    move |data, _| {
        let frames = data.len() / ch.max(1);
        let mut done = 0;
        while done < frames {
            let playing = e.playing.load(Relaxed);
            let pos = e.pos.load(Relaxed);
            let (ls, le) = (e.loop_start.load(Relaxed), e.loop_end.load(Relaxed));
            let looping = playing && e.looping.load(Relaxed) && ls < le && pos < le;
            let mut n = (frames - done).min(BLOCK);
            if looping {
                n = n.min((le - pos) as usize);
            }
            // Entrada con colchón (jitter buffer): evita cortes cuando entrada y salida no van al mismo ritmo.
            let inp = &mut input[..n * ich];
            let need = inp.len();
            let slots = r.in_rx.slots();
            if slots > need * 8
                && let Ok(c) = r.in_rx.read_chunk((slots - need * 3) / ich.max(1) * ich)
            {
                c.commit_all();
            }
            primed = (primed || r.in_rx.slots() >= need * 3) && r.in_rx.slots() >= need;
            match r.in_rx.read_chunk(need) {
                Ok(c) if primed && need > 0 => {
                    let (a, b) = c.as_slices();
                    inp[..a.len()].copy_from_slice(a);
                    inp[a.len()..].copy_from_slice(b);
                    c.commit_all();
                }
                _ => inp.fill(0.0),
            }
            let mut nm = 0;
            while nm < midi.len()
                && let Ok(ev) = r.midi_rx.pop()
            {
                (midi[nm], nm) = (ev, nm + 1);
            }
            let recording = e.recording.load(Relaxed);
            if recording {
                if !was_recording {
                    e.rec_start.store(pos, Relaxed);
                }
                inp.iter().for_each(|&s| _ = r.rec_tx.push(s));
                midi[..nm].iter().for_each(|&ev| _ = r.midi_rec_tx.push((pos, ev)));
            }
            was_recording = recording;
            e.render(&mut buf[..n], pos, playing, false, Live { audio: inp, midi: &midi[..nm] }, None);
            if e.scope_on.load(Relaxed) {
                buf[..n].iter().for_each(|s| _ = r.scope_tx.push((s[0] + s[1]) * 0.5));
            }
            if playing {
                let next = if looping && pos + n as u64 >= le { ls } else { pos + n as u64 };
                // Si la UI movió el cursor mientras tanto, su valor gana.
                let _ = e.pos.compare_exchange(pos, next, Relaxed, Relaxed);
            }
            for (f, s) in data[done * ch..(done + n) * ch].chunks_mut(ch).zip(buf.iter()) {
                f.iter_mut().enumerate().for_each(|(i, v)| *v = s.get(i).copied().unwrap_or(0.0));
            }
            done += n;
        }
    }
}
