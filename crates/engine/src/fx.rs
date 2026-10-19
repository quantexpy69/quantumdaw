//! Efectos nativos QUANTUM. El estado DSP vive tras un `Mutex` que solo usa el hilo de
//! audio (`try_lock`, nunca espera) o la exportación offline.
use crate::AtomicF32;
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{
    f32::consts::{PI, TAU},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering::Relaxed},
    },
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum FxKind {
    Eq,
    Compressor,
    Delay,
    Reverb,
    Drive,
    Tune,
    /// EQ de 7 bandas con analizador y supresor de resonancias (al estilo de Curves AQ).
    Curves,
    /// Limitador con anticipación (lookahead) y techo: para el master.
    Limiter,
    /// Puerta de ruido: silencia lo que queda por debajo del umbral.
    Gate,
    Chorus,
    /// Reduce las eses (sibilancias) bajando solo los agudos cuando superan el umbral.
    DeEsser,
}

const SCALES: [&[u8]; 3] = [&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], &[0, 2, 4, 5, 7, 9, 11], &[0, 2, 3, 5, 7, 8, 10]];
/// Ventana de análisis de tono y salto entre detecciones.
const YIN_W: usize = 1024;
const YIN_HOP: usize = 512;
/// Curves: tamaño de la FFT, salto entre análisis y bandas del visor (20 Hz–20 kHz, logarítmicas).
const FFT_N: usize = 1024;
const FFT_HOP: usize = 256;
pub const BANDAS: usize = 96;

impl FxKind {
    pub const ALL: [Self; 11] = [Self::Eq, Self::Curves, Self::Compressor, Self::Limiter, Self::Gate, Self::DeEsser, Self::Delay, Self::Reverb, Self::Chorus, Self::Drive, Self::Tune];

    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "EQ 3 bandas",
            Self::Compressor => "Compresor",
            Self::Delay => "Delay",
            Self::Reverb => "Reverb",
            Self::Drive => "Saturación",
            Self::Tune => "QUANTUM Tune",
            Self::Curves => "QUANTUM Curves",
            Self::Limiter => "Limitador",
            Self::Gate => "Puerta de ruido",
            Self::Chorus => "Chorus",
            Self::DeEsser => "De-esser",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// (nombre, mínimo, máximo, valor por defecto) de cada parámetro.
    pub fn params(self) -> &'static [(&'static str, f32, f32, f32)] {
        match self {
            Self::Eq => &[
                ("Graves dB", -15.0, 15.0, 0.0),
                ("Medios dB", -15.0, 15.0, 0.0),
                ("Agudos dB", -15.0, 15.0, 0.0),
                ("Frec. graves", 30.0, 600.0, 200.0),
                ("Frec. medios", 200.0, 8000.0, 1000.0),
                ("Q medios", 0.3, 6.0, 0.7),
                ("Frec. agudos", 1500.0, 16000.0, 5000.0),
                ("Salida dB", -12.0, 12.0, 0.0),
            ],
            Self::Compressor => &[
                ("Umbral dB", -60.0, 0.0, -18.0),
                ("Ratio", 1.0, 20.0, 4.0),
                ("Ganancia dB", 0.0, 24.0, 0.0),
                ("Ataque ms", 0.1, 100.0, 5.0),
                ("Release ms", 10.0, 1000.0, 150.0),
                ("Rodilla dB", 0.0, 12.0, 3.0),
                ("Mezcla", 0.0, 1.0, 1.0),
            ],
            Self::Delay => &[
                ("Tiempo ms", 10.0, 2000.0, 375.0),
                ("Feedback", 0.0, 0.95, 0.35),
                ("Mezcla", 0.0, 1.0, 0.3),
                ("Ping-pong", 0.0, 1.0, 0.0),
                ("Corte graves", 20.0, 1000.0, 80.0),
                ("Corte agudos", 1000.0, 20000.0, 8000.0),
                ("Sincronía", 0.0, 1.0, 0.0),
                ("División", 0.0, 5.0, 2.0),
            ],
            Self::Reverb => &[
                ("Tamaño", 0.0, 1.0, 0.6),
                ("Amortiguación", 0.0, 1.0, 0.4),
                ("Mezcla", 0.0, 1.0, 0.25),
                ("Pre-delay ms", 0.0, 200.0, 10.0),
                ("Corte graves", 20.0, 1000.0, 100.0),
                ("Anchura", 0.0, 1.0, 1.0),
            ],
            Self::Drive => &[("Drive dB", 0.0, 36.0, 12.0), ("Mezcla", 0.0, 1.0, 1.0), ("Tono", 500.0, 20000.0, 12000.0), ("Salida dB", -24.0, 6.0, 0.0), ("Modo", 0.0, 2.0, 0.0)],
            Self::Curves => &[
                ("Corte graves", 20.0, 600.0, 20.0),
                ("Graves dB", -18.0, 18.0, 0.0),
                ("Frec. graves", 30.0, 500.0, 120.0),
                ("Banda 1 dB", -18.0, 18.0, 0.0),
                ("Banda 1 Hz", 60.0, 2000.0, 300.0),
                ("Banda 1 Q", 0.3, 12.0, 1.0),
                ("Banda 2 dB", -18.0, 18.0, 0.0),
                ("Banda 2 Hz", 200.0, 8000.0, 1200.0),
                ("Banda 2 Q", 0.3, 12.0, 1.0),
                ("Banda 3 dB", -18.0, 18.0, 0.0),
                ("Banda 3 Hz", 1000.0, 16000.0, 4000.0),
                ("Banda 3 Q", 0.3, 12.0, 1.0),
                ("Agudos dB", -18.0, 18.0, 0.0),
                ("Frec. agudos", 2000.0, 18000.0, 10000.0),
                ("Corte agudos", 2000.0, 20000.0, 20000.0),
                ("Resonancias", 0.0, 1.0, 0.0),
                ("Sensibilidad", 0.0, 1.0, 0.5),
                ("Salida dB", -12.0, 12.0, 0.0),
                ("Mezcla", 0.0, 1.0, 1.0),
                ("Sidechain", 0.0, 1.0, 0.5),
            ],
            Self::Limiter => &[("Techo dB", -12.0, 0.0, -1.0), ("Ganancia dB", 0.0, 24.0, 0.0), ("Release ms", 5.0, 500.0, 60.0), ("Anticipación ms", 0.5, 10.0, 3.0)],
            Self::Gate => &[("Umbral dB", -80.0, 0.0, -45.0), ("Rango dB", -80.0, 0.0, -60.0), ("Ataque ms", 0.1, 50.0, 1.0), ("Release ms", 10.0, 1000.0, 120.0), ("Retención ms", 0.0, 500.0, 30.0)],
            Self::Chorus => &[("Velocidad Hz", 0.05, 5.0, 0.8), ("Profundidad ms", 0.5, 15.0, 4.0), ("Mezcla", 0.0, 1.0, 0.5), ("Anchura", 0.0, 1.0, 1.0), ("Retardo ms", 5.0, 30.0, 12.0)],
            Self::DeEsser => &[("Frecuencia", 2000.0, 12000.0, 6500.0), ("Umbral dB", -60.0, 0.0, -30.0), ("Reducción máx. dB", 0.0, 24.0, 10.0), ("Escuchar eses", 0.0, 1.0, 0.0)],
            Self::Tune => {
                &[("Tonalidad", 0.0, 11.0, 0.0), ("Escala", 0.0, 2.0, 0.0), ("Velocidad ms", 0.0, 200.0, 20.0), ("Mezcla", 0.0, 1.0, 1.0), ("Modo", 0.0, 1.0, 0.0), ("Nota manual", 0.0, 11.0, 0.0)]
            }
        }
    }
}

