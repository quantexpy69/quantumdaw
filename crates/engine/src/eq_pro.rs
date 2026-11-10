//! QUANTUM EQ: ecualizador paramétrico de 8 bandas con analizador de espectro.
//!
//! Cada banda es un filtro de segundo orden diseñado con el método «analog-matched» de M. Vicanek
//! (Matched Second Order Digital Filters, 2016): los polos salen del prototipo analógico por
//! invarianza al impulso y el numerador se elige para que la magnitud digital sea exactamente la
//! analógica en 0 Hz, en la frecuencia de la banda y en Nyquist. Así la curva no se deforma cerca de
//! los agudos (lo que sí pasa con la transformada bilineal del clásico RBJ: las campanas y estantes
//! se encogen al acercarse a 20 kHz). Los cortes de graves y agudos encadenan secciones Butterworth
//! para 12, 24, 36 o 48 dB/octava. Todo el cálculo y el filtrado se hacen en f64.
use crate::AtomicF32;
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{f64::consts::PI, sync::Arc};

pub const BANDAS_EQ: usize = 8;
/// Parámetros de cada banda: activa, tipo, frecuencia, ganancia, Q y pendiente.
pub const POR_BANDA: usize = 6;
pub const P_SALIDA: usize = BANDAS_EQ * POR_BANDA;
/// Analizador: 0 apagado, 1 antes del EQ, 2 después del EQ.
pub const P_ANALIZADOR: usize = P_SALIDA + 1;
pub const N_PARAMS: usize = P_SALIDA + 2;
/// Secciones de segundo orden como máximo por banda (48 dB/octava).
const SECCIONES: usize = 4;

macro_rules! banda {
    ($n:literal, $tipo:expr, $hz:expr) => {
        [
            (concat!("B", $n, " activa"), 0.0, 1.0, 0.0),
            (concat!("B", $n, " tipo"), 0.0, 6.0, $tipo),
            (concat!("B", $n, " Hz"), 10.0, 22000.0, $hz),
            (concat!("B", $n, " dB"), -30.0, 30.0, 0.0),
            (concat!("B", $n, " Q"), 0.1, 40.0, 1.0),
            (concat!("B", $n, " pendiente"), 0.0, 3.0, 1.0),
        ]
    };
}

const fn juntar(b: [[(&'static str, f32, f32, f32); POR_BANDA]; BANDAS_EQ]) -> [(&'static str, f32, f32, f32); N_PARAMS] {
    let mut out = [("", 0.0, 0.0, 0.0); N_PARAMS];
    let mut i = 0;
    while i < P_SALIDA {
        out[i] = b[i / POR_BANDA][i % POR_BANDA];
        i += 1;
    }
    out[P_SALIDA] = ("Salida dB", -24.0, 24.0, 0.0);
    out[P_ANALIZADOR] = ("Analizador", 0.0, 2.0, 2.0);
    out
}

/// (nombre, mínimo, máximo, por defecto) de los 50 parámetros. Por defecto ninguna banda está
/// activa: la 1 es un corte de graves y la 8 un corte de agudos; el resto, campanas repartidas.
pub static PARAMS: [(&str, f32, f32, f32); N_PARAMS] = juntar([
    banda!("1", 3.0, 40.0),
    banda!("2", 1.0, 100.0),
    banda!("3", 0.0, 250.0),
    banda!("4", 0.0, 600.0),
    banda!("5", 0.0, 1500.0),
    banda!("6", 0.0, 4000.0),
    banda!("7", 2.0, 10000.0),
    banda!("8", 4.0, 18000.0),
]);

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Tipo {
    Campana,
    EstanteGraves,
    EstanteAgudos,
    CorteGraves,
    CorteAgudos,
    Notch,
    PasoBanda,
}

impl Tipo {
    pub const TODOS: [Tipo; 7] = [Tipo::Campana, Tipo::EstanteGraves, Tipo::EstanteAgudos, Tipo::CorteGraves, Tipo::CorteAgudos, Tipo::Notch, Tipo::PasoBanda];

    pub fn desde(v: f32) -> Self {
        Self::TODOS[(v.round().max(0.0) as usize).min(Self::TODOS.len() - 1)]
    }
    pub fn valor(self) -> f32 {
        Self::TODOS.iter().position(|t| *t == self).unwrap_or(0) as f32
    }
    pub fn nombre(self) -> &'static str {
        match self {
            Tipo::Campana => "Campana",
            Tipo::EstanteGraves => "Estante de graves",
            Tipo::EstanteAgudos => "Estante de agudos",
            Tipo::CorteGraves => "Corte de graves",
            Tipo::CorteAgudos => "Corte de agudos",
            Tipo::Notch => "Notch",
            Tipo::PasoBanda => "Paso banda",
        }
    }
    /// La ganancia solo cambia algo en campanas y estantes.
    pub fn usa_ganancia(self) -> bool {
        matches!(self, Tipo::Campana | Tipo::EstanteGraves | Tipo::EstanteAgudos)
    }
    pub fn es_corte(self) -> bool {
        matches!(self, Tipo::CorteGraves | Tipo::CorteAgudos)
    }
}

/// Ajustes de una banda leídos de los parámetros.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Banda {
    pub on: bool,
    pub tipo: Tipo,
    pub freq: f32,
    pub gain: f32,
    pub q: f32,
    /// 0..=3: 12, 24, 36 o 48 dB/octava (solo los cortes).
    pub pendiente: usize,
}

