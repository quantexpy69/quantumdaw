//! QUANTUM Voz al frente: quita el eco de la sala (reverberación tardía) para que la voz suene cerca y
//! adelante. Análisis por bandas (STFT de 1024 con salto de 256): la cola de la sala se estima como la
//! energía de hace ~50 ms caída según el tiempo de reverberación (modelo de Lebart/Habets), y cada banda
//! se baja en la proporción que es cola, con un piso y un suavizado en el tiempo para que no aparezcan
//! «burbujas». La voz directa (más fuerte que la cola estimada) pasa sin tocarse.
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::{f32::consts::PI, sync::Arc};

const N: usize = 1024;
const SALTO: usize = 256;
const BINS: usize = N / 2 + 1;

pub struct Ajustes {
    /// 0..1: cuánto eco se quita.
    pub reduccion: f32,
    /// Tiempo de reverberación de la sala (RT60), en segundos.
    pub sala: f32,
    /// 0..1: suavizado en el tiempo de la ganancia de cada banda.
    pub suavidad: f32,
    /// Realce de presencia (2–5 kHz) en dB, para que la voz quede al frente.
    pub presencia: f32,
    pub salida: f32,
    /// Escuchar solo lo que se quita (el eco).
    pub escuchar: bool,
}

pub struct DeReverb {
    sr: f32,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    ventana: Vec<f32>,
    entrada: [Vec<f32>; 2],
    salida: [Vec<f32>; 2],
    cuenta: usize,
    /// Potencia de los últimos cuadros por canal (anillo) para la estimación con retardo.
    historia: [Vec<Vec<f32>>; 2],
    h: usize,
    ganancia: [Vec<f32>; 2],
    buf: Vec<Complex<f32>>,
    /// Reducción media del último cuadro (dB, para el visor).
    pub reduccion_db: f32,
}