#[derive(Default)]
struct State {
    /// Biquads: [banda][canal][z1, z2].
    z: [[[f32; 2]; 2]; 7],
    /// Curves: análisis espectral y supresor de resonancias.
    espectral: Option<Box<Espectral>>,
    env: f32,
    line: Vec<[f32; 2]>,
    idx: usize,
    /// Reverb tipo Freeverb: índices pares = canal izquierdo, impares = derecho.
    combs: Vec<(Vec<f32>, usize, f32)>,
    allpasses: Vec<(Vec<f32>, usize)>,
    /// Filtros de un polo (graves/agudos en delay, reverb y saturación): [filtro][canal].
    lp: [[f32; 2]; 3],
    pre: Vec<[f32; 2]>,
    pre_idx: usize,
    /// Tune: buffer de análisis circular, temporales de YIN y estado del pitch shifter.
    ana: Vec<f32>,
    yin: Vec<f32>,
    hop: usize,
    ratio: f32,
    target: f32,
    phase: f32,
}

pub struct Fx {
    pub kind: FxKind,
    pub params: Vec<AtomicF32>,
    pub bypass: AtomicBool,
    /// Lecturas para la interfaz: compresor = reducción de ganancia (dB);
    /// Tune = nota detectada y nota corregida (MIDI, 0 = sin tono).
    pub readout: [AtomicF32; 2],
    /// Curves (visor): nivel por banda tras el proceso (dB), reducción por resonancias (dB) y
    /// promedio de largo plazo de la señal (dB, para la detección automática).
    pub espectro: Vec<AtomicF32>,
    pub reduccion: Vec<AtomicF32>,
    pub promedio: Vec<AtomicF32>,
    /// Curves con sidechain: pista clave (id de la app, 0 = ninguna) y su posición en el grafo + 1.
    pub sidechain: AtomicU64,
    pub clave: AtomicUsize,
    /// Curves: nivel RMS de entrada y de salida del último bloque (para los vúmetros).
    pub vu: [AtomicF32; 2],
    state: Mutex<State>,
}

