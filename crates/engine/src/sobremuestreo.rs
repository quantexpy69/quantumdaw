//! Sobremuestreo ×4 para las distorsiones: la curva se aplica a 4 veces la frecuencia de muestreo y un
//! filtro FIR de fase lineal (sinc con ventana de Blackman, 49 coeficientes) quita lo que pasa de la mitad
//! de la frecuencia original antes de volver. Así los armónicos altos no se doblan sobre el audio
//! (aliasing). Retardo total: 12 muestras a la frecuencia original (la señal seca se retrasa igual).
const L: usize = 49;
const FASES: usize = 4;
/// Coeficientes por fase de la interpolación (49 + relleno = 52 = 4 × 13).
const POR_FASE: usize = 13;
pub const RETARDO: usize = (L - 1) / FASES;

fn coeficientes() -> [f32; L] {
    let fc = 0.5 / FASES as f64 * 0.92;
    let mut h = [0f32; L];
    let m = (L - 1) as f64 / 2.0;
    let mut suma = 0.0;
    for (n, v) in h.iter_mut().enumerate() {
        let x = n as f64 - m;
        let sinc = if x == 0.0 { 2.0 * fc } else { (2.0 * std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * x) };
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / (L - 1) as f64).cos() + 0.08 * (4.0 * std::f64::consts::PI * n as f64 / (L - 1) as f64).cos();
        *v = (sinc * w) as f32;
        suma += sinc * w;
    }
    h.iter_mut().for_each(|v| *v /= suma as f32);
    h
}

/// Un canal: historia de la entrada (para interpolar), de la señal sobremuestreada (para decimar) y de
/// la señal seca (para alinearla con la procesada).
pub struct Sobremuestreo {
    h: [f32; L],
    entrada: [f32; POR_FASE + 1],
    alta: [f32; L],
    seca: [f32; RETARDO + 1],
    i: usize,
}

impl Default for Sobremuestreo {
    fn default() -> Self {
        Self { h: coeficientes(), entrada: [0.0; POR_FASE + 1], alta: [0.0; L], seca: [0.0; RETARDO + 1], i: 0 }
    }
}

impl Sobremuestreo {
    /// Procesa una muestra con `f` a 4× y devuelve (procesada, seca alineada).
    pub fn procesar(&mut self, x: f32, f: impl Fn(f32) -> f32) -> (f32, f32) {
        self.entrada.copy_within(0..POR_FASE, 1);
        self.entrada[0] = x;
        // Las tres muestras intermedias entre la anterior y esta, y luego esta: así la última que entra
        // a la decimación cae justo en el instante de `x` y el retardo es entero.
        for fase in [1, 2, 3, 0] {
            // Interpolación: ceros intercalados filtrados = una fase de los coeficientes (ganancia ×4).
            let desde = usize::from(fase != 0);
            let mut v = 0.0;
            for (k, e) in self.entrada[desde..].iter().enumerate() {
                let j = fase + FASES * k;
                if j < L {
                    v += self.h[j] * e;
                }
            }
            self.alta.copy_within(0..L - 1, 1);
            self.alta[0] = f(v * FASES as f32);
        }
        // Decimación: filtro y una de cada cuatro.
        let y = self.alta.iter().zip(self.h.iter()).map(|(a, b)| a * b).sum();
        self.seca[self.i] = x;
        self.i = (self.i + 1) % (RETARDO + 1);
        (y, self.seca[self.i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn bin(x: &[f32], f: f32, sr: f32) -> f32 {
        let (mut re, mut im) = (0f32, 0f32);
        for (i, v) in x.iter().enumerate() {
            let a = TAU * f * i as f32 / sr;
            (re, im) = (re + v * a.cos(), im + v * a.sin());
        }
        (re * re + im * im).sqrt() / x.len() as f32
    }

    #[test]
    fn pasa_igual_con_su_retardo() {
        let mut o = Sobremuestreo::default();
        let x: Vec<f32> = (0..2000).map(|i| (i as f32 * 0.03).sin() * 0.5).collect();
        let y: Vec<(f32, f32)> = x.iter().map(|v| o.procesar(*v, |s| s)).collect();
        for i in 200..2000 {
            assert!((y[i].0 - x[i - RETARDO]).abs() < 2e-3, "{i}: {} {}", y[i].0, x[i - RETARDO]);
            assert_eq!(y[i].1, x[i - RETARDO]);
        }
    }

    #[test]
    fn baja_el_aliasing() {
        // 5 kHz muy saturado a 48 kHz: el 9.º armónico (45 kHz) se dobla a 3 kHz sin sobremuestreo.
        let sr = 48_000.0;
        let x: Vec<f32> = (0..9600).map(|i| (TAU * 5000.0 * i as f32 / sr).sin() * 0.9).collect();
        let f = |s: f32| (s * 20.0).tanh();
        let directo: Vec<f32> = x.iter().map(|v| f(*v)).collect();
        let mut o = Sobremuestreo::default();
        let sobre: Vec<f32> = x.iter().map(|v| o.procesar(*v, f).0).collect();
        let (a, b) = (bin(&directo[200..], 3000.0, sr), bin(&sobre[200..], 3000.0, sr));
        assert!(b < a * 0.1, "sin sobremuestreo {a}, con {b}");
    }
}
