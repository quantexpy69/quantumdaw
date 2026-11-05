//! QUANTUM ANTIPOP: baja los golpes de aire de las P y B (plosivas) y el chasquido de las T y K
//! (consonantes duras), sutil y solo en el instante en que aparecen, y no deja que la salida sature.
//!
//! - Plosivas: un golpe de graves (bajo ~160 Hz) mucho más fuerte que el resto de la voz. Se baja solo
//!   la banda grave y solo mientras dura el golpe; la voz grave normal (que no supera al resto) no se toca.
//! - Consonantes: un ataque súbito de agudos (sobre ~4 kHz) respecto de su propio promedio. Se baja
//!   solo la banda aguda unos milisegundos.
//! - La reducción es una mezcla entre la señal y la misma señal filtrada (paso alto para las P, paso
//!   bajo para las T): sin reducción la salida es idéntica a la entrada, y a fondo el golpe baja hasta
//!   ~24 dB sin los residuos de fase de restar bandas.
//! - Anticipación de 3 ms: la reducción ya está aplicada cuando llega el golpe.
//! - Techo suave: nada pasa del techo (curva continua, sin recorte duro).
use crate::fx::biquad;

/// Anticipación (s) entre la detección y la señal que sale.
const ANTICIPACION: f32 = 0.003;

/// Biquad estéreo en forma directa transpuesta.
#[derive(Default, Clone, Copy)]
struct Filtro {
    c: [f32; 5],
    z: [[f32; 2]; 2],
}

impl Filtro {
    fn poner(&mut self, c: [f32; 5]) {
        self.c = c;
    }
    fn paso(&mut self, x: f32, ch: usize) -> f32 {
        let (c, z) = (&self.c, &mut self.z[ch]);
        let y = c[0] * x + z[0];
        (z[0], z[1]) = (c[1] * x - c[3] * y + z[1], c[2] * x - c[4] * y);
        y
    }
}

/// Seguidor de envolvente con ataque y relevo (coeficientes por muestra).
fn seguir(env: &mut f32, x: f32, (att, rel): (f32, f32)) {
    *env += (x - *env) * if x > *env { att } else { rel };
}

fn coef(seg: f32, sr: f32) -> f32 {
    1.0 - (-1.0 / (seg * sr).max(1.0)).exp()
}

fn db(x: f32) -> f32 {
    20.0 * x.max(1e-9).log10()
}

#[derive(Default)]
pub struct AntiPop {
    /// Detección (sin retardo): graves de 4.º orden, resto y agudos de 2.º orden.
    det_graves: [Filtro; 2],
    det_agudos: Filtro,
    /// El resto de la voz (sobre el doble del corte de plosivas), para comparar.
    det_resto: Filtro,
    /// Filtros de la señal retrasada: paso alto (sin el golpe de aire) y paso bajo (sin el chasquido).
    sin_graves: [Filtro; 2],
    sin_agudos: Filtro,
    linea: Vec<[f32; 2]>,
    idx: usize,
    env_graves: f32,
    env_resto: f32,
    env_rapida: f32,
    env_lenta: f32,
    /// Reducción actual en dB (suavizada) de graves y agudos.
    red_p: f32,
    red_t: f32,
    ultimos: (f32, f32, f32),
}

/// Parámetros, en el orden de `FxKind::AntiPop::params()`.
pub struct Ajustes {
    pub plosivas: f32,
    pub frec_p: f32,
    pub consonantes: f32,
    pub frec_t: f32,
    pub sensibilidad: f32,
    pub techo_db: f32,
    pub escuchar: bool,
}

impl AntiPop {
    pub fn new(sr: f32) -> Self {
        Self { linea: vec![[0.0; 2]; (ANTICIPACION * sr) as usize + 1], ..Default::default() }
    }

