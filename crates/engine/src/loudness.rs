//! Medidor de loudness del master (ITU-R BS.1770 / EBU R128): sonoridad momentánea (400 ms), de
//! corto plazo (3 s) e integrada con compuertas, rango de sonoridad (LRA), true peak con
//! sobremuestreo 4× y correlación de fase. El hilo de audio solo filtra y suma por bloques de 100 ms;
//! los cálculos con historial (integrado y LRA) los hace la interfaz con `resumen`.
use crate::AtomicF32;
use crate::fx::biquad;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

/// Una hora de bloques de 100 ms.
const MAX_BLOQUES: usize = 36_000;

struct Estado {
    /// Ponderación K: estante de agudos y paso alto, por canal.
    coefs: [[f32; 5]; 2],
    z: [[[f32; 2]; 2]; 2],
    largo_bloque: usize,
    suma: f64,
    n: usize,
    /// Energía media de los últimos 30 bloques (3 s).
    bloques: VecDeque<f64>,
    /// Historial de energías de 400 ms (para el integrado) y de 3 s (para el LRA), cada 100 ms.
    momentaneas: Vec<f64>,
    cortas: Vec<f64>,
    /// Últimas 4 muestras por canal para interpolar el true peak.
    hist: [[f32; 4]; 2],
    corr: [f64; 3],
}

pub struct Medidor {
    estado: Mutex<Estado>,
    pub momentaneo: AtomicF32,
    pub corto: AtomicF32,
    /// True peak máximo desde el último reinicio (dBTP).
    pub true_peak: AtomicF32,
    pub correlacion: AtomicF32,
    pub reiniciar: AtomicBool,
}

/// Sonoridad (LUFS) de una energía media.
pub fn lufs(energia: f64) -> f32 {
    if energia <= 0.0 { -120.0 } else { (-0.691 + 10.0 * energia.log10()) as f32 }
}

impl Medidor {
    pub fn new(sr: u32) -> Self {
        let f = sr as f32;
        let estado = Estado {
            coefs: [biquad(2, 1681.97, 4.0, 0.7072, f), biquad(3, 38.13, 0.0, 0.5003, f)],
            z: Default::default(),
            largo_bloque: (sr / 10).max(1) as usize,
            suma: 0.0,
            n: 0,
            bloques: VecDeque::with_capacity(31),
            momentaneas: Vec::with_capacity(MAX_BLOQUES),
            cortas: Vec::with_capacity(MAX_BLOQUES),
            hist: Default::default(),
            corr: [0.0; 3],
        };
        Self { estado: Mutex::new(estado), momentaneo: AtomicF32::new(-120.0), corto: AtomicF32::new(-120.0), true_peak: AtomicF32::new(-120.0), correlacion: AtomicF32::new(1.0), reiniciar: AtomicBool::new(false) }
    }

