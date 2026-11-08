//! QUANTUM Keys: teclados clásicos para pistas MIDI, sin muestras.
//! - **Piano eléctrico** (tipo Rhodes): púa modelada con FM (portadora y moduladora 1:1 cuyo índice cae
//!   rápido, «ladrido» con la velocidad), campana aguda de la púa, pastilla asimétrica y trémolo estéreo.
//! - **Wurli**: lengüeta con más armónicos impares, caída corta y trémolo de amplitud.
//! - **Órgano** de ruedas fónicas: 9 drawbars (16' a 1'), percusión de 2.º o 3.er armónico (disparo
//!   único, como el original), clic de tecla y Leslie con bocina y tambor que aceleran y frenan.
use crate::MidiEvent;
use std::f32::consts::TAU;

/// (nombre, mínimo, máximo, valor por defecto), en el orden en que se guardan tras los del Synth.
pub const KEYS_PARAMS: [(&str, f32, f32, f32); 20] = [
    ("Brillo", 0.0, 1.0, 0.5),
    ("Campana", 0.0, 1.0, 0.4),
    ("Decaimiento s", 0.5, 12.0, 4.0),
    ("Trémolo", 0.0, 1.0, 0.25),
    ("Vel. trémolo Hz", 0.5, 10.0, 4.5),
    ("Modelo", 0.0, 2.0, 0.0),
    ("Saturación", 0.0, 1.0, 0.15),
    ("Volumen", -24.0, 6.0, -6.0),
    ("16'", 0.0, 8.0, 8.0),
    ("5⅓'", 0.0, 8.0, 8.0),
    ("8'", 0.0, 8.0, 8.0),
    ("4'", 0.0, 8.0, 0.0),
    ("2⅔'", 0.0, 8.0, 0.0),
    ("2'", 0.0, 8.0, 0.0),
    ("1⅗'", 0.0, 8.0, 0.0),
    ("1⅓'", 0.0, 8.0, 0.0),
    ("1'", 0.0, 8.0, 0.0),
    ("Percusión", 0.0, 2.0, 0.0),
    ("Clic", 0.0, 1.0, 0.3),
    ("Leslie", 0.0, 2.0, 1.0),
];

/// Índices dentro de `KEYS_PARAMS`.
const BRILLO: usize = 0;
const CAMPANA: usize = 1;
const DECAIMIENTO: usize = 2;
const TREMOLO: usize = 3;
const VEL_TREMOLO: usize = 4;
pub const MODELO: usize = 5;
const SATURACION: usize = 6;
const VOLUMEN: usize = 7;
const DRAWBARS: usize = 8;
const PERCUSION: usize = 17;
const CLIC: usize = 18;
const LESLIE: usize = 19;

/// Parámetros que se ven según el modelo (contiguos): los del piano (eléctrico o Wurli), con el
/// modelo, la saturación y el volumen al final, y los del órgano, que empiezan por esos mismos tres.
pub const VISTA_PIANO: std::ops::Range<usize> = 0..8;
pub const VISTA_ORGANO: std::ops::Range<usize> = 5..20;

/// Múltiplos de la fundamental de cada drawbar (16', 5⅓', 8', 4', 2⅔', 2', 1⅗', 1⅓', 1').
const RATIOS: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
/// Las ruedas más agudas del órgano real se repiten una octava abajo («foldback»).
const TECHO_RUEDAS: f32 = 5900.0;
const VOCES: usize = 16;
const TABLA: usize = 2048;
const RETARDO: usize = 1024;

#[derive(Clone, Copy, Default)]
struct Voz {
    key: u8,
    /// Curva de velocidad del golpe del piano (`vel^1.4`).
    golpe: f32,
    /// 0 libre, 1 sonando, 2 soltando.
    etapa: u8,
    edad: u32,
    /// Muestras desde que se tocó.
    n: u32,
    amp: f32,
    /// Índice de modulación (piano) que cae tras el golpe; campana de la púa.
    indice: f32,
    campana: f32,
    fase: [f32; 9],
    fase_mod: f32,
    fase_campana: f32,
    clic: f32,
    gain: f32,
    pedal: bool,
}

pub struct Keys {
    voces: [Voz; VOCES],
    reloj: u32,
    sonando: bool,
    siguiente: u64,
    cc_gain: Option<u8>,
    expresion: crate::expresion::Expresion,
    seno: Box<[f32]>,
    /// Percusión del órgano: envolvente compartida (disparo único) y si hay teclas apretadas.
    percusion: f32,
    lfo: f32,
    ruido: u32,
    ruido_lp: f32,
    /// Bloqueo de continua de la pastilla, por canal.
    dc: [[f32; 2]; 2],
    leslie: Leslie,
}