pub fn banda(p: &[f32], b: usize) -> Banda {
    let v = |k: usize| p.get(b * POR_BANDA + k).copied().unwrap_or(PARAMS[b * POR_BANDA + k].3);
    Banda { on: v(0) >= 0.5, tipo: Tipo::desde(v(1)), freq: v(2), gain: v(3), q: v(4), pendiente: (v(5).round().max(0.0) as usize).min(SECCIONES - 1) }
}

/// Biquad normalizado: [b0, b1, b2, a1, a2] (a0 = 1).
pub type Coefs = [f64; 5];
const NEUTRO: Coefs = [1.0, 0.0, 0.0, 0.0, 0.0];

/// Prototipo analógico normalizado a la frecuencia de la banda (S = s/ω0):
/// H(S) = (n2·S² + n1·S + n0) / (d2·S² + d1·S + d0).
fn prototipo(tipo: Tipo, gain_db: f64, q: f64) -> ([f64; 3], [f64; 3]) {
    let a = 10f64.powf(gain_db / 40.0);
    let ra = a.sqrt();
    match tipo {
        Tipo::Campana => ([1.0, a / q, 1.0], [1.0, 1.0 / (a * q), 1.0]),
        Tipo::EstanteGraves => ([a, a * ra / q, a * a], [a, ra / q, 1.0]),
        Tipo::EstanteAgudos => ([a * a, a * ra / q, a], [1.0, ra / q, a]),
        Tipo::CorteGraves => ([1.0, 0.0, 0.0], [1.0, 1.0 / q, 1.0]),
        Tipo::CorteAgudos => ([0.0, 0.0, 1.0], [1.0, 1.0 / q, 1.0]),
        Tipo::Notch => ([1.0, 0.0, 1.0], [1.0, 1.0 / q, 1.0]),
        Tipo::PasoBanda => ([0.0, 1.0 / q, 0.0], [1.0, 1.0 / q, 1.0]),
    }
}

/// |H(jΩ)|² del prototipo analógico, con `x` = Ω/ω0.
fn magnitud2_analogica((n, d): ([f64; 3], [f64; 3]), x: f64) -> f64 {
    let num = (n[2] - n[0] * x * x).powi(2) + (n[1] * x).powi(2);
    let den = (d[2] - d[0] * x * x).powi(2) + (d[1] * x).powi(2);
    num / den.max(1e-300)
}

