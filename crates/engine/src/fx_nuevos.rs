//! Efectos nativos de movimiento, estéreo, color y utilidad: ancho estéreo, auto pan, filtro DJ, auto
//! filtro, puerta rítmica, phaser, flanger, lo-fi, tape stop, utilidad, transitorios, desplazador de
//! frecuencia y afinador. Cada uno guarda su estado en `Estado`; los que van a tempo leen la posición
//! de la canción (`POSICION`) para caer siempre en el mismo lugar del compás.
use crate::{AtomicF32, fx::FxKind};
use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

/// Tiempo de la canción (en negras, como bits de f64) al comienzo del bloque y si está sonando.
pub static POSICION: AtomicU64 = AtomicU64::new(0);
pub static SONANDO: AtomicBool = AtomicBool::new(false);

/// Duración de las divisiones (en negras): 1/1, 1/2, 1/4, 1/8, 1/16 y 1/8 con puntillo.
const DIVISIONES: [f64; 6] = [4.0, 2.0, 1.0, 0.5, 0.25, 0.75];

/// Patrones de la puerta rítmica: 16 pasos, el bit más alto es el primero.
pub const PATRONES: [u16; 8] = [0xAAAA, 0xEEEE, 0xCCCC, 0x9249, 0xDB6D, 0xF0F0, 0x8888, 0xAEEA];

/// Coeficientes de los dos caminos de la red de Hilbert (fase 0° y 90°), de 4 pasa-todos cada uno.
const HILBERT: [[f32; 4]; 2] = [[0.479_401_7, 0.876_218_5, 0.976_597_6, 0.997_499_7], [0.161_758_5, 0.733_028_9, 0.945_349_7, 0.990_599_2]];

const AFINADOR_N: usize = 2048;

fn division(v: f32) -> f64 {
    DIVISIONES[(v.round().max(0.0) as usize).min(5)]
}

fn db(v: f32) -> f32 {
    10f32.powf(v / 20.0)
}

/// Filtro de estado variable (Cytomic/Simper): devuelve (paso bajo, paso banda, paso alto).
#[derive(Default, Clone, Copy)]
struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    fn tick(&mut self, x: f32, g: f32, k: f32) -> (f32, f32, f32) {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + g * a1 * v3;
        let v2 = self.ic2 + g * v1;
        (self.ic1, self.ic2) = (2.0 * v1 - self.ic1, 2.0 * v2 - self.ic2);
        (v2, v1, x - k * v1 - v2)
    }
}

fn svf_g(f: f32, sr: f32) -> f32 {
    (PI * f.clamp(10.0, sr * 0.45) / sr).tan()
}

pub struct Estado {
    sr: f32,
    /// Fase propia (en negras) cuando la canción está parada.
    fase: f64,
    svf: [Svf; 2],
    env: f32,
    env2: f32,
    ganancia: f32,
    ap: [[f32; 8]; 2],
    fb: [f32; 2],
    linea: Vec<[f32; 2]>,
    idx: usize,
    retenido: [f32; 2],
    cuenta: f32,
    /// Tape stop: velocidad actual y posición de lectura (en muestras detrás de la escritura).
    velocidad: f32,
    atraso: f64,
    hilbert: [[[f32; 2]; 4]; 4],
    osc: f32,
    afinador: Vec<f32>,
    ai: usize,
}

impl Estado {
    pub fn new(kind: FxKind, sr: f32) -> Self {
        let linea = match kind {
            FxKind::Flanger => vec![[0.0; 2]; (sr * 0.03) as usize + 4],
            FxKind::TapeStop => vec![[0.0; 2]; (sr * 8.0) as usize],
            _ => vec![],
        };
        let afinador = if kind == FxKind::Afinador { vec![0.0; AFINADOR_N] } else { vec![] };
        Self { sr, fase: 0.0, svf: Default::default(), env: 0.0, env2: 0.0, ganancia: 1.0, ap: [[0.0; 8]; 2], fb: [0.0; 2], linea, idx: 0, retenido: [0.0; 2], cuenta: 0.0, velocidad: 1.0, atraso: 0.0, hilbert: [[[0.0; 2]; 4]; 4], osc: 0.0, afinador, ai: 0 }
    }

