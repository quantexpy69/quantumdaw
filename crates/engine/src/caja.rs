//! Caja de ritmos sintetizada (modo kit del QUANTUM Synth): cada nota General MIDI dispara un sonido
//! de batería generado al momento (bombo, caja, palmas, hi-hats, toms, platillos, cencerro y aro),
//! sin muestras que descargar.
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq)]
enum Sonido {
    Bombo,
    Caja,
    Aro,
    Palmas,
    HatCerrado,
    HatAbierto,
    Tom(f32),
    Crash,
    Ride,
    Cencerro,
}

/// Qué sonido corresponde a cada nota (mapa General MIDI de batería).
fn sonido(nota: u8) -> Sonido {
    match nota {
        35 | 36 => Sonido::Bombo,
        37 => Sonido::Aro,
        38 | 40 => Sonido::Caja,
        39 => Sonido::Palmas,
        42 | 44 => Sonido::HatCerrado,
        46 => Sonido::HatAbierto,
        41 | 43 | 45 | 47 | 48 | 50 => Sonido::Tom(70.0 + (nota - 41) as f32 * 14.0),
        49 | 52 | 55 | 57 => Sonido::Crash,
        51 | 53 | 59 => Sonido::Ride,
        56 => Sonido::Cencerro,
        _ => Sonido::HatCerrado,
    }
}

#[derive(Clone, Copy)]
struct Golpe {
    sonido: Sonido,
    t: f32,
    vel: f32,
    fase: [f32; 6],
    hp: [f32; 2],
}

#[derive(Default)]
pub struct Caja {
    golpes: Vec<Golpe>,
    ruido: u32,
}

/// Frecuencias inarmónicas de los platillos y el cencerro.
const METAL: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

impl Caja {
    pub fn golpear(&mut self, nota: u8, vel: u8) {
        let s = sonido(nota);
        // El hi-hat cerrado corta al abierto, como en una batería real.
        if s == Sonido::HatCerrado {
            self.golpes.retain(|g| g.sonido != Sonido::HatAbierto);
        }
        if self.golpes.len() >= 24 {
            self.golpes.remove(0);
        }
        self.golpes.push(Golpe { sonido: s, t: 0.0, vel: (vel as f32 / 127.0).powf(1.4), fase: [0.0; 6], hp: [0.0; 2] });
    }

    pub fn callar(&mut self) {
        self.golpes.clear();
    }

    fn ruido(&mut self) -> f32 {
        self.ruido ^= self.ruido << 13;
        self.ruido ^= self.ruido >> 17;
        self.ruido ^= self.ruido << 5;
        if self.ruido == 0 {
            self.ruido = 0x1234_5678;
        }
        self.ruido as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    /// Una muestra de todos los golpes que suenan (mono).
    pub fn muestra(&mut self, sr: f32) -> f32 {
        let dt = 1.0 / sr;
        let mut suma = 0.0;
        for k in 0..self.golpes.len() {
            let n = self.ruido();
            let g = &mut self.golpes[k];
            let t = g.t;
            let env = |d: f32| (-t / d).exp();
            // Paso alto de un polo (dos en cascada) para el ruido de hi-hats y platillos.
            let agudo = |x: f32, g: &mut Golpe| {
                let a = 0.86;
                let y0 = a * (g.hp[0] + x - g.hp[1]);
                g.hp = [y0, x];
                y0
            };
            let v = match g.sonido {
                Sonido::Bombo => {
                    let f = 45.0 + 110.0 * (-t / 0.035).exp();
                    g.fase[0] = (g.fase[0] + f * dt).fract();
                    (TAU * g.fase[0]).sin() * env(0.33) + n * 0.25 * env(0.004)
                }
                Sonido::Caja => {
                    g.fase[0] = (g.fase[0] + 190.0 * dt).fract();
                    (TAU * g.fase[0]).sin() * 0.55 * env(0.09) + agudo(n, g) * 0.75 * env(0.16)
                }
                Sonido::Aro => {
                    g.fase[0] = (g.fase[0] + 1700.0 * dt).fract();
                    (TAU * g.fase[0]).sin() * env(0.012) + n * 0.3 * env(0.008)
                }
                Sonido::Palmas => {
                    // Tres golpes de ruido muy juntos y una cola.
                    let rafaga = [0.0, 0.011, 0.022].iter().map(|&o| if t >= o { (-(t - o) / 0.006).exp() } else { 0.0 }).sum::<f32>();
                    agudo(n, g) * (rafaga * 0.7 + 0.5 * env(0.13))
                }
                Sonido::HatCerrado | Sonido::HatAbierto => {
                    let d = if g.sonido == Sonido::HatCerrado { 0.045 } else { 0.32 };
                    let x = agudo(n, g);
                    agudo(x, g) * 0.8 * env(d)
                }
                Sonido::Tom(f0) => {
                    let f = f0 * (1.0 + 0.6 * (-t / 0.05).exp());
                    g.fase[0] = (g.fase[0] + f * dt).fract();
                    (TAU * g.fase[0]).sin() * env(0.28)
                }
                Sonido::Crash | Sonido::Ride => {
                    let mut m = 0.0;
                    for (i, f) in METAL.iter().enumerate() {
                        g.fase[i] = (g.fase[i] + f * 3.1 * dt).fract();
                        m += if g.fase[i] < 0.5 { 1.0 } else { -1.0 };
                    }
                    let (d, mezcla) = if g.sonido == Sonido::Crash { (1.1, 0.7) } else { (0.75, 0.35) };
                    let x = agudo(n * mezcla + m / 6.0 * (1.0 - mezcla), g);
                    x * 0.6 * env(d)
                }
                Sonido::Cencerro => {
                    let mut m = 0.0;
                    for i in [4, 5] {
                        g.fase[i] = (g.fase[i] + METAL[i] * dt).fract();
                        m += if g.fase[i] < 0.5 { 0.5 } else { -0.5 };
                    }
                    m * 0.5 * env(0.12)
                }
            };
            suma += v * g.vel;
            g.t += dt;
        }
        self.golpes.retain(|g| g.t < 2.5);
        suma * 0.6
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn suenan_y_se_apagan() {
        let mut c = Caja::default();
        for nota in [36, 38, 39, 42, 46, 45, 49, 51, 56, 37] {
            c.golpear(nota, 110);
            let energia: f32 = (0..4800).map(|_| c.muestra(48_000.0).powi(2)).sum();
            assert!(energia > 1.0, "nota {nota} sin sonido");
            c.callar();
        }
        c.golpear(36, 127);
        (0..48_000 * 3).for_each(|_| _ = c.muestra(48_000.0));
        assert_eq!(c.muestra(48_000.0), 0.0, "los golpes deben terminar");
    }
}