/// φ0, φ1, φ2 de Vicanek a la pulsación digital `w`: |B(e^jw)|² = B0·φ0 + B1·φ1 + B2·φ2.
fn phi(w: f64) -> [f64; 3] {
    let s = (w / 2.0).sin().powi(2);
    [1.0 - s, s, 4.0 * (1.0 - s) * s]
}

/// Diseña una sección «analog-matched» del prototipo `pr` con frecuencia `f` (Hz). `paso_alto`: el
/// numerador es b0·(1, −2, 1), con el doble cero en 0 Hz que da los 12 dB/octava (el ajuste
/// genérico de tres puntos dejaría un solo cero y la pendiente se quedaría en 6 dB/octava).
fn emparejado(pr: ([f64; 3], [f64; 3]), f: f64, sr: f64, paso_alto: bool) -> Coefs {
    let w0 = 2.0 * PI * f.clamp(5.0, 0.49 * sr) / sr;
    let (_, d) = pr;
    // Polos por invarianza al impulso: frecuencia natural y amortiguamiento del denominador. Si la
    // frecuencia natural pasa de Nyquist (estantes muy fuertes en los agudos) se limita: la magnitud
    // sigue coincidiendo exactamente en los tres puntos de ajuste.
    let wn = (w0 * (d[2] / d[0]).sqrt()).min(0.97 * PI);
    let zeta = d[1] / (2.0 * (d[0] * d[2]).sqrt());
    let e = (-zeta * wn).exp();
    let a1 = if zeta <= 1.0 { -2.0 * e * (wn * (1.0 - zeta * zeta).sqrt()).cos() } else { -2.0 * e * (wn * (zeta * zeta - 1.0).sqrt()).cosh() };
    let a2 = (-2.0 * zeta * wn).exp();
    let (ap0, ap1, ap2) = ((1.0 + a1 + a2).powi(2), (1.0 - a1 + a2).powi(2), -4.0 * a2);
    let den = |w: f64| {
        let p = phi(w);
        ap0 * p[0] + ap1 * p[1] + ap2 * p[2]
    };
    // Magnitudes analógicas a igualar: 0 Hz, Nyquist y la frecuencia de la banda.
    let h0 = magnitud2_analogica(pr, 0.0);
    let hpi = magnitud2_analogica(pr, PI / w0);
    let hm = magnitud2_analogica(pr, 1.0);
    let pm = phi(w0);
    if paso_alto {
        // |B|² = 16·b0²·φ1²  →  b0 = √(A(w0)·|H(w0)|²) / (4·φ1).
        let b0 = (den(w0) * hm).sqrt() / (4.0 * pm[1].max(1e-300));
        return [b0, -2.0 * b0, b0, a1, a2];
    }
    let b0_ = ap0 * h0;
    let b1_ = ap1 * hpi;
    let b2_ = (den(w0) * hm - b0_ * pm[0] - b1_ * pm[1]) / pm[2].max(1e-300);
    // Numerador con esas tres magnitudes (raíces reales, de fase mínima).
    let (s0, s1) = (b0_.max(0.0).sqrt(), b1_.max(0.0).sqrt());
    let w = (s0 + s1) / 2.0;
    let disc = (w * w + b2_).max(0.0).sqrt();
    [(w + disc) / 2.0, (s0 - s1) / 2.0, (w - disc) / 2.0, a1, a2]
}