    /// Fase (en negras) de cada muestra del bloque: la de la canción si suena, si no la propia.
    fn fases(&mut self, n: usize, bpm: f32) -> impl Iterator<Item = f64> + use<> {
        let paso = bpm.max(1.0) as f64 / 60.0 / self.sr as f64;
        let inicio = if SONANDO.load(Relaxed) { f64::from_bits(POSICION.load(Relaxed)) } else { self.fase };
        self.fase = inicio + paso * n as f64;
        (0..n).map(move |i| inicio + paso * i as f64)
    }

    pub fn procesar(&mut self, kind: FxKind, buf: &mut [[f32; 2]], p: &[f32; 32], bpm: f32, readout: &[AtomicF32; 2]) {
        let sr = self.sr;
        match kind {
            FxKind::Ancho => {
                // Anchura del lado (S) y graves por debajo del corte siempre en mono.
                let (g, k) = (svf_g(p[1], sr), 1.41);
                let out = db(p[2]);
                let (mut lr, mut ll, mut rr) = (0f32, 0f32, 0f32);
                for s in buf.iter_mut() {
                    let (m, mut side) = ((s[0] + s[1]) * 0.5, (s[0] - s[1]) * 0.5);
                    let (bajo, _, _) = self.svf[0].tick(side, g, k);
                    side = (side - bajo) * p[0] + if p[1] > 21.0 { 0.0 } else { bajo * p[0] };
                    *s = [(m + side) * out, (m - side) * out];
                    (lr, ll, rr) = (lr + s[0] * s[1], ll + s[0] * s[0], rr + s[1] * s[1]);
                }
                readout[0].set(if ll * rr > 1e-12 { lr / (ll * rr).sqrt() } else { 1.0 });
            }
            FxKind::AutoPan => {
                let ciclo = division(p[0]);
                let forma = p[2].round() as u8;
                let mut pan = 0.0;
                let fases = self.fases(buf.len(), bpm);
                for (s, b) in buf.iter_mut().zip(fases) {
                    let t = ((b / ciclo + p[3] as f64).fract()) as f32;
                    let lfo = match forma {
                        1 => 1.0 - 4.0 * (t - 0.5).abs(),
                        2 => {
                            if t < 0.5 {
                                1.0
                            } else {
                                -1.0
                            }
                        }
                        _ => (TAU * t).sin(),
                    };
                    // Cuadrada suavizada para que no haga clic al cambiar de lado.
                    self.env += (lfo - self.env) * if forma == 2 { 0.004 } else { 1.0 };
                    pan = self.env * p[1];
                    let a = (pan + 1.0) * PI / 4.0;
                    *s = [s[0] * a.cos() * 1.414, s[1] * a.sin() * 1.414];
                }
                readout[0].set(pan);
            }
            FxKind::FiltroDj => {
                // Izquierda: paso bajo que cierra; derecha: paso alto que sube; al centro, sin filtro.
                let v = p[0];
                let k = 2.0 - 1.9 * p[1];
                let (glp, ghp) = (svf_g(20000.0 * (0.01f32).powf((-v).max(0.0)), sr), svf_g(20.0 * 500f32.powf(v.max(0.0)), sr));
                let lado = if v.abs() < 0.02 { 0.0 } else { 1.0 };
                for s in buf.iter_mut() {
                    for (ch, x) in s.iter_mut().enumerate() {
                        let y = if v < 0.0 { self.svf[ch].tick(*x, glp, k).0 } else { self.svf[ch].tick(*x, ghp, k).2 };
                        *x += (y - *x) * lado;
                    }
                }
            }
            FxKind::AutoFiltro => {
                let ciclo = division(p[3]);
                let k = 2.0 - 1.9 * p[2];
                let (at, re) = ((-1.0 / (0.005 * sr)).exp(), (-1.0 / (0.15 * sr)).exp());
                let tipo = p[0].round() as u8;
                let mut corte = p[1];
                let fases = self.fases(buf.len(), bpm);
                for (s, b) in buf.iter_mut().zip(fases) {
                    let lvl = s[0].abs().max(s[1].abs());
                    self.env = lvl + if lvl > self.env { at } else { re } * (self.env - lvl);
                    let lfo = (TAU * (b / ciclo).fract() as f32).sin();
                    // LFO de ±3 octavas y envolvente de ±4 octavas sobre el corte.
                    let oct = lfo * p[4] * 3.0 + (self.env * 4.0).min(1.0) * p[5] * 4.0;
                    corte = (p[1] * 2f32.powf(oct)).clamp(30.0, 18000.0);
                    let g = svf_g(corte, sr);
                    for (ch, x) in s.iter_mut().enumerate() {
                        let (lp, bp, hp) = self.svf[ch].tick(*x, g, k);
                        let y = match tipo {
                            1 => hp,
                            2 => bp,
                            _ => lp,
                        };
                        *x += (y - *x) * p[6];
                    }
                }
                readout[0].set(corte);
            }
            FxKind::PuertaRitmica => {
                let paso = division(p[1]);
                let patron = PATRONES[(p[0].round().max(0.0) as usize).min(7)];
                let suave = 1.0 - (-1.0 / (p[3].max(0.1) * 0.001 * sr)).exp();
                let mut actual = 0;
                let fases = self.fases(buf.len(), bpm);
                for (s, b) in buf.iter_mut().zip(fases) {
                    actual = ((b / paso).floor() as i64).rem_euclid(16) as u32;
                    let on = patron >> (15 - actual) & 1 == 1;
                    let objetivo = if on { 1.0 } else { 1.0 - p[2] };
                    self.ganancia += (objetivo - self.ganancia) * suave;
                    let g = 1.0 + (self.ganancia - 1.0) * p[4];
                    *s = [s[0] * g, s[1] * g];
                }
                readout[0].set(actual as f32);
            }
            FxKind::Phaser => {
                let etapas = [4, 6, 8][(p[3].round().max(0.0) as usize).min(2)];
                let inc = p[0] / sr;
                for s in buf.iter_mut() {
                    self.osc = (self.osc + inc).fract();
                    for (ch, x) in s.iter_mut().enumerate() {
                        // Los dos canales desfasados un cuarto de ciclo: el barrido se abre en estéreo.
                        let t = (self.osc + ch as f32 * 0.25).fract();
                        let lfo = 0.5 + 0.5 * (TAU * t).sin();
                        let f = 200.0 * 20f32.powf(lfo * p[1]);
                        let w = (PI * f / sr).tan();
                        let a = (w - 1.0) / (w + 1.0);
                        let mut y = *x + self.fb[ch] * p[2];
                        for z in self.ap[ch].iter_mut().take(etapas) {
                            let o = a * y + *z;
                            *z = y - a * o;
                            y = o;
                        }
                        self.fb[ch] = y;
                        *x = *x * (1.0 - p[4] * 0.5) + y * p[4] * 0.5;
                    }
                }
            }
            FxKind::Flanger => {
                let n = self.linea.len();
                let inc = p[0] / sr;
                for s in buf.iter_mut() {
                    self.osc = (self.osc + inc).fract();
                    let sal: [f32; 2] = std::array::from_fn(|ch| {
                        let t = (self.osc + ch as f32 * 0.25).fract();
                        let d = (0.3 + p[1] * (0.5 + 0.5 * (TAU * t).sin())) * 0.001 * sr;
                        let pos = self.idx as f32 + n as f32 - d;
                        let (i0, f) = (pos.floor() as usize % n, pos.fract());
                        self.linea[i0][ch] * (1.0 - f) + self.linea[(i0 + 1) % n][ch] * f
                    });
                    self.linea[self.idx] = [s[0] + sal[0] * p[2], s[1] + sal[1] * p[2]];
                    self.idx = (self.idx + 1) % n;
                    *s = [s[0] * (1.0 - p[3] * 0.5) + sal[0] * p[3] * 0.5, s[1] * (1.0 - p[3] * 0.5) + sal[1] * p[3] * 0.5];
                }
            }
            FxKind::LoFi => {
                let niveles = 2f32.powf(p[0].round().clamp(2.0, 16.0) - 1.0);
                let (salto, tono) = (p[1].max(1.0), svf_g(p[2], sr));
                for s in buf.iter_mut() {
                    self.cuenta += 1.0;
                    if self.cuenta >= salto {
                        self.cuenta -= salto;
                        self.retenido = s.map(|x| (x * niveles).round() / niveles);
                    }
                    for (ch, x) in s.iter_mut().enumerate() {
                        let (lp, _, _) = self.svf[ch].tick(self.retenido[ch], tono, 1.41);
                        *x += (lp - *x) * p[3];
                    }
                }
            }
            FxKind::TapeStop => {
                let n = self.linea.len();
                let parar = p[0] >= 0.5;
                let (baja, sube) = (1.0 / (p[1].max(10.0) * 0.001 * sr), 1.0 / (p[2].max(1.0) * 0.001 * sr));
                for s in buf.iter_mut() {
                    self.linea[self.idx] = *s;
                    // Al parar la velocidad cae en curva (como un motor); al soltar vuelve a 1 y el atraso
                    // se recupera con la velocidad por encima de 1 hasta alcanzar la cinta en vivo.
                    if parar {
                        self.velocidad = (self.velocidad - baja * (0.3 + self.velocidad)).max(0.0);
                    } else if p[2] < 1.0 {
                        (self.velocidad, self.atraso) = (1.0, 0.0);
                    } else {
                        self.velocidad = (self.velocidad + sube).min(if self.atraso > 1.0 { 1.0 + (self.atraso / sr as f64).min(1.0) as f32 } else { 1.0 });
                    }
                    self.atraso = (self.atraso + 1.0 - self.velocidad as f64).clamp(0.0, (n - 4) as f64);
                    if self.atraso > 0.5 || parar {
                        let pos = self.idx as f64 + n as f64 - self.atraso;
                        let (i0, f) = (pos.floor() as usize % n, pos.fract() as f32);
                        let a = self.linea[i0];
                        let b = self.linea[(i0 + 1) % n];
                        *s = [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
                    }
                    self.idx = (self.idx + 1) % n;
                }
                readout[0].set(self.velocidad);
            }
            FxKind::Utilidad => {
                let g = db(p[0]);
                let a = (p[1] + 1.0) * PI / 4.0;
                let (pl, pr) = (a.cos() * 1.414, a.sin() * 1.414);
                let mut pico = 0f32;
                for s in buf.iter_mut() {
                    let mut x = *s;
                    if p[6] >= 0.5 {
                        x = [x[1], x[0]];
                    }
                    x[0] *= if p[4] >= 0.5 { -1.0 } else { 1.0 };
                    x[1] *= if p[5] >= 0.5 { -1.0 } else { 1.0 };
                    let (m, side) = ((x[0] + x[1]) * 0.5, (x[0] - x[1]) * 0.5 * if p[3] >= 0.5 { 0.0 } else { p[2] });
                    *s = [(m + side) * g * pl.min(1.0), (m - side) * g * pr.min(1.0)];
                    pico = pico.max(s[0].abs()).max(s[1].abs());
                }
                readout[0].set(20.0 * pico.max(1e-6).log10());
            }
            FxKind::Transitorios => {
                // Diferencia entre una envolvente rápida y una lenta: positiva en el golpe, negativa en la cola.
                let (r1, r2) = ((-1.0 / (0.001 * sr)).exp(), (-1.0 / (0.03 * sr)).exp());
                let (rel1, rel2) = ((-1.0 / (0.05 * sr)).exp(), (-1.0 / (0.25 * sr)).exp());
                let out = db(p[2]);
                for s in buf.iter_mut() {
                    let x = s[0].abs().max(s[1].abs());
                    self.env = x + if x > self.env { r1 } else { rel1 } * (self.env - x);
                    self.env2 = x + if x > self.env2 { r2 } else { rel2 } * (self.env2 - x);
                    let d = 20.0 * (self.env.max(1e-6) / self.env2.max(1e-6)).log10();
                    let g = if d > 0.0 { p[0] * d.min(18.0) } else { -p[1] * d.max(-18.0) };
                    let g = db(g.clamp(-18.0, 18.0)) * out;
                    *s = [s[0] * g, s[1] * g];
                }
            }
            FxKind::Desplazador => {
                let inc = p[0] / sr;
                for s in buf.iter_mut() {
                    self.osc = (self.osc + inc).rem_euclid(1.0);
                    let (c, si) = ((TAU * self.osc).cos(), (TAU * self.osc).sin());
                    for (ch, x) in s.iter_mut().enumerate() {
                        let mut ramas = [*x; 2];
                        for (rama, coef) in HILBERT.iter().enumerate() {
                            for (k, a) in coef.iter().enumerate() {
                                let z = &mut self.hilbert[rama * 2 + ch][k];
                                // Pasa-todos de segundo orden: y = a(x + y[n-2]) - x[n-2].
                                let a2 = a * a;
                                let y = a2 * (ramas[rama] + z[1]) - z[0];
                                (z[0], z[1]) = (ramas[rama], y);
                                ramas[rama] = y;
                            }
                        }
                        // El camino en fase lleva una muestra de retardo para quedar a 90° del otro.
                        let re = std::mem::replace(&mut self.retenido[ch], ramas[0]);
                        let y = re * c - ramas[1] * si;
                        *x += (y - *x) * p[1];
                    }
                }
            }
            FxKind::Afinador => {
                for s in buf.iter() {
                    self.afinador[self.ai] = (s[0] + s[1]) * 0.5;
                    self.ai = (self.ai + 1) % AFINADOR_N;
                    self.cuenta += 1.0;
                }
                // Una detección cada 1024 muestras sobre la ventana más reciente.
                if self.cuenta >= 1024.0 {
                    self.cuenta = 0.0;
                    let v: Vec<f32> = (0..AFINADOR_N).map(|k| self.afinador[(self.ai + k) % AFINADOR_N]).collect();
                    match crate::fx::yin(&v, sr) {
                        Some(f) => readout[0].set(69.0 + 12.0 * (f / p[0].max(400.0)).log2()),
                        None => readout[0].set(0.0),
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seno(f: f32, sr: f32, n: usize) -> Vec<[f32; 2]> {
        (0..n).map(|i| [(TAU * f * i as f32 / sr).sin() * 0.5; 2]).collect()
    }

    fn params(defs: &[(&str, f32, f32, f32)]) -> [f32; 32] {
        std::array::from_fn(|i| defs.get(i).map_or(0.0, |d| d.3))
    }

    #[test]
    fn todos_dan_audio_finito() {
        let sr = 48_000.0;
        for k in FxKind::NUEVOS {
            let mut e = Estado::new(k, sr);
            let mut buf = seno(220.0, sr, 4096);
            let r: [AtomicF32; 2] = Default::default();
            e.procesar(k, &mut buf, &params(k.params()), 120.0, &r);
            assert!(buf.iter().all(|s| s[0].is_finite() && s[1].is_finite() && s[0].abs() < 4.0), "{k:?}");
        }
    }

    #[test]
    fn afinador_detecta_la_440() {
        let sr = 48_000.0;
        let mut e = Estado::new(FxKind::Afinador, sr);
        let r: [AtomicF32; 2] = Default::default();
        let mut buf = seno(440.0, sr, 4096);
        e.procesar(FxKind::Afinador, &mut buf, &params(FxKind::Afinador.params()), 120.0, &r);
        assert!((r[0].get() - 69.0).abs() < 0.2, "{}", r[0].get());
    }

    #[test]
    fn desplazador_mueve_la_frecuencia() {
        // 1 kHz desplazado +200 Hz: la energía debe quedar cerca de 1200 Hz.
        let sr = 48_000.0;
        let mut e = Estado::new(FxKind::Desplazador, sr);
        let r: [AtomicF32; 2] = Default::default();
        let mut p = params(FxKind::Desplazador.params());
        p[0] = 200.0;
        let mut buf = seno(1000.0, sr, 9600);
        e.procesar(FxKind::Desplazador, &mut buf, &p, 120.0, &r);
        let energia = |f: f32| {
            let (mut re, mut im) = (0f32, 0f32);
            for (i, s) in buf[4800..].iter().enumerate() {
                let a = TAU * f * i as f32 / sr;
                (re, im) = (re + s[0] * a.cos(), im + s[0] * a.sin());
            }
            (re * re + im * im).sqrt()
        };
        assert!(energia(1200.0) > 10.0 * energia(1000.0), "{} {}", energia(1200.0), energia(1000.0));
    }

    #[test]
    fn ancho_cero_es_mono() {
        let sr = 48_000.0;
        let mut e = Estado::new(FxKind::Ancho, sr);
        let r: [AtomicF32; 2] = Default::default();
        let mut p = params(FxKind::Ancho.params());
        p[0] = 0.0;
        let mut buf: Vec<[f32; 2]> = (0..1000).map(|i| [(i as f32 * 0.1).sin(), (i as f32 * 0.13).cos()]).collect();
        e.procesar(FxKind::Ancho, &mut buf, &p, 120.0, &r);
        assert!(buf.iter().all(|s| (s[0] - s[1]).abs() < 1e-4));
    }
}