impl Fx {
    pub fn new(kind: FxKind, sample_rate: u32) -> Self {
        let scale = sample_rate as f32 / 44_100.0;
        let mut st = State { ratio: 1.0, target: 1.0, ..Default::default() };
        match kind {
            FxKind::Delay => st.line = vec![[0.0; 2]; sample_rate as usize * 2 + 1],
            FxKind::Reverb => {
                st.pre = vec![[0.0; 2]; (sample_rate as f32 * 0.25) as usize];
                st.combs = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617].map(|n| (vec![0.0; (n as f32 * scale) as usize], 0, 0.0)).into();
                st.allpasses = [556, 579, 441, 464].map(|n| (vec![0.0; (n as f32 * scale) as usize], 0)).into();
            }
            FxKind::Tune => (st.line, st.ana, st.yin) = (vec![[0.0; 2]; 8192], vec![0.0; YIN_W], vec![0.0; YIN_W]),
            FxKind::Curves => st.espectral = Some(Box::new(Espectral::new(sample_rate as f32))),
            // Limitador: retardo de anticipación y ganancia necesaria de cada muestra (10 ms como máximo).
            FxKind::Limiter => {
                let n = sample_rate as usize / 100 + 2;
                (st.line, st.ana, st.env) = (vec![[0.0; 2]; n], vec![1.0; n], 1.0);
            }
            FxKind::Chorus => st.line = vec![[0.0; 2]; sample_rate as usize / 20 + 2],
            _ => {}
        }
        let params = kind.params().iter().map(|p| AtomicF32::new(p.3)).collect();
        let bandas = |v: f32| if kind == FxKind::Curves { (0..BANDAS).map(|_| AtomicF32::new(v)).collect() } else { vec![] };
        let (espectro, reduccion, promedio) = (bandas(-120.0), bandas(0.0), bandas(-120.0));
        Self { kind, params, bypass: false.into(), readout: Default::default(), espectro, reduccion, promedio, sidechain: AtomicU64::new(0), clave: AtomicUsize::new(0), vu: Default::default(), state: Mutex::new(st) }
    }

    /// `bpm`: tempo actual (para el delay sincronizado).
    pub fn process(&self, buf: &mut [[f32; 2]], sr: f32, bpm: f32, offline: bool) {
        self.process_clave(buf, sr, bpm, offline, None);
    }

    /// Como `process`, con la señal clave del sidechain (Curves).
    pub fn process_clave(&self, buf: &mut [[f32; 2]], sr: f32, bpm: f32, offline: bool, clave: Option<&[[f32; 2]]>) {
        if self.bypass.load(Relaxed) {
            return;
        }
        let Some(mut st) = (if offline { self.state.lock().ok() } else { self.state.try_lock().ok() }) else {
            return;
        };
        let st = &mut *st;
        let p: [f32; 20] = std::array::from_fn(|i| self.params.get(i).map_or(0.0, |a| a.get()));
        match self.kind {
            FxKind::Eq => {
                let co = [biquad(0, p[3], p[0], 0.707, sr), biquad(1, p[4], p[1], p[5], sr), biquad(2, p[6], p[2], 0.707, sr)];
                let out = db(p[7]);
                for s in buf.iter_mut() {
                    for (ch, x) in s.iter_mut().enumerate() {
                        for (b, c) in co.iter().enumerate() {
                            let z = &mut st.z[b][ch];
                            let y = c[0] * *x + z[0];
                            (z[0], z[1]) = (c[1] * *x - c[3] * y + z[1], c[2] * *x - c[4] * y);
                            *x = y;
                        }
                        *x *= out;
                    }
                }
            }
            FxKind::Compressor => {
                let (att, rel, makeup) = ((-1.0 / (p[3].max(0.05) * 0.001 * sr)).exp(), (-1.0 / (p[4] * 0.001 * sr)).exp(), db(p[2]));
                let (knee, slope, mix) = (p[5].max(0.01), 1.0 - 1.0 / p[1], p[6]);
                let mut gr = 0f32;
                for (i, s) in buf.iter_mut().enumerate() {
                    // Con sidechain el detector escucha la pista clave en vez de la propia señal.
                    let det = clave.and_then(|c| c.get(i)).unwrap_or(s);
                    let lvl = det[0].abs().max(det[1].abs());
                    let coef = if lvl > st.env { att } else { rel };
                    st.env = lvl + coef * (st.env - lvl);
                    let over = 20.0 * st.env.max(1e-6).log10() - p[0];
                    // Rodilla suave: transición cuadrática alrededor del umbral.
                    let red = match over {
                        o if o <= -knee / 2.0 => 0.0,
                        o if o < knee / 2.0 => slope * (o + knee / 2.0).powi(2) / (2.0 * knee),
                        o => slope * o,
                    };
                    gr = gr.max(red);
                    let g = db(-red) * makeup;
                    *s = [s[0] + (s[0] * g - s[0]) * mix, s[1] + (s[1] * g - s[1]) * mix];
                }
                self.readout[0].set(gr);
            }
            FxKind::Delay => {
                let n = st.line.len();
                // Sincronía: 1/1, 1/2, 1/4, 1/8, 1/16 y 1/8 con puntillo, a partir del tempo.
                let ms = if p[6] >= 0.5 { 60_000.0 / bpm.max(20.0) * [4.0, 2.0, 1.0, 0.5, 0.25, 0.75][(p[7].round() as usize).min(5)] } else { p[0] };
                let d = ((ms / 1000.0 * sr) as usize).clamp(1, n - 1);
                let (hp, lp) = (one_pole(p[4], sr), one_pole(p[5], sr));
                let ping = p[3] >= 0.5;
                for s in buf.iter_mut() {
                    let r = st.line[(st.idx + n - d) % n];
                    // La repetición pasa por un filtro de graves (paso alto) y de agudos (paso bajo).
                    let mut fb = [0f32; 2];
                    for ch in 0..2 {
                        st.lp[0][ch] += (r[ch] - st.lp[0][ch]) * hp;
                        st.lp[1][ch] += ((r[ch] - st.lp[0][ch]) - st.lp[1][ch]) * lp;
                        fb[ch] = st.lp[1][ch] * p[1];
                    }
                    st.line[st.idx] = if ping { [(s[0] + s[1]) * 0.5 + fb[1], fb[0]] } else { [s[0] + fb[0], s[1] + fb[1]] };
                    st.idx = (st.idx + 1) % n;
                    *s = [s[0] + r[0] * p[2], s[1] + r[1] * p[2]];
                }
            }
            FxKind::Reverb => {
                let (fb, damp) = (0.7 + 0.28 * p[0], p[1] * 0.4);
                let (pre, hp, width) = (((p[3] / 1000.0 * sr) as usize).min(st.pre.len().saturating_sub(1)), one_pole(p[4], sr), p[5]);
                for s in buf.iter_mut() {
                    // Pre-delay y corte de graves antes de la reverb.
                    let n = st.pre.len().max(1);
                    let d = st.pre.get((st.pre_idx + n - pre) % n).copied().unwrap_or(*s);
                    if let Some(slot) = st.pre.get_mut(st.pre_idx) {
                        *slot = *s;
                    }
                    st.pre_idx = (st.pre_idx + 1) % n;
                    let m = if pre == 0 { (s[0] + s[1]) * 0.5 } else { (d[0] + d[1]) * 0.5 };
                    st.lp[0][0] += (m - st.lp[0][0]) * hp;
                    let input = (m - st.lp[0][0]) * 0.06;
                    let mut wet = [0f32; 2];
                    for (i, (line, idx, lp)) in st.combs.iter_mut().enumerate() {
                        let y = line[*idx];
                        *lp = y * (1.0 - damp) + *lp * damp;
                        line[*idx] = input + *lp * fb;
                        *idx = (*idx + 1) % line.len();
                        wet[i % 2] += y;
                    }
                    for (i, (line, idx)) in st.allpasses.iter_mut().enumerate() {
                        let (b, x) = (line[*idx], wet[i % 2]);
                        line[*idx] = x + b * 0.5;
                        wet[i % 2] = b - x;
                        *idx = (*idx + 1) % line.len();
                    }
                    // Anchura: 0 = mono, 1 = estéreo completo.
                    let mid = (wet[0] + wet[1]) * 0.5;
                    let wet = [mid + (wet[0] - mid) * width, mid + (wet[1] - mid) * width];
                    *s = [s[0] * (1.0 - p[2]) + wet[0] * p[2] * 3.0, s[1] * (1.0 - p[2]) + wet[1] * p[2] * 3.0];
                }
            }
            FxKind::Drive => {
                let (g, tone, out, mode) = (db(p[0]), one_pole(p[2], sr), db(p[3]), p[4].round() as u8);
                for s in buf.iter_mut() {
                    for (ch, x) in s.iter_mut().enumerate() {
                        let d = *x * g;
                        // Modos: suave (tanh), duro (recorte) y válvula (asimétrico).
                        let sat = match mode {
                            1 => d.clamp(-1.0, 1.0),
                            2 => {
                                if d >= 0.0 {
                                    d.tanh()
                                } else {
                                    (d * 0.6).tanh() / 0.6 * 0.8
                                }
                            }
                            _ => d.tanh(),
                        } / g.sqrt();
                        st.lp[2][ch] += (sat - st.lp[2][ch]) * tone;
                        *x = (*x + (st.lp[2][ch] - *x) * p[1]) * out;
                    }
                }
            }
            FxKind::Tune => self.tune(st, buf, sr, &p),
            FxKind::Limiter => {
                let (techo, entrada) = (db(p[0]), db(p[1]));
                let n = st.line.len();
                let l = ((p[3] * 0.001 * sr) as usize).clamp(1, n - 2);
                let rel = 1.0 - (-1.0 / (p[2].max(1.0) * 0.001 * sr)).exp();
                // El ataque llega a la ganancia necesaria antes de que el pico salga del retardo.
                let att = 1.0 - (-4.0 / l as f32).exp();
                let mut gr = 0f32;
                for s in buf.iter_mut() {
                    let x = [s[0] * entrada, s[1] * entrada];
                    let pico = x[0].abs().max(x[1].abs());
                    (st.ana[st.idx], st.line[st.idx]) = (if pico > techo { techo / pico } else { 1.0 }, x);
                    let m = (0..=l).map(|k| st.ana[(st.idx + n - k) % n]).fold(1.0f32, f32::min);
                    st.env += (m - st.env) * if m < st.env { att } else { rel };
                    let y = st.line[(st.idx + n - l) % n];
                    st.idx = (st.idx + 1) % n;
                    *s = [(y[0] * st.env).clamp(-techo, techo), (y[1] * st.env).clamp(-techo, techo)];
                    gr = gr.max(-20.0 * st.env.max(1e-6).log10());
                }
                self.readout[0].set(gr);
            }
            FxKind::Gate => {
                let (umbral, rango) = (db(p[0]), db(p[1]));
                let coef = |ms: f32| 1.0 - (-1.0 / (ms.max(0.05) * 0.001 * sr)).exp();
                let (att, rel, caida, retencion) = (coef(p[2]), coef(p[3]), coef(10.0), (p[4] * 0.001 * sr) as usize);
                for (i, s) in buf.iter_mut().enumerate() {
                    let det = clave.and_then(|c| c.get(i)).unwrap_or(s);
                    let lvl = det[0].abs().max(det[1].abs());
                    st.env = if lvl > st.env { lvl } else { st.env + (lvl - st.env) * caida };
                    if st.env > umbral {
                        st.hop = retencion;
                    } else {
                        st.hop = st.hop.saturating_sub(1);
                    }
                    let objetivo = if st.env > umbral || st.hop > 0 { 1.0 } else { rango };
                    st.ratio += (objetivo - st.ratio) * if objetivo > st.ratio { att } else { rel };
                    *s = [s[0] * st.ratio, s[1] * st.ratio];
                }
                self.readout[0].set(-20.0 * st.ratio.max(1e-6).log10());
            }
            FxKind::Chorus => {
                let n = st.line.len();
                let paso = p[0] / sr;
                for s in buf.iter_mut() {
                    st.line[st.idx] = *s;
                    let mut out = *s;
                    for (ch, o) in out.iter_mut().enumerate() {
                        let lfo = (TAU * (st.phase + ch as f32 * 0.25 * p[3])).sin();
                        let d = ((p[4] + p[1] * 0.5 * (1.0 + lfo)) * 0.001 * sr).clamp(1.0, n as f32 - 2.0);
                        let (k, f) = (d as usize, d.fract());
                        let a = st.line[(st.idx + n - k) % n][ch];
                        let b = st.line[(st.idx + n - k - 1) % n][ch];
                        *o = s[ch] * (1.0 - p[2] * 0.5) + (a + (b - a) * f) * p[2];
                    }
                    *s = out;
                    st.idx = (st.idx + 1) % n;
                    st.phase = (st.phase + paso).fract();
                }
            }
            FxKind::DeEsser => {
                let co = biquad(3, p[0], 0.0, 0.707, sr);
                let (umbral, maximo, escuchar) = (p[1], p[2], p[3] >= 0.5);
                let (att, rel) = (1.0 - (-1.0 / (0.001 * sr)).exp(), 1.0 - (-1.0 / (0.06 * sr)).exp());
                let mut gr = 0f32;
                for s in buf.iter_mut() {
                    let mut agudos = [0f32; 2];
                    for (ch, x) in s.iter().enumerate() {
                        let z = &mut st.z[0][ch];
                        let y = co[0] * x + z[0];
                        (z[0], z[1]) = (co[1] * x - co[3] * y + z[1], co[2] * x - co[4] * y);
                        agudos[ch] = y;
                    }
                    let lvl = agudos[0].abs().max(agudos[1].abs());
                    st.env += (lvl - st.env) * if lvl > st.env { att } else { rel };
                    let red = ((20.0 * st.env.max(1e-6).log10() - umbral) * 0.75).clamp(0.0, maximo);
                    gr = gr.max(red);
                    let g = db(-red);
                    // Banda completa: mientras hay eses baja toda la señal (sin desfases de un filtro
                    // separador). «Escuchar eses» deja oír solo lo que dispara la reducción.
                    *s = if escuchar { agudos } else { [s[0] * g, s[1] * g] };
                }
                self.readout[0].set(gr);
            }
            FxKind::Curves => {
                let co = curves_coefs(&p, sr);
                let (salida, mezcla, cantidad, sens, lateral) = (db(p[17]), p[18], p[15], p[16], if clave.is_some() { p[19] } else { 0.0 });
                let rms = |b: &[[f32; 2]]| (b.iter().map(|s| (s[0] * s[0] + s[1] * s[1]) * 0.5).sum::<f32>() / b.len().max(1) as f32).sqrt();
                self.vu[0].set(rms(buf));
                for (n, s) in buf.iter_mut().enumerate() {
                    let seco = *s;
                    for (ch, x) in s.iter_mut().enumerate() {
                        for (b, c) in co.iter().enumerate() {
                            let z = &mut st.z[b][ch];
                            let y = c[0] * *x + z[0];
                            (z[0], z[1]) = (c[1] * *x - c[3] * y + z[1], c[2] * *x - c[4] * y);
                            *x = y;
                        }
                    }
                    if let Some(e) = st.espectral.as_deref_mut() {
                        let k = clave.and_then(|c| c.get(n)).map_or(0.0, |c| (c[0] + c[1]) * 0.5);
                        *s = e.muestra(*s, cantidad, sens, lateral, k, self);
                    }
                    *s = [(seco[0] + (s[0] - seco[0]) * mezcla) * salida, (seco[1] + (s[1] - seco[1]) * mezcla) * salida];
                }
                self.vu[1].set(rms(buf));
            }
        }
    }

    /// Autotune: detecta el tono (YIN) cada `YIN_HOP` muestras, elige la nota destino (escala o nota
    /// manual) y corrige con un pitch shifter de dos cabezales cruzados sobre una línea de retardo.
    fn tune(&self, st: &mut State, buf: &mut [[f32; 2]], sr: f32, p: &[f32; 20]) {
        let speed = 1.0 - (-1.0 / (p[2].max(0.5) * 0.001 * sr)).exp();
        let (win, n) = (0.03 * sr, st.line.len());
        for s in buf.iter_mut() {
            st.ana[st.hop % YIN_W] = (s[0] + s[1]) * 0.5;
            st.hop += 1;
            if st.hop.is_multiple_of(YIN_HOP) {
                let start = st.hop % YIN_W;
                for j in 0..YIN_W {
                    st.yin[j] = st.ana[(start + j) % YIN_W];
                }
                st.target = match yin(&st.yin, sr) {
                    Some(f0) => {
                        let note = 69.0 + 12.0 * (f0 / 440.0).log2();
                        let dest = if p[4] >= 0.5 { nearest(note, p[5].round() as i32, &[0]) } else { nearest(note, p[0].round() as i32, SCALES[p[1].round() as usize % 3]) };
                        self.readout[0].set(note);
                        self.readout[1].set(dest);
                        2f32.powf((dest - note) / 12.0)
                    }
                    None => {
                        self.readout[0].set(0.0);
                        1.0
                    }
                };
            }
            st.ratio += (st.target - st.ratio) * speed;
            st.line[st.idx] = *s;
            st.idx = (st.idx + 1) % n;
            st.phase = (st.phase + (1.0 - st.ratio) / win).rem_euclid(1.0);
            let mut acc = [0f32; 2];
            for tap in 0..2 {
                let ph = (st.phase + tap as f32 * 0.5).fract();
                let d = ph * win + 2.0;
                let (i0, frac) = ((st.idx as f32 + n as f32 - d) as usize % n, d.fract());
                let (a, b) = (st.line[i0], st.line[(i0 + n - 1) % n]);
                let w = (PI * ph).sin().powi(2);
                acc = [acc[0] + (a[0] + (b[0] - a[0]) * frac) * w, acc[1] + (a[1] + (b[1] - a[1]) * frac) * w];
            }
            *s = [s[0] + (acc[0] - s[0]) * p[3], s[1] + (acc[1] - s[1]) * p[3]];
        }
    }
}