struct Leslie {
    bocina: f32,
    tambor: f32,
    vel_bocina: f32,
    vel_tambor: f32,
    cruce: f32,
    linea: Box<[f32]>,
    escritura: usize,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            voces: [Voz::default(); VOCES],
            reloj: 0,
            sonando: false,
            siguiente: 0,
            cc_gain: None,
            expresion: Default::default(),
            seno: (0..=TABLA).map(|i| (TAU * i as f32 / TABLA as f32).sin()).collect(),
            percusion: 0.0,
            lfo: 0.0,
            ruido: 0x1234_5678,
            ruido_lp: 0.0,
            dc: [[0.0; 2]; 2],
            leslie: Leslie { bocina: 0.0, tambor: 0.0, vel_bocina: 0.8, vel_tambor: 0.7, cruce: 0.0, linea: vec![0.0; RETARDO].into_boxed_slice(), escritura: 0 },
        }
    }
}

/// Parte fraccionaria en [0, 1) sin llamar a la biblioteca matemática (`fract` y `rem_euclid` son
/// llamadas lentas en x86_64 genérico y aquí se usan cientos de veces por muestra).
#[inline]
fn envolver(x: f32) -> f32 {
    let f = x - (x as i32) as f32;
    if f < 0.0 { f + 1.0 } else { f }
}

/// Margen antes de la saturación: una nota fuerte entra cerca de 0,2 y un acorde de 5 notas a fondo
/// cerca de 1, donde `tanh` apenas empieza a curvarse. Así la saturación solo se oye fuerte cuando se
/// sube la perilla «Saturación» (y no aplasta los acordes, como pasaba antes).
const HOLGURA_PIANO: f32 = 0.2;
const HOLGURA_ORGANO: f32 = 0.3;
/// Ganancia de salida: una nota normal suena a unos −12 dBFS con el volumen en −6 dB.
const SALIDA: f32 = 2.5;
/// El órgano no depende de la velocidad y sus acordes suman más: sale más bajo.
const SALIDA_ORGANO: f32 = 1.1;

/// Saturación suave: con `drive` 1 es casi lineal en el rango normal; más `drive`, más armónicos.
/// Se divide por √drive: subir la perilla da más armónicos y algo más de cuerpo, sin saltos de volumen.
#[inline]
fn saturar(x: f32, drive: f32) -> f32 {
    (x * drive).tanh() / drive.sqrt()
}

#[inline]
fn cortar(x: f32) -> f32 {
    if x.abs() < 1e-6 { 0.0 } else { x }
}

fn frecuencia(key: u8) -> f32 {
    440.0 * 2f32.powf((key as f32 - 69.0) / 12.0)
}

/// Ganancia de un drawbar (posiciones enteras, como el original): cada paso abajo de 8 son −3 dB; 0 lo apaga.
fn drawbar(v: f32) -> f32 {
    let v = v.round().clamp(0.0, 8.0);
    if v == 0.0 { 0.0 } else { 10f32.powf(-(8.0 - v) * 3.0 / 20.0) }
}

impl Keys {
    #[inline]
    fn sen(&self, fase: f32) -> f32 {
        let x = envolver(fase) * TABLA as f32;
        let i = x as usize;
        let f = x - i as f32;
        let (a, b) = (self.seno[i.min(TABLA)], self.seno[(i + 1).min(TABLA)]);
        a + (b - a) * f
    }