    /// Procesa el bloque; devuelve la reducción máxima de plosivas y de consonantes (dB).
    pub fn procesar(&mut self, buf: &mut [[f32; 2]], a: &Ajustes, sr: f32) -> (f32, f32) {
        if self.ultimos != (a.frec_p, a.frec_t, sr) {
            self.ultimos = (a.frec_p, a.frec_t, sr);
            // Detección de golpes más abajo que el corte (lo que es aire, no la voz).
            let det = biquad(4, a.frec_p * 0.7, 0.0, 0.707, sr);
            self.det_graves.iter_mut().for_each(|f| f.poner(det));
            self.det_agudos.poner(biquad(3, a.frec_t, 0.0, 0.707, sr));
            self.det_resto.poner(biquad(3, a.frec_p * 2.0, 0.0, 0.707, sr));
            // Paso alto de 4.º orden: el aire (20–80 Hz) cae fuerte y la voz casi no se toca.
            self.sin_graves.iter_mut().for_each(|f| f.poner(biquad(3, a.frec_p, 0.0, 0.707, sr)));
            self.sin_agudos.poner(biquad(4, a.frec_t, 0.0, 0.707, sr));
        }
        let s = a.sensibilidad.clamp(0.0, 1.0);
        // Cuánto deben superar los graves al resto, y el ataque de agudos a su promedio (dB).
        // En una voz normal lo que hay bajo ~85 Hz queda muy por debajo del resto (−15 dB o menos); un
        // golpe de aire lo iguala o lo supera.
        let (umbral_p, umbral_t) = (-2.0 - 10.0 * s, 10.0 - 8.0 * s);
        let (max_p, max_t) = (a.plosivas.clamp(0.0, 1.0) * 18.0, a.consonantes.clamp(0.0, 1.0) * 10.0);
        let (env_p, env_r) = ((coef(0.0003, sr), coef(0.06, sr)), (coef(0.002, sr), coef(0.06, sr)));
        // Agudos en potencia: rápida (5 ms) contra lenta (60 ms); la voz sostenida no dispara.
        let (rapida, lenta) = ((coef(0.005, sr), coef(0.005, sr)), (coef(0.06, sr), coef(0.06, sr)));
        let (suave_p, suave_t) = ((coef(0.0002, sr), coef(0.05, sr)), (coef(0.0003, sr), coef(0.03, sr)));
        let techo = 10f32.powf(a.techo_db.min(0.0) / 20.0);
        let rodilla = techo * 0.9;
        let n = self.linea.len();
        let mut picos = (0f32, 0f32);
        for x in buf.iter_mut() {
            // Detección sobre la señal que entra.
            let (mut g, mut r, mut h) = (0f32, 0f32, 0f32);
            for (ch, &v) in x.iter().enumerate() {
                let medio = self.det_graves[0].paso(v, ch);
                let lo = self.det_graves[1].paso(medio, ch);
                g = g.max(lo.abs());
                r = r.max(self.det_resto.paso(v, ch).abs());
                let a = self.det_agudos.paso(v, ch);
                h = h.max(a * a);
            }
            seguir(&mut self.env_graves, g, env_p);
            seguir(&mut self.env_resto, r, env_r);
            seguir(&mut self.env_rapida, h, rapida);
            seguir(&mut self.env_lenta, h, lenta);
            let lg = db(self.env_graves);
            let objetivo_p = if lg > -45.0 { ((lg - db(self.env_resto) - umbral_p) * 2.0).clamp(0.0, max_p) } else { 0.0 };
            let (fr, le) = (db(self.env_rapida) / 2.0, db(self.env_lenta) / 2.0);
            let objetivo_t = if fr > -50.0 { ((fr - le - umbral_t) * 0.8).clamp(0.0, max_t) } else { 0.0 };
            seguir(&mut self.red_p, objetivo_p, suave_p);
            seguir(&mut self.red_t, objetivo_t, suave_t);
            picos = (picos.0.max(self.red_p), picos.1.max(self.red_t));

            // La señal sale con la anticipación: la reducción llega antes que el golpe.
            let d = std::mem::replace(&mut self.linea[self.idx], *x);
            self.idx = (self.idx + 1) % n;
            let (kp, kt) = (1.0 - 10f32.powf(-self.red_p / 20.0), 1.0 - 10f32.powf(-self.red_t / 20.0));
            for ch in 0..2 {
                let medio = self.sin_graves[0].paso(d[ch], ch);
                let sin_p = self.sin_graves[1].paso(medio, ch);
                let y = d[ch] + (sin_p - d[ch]) * kp;
                let sin_t = self.sin_agudos.paso(y, ch);
                let y = y + (sin_t - y) * kt;
                let y = if a.escuchar { d[ch] - y } else { y };
                // Techo suave: lineal hasta la rodilla, luego se curva hacia el techo sin pasarlo.
                let m = y.abs();
                x[ch] = if m > rodilla { y.signum() * (rodilla + (techo - rodilla) * ((m - rodilla) / (techo - rodilla)).tanh()) } else { y };
            }
        }
        (picos.0, picos.1)
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use std::f32::consts::TAU;

    const SR: f32 = 48_000.0;

    fn ajustes() -> Ajustes {
        Ajustes { plosivas: 0.6, frec_p: 120.0, consonantes: 0.4, frec_t: 4000.0, sensibilidad: 0.5, techo_db: -1.0, escuchar: false }
    }

    fn pico(b: &[[f32; 2]]) -> f32 {
        b.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()))
    }

    /// «Voz»: armónicos de 200 Hz a 3,8 kHz a nivel moderado.
    fn voz(n: usize) -> Vec<[f32; 2]> {
        (0..n).map(|i| [(1..20).map(|k| (TAU * 200.0 * k as f32 * i as f32 / SR).sin() * 0.3 / k as f32).sum::<f32>(); 2]).collect()
    }

    #[test]
    fn voz_normal_pasa_igual() {
        let mut ap = AntiPop::new(SR);
        let entrada = voz(48_000);
        let mut b = entrada.clone();
        let mut red = (0f32, 0f32);
        for (k, c) in b.chunks_mut(256).enumerate() {
            let r = ap.procesar(c, &ajustes(), SR);
            if k > 40 {
                red = (red.0.max(r.0), red.1.max(r.1));
            }
        }
        assert!(red.0 < 0.5 && red.1 < 0.5, "reducción en voz normal: {red:?}");
        // Salida = entrada retrasada 3 ms, sin reducción (salvo el arranque).
        let n = ap.linea.len();
        let dif = b[24_000..].iter().zip(&entrada[24_000 - n..]).map(|(a, b)| (a[0] - b[0]).abs()).fold(0f32, f32::max);
        assert!(dif < 0.02, "la voz normal no debe cambiar: {dif}");
    }

    #[test]
    fn baja_el_golpe_de_una_p() {
        // Voz con un golpe de aire: 40 Hz fuerte que se apaga en ~60 ms.
        let limpia = voz(48_000);
        let golpe: Vec<f32> = (0..2880).map(|i| (TAU * 40.0 * i as f32 / SR).sin() * 0.6 * (-(i as f32) / 1500.0).exp()).collect();
        let mut b = limpia.clone();
        for (s, p) in b[20_000..].iter_mut().zip(&golpe) {
            *s = [s[0] + p, s[1] + p];
        }
        let mut ap = AntiPop::new(SR);
        let mut red = 0f32;
        b.chunks_mut(256).for_each(|c| red = red.max(ap.procesar(c, &ajustes(), SR).0));
        assert!(red > 6.0, "debió reducir la plosiva: {red} dB");
        // Energía bajo 80 Hz (el aire del golpe) antes y después, en la ventana del golpe.
        let graves = |v: &[[f32; 2]]| {
            let mut f = [Filtro::default(); 2];
            f.iter_mut().for_each(|f| f.poner(biquad(4, 80.0, 0.0, 0.707, SR)));
            let [a, b] = &mut f;
            let y: Vec<f32> = v.iter().map(|s| b.paso(a.paso(s[0], 0), 0)).collect();
            (y[20_000..23_000].iter().map(|x| x * x).sum::<f32>() / 3000.0).sqrt()
        };
        let mut con_golpe = limpia.clone();
        for (s, p) in con_golpe[20_000..].iter_mut().zip(&golpe) {
            *s = [s[0] + p, s[1] + p];
        }
        let (antes, queda) = (graves(&con_golpe), graves(&b));
        assert!(queda < antes * 0.5, "queda {queda} de {antes} (reducción {red} dB)");
    }

    #[test]
    fn baja_el_chasquido_de_una_t() {
        // Voz con una «t»: ruido agudo (6–9 kHz) de 15 ms.
        let mut b = voz(48_000);
        let mut semilla = 1u32;
        for (i, s) in b[20_000..20_720].iter_mut().enumerate() {
            semilla = semilla.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let ruido = (semilla >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0;
            let t = ((TAU * 7500.0 * i as f32 / SR).sin() * 0.5 + ruido * 0.2) * (-(i as f32) / 300.0).exp();
            *s = [s[0] + t, s[1] + t];
        }
        let mut ap = AntiPop::new(SR);
        let mut red = (0f32, 0f32);
        for (k, c) in b.chunks_mut(256).enumerate() {
            let r = ap.procesar(c, &ajustes(), SR);
            if k > 40 {
                red = (red.0.max(r.0), red.1.max(r.1));
            }
        }
        assert!(red.1 > 2.5, "debió bajar la t: {red:?}");
        assert!(red.0 < 0.5, "una t no es una plosiva: {red:?}");
    }

    #[test]
    fn nunca_pasa_del_techo() {
        let mut b: Vec<[f32; 2]> = (0..9600).map(|i| [(TAU * 300.0 * i as f32 / SR).sin() * 1.6; 2]).collect();
        let mut ap = AntiPop::new(SR);
        b.chunks_mut(256).for_each(|c| _ = ap.procesar(c, &ajustes(), SR));
        assert!(pico(&b) <= 10f32.powf(-1.0 / 20.0) + 1e-4, "pico {}", pico(&b));
    }
}