/// Nota de la escala (relativa a `key`) más cercana a `note`.
fn nearest(note: f32, key: i32, scale: &[u8]) -> f32 {
    let base = note.round() as i32;
    (base - 6..=base + 6).filter(|n| scale.contains(&((n - key).rem_euclid(12) as u8))).min_by(|a, b| (*a as f32 - note).abs().total_cmp(&(*b as f32 - note).abs())).unwrap_or(base) as f32
}

/// Detección de frecuencia fundamental YIN (70 Hz–1 kHz); `None` si no hay tono claro.
fn yin(x: &[f32], sr: f32) -> Option<f32> {
    let w = x.len() / 2;
    if x.iter().map(|v| v * v).sum::<f32>() / (x.len() as f32) < 1e-5 {
        return None;
    }
    let (tmin, tmax) = ((sr / 1000.0) as usize, ((sr / 70.0) as usize).min(w - 1));
    let diff = |tau: usize| (0..w).map(|j| (x[j] - x[j + tau]).powi(2)).sum::<f32>();
    let mut running = 0.0;
    let mut prev = (0usize, f32::MAX);
    for tau in 1..=tmax {
        let d = diff(tau);
        running += d;
        let cmnd = d * tau as f32 / running.max(1e-9);
        if tau >= tmin && cmnd < 0.15 {
            if cmnd < prev.1 {
                prev = (tau, cmnd);
                continue;
            }
            break;
        }
        if prev.0 != 0 {
            break;
        }
    }
    (prev.0 != 0).then(|| sr / prev.0 as f32)
}