impl DeReverb {
    pub fn new(sr: f32) -> Self {
        let mut plan = FftPlanner::new();
        let ventana = (0..N).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / N as f32).cos()).collect();
        let retardo = ((0.05 * sr) / SALTO as f32).round().max(2.0) as usize;
        Self {
            sr,
            fft: plan.plan_fft_forward(N),
            ifft: plan.plan_fft_inverse(N),
            ventana,
            entrada: [vec![0.0; N], vec![0.0; N]],
            salida: [vec![0.0; N], vec![0.0; N]],
            cuenta: 0,
            historia: std::array::from_fn(|_| vec![vec![0.0; BINS]; retardo + 1]),
            h: 0,
            ganancia: [vec![1.0; BINS], vec![1.0; BINS]],
            buf: vec![Complex::default(); N],
            reduccion_db: 0.0,
        }
    }

    /// Latencia del proceso en muestras.
    pub const LATENCIA: usize = N;

    pub fn procesar(&mut self, audio: &mut [[f32; 2]], a: &Ajustes) {
        for s in audio.iter_mut() {
            let k = N - SALTO + self.cuenta;
            let sal = [self.salida[0][self.cuenta], self.salida[1][self.cuenta]];
            self.entrada[0][k] = s[0];
            self.entrada[1][k] = s[1];
            *s = sal;
            self.cuenta += 1;
            if self.cuenta == SALTO {
                self.cuenta = 0;
                self.cuadro(a);
            }
        }
    }

    fn cuadro(&mut self, a: &Ajustes) {
        let sr = self.sr;
        let retardo = self.historia[0].len() - 1;
        // Caída de energía de la sala en el retardo: e^(-2·δ·Td) con δ = 3·ln10 / RT60.
        let td = retardo as f32 * SALTO as f32 / sr;
        let caida = (-2.0 * 3.0 * std::f32::consts::LN_10 / a.sala.max(0.1) * td).exp();
        // Hasta 30 dB menos en lo que es pura cola; con reducción 0 no se toca nada.
        let piso = 10f32.powf(-(30.0 * a.reduccion) / 20.0);
        let sostenido = (1.0 / caida.max(1e-4) - 1.0).max(1e-3);
        let suave = 0.2 + 0.75 * a.suavidad;
        let salida = 10f32.powf(a.salida / 20.0);
        let bin_hz = sr / N as f32;
        let (mut e_antes, mut e_despues) = (0f32, 0f32);
        for ch in 0..2 {
            for i in 0..N {
                self.buf[i] = Complex::new(self.entrada[ch][i] * self.ventana[i], 0.0);
            }
            self.fft.process(&mut self.buf);
            let anterior = (self.h + 1) % (retardo + 1);
            for k in 0..BINS {
                // Potencia algo suavizada entre cuadros (menos ruido en la comparación).
                let p = 0.5 * self.buf[k].norm_sqr() + 0.5 * self.historia[ch][(self.h + retardo) % (retardo + 1)][k];
                let cola = caida * self.historia[ch][anterior][k];
                self.historia[ch][self.h][k] = p;
                // Cómo cae la banda respecto de cómo caería la sala: r ≈ 1 es cola pura; r ≥ 1/caída es
                // sonido que se sostiene o empieza (voz directa) y pasa entero.
                let r = p / cola.max(1e-12);
                let directo = ((r - 1.0) / sostenido).clamp(0.0, 1.0);
                // Sin señal no hay nada que quitar.
                let g_obj = if p < 1e-10 { 1.0 } else { piso + (1.0 - piso) * directo.sqrt() };
                // Sube rápido (no se come los ataques de la voz) y baja suavizado.
                let g = &mut self.ganancia[ch][k];
                *g = if g_obj > *g { g_obj } else { *g * suave + g_obj * (1.0 - suave) };
                let f = k as f32 * bin_hz;
                let realce = 10f32.powf(a.presencia * (-((f.max(1.0) / 3200.0).log2().powi(2)) * 2.0).exp() / 20.0);
                let gf = if a.escuchar { 1.0 - *g } else { *g * realce };
                (e_antes, e_despues) = (e_antes + p, e_despues + p * *g * *g);
                self.buf[k] *= gf * salida;
                if k > 0 && k < N / 2 {
                    self.buf[N - k] = self.buf[k].conj();
                }
            }
            self.ifft.process(&mut self.buf);
            // Ventana de Hann con salto N/4: la suma de solapes vale 2 → 1/(2N) junto con la IFFT.
            let norma = 1.0 / (2.0 * N as f32);
            self.salida[ch].copy_within(SALTO.., 0);
            for v in &mut self.salida[ch][N - SALTO..] {
                *v = 0.0;
            }
            for i in 0..N {
                self.salida[ch][i] += self.buf[i].re * norma;
            }
            self.entrada[ch].copy_within(SALTO.., 0);
        }
        self.h = (self.h + 1) % (retardo + 1);
        // Reducción de energía del cuadro (0 dB si no hay señal), algo suavizada para leerla.
        let red = if e_antes > 1e-9 { -10.0 * (e_despues / e_antes).max(1e-6).log10() } else { 0.0 };
        self.reduccion_db = self.reduccion_db * 0.7 + red * 0.3;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ajustes(reduccion: f32) -> Ajustes {
        Ajustes { reduccion, sala: 1.2, suavidad: 0.5, presencia: 0.0, salida: 0.0, escuchar: false }
    }

    #[test]
    fn sin_reduccion_pasa_igual_con_latencia() {
        let sr = 48_000.0;
        let mut d = DeReverb::new(sr);
        let x: Vec<[f32; 2]> = (0..8192).map(|i| [((i as f32) * 0.05).sin() * 0.5; 2]).collect();
        let mut y = x.clone();
        // Con reducción 0 la ganancia sigue cerca de 1 en una señal estable (la cola nunca supera la señal).
        d.procesar(&mut y, &ajustes(0.0));
        let l = DeReverb::LATENCIA;
        let err: f32 = (4096..8000).map(|i| (y[i][0] - x[i - l][0]).abs()).fold(0.0, f32::max);
        assert!(err < 1e-4, "error {err}");
    }

    #[test]
    fn baja_la_cola_de_un_golpe() {
        // Un golpe seguido de una cola que decae (como una sala): la cola debe quedar más baja.
        let sr = 48_000.0;
        let mut ruido = 12345u32;
        let mut al = || {
            ruido = ruido.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (ruido >> 9) as f32 / (1 << 23) as f32 - 1.0
        };
        let x: Vec<[f32; 2]> = (0..48_000)
            .map(|i| {
                let t = i as f32 / sr;
                let golpe = if t < 0.1 { 0.8 } else { 0.0 };
                let cola = if t >= 0.1 { 0.5 * (-(t - 0.1) * 5.0).exp() } else { 0.0 };
                let v = al() * (golpe + cola);
                [v, v]
            })
            .collect();
        let energia = |v: &[[f32; 2]], a: usize, b: usize| v[a..b].iter().map(|s| s[0] * s[0]).sum::<f32>();
        let mut y = x.clone();
        DeReverb::new(sr).procesar(&mut y, &ajustes(1.0));
        let l = DeReverb::LATENCIA;
        let (a, b) = ((0.35 * sr) as usize, (0.8 * sr) as usize);
        assert!(energia(&y, a + l, b + l) < 0.5 * energia(&x, a, b), "{} {}", energia(&y, a + l, b + l), energia(&x, a, b));
    }
}
