//! Efectos nativos QUANTUM. El estado DSP vive tras un `Mutex` que solo usa el hilo de
//! audio (`try_lock`, nunca espera) o la exportación offline.
use crate::AtomicF32;
use std::{
    f32::consts::{PI, TAU},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering::Relaxed},
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
}

const SCALES: [&[u8]; 3] = [&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], &[0, 2, 4, 5, 7, 9, 11], &[0, 2, 3, 5, 7, 8, 10]];
/// Ventana de análisis de tono y salto entre detecciones.
const YIN_W: usize = 1024;
const YIN_HOP: usize = 512;

impl FxKind {
    pub const ALL: [Self; 6] = [Self::Eq, Self::Compressor, Self::Delay, Self::Reverb, Self::Drive, Self::Tune];

    pub fn name(self) -> &'static str {
        match self {
            Self::Eq => "EQ 3 bandas",
            Self::Compressor => "Compresor",
            Self::Delay => "Delay",
            Self::Reverb => "Reverb",
            Self::Drive => "Saturación",
            Self::Tune => "QUANTUM Tune",
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
            Self::Tune => {
                &[("Tonalidad", 0.0, 11.0, 0.0), ("Escala", 0.0, 2.0, 0.0), ("Velocidad ms", 0.0, 200.0, 20.0), ("Mezcla", 0.0, 1.0, 1.0), ("Modo", 0.0, 1.0, 0.0), ("Nota manual", 0.0, 11.0, 0.0)]
            }
        }
    }
}

#[derive(Default)]
struct State {
    /// Biquads: [banda][canal][z1, z2].
    z: [[[f32; 2]; 2]; 3],
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
            _ => {}
        }
        let params = kind.params().iter().map(|p| AtomicF32::new(p.3)).collect();
        Self { kind, params, bypass: false.into(), readout: Default::default(), state: Mutex::new(st) }
    }

    /// `bpm`: tempo actual (para el delay sincronizado).
    pub fn process(&self, buf: &mut [[f32; 2]], sr: f32, bpm: f32, offline: bool) {
        if self.bypass.load(Relaxed) {
            return;
        }
        let Some(mut st) = (if offline { self.state.lock().ok() } else { self.state.try_lock().ok() }) else {
            return;
        };
        let st = &mut *st;
        let p: [f32; 8] = std::array::from_fn(|i| self.params.get(i).map_or(0.0, |a| a.get()));
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
                for s in buf.iter_mut() {
                    let lvl = s[0].abs().max(s[1].abs());
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
        }
    }

    /// Autotune: detecta el tono (YIN) cada `YIN_HOP` muestras, elige la nota destino (escala o nota
    /// manual) y corrige con un pitch shifter de dos cabezales cruzados sobre una línea de retardo.
    fn tune(&self, st: &mut State, buf: &mut [[f32; 2]], sr: f32, p: &[f32; 8]) {
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
        _ => [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a, 1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
    };
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}

/// Respuesta en magnitud (dB) del EQ a una frecuencia, para dibujar la curva en la interfaz.
pub fn eq_response(p: &[f32], freq: f32, sr: f32) -> f32 {
    let w = TAU * freq / sr;
    let mag = |c: [f32; 5]| {
        let (cw, c2) = (w.cos(), (2.0 * w).cos());
        let (sw, s2) = (w.sin(), (2.0 * w).sin());
        let num = (c[0] + c[1] * cw + c[2] * c2).powi(2) + (c[1] * sw + c[2] * s2).powi(2);
        let den = (1.0 + c[3] * cw + c[4] * c2).powi(2) + (c[3] * sw + c[4] * s2).powi(2);
        10.0 * (num / den).log10()
    };
    let g = |i: usize, d: f32| p.get(i).copied().unwrap_or(d);
    mag(biquad(0, g(3, 200.0), g(0, 0.0), 0.707, sr)) + mag(biquad(1, g(4, 1000.0), g(1, 0.0), g(5, 0.7), sr)) + mag(biquad(2, g(6, 5000.0), g(2, 0.0), 0.707, sr)) + g(7, 0.0)
}