/// Secciones de una banda (1 para campanas, estantes, notch y paso banda; 1 a 4 en los cortes).
pub fn secciones(b: &Banda, sr: f32) -> ([Coefs; SECCIONES], usize) {
    let mut out = [NEUTRO; SECCIONES];
    if !b.on {
        return (out, 0);
    }
    let (f, g, q, sr) = (b.freq as f64, b.gain as f64, (b.q as f64).clamp(0.05, 60.0), sr as f64);
    if !b.tipo.es_corte() {
        out[0] = emparejado(prototipo(b.tipo, g, q), f, sr, false);
        return (out, 1);
    }
    // Butterworth de orden 2N: Q_i = 1 / (2·cos((2i−1)·π/(4N))). La Q de la banda escala la
    // resonancia de la sección más aguda (con Q = 0,707 el corte queda Butterworth exacto).
    let n = b.pendiente + 1;
    for (i, c) in out.iter_mut().enumerate().take(n) {
        let qb = 1.0 / (2.0 * ((2.0 * i as f64 + 1.0) * PI / (4.0 * n as f64)).cos());
        let qi = if i == n - 1 { qb * q / std::f64::consts::FRAC_1_SQRT_2 } else { qb };
        *c = emparejado(prototipo(b.tipo, 0.0, qi), f, sr, b.tipo == Tipo::CorteGraves);
    }
    (out, n)
}

/// Magnitud (dB) de un biquad a la frecuencia `f`.
pub fn magnitud_db(c: &Coefs, f: f64, sr: f64) -> f64 {
    let w = 2.0 * PI * f / sr;
    let (cw, c2, sw, s2) = (w.cos(), (2.0 * w).cos(), w.sin(), (2.0 * w).sin());
    let num = (c[0] + c[1] * cw + c[2] * c2).powi(2) + (c[1] * sw + c[2] * s2).powi(2);
    let den = (1.0 + c[3] * cw + c[4] * c2).powi(2) + (c[3] * sw + c[4] * s2).powi(2);
    10.0 * (num / den).max(1e-30).log10()
}

/// Respuesta (dB) de la banda `b` a la frecuencia `f`, la misma que se aplica al audio.
pub fn respuesta_banda(p: &[f32], b: usize, f: f32, sr: f32) -> f32 {
    let (c, n) = secciones(&banda(p, b), sr);
    c[..n].iter().map(|c| magnitud_db(c, f as f64, sr as f64)).sum::<f64>() as f32
}

/// Respuesta total del EQ (dB) a la frecuencia `f`, con la salida.
pub fn respuesta(p: &[f32], f: f32, sr: f32) -> f32 {
    (0..BANDAS_EQ).map(|b| respuesta_banda(p, b, f, sr)).sum::<f32>() + p.get(P_SALIDA).copied().unwrap_or(0.0)
}

/// Analizador: FFT de 4096 puntos con ventana de Hann cada 1024 muestras, resumida en las bandas
/// logarítmicas del visor (20 Hz–20 kHz) con caída suave, como en los analizadores de estudio.
struct Analizador {
    fft: Arc<dyn Fft<f32>>,
    anillo: Vec<f32>,
    pos: usize,
    cuenta: usize,
    ventana: Vec<f32>,
    datos: Vec<Complex<f32>>,
    norma: f32,
}

const FFT_EQ: usize = 4096;
const SALTO_EQ: usize = 1024;