    fn ruido(&mut self) -> f32 {
        self.ruido ^= self.ruido << 13;
        self.ruido ^= self.ruido >> 17;
        self.ruido ^= self.ruido << 5;
        self.ruido as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    pub fn soltar_todo(&mut self) {
        self.voces.iter_mut().filter(|v| v.etapa == 1).for_each(|v| (v.pedal, v.etapa) = (false, 2));
    }

    fn evento(&mut self, e: MidiEvent, p: &[f32]) {
        self.reloj += 1;
        use crate::expresion::Cambio;
        match self.expresion.evento(e) {
            Cambio::Nada => {}
            Cambio::Hecho => return,
            Cambio::PedalSuelto => {
                self.voces.iter_mut().filter(|v| v.pedal).for_each(|v| (v.pedal, v.etapa) = (false, 2));
                return;
            }
        }
        let pedal = self.expresion.pedal;
        match (e[0] & 0xF0, e[2]) {
            (0x90, vel) if vel > 0 => {
                // Percusión de disparo único: solo suena si no había teclas apretadas (como el original).
                if !self.voces.iter().any(|v| v.etapa == 1 && !v.pedal) {
                    self.percusion = 1.0;
                }
                let i = (0..VOCES)
                    .find(|&i| self.voces[i].key == e[1] && self.voces[i].etapa > 0)
                    .or_else(|| (0..VOCES).find(|&i| self.voces[i].etapa == 0))
                    .unwrap_or_else(|| (0..VOCES).min_by_key(|&i| self.voces[i].edad).unwrap_or(0));
                let vel = vel as f32 / 127.0;
                let piano = p[MODELO].round() < 2.0;
                let v = &mut self.voces[i];
                let reusar = v.etapa > 0 && v.key == e[1];
                *v = Voz {
                    key: e[1],
                    golpe: vel.powf(1.4),
                    etapa: 1,
                    edad: self.reloj,
                    n: 0,
                    amp: if piano { 1.0 } else if reusar { v.amp } else { 0.0 },
                    indice: 1.0,
                    campana: 1.0,
                    // El órgano conserva las fases (las ruedas giran siempre); el piano empieza en cero.
                    fase: if piano { [0.0; 9] } else { v.fase },
                    fase_mod: 0.0,
                    fase_campana: 0.0,
                    clic: 1.0,
                    gain: self.cc_gain.take().map_or(1.0, crate::ganancia_nota),
                    pedal: false,
                };
            }
            (0xB0, _) if e[1] == crate::CC_GANANCIA => self.cc_gain = Some(e[2]),
            (0x80 | 0x90, _) => self.voces.iter_mut().filter(|v| v.key == e[1] && v.etapa == 1 && !v.pedal).for_each(|v| if pedal { v.pedal = true } else { v.etapa = 2 }),
            (0xB0, _) if e[1] >= 120 => self.soltar_todo(),
            _ => {}
        }
    }

    /// Suma al buffer el instrumento, con los eventos en su posición dentro del bloque.
    /// `p`: parámetros en el orden de `KEYS_PARAMS`.
    pub fn render(&mut self, out: &mut [[f32; 2]], events: &[(usize, MidiEvent)], sr: f32, p: &[f32], pos: u64, playing: bool) {
        if (self.sonando && !playing) || (playing && pos != self.siguiente) {
            self.soltar_todo();
        }
        (self.sonando, self.siguiente) = (playing, pos + out.len() as u64);
        let modelo = p[MODELO].round() as u8;
        let vol = 10f32.powf(p[VOLUMEN] / 20.0);
        let caida = |seg: f32| (-6.9 / (seg.max(0.005) * sr)).exp();
        let mut ev = events.iter().peekable();
        if modelo >= 2 {
            self.render_organo(out, &mut ev, sr, p, vol, caida);
        } else {
            self.render_piano(out, &mut ev, sr, p, vol, caida, modelo == 1);
        }
        ev.for_each(|&(_, e)| self.evento(e, p));
    }

    #[allow(clippy::too_many_arguments)]
    fn render_piano<'a>(&mut self, out: &mut [[f32; 2]], ev: &mut std::iter::Peekable<std::slice::Iter<'a, (usize, MidiEvent)>>, sr: f32, p: &[f32], vol: f32, caida: impl Fn(f32) -> f32, wurli: bool) {
        let brillo = p[BRILLO];
        let (soltar, cae_indice, cae_campana) = (caida(if wurli { 0.07 } else { 0.12 }), caida(if wurli { 0.5 } else { 0.35 }), caida(0.25));
        let (prof, vel_lfo) = (p[TREMOLO], p[VEL_TREMOLO]);
        let drive = 1.0 + p[SATURACION] * 8.0;
        let ataque = (0.0015 * sr).max(1.0);
        // Caída de cada tecla (las agudas se apagan antes, como en el instrumento real) y frecuencia
        // base: se calculan una vez por bloque.
        let caidas: [f32; 128] = std::array::from_fn(|k| caida(p[DECAIMIENTO] * 2f32.powf(-(k as f32 - 60.0) / 24.0) * if wurli { 0.6 } else { 1.0 }));
        for (i, o) in out.iter_mut().enumerate() {
            while let Some(&(_, e)) = ev.next_if(|e| e.0 <= i) {
                self.evento(e, p);
            }
            let tono = self.expresion.factor(sr);
            let mut s = 0.0;
            for k in 0..VOCES {
                let v = self.voces[k];
                if v.etapa == 0 {
                    continue;
                }
                let dec = if v.etapa == 2 { soltar } else { caidas[v.key as usize & 127] };
                let f = frecuencia(v.key) * tono;
                let dt = f / sr;
                let golpe = v.golpe;
                let indice = v.indice * golpe * if wurli { 0.9 + 2.0 * brillo } else { 0.4 + 2.6 * brillo };
                let modu = self.sen(v.fase_mod);
                let mut x = self.sen(v.fase[0] + indice * modu / TAU);
                if wurli {
                    // Lengüeta: armónicos impares (recorte suave) y un poco de pares.
                    x = (x * (1.2 + 1.2 * brillo * golpe)).tanh() + 0.12 * x * x;
                } else {
                    // Pastilla del Rhodes: asimétrica, más con golpes fuertes.
                    x += (0.15 + 0.35 * brillo) * golpe * x * x;
                }
                let campana = p[CAMPANA] * v.campana * golpe * self.sen(v.fase_campana) * if wurli { 0.35 } else { 0.6 };
                let env = v.amp * (v.n as f32 / ataque).min(1.0);
                s += (x + campana) * env * (0.08 + 0.92 * golpe) * v.gain;
                let v = &mut self.voces[k];
                v.fase[0] = envolver(v.fase[0] + dt);
                v.fase_mod = envolver(v.fase_mod + dt);
                v.fase_campana = envolver(v.fase_campana + dt * if wurli { 4.0 } else { 7.1 });
                // Lo que cae hacia cero se corta a tiempo: los números «subnormales» hacen muy lenta la CPU.
                v.indice = cortar(v.indice * cae_indice);
                v.campana = cortar(v.campana * cae_campana * if f * 7.1 > sr * 0.45 { 0.0 } else { 1.0 });
                v.amp *= dec;
                v.n = v.n.saturating_add(1);
                if v.amp < 1e-4 {
                    v.etapa = 0;
                }
            }
            s = saturar(s * HOLGURA_PIANO, drive);
            self.lfo = envolver(self.lfo + vel_lfo / sr);
            let l = self.sen(self.lfo);
            let par = if wurli {
                let g = 1.0 - prof * 0.5 * (1.0 + l);
                [s * g, s * g]
            } else {
                // Trémolo estéreo del Suitcase: el sonido va de un lado al otro.
                [s * (1.0 - prof * 0.5 * (1.0 + l)), s * (1.0 - prof * 0.5 * (1.0 - l))]
            };
            for c in 0..2 {
                // Bloqueo de continua (la pastilla asimétrica la genera).
                let [x1, y1] = self.dc[c];
                let y = par[c] - x1 + 0.9995 * y1;
                self.dc[c] = [par[c], y];
                o[c] += y * vol * SALIDA;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_organo<'a>(&mut self, out: &mut [[f32; 2]], ev: &mut std::iter::Peekable<std::slice::Iter<'a, (usize, MidiEvent)>>, sr: f32, p: &[f32], vol: f32, caida: impl Fn(f32) -> f32) {
        let barras: [f32; 9] = std::array::from_fn(|i| drawbar(p[DRAWBARS + i]));
        let normal = 1.0 / barras.iter().sum::<f32>().max(1.0);
        let perc = p[PERCUSION].round() as u8;
        let (cae_perc, cae_clic, soltar) = (caida(0.45), caida(0.004), caida(0.012));
        let subida = 1.0 / ((0.0005 + (1.0 - p[CLIC]) * 0.004) * sr);
        let clic = p[CLIC];
        let drive = 1.0 + p[SATURACION] * 8.0;
        let leslie = p[LESLIE].round() as u8;
        // Avance de fase de cada rueda por tecla (con el «foldback» de las más agudas).
        let avance = |key: u8| -> [f32; 9] {
            std::array::from_fn(|h| {
                let mut fh = frecuencia(key) * RATIOS[h];
                while fh > TECHO_RUEDAS {
                    fh *= 0.5;
                }
                fh / sr
            })
        };
        let mut avances = [[0f32; 9]; VOCES];
        let mut teclas = [255u8; VOCES];
        for (i, o) in out.iter_mut().enumerate() {
            while let Some(&(_, e)) = ev.next_if(|e| e.0 <= i) {
                self.evento(e, p);
            }
            let tono = self.expresion.factor(sr);
            let mut s = 0.0;
            let mut chasquido = 0.0;
            for k in 0..VOCES {
                let v = self.voces[k];
                if v.etapa == 0 {
                    continue;
                }
                if teclas[k] != v.key {
                    (teclas[k], avances[k]) = (v.key, avance(v.key));
                }
                let mut x = 0.0;
                let mut fases = v.fase;
                for (h, fase) in fases.iter_mut().enumerate() {
                    let mut g = barras[h];
                    // Percusión: suma el 4' (2.º armónico) o el 2⅔' (3.º) con su caída rápida.
                    if (perc == 1 && h == 3) || (perc == 2 && h == 4) {
                        g += self.percusion * 0.9;
                    }
                    if g > 0.0 {
                        x += self.sen(*fase) * g;
                    }
                    *fase = envolver(*fase + avances[k][h] * tono);
                }
                s += x * v.amp * normal * v.gain;
                chasquido += v.clic;
                let v = &mut self.voces[k];
                v.fase = fases;
                v.clic = cortar(v.clic * cae_clic);
                if v.etapa == 1 {
                    v.amp = (v.amp + subida).min(1.0);
                } else {
                    v.amp *= soltar;
                    if v.amp < 1e-4 {
                        v.etapa = 0;
                    }
                }
            }
            if perc > 0 {
                self.percusion = cortar(self.percusion * cae_perc);
            }
            // Clic de tecla: ruido filtrado que dura unos milisegundos.
            if clic > 0.0 && chasquido > 1e-3 {
                let n = self.ruido();
                self.ruido_lp += (n - self.ruido_lp) * 0.35;
                s += self.ruido_lp * chasquido.min(3.0) * clic * 0.12;
            }
            s = saturar(s * HOLGURA_ORGANO, drive);
            let [l, r] = self.leslie(s, sr, leslie);
            o[0] += l * vol * SALIDA_ORGANO;
            o[1] += r * vol * SALIDA_ORGANO;
        }
    }

    /// Altavoz rotativo: graves al tambor (solo amplitud) y agudos a la bocina (amplitud y Doppler).
    fn leslie(&mut self, x: f32, sr: f32, modo: u8) -> [f32; 2] {
        if modo == 0 {
            return [x, x];
        }
        let (meta_b, meta_t) = if modo == 2 { (6.7, 5.7) } else { (0.83, 0.67) };
        let ls = &mut self.leslie;
        // La bocina acelera en ~1 s y el tambor, más pesado, en ~3 s.
        ls.vel_bocina += (meta_b - ls.vel_bocina) * (1.0 / (0.8 * sr));
        ls.vel_tambor += (meta_t - ls.vel_tambor) * (1.0 / (3.0 * sr));
        ls.bocina = (ls.bocina + ls.vel_bocina / sr).fract();
        ls.tambor = (ls.tambor + ls.vel_tambor / sr).fract();
        ls.cruce += (x - ls.cruce) * (1.0 - (-TAU * 800.0 / sr).exp());
        let (graves, agudos) = (ls.cruce, x - ls.cruce);
        ls.linea[ls.escritura] = agudos;
        let leer = |ls: &Leslie, ms: f32| {
            let d = (ms * 0.001 * sr).clamp(1.0, (RETARDO - 2) as f32);
            let pos = ls.escritura as f32 + RETARDO as f32 - d;
            let i = pos as usize;
            let f = pos - i as f32;
            let (a, b) = (ls.linea[i % RETARDO], ls.linea[(i + 1) % RETARDO]);
            a + (b - a) * f
        };
        let (sb, cb) = ((TAU * ls.bocina).sin(), (TAU * ls.bocina).cos());
        let st = (TAU * ls.tambor).sin();
        // Dos micrófonos a los lados: cada uno oye la bocina acercarse cuando el otro la oye alejarse.
        let hl = leer(ls, 1.6 + 0.9 * sb) * (1.0 + 0.35 * cb);
        let hr = leer(ls, 1.6 - 0.9 * sb) * (1.0 - 0.35 * cb);
        ls.escritura = (ls.escritura + 1) % RETARDO;
        let gl = graves * (1.0 + 0.18 * st);
        let gr = graves * (1.0 - 0.18 * st);
        [gl + hl, gr + hr]
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn params(modelo: f32) -> [f32; KEYS_PARAMS.len()] {
        let mut p = KEYS_PARAMS.map(|d| d.3);
        p[MODELO] = modelo;
        p
    }

    /// Energía por bloques de 0,1 s: nota a los 0 s, soltada a los `suelta` bloques.
    fn bloques(modelo: f32, eventos: &[(usize, MidiEvent)], n: usize) -> Vec<f32> {
        let mut k = Keys::default();
        let p = params(modelo);
        let mut out = vec![[0f32; 2]; 4800 * n];
        // En bloques de 256 como el motor, con los eventos en su bloque.
        for (b, trozo) in out.chunks_mut(256).enumerate() {
            let ini = b * 256;
            let ev: Vec<(usize, MidiEvent)> = eventos.iter().filter(|e| e.0 >= ini && e.0 < ini + trozo.len()).map(|e| (e.0 - ini, e.1)).collect();
            k.render(trozo, &ev, 48_000.0, &p, ini as u64, true);
        }
        assert!(out.iter().all(|s| s[0].is_finite() && s[1].is_finite() && s[0].abs() < 4.0), "modelo {modelo}");
        out.chunks(4800).map(|b| b.iter().map(|s| s[0] * s[0] + s[1] * s[1]).sum()).collect()
    }

    #[test]
    fn los_tres_modelos_suenan_y_se_apagan() {
        for modelo in [0.0, 1.0, 2.0] {
            let e = bloques(modelo, &[(0, [0x90, 60, 100]), (4800 * 3, [0x80, 60, 0])], 10);
            assert!(e[1] > 1.0, "modelo {modelo} no suena: {e:?}");
            assert!(e[9] < e[1] * 1e-3, "modelo {modelo} no se apaga: {e:?}");
        }
    }

    #[test]
    fn piano_mas_fuerte_con_mas_velocidad() {
        let suave = bloques(0.0, &[(0, [0x90, 60, 30])], 2);
        let fuerte = bloques(0.0, &[(0, [0x90, 60, 120])], 2);
        assert!(fuerte[0] > suave[0] * 3.0, "{} {}", fuerte[0], suave[0]);
    }

    #[test]
    fn pedal_sostiene_el_piano() {
        let sin = bloques(0.0, &[(0, [0x90, 60, 100]), (4800, [0x80, 60, 0])], 6);
        let con = bloques(0.0, &[(0, [0xB0, 64, 127]), (0, [0x90, 60, 100]), (4800, [0x80, 60, 0])], 6);
        assert!(con[4] > sin[4] * 50.0, "{} {}", con[4], sin[4]);
    }

    #[test]
    fn acorde_de_organo_sin_saturar_ni_cortes() {
        let ev: Vec<(usize, MidiEvent)> = [48, 55, 60, 64, 67, 72, 76, 79].iter().map(|&k| (0, [0x90, k, 127])).collect();
        let e = bloques(2.0, &ev, 5);
        // El órgano sostiene parejo mientras se mantienen las teclas.
        assert!(e[4] > e[1] * 0.5 && e[4] < e[1] * 2.0, "{e:?}");
    }

    /// Pico de un render de 1 s con las notas dadas.
    fn pico(modelo: f32, notas: &[u8], vel: u8) -> f32 {
        let mut k = Keys::default();
        let p = params(modelo);
        let mut out = vec![[0f32; 2]; 48_000];
        let ev: Vec<(usize, MidiEvent)> = notas.iter().map(|&n| (0, [0x90, n, vel])).collect();
        k.render(&mut out, &ev, 48_000.0, &p, 0, true);
        out.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()))
    }

    /// Los acordes no se aplastan contra la saturación interna (antes una nota sola ya llegaba al
    /// límite y un acorde sonaba casi igual de fuerte pero distorsionado) y nada pasa de 0 dBFS.
    #[test]
    fn acordes_con_margen_sin_saturar() {
        for modelo in [0.0, 1.0, 2.0] {
            let (nota, acorde) = (pico(modelo, &[60], 100), pico(modelo, &[48, 55, 60, 64, 67], 127));
            assert!(acorde < 1.0, "modelo {modelo}: el acorde pasa de 0 dBFS ({acorde})");
            assert!(acorde > nota * 2.0, "modelo {modelo}: acorde aplastado ({nota} → {acorde})");
        }
    }

    #[test]
    fn drawbars_en_cero_no_suenan() {
        let mut k = Keys::default();
        let mut p = params(2.0);
        (DRAWBARS..DRAWBARS + 9).for_each(|i| p[i] = 0.0);
        p[CLIC] = 0.0;
        let mut out = vec![[0f32; 2]; 4800];
        k.render(&mut out, &[(0, [0x90, 60, 100])], 48_000.0, &p, 0, true);
        assert!(out.iter().all(|s| s[0].abs() < 1e-6));
    }
}