pub fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// Coeficiente de un filtro de un polo con frecuencia de corte `freq`.
fn one_pole(freq: f32, sr: f32) -> f32 {
    1.0 - (-TAU * freq.min(sr * 0.45) / sr).exp()
}

/// Coeficientes RBJ normalizados [b0, b1, b2, a1, a2]. `kind`: 0 = low shelf, 1 = peak, 2 = high shelf.
pub fn biquad(kind: u8, freq: f32, gain_db: f32, q: f32, sr: f32) -> [f32; 5] {
    let a = 10f32.powf(gain_db / 40.0);
    let w = TAU * freq.min(sr * 0.45) / sr;
    let (c, alpha) = (w.cos(), w.sin() / (2.0 * q.max(0.1)));
    let k = 2.0 * a.sqrt() * alpha;
    let [b0, b1, b2, a0, a1, a2] = match kind {
        0 => [
            a * ((a + 1.0) - (a - 1.0) * c + k),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - k),
            (a + 1.0) + (a - 1.0) * c + k,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - k,
        ],
        2 => [
            a * ((a + 1.0) + (a - 1.0) * c + k),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - k),
            (a + 1.0) - (a - 1.0) * c + k,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - k,
        ],
        // Paso alto y paso bajo (cortes de graves y agudos).
        3 => [(1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha],
        4 => [(1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha],
        _ => [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// Magnitud (dB) de un biquad a la pulsación `w`.
fn magnitud(c: [f32; 5], w: f32) -> f32 {
    let (cw, c2) = (w.cos(), (2.0 * w).cos());
    let (sw, s2) = (w.sin(), (2.0 * w).sin());
    let num = (c[0] + c[1] * cw + c[2] * c2).powi(2) + (c[1] * sw + c[2] * s2).powi(2);
    let den = (1.0 + c[3] * cw + c[4] * c2).powi(2) + (c[3] * sw + c[4] * s2).powi(2);
    10.0 * (num / den).max(1e-12).log10()
}

/// Coeficientes de las 7 bandas de Curves: corte de graves, estante de graves, 3 campanas,
/// estante de agudos y corte de agudos.
pub fn curves_coefs(p: &[f32], sr: f32) -> [[f32; 5]; 7] {
    [
        biquad(3, p[0], 0.0, 0.707, sr),
        biquad(0, p[2], p[1], 0.707, sr),
        biquad(1, p[4], p[3], p[5], sr),
        biquad(1, p[7], p[6], p[8], sr),
        biquad(1, p[10], p[9], p[11], sr),
        biquad(2, p[13], p[12], 0.707, sr),
        biquad(4, p[14], 0.0, 0.707, sr),
    ]
}

/// Respuesta del EQ de Curves (dB) a una frecuencia, para dibujar la curva.
pub fn curves_response(p: &[f32], freq: f32, sr: f32) -> f32 {
    let w = TAU * freq / sr;
    curves_coefs(p, sr).iter().map(|c| magnitud(*c, w)).sum::<f32>() + p[17]
}

/// Respuesta (dB) de una sola banda `b` (0–6) de Curves, para dibujar su curva por separado.
pub fn curves_banda(p: &[f32], b: usize, freq: f32, sr: f32) -> f32 {
    magnitud(curves_coefs(p, sr)[b], TAU * freq / sr)
}

/// Frecuencia central de una banda del visor de Curves.
pub fn frecuencia_banda(b: usize) -> f32 {
    20.0 * 1000f32.powf((b as f32 + 0.5) / BANDAS as f32)
}

/// Análisis y supresión de resonancias por FFT (ventanas de 1024 con salto de 256, solapamiento y
/// suma con ventana raíz de Hann). Cada frecuencia se compara con la envolvente suave del espectro:
/// lo que sobresale más que el umbral se atenúa de forma dinámica (ataque rápido, liberación suave).
struct Espectral {
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    ventana: Vec<f32>,
    norma: f32,
    entrada: [Vec<f32>; 2],
    salida: [Vec<f32>; 2],
    /// Sidechain: señal clave (mono) y su espectro.
    clave: Vec<f32>,
    clave_bins: Vec<Complex<f32>>,
    pos: usize,
    cuenta: usize,
    bins: [Vec<Complex<f32>>; 2],
    scratch: Vec<Complex<f32>>,
    nivel: Vec<f32>,
    suma: Vec<f32>,
    /// Reducción actual por frecuencia (dB).
    reduccion: Vec<f32>,
    /// Banda del visor de cada frecuencia (o BANDAS si queda fuera de 20 Hz–20 kHz) y frecuencia (Hz).
    banda: Vec<usize>,
    hz: Vec<f32>,
}

impl Espectral {
    fn new(sr: f32) -> Self {
        let mut plan = FftPlanner::new();
        let (fft, ifft) = (plan.plan_fft_forward(FFT_N), plan.plan_fft_inverse(FFT_N));
        let ventana: Vec<f32> = (0..FFT_N).map(|i| (0.5 - 0.5 * (TAU * i as f32 / FFT_N as f32).cos()).sqrt()).collect();
        let norma = ventana.iter().sum::<f32>() / 2.0;
        let scratch = vec![Complex::default(); fft.get_inplace_scratch_len().max(ifft.get_inplace_scratch_len())];
        let mitad = FFT_N / 2 + 1;
        let hz: Vec<f32> = (0..mitad).map(|k| k as f32 * sr / FFT_N as f32).collect();
        let banda = hz.iter().map(|&f| if (20.0..20000.0).contains(&f) { (((f / 20.0).log10() / 3.0) * BANDAS as f32) as usize } else { BANDAS }).collect();
        Self {
            fft,
            ifft,
            ventana,
            norma,
            entrada: [vec![0.0; FFT_N], vec![0.0; FFT_N]],
            salida: [vec![0.0; FFT_N], vec![0.0; FFT_N]],
            clave: vec![0.0; FFT_N],
            clave_bins: vec![Complex::default(); FFT_N],
            pos: 0,
            cuenta: 0,
            bins: [vec![Complex::default(); FFT_N], vec![Complex::default(); FFT_N]],
            scratch,
            nivel: vec![-120.0; mitad],
            suma: vec![0.0; mitad + 1],
            reduccion: vec![0.0; mitad],
            banda,
            hz,
        }
    }

    /// Procesa una muestra; con `cantidad` = 0 solo analiza (sin latencia ni cambios en el sonido).
    fn muestra(&mut self, x: [f32; 2], cantidad: f32, sens: f32, lateral: f32, clave: f32, fx: &Fx) -> [f32; 2] {
        let activo = cantidad > 0.001 || lateral > 0.001;
        self.clave[self.pos] = clave;
        let mut y = x;
        for ch in 0..2 {
            self.entrada[ch][self.pos] = x[ch];
            if activo {
                y[ch] = self.salida[ch][self.pos];
            }
            self.salida[ch][self.pos] = 0.0;
        }
        self.pos = (self.pos + 1) % FFT_N;
        self.cuenta += 1;
        if self.cuenta == FFT_HOP {
            self.cuenta = 0;
            self.salto(activo, cantidad, sens, lateral, fx);
        }
        y
    }

    fn salto(&mut self, activo: bool, cantidad: f32, sens: f32, lateral: f32, fx: &Fx) {
        let mitad = FFT_N / 2 + 1;
        // Espectro de la clave (sidechain): dónde suena la otra pista.
        if lateral > 0.001 {
            for i in 0..FFT_N {
                self.clave_bins[i] = Complex::new(self.clave[(self.pos + i) % FFT_N] * self.ventana[i], 0.0);
            }
            self.fft.process_with_scratch(&mut self.clave_bins, &mut self.scratch);
        }
        for ch in 0..2 {
            for i in 0..FFT_N {
                self.bins[ch][i] = Complex::new(self.entrada[ch][(self.pos + i) % FFT_N] * self.ventana[i], 0.0);
            }
            self.fft.process_with_scratch(&mut self.bins[ch], &mut self.scratch);
        }
        for k in 0..mitad {
            let m = (self.bins[0][k].norm() + self.bins[1][k].norm()) * 0.5 / self.norma;
            self.nivel[k] = 20.0 * (m + 1e-9).log10();
        }
        // Envolvente: media del nivel en una ventana proporcional a la frecuencia (≈ 1/3 de octava).
        for k in 0..mitad {
            self.suma[k + 1] = self.suma[k] + self.nivel[k];
        }
        let umbral = 3.0 + (1.0 - sens.clamp(0.0, 1.0)) * 9.0;
        for k in 1..mitad {
            let w = (k / 6).max(3);
            let (a, b) = (k.saturating_sub(w), (k + w + 1).min(mitad));
            let envolvente = (self.suma[b] - self.suma[a]) / (b - a) as f32;
            let exceso = self.nivel[k] - envolvente;
            let mut objetivo = if (80.0..18000.0).contains(&self.hz[k]) && self.nivel[k] > -70.0 && exceso > umbral { ((exceso - umbral) * 1.5 * cantidad).min(18.0) } else { 0.0 };
            // Sidechain: cuanto más fuerte suena la clave en esta frecuencia, más se abre espacio aquí.
            if lateral > 0.001 {
                let k_db = 20.0 * (self.clave_bins[k].norm() / self.norma + 1e-9).log10();
                objetivo = objetivo.max(((k_db + 54.0) / 42.0).clamp(0.0, 1.0) * 12.0 * lateral);
            }
            let r = &mut self.reduccion[k];
            *r += (objetivo - *r) * if objetivo > *r { 0.6 } else { 0.12 };
        }
        // Visor: máximo por banda (con caída suave), reducción y promedio de largo plazo.
        let mut nivel_b = [-120f32; BANDAS];
        let mut red_b = [0f32; BANDAS];
        for k in 1..mitad {
            if let Some(b) = self.banda.get(k).copied().filter(|&b| b < BANDAS) {
                let red = if activo { self.reduccion[k] } else { 0.0 };
                nivel_b[b] = nivel_b[b].max(self.nivel[k] - red);
                red_b[b] = red_b[b].max(red);
            }
        }
        for b in 0..BANDAS {
            // Las bandas graves sin frecuencias de la FFT quedan en -120 (el visor las interpola).
            let v = nivel_b[b];
            fx.espectro[b].set(if v <= -119.0 { -120.0 } else { v.max(fx.espectro[b].get() - 1.5) });
            fx.reduccion[b].set(red_b[b]);
            if v > -90.0 {
                let p = fx.promedio[b].get();
                fx.promedio[b].set(if p < -100.0 { v } else { p + (v - p) * 0.01 });
            }
        }
        if !activo {
            return;
        }
        let escala = 1.0 / (FFT_N as f32 * 2.0);
        for ch in 0..2 {
            for k in 1..mitad {
                let g = 10f32.powf(-self.reduccion[k] / 20.0);
                self.bins[ch][k] *= g;
                if k < FFT_N - k {
                    self.bins[ch][FFT_N - k] *= g;
                }
            }
            self.ifft.process_with_scratch(&mut self.bins[ch], &mut self.scratch);
            for i in 0..FFT_N {
                self.salida[ch][(self.pos + i) % FFT_N] += self.bins[ch][i].re * self.ventana[i] * escala;
            }
        }
    }
}

/// Respuesta en magnitud (dB) del EQ a una frecuencia, para dibujar la curva en la interfaz.
pub fn eq_response(p: &[f32], freq: f32, sr: f32) -> f32 {
    let w = TAU * freq / sr;
    let mag = |c: [f32; 5]| magnitud(c, w);
    let g = |i: usize, d: f32| p.get(i).copied().unwrap_or(d);
    mag(biquad(0, g(3, 200.0), g(0, 0.0), 0.707, sr)) + mag(biquad(1, g(4, 1000.0), g(1, 0.0), g(5, 0.7), sr)) + mag(biquad(2, g(6, 5000.0), g(2, 0.0), 0.707, sr)) + g(7, 0.0)
}

#[cfg(test)]
mod pruebas_curves {
    use super::*;

    /// Energía de `x` en la frecuencia `f` (correlación con seno y coseno).
    fn energia(x: &[[f32; 2]], f: f32, sr: f32) -> f32 {
        let (mut s, mut c) = (0f32, 0f32);
        for (i, v) in x.iter().enumerate() {
            let w = TAU * f * i as f32 / sr;
            (s, c) = (s + v[0] * w.sin(), c + v[0] * w.cos());
        }
        (s * s + c * c).sqrt() / x.len() as f32
    }

    fn procesar(fx: &Fx, señal: &[[f32; 2]]) -> Vec<[f32; 2]> {
        let mut out = señal.to_vec();
        for bloque in out.chunks_mut(512) {
            fx.process(bloque, 48_000.0, 120.0, true);
        }
        out
    }

    #[test]
    fn plano_es_transparente() {
        let sr = 48_000.0;
        let fx = Fx::new(FxKind::Curves, 48_000);
        let señal: Vec<[f32; 2]> = (0..48_000).map(|i| { let v = (TAU * 1000.0 * i as f32 / sr).sin() * 0.5; [v, v] }).collect();
        let out = procesar(&fx, &señal);
        let (a, b) = (energia(&señal[24_000..], 1000.0, sr), energia(&out[24_000..], 1000.0, sr));
        assert!((b / a - 1.0).abs() < 0.03, "1 kHz: {a} → {b}");
    }

    #[test]
    fn baja_la_resonancia() {
        let sr = 48_000.0;
        let fx = Fx::new(FxKind::Curves, 48_000);
        fx.params[15].set(1.0);
        // Ruido de banda ancha más un tono resonante fuerte a 2,5 kHz.
        let mut semilla = 1u32;
        let señal: Vec<[f32; 2]> = (0..96_000)
            .map(|i| {
                semilla = semilla.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let ruido = (semilla >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                let v = ruido * 0.2 + (TAU * 2500.0 * i as f32 / sr).sin() * 0.3;
                [v, v]
            })
            .collect();
        let out = procesar(&fx, &señal);
        let (antes, despues) = (energia(&señal[48_000..], 2500.0, sr), energia(&out[49_024..], 2500.0, sr));
        let reduccion = 20.0 * (despues / antes).log10();
        assert!(reduccion < -4.0, "la resonancia solo bajó {reduccion:.1} dB");
        // El resto del espectro (500 Hz) casi no cambia.
        let (a, b) = (energia(&señal[48_000..], 500.0, sr), energia(&out[49_024..], 500.0, sr));
        assert!((20.0 * (b / a).log10()).abs() < 3.0, "500 Hz cambió {:.1} dB", 20.0 * (b / a).log10());
        assert!(fx.reduccion.iter().any(|r| r.get() > 3.0), "el visor no muestra la reducción");
    }
}

#[cfg(test)]
mod pruebas_dinamica {
    use super::*;

    fn seno(f: f32, amp: f32, n: usize) -> Vec<[f32; 2]> {
        (0..n).map(|i| [(TAU * f * i as f32 / 48_000.0).sin() * amp; 2]).collect()
    }

    fn pico(b: &[[f32; 2]]) -> f32 {
        b.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()))
    }

    #[test]
    fn limitador_puerta_y_deesser() {
        // Limitador: +12 dB sobre un seno a -6 dBFS nunca pasa del techo de -1 dB.
        let lim = Fx::new(FxKind::Limiter, 48_000);
        lim.params[1].set(12.0);
        let mut b = seno(220.0, 0.5, 48_000);
        b.chunks_mut(256).for_each(|c| lim.process(c, 48_000.0, 120.0, true));
        assert!(pico(&b) <= db(-1.0) + 1e-4, "pico {}", pico(&b));
        assert!(pico(&b[24_000..]) > db(-1.5), "el limitador no debe hundir el nivel");
        // Puerta: un seno a -60 dBFS (bajo el umbral de -45) queda casi en silencio.
        let gate = Fx::new(FxKind::Gate, 48_000);
        let mut b = seno(220.0, db(-60.0), 48_000);
        b.chunks_mut(256).for_each(|c| gate.process(c, 48_000.0, 120.0, true));
        assert!(pico(&b[36_000..]) < db(-100.0), "puerta {}", pico(&b[36_000..]));
        // De-esser: baja un agudo fuerte (8 kHz) y deja pasar un grave (200 Hz).
        let de = Fx::new(FxKind::DeEsser, 48_000);
        let mut agudo = seno(8000.0, 0.5, 24_000);
        agudo.chunks_mut(256).for_each(|c| de.process(c, 48_000.0, 120.0, true));
        let de = Fx::new(FxKind::DeEsser, 48_000);
        let mut grave = seno(200.0, 0.5, 24_000);
        grave.chunks_mut(256).for_each(|c| de.process(c, 48_000.0, 120.0, true));
        assert!(pico(&agudo[12_000..]) < 0.5 * db(-8.0), "eses {}", pico(&agudo[12_000..]));
        assert!(pico(&grave[12_000..]) > 0.45);
    }
}

#[cfg(test)]
mod pruebas_sidechain {
    use super::*;

    #[test]
    fn compresor_con_clave() {
        let comp = Fx::new(FxKind::Compressor, 48_000);
        comp.params[0].set(-30.0);
        comp.params[1].set(10.0);
        let (mut bajo, clave) = (vec![[0.1f32; 2]; 4800], vec![[0.9f32; 2]; 4800]);
        comp.process_clave(&mut bajo, 48_000.0, 120.0, true, Some(&clave));
        // La señal (-20 dBFS, bajo el efecto propio sería poca reducción) baja por la clave fuerte.
        assert!(bajo[4000][0] < 0.1 * db(-10.0), "{}", bajo[4000][0]);
        let comp = Fx::new(FxKind::Compressor, 48_000);
        comp.params[0].set(-30.0);
        comp.params[1].set(10.0);
        let mut solo = vec![[0.1f32; 2]; 4800];
        comp.process_clave(&mut solo, 48_000.0, 120.0, true, Some(&vec![[0.0f32; 2]; 4800]));
        assert!(solo[4000][0] > 0.099, "sin clave no debe agacharse");
    }
}