impl Analizador {
    fn new() -> Self {
        let ventana: Vec<f32> = (0..FFT_EQ).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / FFT_EQ as f32).cos()).collect();
        let norma = 2.0 / ventana.iter().sum::<f32>();
        Self { fft: FftPlanner::new().plan_fft_forward(FFT_EQ), anillo: vec![0.0; FFT_EQ], pos: 0, cuenta: 0, ventana, datos: vec![Complex::default(); FFT_EQ], norma }
    }

    /// Recibe la señal en mono (L+R)/2.
    fn alimentar(&mut self, muestras: impl Iterator<Item = f32>, salida: &[AtomicF32], sr: f32) {
        for m in muestras {
            self.anillo[self.pos] = m;
            self.pos = (self.pos + 1) % FFT_EQ;
            self.cuenta += 1;
            if self.cuenta >= SALTO_EQ {
                self.cuenta = 0;
                self.analizar(salida, sr, SALTO_EQ as f32 / sr);
            }
        }
    }

    fn analizar(&mut self, salida: &[AtomicF32], sr: f32, dt: f32) {
        for i in 0..FFT_EQ {
            self.datos[i] = Complex::new(self.anillo[(self.pos + i) % FFT_EQ] * self.ventana[i], 0.0);
        }
        self.fft.process(&mut self.datos);
        let bin_hz = sr / FFT_EQ as f32;
        let n = salida.len();
        // Caída de 20 dB por segundo; inclinación de 4,5 dB/octava (centrada en 1 kHz) para que el
        // espectro de una mezcla real se vea plano.
        let caida = 20.0 * dt;
        for (k, s) in salida.iter().enumerate() {
            let (f0, f1) = (20.0 * 1000f32.powf(k as f32 / n as f32), 20.0 * 1000f32.powf((k + 1) as f32 / n as f32));
            let (i0, i1) = (((f0 / bin_hz).floor() as usize).max(1), ((f1 / bin_hz).ceil() as usize).min(FFT_EQ / 2 - 1));
            let pot = (i0..=i1.max(i0)).map(|i| self.datos[i].norm_sqr()).fold(0f32, f32::max);
            let fc = (f0 * f1).sqrt();
            let db = 10.0 * (pot * self.norma * self.norma).max(1e-14).log10() + 4.5 * (fc / 1000.0).log2();
            s.set(db.max(s.get() - caida).max(-120.0));
        }
    }
}

/// Estado del EQ en el hilo de audio: coeficientes vigentes, memoria de los filtros y analizador.
pub struct EqPro {
    ultimos: [f32; N_PARAMS],
    sr: f32,
    coefs: [([Coefs; SECCIONES], usize); BANDAS_EQ],
    /// [banda][sección][canal][z1, z2]
    z: [[[[f64; 2]; 2]; SECCIONES]; BANDAS_EQ],
    temp: Vec<[f64; 2]>,
    analizador: Box<Analizador>,
}

impl EqPro {
    pub fn new(sr: f32) -> Self {
        Self { ultimos: [f32::NAN; N_PARAMS], sr, coefs: [([NEUTRO; SECCIONES], 0); BANDAS_EQ], z: Default::default(), temp: Vec::new(), analizador: Box::new(Analizador::new()) }
    }

