//! Reverb de red de retardos realimentada (FDN): 8 líneas de largos sin divisores comunes, mezcladas con
//! una matriz de Householder (sin pérdida de energía), cada una con su ganancia para que la cola caiga
//! -60 dB en el tiempo de reverberación pedido y un filtro de agudos que apaga antes los agudos (como el
//! aire y las paredes). Delante, cuatro pasa-todos difusores; dos líneas se modulan muy poco para que la
//! cola no suene metálica. Sale en estéreo con las líneas pares a la izquierda y las impares a la derecha.
use std::f32::consts::TAU;

const N: usize = 8;
/// Largos base en ms (primos entre sí, separados para una densidad pareja).
const LARGOS_MS: [f32; N] = [29.7, 37.1, 41.1, 43.7, 53.0, 59.9, 67.7, 73.1];
const DIFUSORES_MS: [f32; 4] = [4.7, 3.6, 12.7, 9.3];

struct Linea {
    buf: Vec<f32>,
    i: usize,
}

impl Linea {
    fn new(n: usize) -> Self {
        Self { buf: vec![0.0; n.max(4)], i: 0 }
    }
    fn leer(&self, atras: f32) -> f32 {
        let n = self.buf.len();
        let a = atras.clamp(1.0, (n - 2) as f32);
        let (k, f) = (a as usize, a.fract());
        let x0 = self.buf[(self.i + n - k) % n];
        let x1 = self.buf[(self.i + n - k - 1) % n];
        x0 + (x1 - x0) * f
    }
    fn escribir(&mut self, v: f32) {
        self.buf[self.i] = if v.abs() < 1e-20 { 0.0 } else { v };
        self.i = (self.i + 1) % self.buf.len();
    }
}

pub struct Fdn {
    sr: f32,
    lineas: Vec<Linea>,
    filtro: [f32; N],
    difusores: Vec<Linea>,
    fase: f32,
}

pub struct Ajustes {
    /// 0..1: tamaño de la sala (largos de las líneas y tiempo de reverberación).
    pub tamano: f32,
    /// 0..1: cuánto antes se apagan los agudos.
    pub amortiguacion: f32,
}

impl Fdn {
    pub fn new(sr: f32) -> Self {
        // Las líneas se crean para el tamaño máximo (×1,6); el tamaño elige cuánto se lee.
        let lineas = LARGOS_MS.iter().map(|ms| Linea::new((ms * 1.6 * 0.001 * sr) as usize + 8)).collect();
        let difusores = DIFUSORES_MS.iter().map(|ms| Linea::new((ms * 0.001 * sr) as usize + 2)).collect();
        Self { sr, lineas, filtro: [0.0; N], difusores, fase: 0.0 }
    }

    /// Tiempo de reverberación (s) que corresponde al tamaño: de 0,4 s a 8 s.
    pub fn rt60(tamano: f32) -> f32 {
        0.4 * 20f32.powf(tamano.clamp(0.0, 1.0))
    }

    /// Procesa una muestra mono y devuelve la cola estéreo.
    pub fn procesar(&mut self, x: f32, a: &Ajustes) -> [f32; 2] {
        let sr = self.sr;
        let escala = 0.6 + a.tamano.clamp(0.0, 1.0);
        let rt = Self::rt60(a.tamano);
        let corte = 16_000.0 * 0.06f32.powf(a.amortiguacion.clamp(0.0, 1.0));
        let lp = 1.0 - (-TAU * corte.min(sr * 0.45) / sr).exp();
        // Difusión de la entrada: pasa-todos en serie (g = 0,6).
        let mut d = x;
        for (k, ap) in self.difusores.iter_mut().enumerate() {
            let largo = (DIFUSORES_MS[k] * 0.001 * sr).max(2.0);
            let b = ap.leer(largo);
            let v = d + b * 0.6;
            ap.escribir(v);
            d = b - v * 0.6;
        }
        // Modulación lenta y mínima (±0,3 ms) en dos líneas.
        self.fase = (self.fase + 0.37 / sr).fract();
        let m = (TAU * self.fase).sin() * 0.0003 * sr;
        let mut salida = [0f32; N];
        for (k, l) in self.lineas.iter().enumerate() {
            let largo = LARGOS_MS[k] * escala * 0.001 * sr + if k == 2 { m } else if k == 5 { -m } else { 0.0 };
            // Ganancia para caer 60 dB en `rt` según el largo de la línea.
            let g = 10f32.powf(-3.0 * (largo / sr) / rt);
            let v = l.leer(largo) * g;
            self.filtro[k] += (v - self.filtro[k]) * lp;
            salida[k] = self.filtro[k];
        }
        // Householder: y = x − (2/N)·Σx.
        let suma: f32 = salida.iter().sum::<f32>() * (2.0 / N as f32);
        for (k, l) in self.lineas.iter_mut().enumerate() {
            l.escribir(salida[k] - suma + d * if k % 2 == 0 { 1.0 } else { -1.0 } * 0.5);
        }
        let (mut izq, mut der) = (0f32, 0f32);
        for (k, v) in salida.iter().enumerate() {
            if k % 2 == 0 {
                izq += v * if k % 4 == 0 { 1.0 } else { -1.0 };
            } else {
                der += v * if k % 4 == 1 { 1.0 } else { -1.0 };
            }
        }
        [izq * 0.5, der * 0.5]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiempo en que la energía de la respuesta cae 30 dB (×2 = RT60 estimado).
    fn rt_medido(tamano: f32) -> f32 {
        let sr = 48_000.0;
        let mut f = Fdn::new(sr);
        let a = Ajustes { tamano, amortiguacion: 0.0 };
        let n = (sr * 12.0) as usize;
        let y: Vec<f32> = (0..n).map(|i| f.procesar(if i == 0 { 1.0 } else { 0.0 }, &a)).map(|s| s[0] * s[0] + s[1] * s[1]).collect();
        // Integral de Schroeder.
        let mut cola = vec![0f32; n + 1];
        for i in (0..n).rev() {
            cola[i] = cola[i + 1] + y[i];
        }
        let total = cola[0];
        let t5 = cola.iter().position(|c| *c < total * 10f32.powf(-0.5)).unwrap_or(n);
        let t35 = cola.iter().position(|c| *c < total * 10f32.powf(-3.5)).unwrap_or(n);
        (t35 - t5) as f32 / sr * 2.0
    }

    #[test]
    fn el_tiempo_de_reverberacion_es_el_pedido() {
        for tamano in [0.2, 0.6] {
            let (pedido, medido) = (Fdn::rt60(tamano), rt_medido(tamano));
            assert!((medido / pedido - 1.0).abs() < 0.25, "tamaño {tamano}: pedido {pedido} s, medido {medido} s");
        }
    }

    #[test]
    fn es_estable_y_no_satura() {
        let sr = 44_100.0;
        let mut f = Fdn::new(sr);
        let a = Ajustes { tamano: 1.0, amortiguacion: 0.0 };
        let mut pico = 0f32;
        for i in 0..(sr as usize * 4) {
            let x = if i < 2000 { ((i * 7919) % 200) as f32 / 100.0 - 1.0 } else { 0.0 };
            let s = f.procesar(x * 0.5, &a);
            pico = pico.max(s[0].abs()).max(s[1].abs());
            assert!(s[0].is_finite() && s[1].is_finite());
        }
        assert!(pico < 2.0, "pico {pico}");
    }
}