    /// Mide un bloque de la salida del master (hilo de audio: no bloquea).
    pub fn medir(&self, out: &[[f32; 2]]) {
        let Ok(mut st) = self.estado.try_lock() else { return };
        let st = &mut *st;
        if self.reiniciar.swap(false, Relaxed) {
            st.momentaneas.clear();
            st.cortas.clear();
            st.bloques.clear();
            self.true_peak.set(-120.0);
        }
        let mut tp = 0f32;
        for s in out {
            let mut e = 0f64;
            for (ch, &muestra) in s.iter().enumerate() {
                let mut x = muestra;
                for (k, c) in st.coefs.iter().enumerate() {
                    let z = &mut st.z[k][ch];
                    let y = c[0] * x + z[0];
                    (z[0], z[1]) = (c[1] * x - c[3] * y + z[1], c[2] * x - c[4] * y);
                    x = y;
                }
                e += (x * x) as f64;
                // True peak: tres puntos interpolados (Hermite cúbico) entre las dos muestras centrales.
                let h = &mut st.hist[ch];
                (h[0], h[1], h[2], h[3]) = (h[1], h[2], h[3], muestra);
                for t in [0.25f32, 0.5, 0.75] {
                    let (c0, c1) = (h[1], 0.5 * (h[2] - h[0]));
                    let c2 = h[0] - 2.5 * h[1] + 2.0 * h[2] - 0.5 * h[3];
                    let c3 = 0.5 * (h[3] - h[0]) + 1.5 * (h[1] - h[2]);
                    tp = tp.max((((c3 * t + c2) * t + c1) * t + c0).abs());
                }
                tp = tp.max(h[3].abs());
            }
            st.suma += e;
            st.corr[0] += (s[0] * s[1]) as f64;
            st.corr[1] += (s[0] * s[0]) as f64;
            st.corr[2] += (s[1] * s[1]) as f64;
            st.n += 1;
            if st.n >= st.largo_bloque {
                let energia = st.suma / st.n as f64;
                if st.bloques.len() == 30 {
                    st.bloques.pop_front();
                }
                st.bloques.push_back(energia);
                let media = |k: usize| st.bloques.iter().rev().take(k).sum::<f64>() / k.min(st.bloques.len()).max(1) as f64;
                let (m, c) = (media(4), media(30));
                self.momentaneo.set(lufs(m));
                self.corto.set(lufs(c));
                if st.momentaneas.len() < MAX_BLOQUES {
                    st.momentaneas.push(m);
                    st.cortas.push(c);
                }
                let [lr, ll, rr] = st.corr;
                if ll > 1e-9 && rr > 1e-9 {
                    let actual = (lr / (ll * rr).sqrt()) as f32;
                    self.correlacion.set(self.correlacion.get() * 0.6 + actual * 0.4);
                }
                (st.suma, st.n, st.corr) = (0.0, 0, [0.0; 3]);
            }
        }
        let tp_db = 20.0 * tp.max(1e-9).log10();
        if tp_db > self.true_peak.get() {
            self.true_peak.set(tp_db);
        }
    }

    /// Sonoridad integrada (compuerta absoluta de -70 LUFS y relativa de -10 LU) y rango de sonoridad
    /// (percentiles 10–95 de la de corto plazo con compuerta relativa de -20 LU). Para la interfaz.
    pub fn resumen(&self) -> (f32, f32) {
        let Ok(st) = self.estado.lock() else { return (-120.0, 0.0) };
        let integrado = |v: &[f64], relativa: f32| -> (f32, Vec<f64>) {
            let sobre: Vec<f64> = v.iter().copied().filter(|&e| lufs(e) > -70.0).collect();
            if sobre.is_empty() {
                return (-120.0, vec![]);
            }
            let previo = lufs(sobre.iter().sum::<f64>() / sobre.len() as f64);
            let dentro: Vec<f64> = sobre.into_iter().filter(|&e| lufs(e) > previo + relativa).collect();
            (lufs(dentro.iter().sum::<f64>() / dentro.len().max(1) as f64), dentro)
        };
        let (i, _) = integrado(&st.momentaneas, -10.0);
        let (_, mut cortas) = integrado(&st.cortas, -20.0);
        cortas.sort_by(|a, b| a.total_cmp(b));
        let lra = if cortas.len() < 10 {
            0.0
        } else {
            let p = |q: f64| lufs(cortas[((cortas.len() - 1) as f64 * q) as usize]);
            p(0.95) - p(0.10)
        };
        (i, lra)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Un seno de 1 kHz a -20 dBFS en ambos canales mide unos -20 LUFS (+3 dB por ser estéreo:
    /// -23 + 3), el true peak ronda -20 dBTP y la correlación es 1.
    #[test]
    fn seno_de_referencia() {
        let sr = 48_000;
        let m = Medidor::new(sr);
        let a = 10f32.powf(-20.0 / 20.0);
        let buf: Vec<[f32; 2]> = (0..sr as usize * 5).map(|i| [(i as f32 * std::f32::consts::TAU * 1000.0 / sr as f32).sin() * a; 2]).collect();
        buf.chunks(512).for_each(|b| m.medir(b));
        let (i, _) = m.resumen();
        assert!((i - (-20.0)).abs() < 0.6, "integrado {i}");
        assert!((m.true_peak.get() + 20.0).abs() < 0.5, "true peak {}", m.true_peak.get());
        assert!(m.correlacion.get() > 0.99);
    }
}