    /// Filtra `buf` con los parámetros `p` y, si el analizador está encendido, escribe el espectro.
    pub fn procesar(&mut self, buf: &mut [[f32; 2]], p: &[f32; N_PARAMS], sr: f32, espectro: &[AtomicF32]) {
        if *p != self.ultimos || sr != self.sr {
            for b in 0..BANDAS_EQ {
                let nueva = secciones(&banda(p, b), sr);
                // Una banda que se apaga o cambia de tipo empieza con memoria limpia.
                if nueva.1 != self.coefs[b].1 {
                    self.z[b] = Default::default();
                }
                self.coefs[b] = nueva;
            }
            (self.ultimos, self.sr) = (*p, sr);
        }
        let modo = p[P_ANALIZADOR].round() as i32;
        if self.temp.len() < buf.len() {
            self.temp.resize(buf.len(), [0.0; 2]);
        }
        let x = &mut self.temp[..buf.len()];
        for (d, s) in x.iter_mut().zip(buf.iter()) {
            *d = [s[0] as f64, s[1] as f64];
        }
        if modo == 1 {
            self.analizador.alimentar(x.iter().map(|s| ((s[0] + s[1]) * 0.5) as f32), espectro, sr);
        }
        for b in 0..BANDAS_EQ {
            let (c, n) = &self.coefs[b];
            for (k, c) in c.iter().enumerate().take(*n) {
                for ch in 0..2 {
                    let z = &mut self.z[b][k][ch];
                    for s in x.iter_mut() {
                        // Forma directa transpuesta II.
                        let v = s[ch];
                        let y = c[0] * v + z[0];
                        z[0] = c[1] * v - c[3] * y + z[1];
                        z[1] = c[2] * v - c[4] * y;
                        s[ch] = y;
                    }
                    // Sin denormales ni NaN que se arrastren.
                    if !z[0].is_finite() || !z[1].is_finite() {
                        *z = [0.0; 2];
                    }
                }
            }
        }
        let g = 10f64.powf(p[P_SALIDA] as f64 / 20.0);
        for (d, s) in buf.iter_mut().zip(x.iter()) {
            *d = [(s[0] * g) as f32, (s[1] * g) as f32];
        }
        // Después del EQ: lo que sale, con la ganancia final incluida.
        if modo == 2 {
            self.analizador.alimentar(buf.iter().map(|s| (s[0] + s[1]) * 0.5), espectro, sr);
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn params_con(b: usize, tipo: Tipo, f: f32, g: f32, q: f32, pendiente: f32) -> [f32; N_PARAMS] {
        let mut p: [f32; N_PARAMS] = std::array::from_fn(|i| PARAMS[i].3);
        p[b * POR_BANDA..b * POR_BANDA + POR_BANDA].copy_from_slice(&[1.0, tipo.valor(), f, g, q, pendiente]);
        p
    }

    /// Respuesta analógica exacta (dB) de una banda simple, para comparar.
    fn analogica(tipo: Tipo, f0: f64, g: f64, q: f64, f: f64) -> f64 {
        10.0 * magnitud2_analogica(prototipo(tipo, g, q), f / f0).log10()
    }

    #[test]
    fn campana_exacta_en_el_centro_y_cerca_de_la_analogica() {
        let sr = 48_000.0;
        for &(f0, g, q) in &[(1000.0, 6.0, 1.0), (100.0, -12.0, 4.0), (12_000.0, 9.0, 0.7), (16_000.0, -6.0, 2.0)] {
            let p = params_con(0, Tipo::Campana, f0 as f32, g as f32, q as f32, 0.0);
            let centro = respuesta_banda(&p, 0, f0 as f32, sr) as f64;
            assert!((centro - g).abs() < 0.01, "{f0} Hz: {centro} dB en el centro (esperado {g})");
            // Sigue a la analógica en todo el rango (la bilineal se aleja varios dB a 16 kHz).
            for f in [30.0, 200.0, 1000.0, 5000.0, 10_000.0, 15_000.0, 20_000.0] {
                let (d, a) = (respuesta_banda(&p, 0, f as f32, sr) as f64, analogica(Tipo::Campana, f0, g, q, f));
                // Bandas altas o puntos por encima de 0,8·Nyquist: el método se aparta algo (hasta 1 dB).
                let tolerancia = if f > 0.4 * sr as f64 || f0 > 0.25 * sr as f64 { 1.0 } else { 0.5 };
                assert!((d - a).abs() < tolerancia,"campana {f0} Hz {g} dB Q{q} a {f} Hz: digital {d:.2}, analógica {a:.2}");
            }
        }
    }

    /// En los agudos el diseño emparejado sigue a la analógica mucho mejor que la bilineal (RBJ).
    #[test]
    fn mejor_que_la_bilineal_en_los_agudos() {
        let sr = 48_000.0f32;
        let (f0, g, q) = (16_000.0, -6.0, 2.0);
        let p = params_con(0, Tipo::Campana, f0 as f32, g as f32, q as f32, 0.0);
        let rbj = crate::fx::biquad(1, f0 as f32, g as f32, q as f32, sr).map(|v| v as f64);
        let (mut err_m, mut err_b) = (0f64, 0f64);
        for f in [8000.0, 12_000.0, 15_000.0, 16_000.0, 18_000.0, 20_000.0] {
            let a = analogica(Tipo::Campana, f0, g, q, f);
            err_m = err_m.max((respuesta_banda(&p, 0, f as f32, sr) as f64 - a).abs());
            err_b = err_b.max((magnitud_db(&rbj, f, sr as f64) - a).abs());
        }
        assert!(err_m < 1.0 && err_m * 2.0 < err_b, "error máximo: emparejado {err_m:.2} dB, bilineal {err_b:.2} dB");
    }

    #[test]
    fn estantes_llegan_a_su_ganancia() {
        let sr = 48_000.0;
        let p = params_con(0, Tipo::EstanteGraves, 200.0, 6.0, 0.707, 0.0);
        assert!((respuesta_banda(&p, 0, 20.0, sr) - 6.0).abs() < 0.1);
        assert!(respuesta_banda(&p, 0, 10_000.0, sr).abs() < 0.1);
        let p = params_con(0, Tipo::EstanteAgudos, 5000.0, -9.0, 0.707, 0.0);
        assert!((respuesta_banda(&p, 0, 20_000.0, sr) + 9.0).abs() < 0.5);
        assert!(respuesta_banda(&p, 0, 50.0, sr).abs() < 0.1);
        // A mitad del estante la ganancia es la mitad (en dB).
        assert!((respuesta_banda(&p, 0, 5000.0, sr) + 4.5).abs() < 0.05);
    }

    #[test]
    fn cortes_con_su_pendiente() {
        let sr = 48_000.0;
        for (pend, db_oct) in [(0.0, 12.0), (1.0, 24.0), (2.0, 36.0), (3.0, 48.0)] {
            let p = params_con(0, Tipo::CorteGraves, 1000.0, 0.0, 0.707, pend);
            // Butterworth: −3 dB en la frecuencia de corte.
            let fc = respuesta_banda(&p, 0, 1000.0, sr);
            assert!((fc + 3.01).abs() < 0.1, "{db_oct} dB/oct: {fc} dB en el corte");
            // Lejos del corte (5 y 6 octavas abajo) la caída por octava es la pendiente.
            let (a, b) = (respuesta_banda(&p, 0, 31.25, sr), respuesta_banda(&p, 0, 15.625, sr));
            assert!(((a - b) - db_oct).abs() < 0.5, "{db_oct} dB/oct: cae {} dB por octava", a - b);
            assert!(respuesta_banda(&p, 0, 10_000.0, sr).abs() < 0.05);
        }
    }

    #[test]
    fn filtros_estables_y_audio_igual_a_la_curva() {
        let sr = 48_000.0f32;
        let mut p = params_con(2, Tipo::Campana, 3000.0, 12.0, 2.0, 0.0);
        p[P_ANALIZADOR] = 0.0;
        for b in 0..BANDAS_EQ {
            for tipo in Tipo::TODOS {
                for f in [20.0, 1000.0, 20_000.0] {
                    let (c, n) = secciones(&Banda { on: true, tipo, freq: f, gain: 18.0, q: 10.0, pendiente: 3 }, sr);
                    for c in &c[..n] {
                        // Polos dentro del círculo unidad: |a2| < 1 y |a1| < 1 + a2.
                        assert!(c[4].abs() < 1.0 && c[3].abs() < 1.0 + c[4], "{tipo:?} {f} Hz banda {b}: inestable {c:?}");
                    }
                }
            }
        }
        // Un seno de 3 kHz sale con la ganancia que dibuja la curva (+12 dB).
        let mut eq = EqPro::new(sr);
        let espectro: Vec<AtomicF32> = (0..96).map(|_| AtomicF32::new(-120.0)).collect();
        let n = 48_000;
        let mut buf: Vec<[f32; 2]> = (0..n).map(|i| [((2.0 * std::f32::consts::PI * 3000.0 * i as f32 / sr).sin()) * 0.1; 2]).collect();
        for trozo in buf.chunks_mut(512) {
            eq.procesar(trozo, &p, sr, &espectro);
        }
        let pico = buf[n / 2..].iter().fold(0f32, |m, s| m.max(s[0].abs()));
        let g = 20.0 * (pico / 0.1).log10();
        assert!((g - respuesta(&p, 3000.0, sr)).abs() < 0.1, "seno de 3 kHz: {g} dB");
    }
}
