//! QUANTUM Synth: sintetizador polifónico sustractivo (2 osciladores, filtro SVF y ADSR) para pistas MIDI.
use crate::{AtomicF32, MidiEvent};
use std::{
    f32::consts::{PI, TAU},
    sync::Mutex,
};

/// (nombre, mínimo, máximo, valor por defecto). Onda: 0 sierra, 1 cuadrada, 2 triángulo, 3 seno.
pub const SYNTH_PARAMS: [(&str, f32, f32, f32); 10] = [
    ("Onda", 0.0, 3.0, 0.0),
    ("Desafinar", 0.0, 50.0, 8.0),
    ("Corte", 80.0, 16000.0, 2400.0),
    ("Resonancia", 0.0, 0.95, 0.25),
    ("Env. filtro", 0.0, 1.0, 0.45),
    ("Ataque", 1.0, 2000.0, 4.0),
    ("Caída", 5.0, 3000.0, 350.0),
    ("Sostenido", 0.0, 1.0, 0.6),
    ("Liberación", 5.0, 5000.0, 260.0),
    ("Volumen", -24.0, 6.0, -8.0),
];
const VOICES: usize = 16;

#[derive(Clone, Copy, Default)]
struct Voice {
    key: u8,
    vel: f32,
    /// 0 libre, 1 ataque, 2 caída/sostenido, 3 liberación.
    stage: u8,
    env: f32,
    age: u32,
    phase: [f32; 2],
    /// Estado del filtro por oscilador (ic1, ic2) y coeficiente g actual.
    svf: [[f32; 2]; 2],
    g: f32,
}

#[derive(Default)]
struct State {
    voices: [Voice; VOICES],
    next: u64,
    playing: bool,
    clock: u32,
}

pub struct Synth {
    pub params: Vec<AtomicF32>,
    state: Mutex<State>,
}

impl Default for Synth {
    fn default() -> Self {
        Self { params: SYNTH_PARAMS.iter().map(|p| AtomicF32::new(p.3)).collect(), state: Default::default() }
    }
}

fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        2.0 * t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

impl State {
    fn handle(&mut self, e: MidiEvent) {
        self.clock += 1;
        match (e[0] & 0xF0, e[2]) {
            (0x90, vel) if vel > 0 => {
                let i = (0..VOICES).find(|&i| self.voices[i].stage == 0 || self.voices[i].key == e[1]).unwrap_or_else(|| (0..VOICES).min_by_key(|&i| self.voices[i].age).unwrap_or(0));
                let v = &mut self.voices[i];
                (v.key, v.vel, v.stage, v.age) = (e[1], vel as f32 / 127.0, 1, self.clock);
            }
            (0x80 | 0x90, _) => self.voices.iter_mut().filter(|v| v.key == e[1] && matches!(v.stage, 1 | 2)).for_each(|v| v.stage = 3),
            // Controladores 120/123: silenciar todo.
            (0xB0, _) if e[1] >= 120 => self.release_all(),
            _ => {}
        }
    }

    fn release_all(&mut self) {
        self.voices.iter_mut().filter(|v| v.stage > 0).for_each(|v| v.stage = 3);
    }
}

impl Synth {
    /// Suma al buffer las voces activas, aplicando los eventos en su posición exacta dentro del bloque.
    pub fn render(&self, out: &mut [[f32; 2]], events: &[(usize, MidiEvent)], sr: f32, offline: bool, pos: u64, playing: bool) {
        let Some(mut st) = (if offline { self.state.lock().ok() } else { self.state.try_lock().ok() }) else {
            return;
        };
        let st = &mut *st;
        let p: [f32; 10] = std::array::from_fn(|i| self.params[i].get());
        // Stop, loop o salto del cursor: las notas que venían de las regiones se liberan.
        if (st.playing && !playing) || (playing && pos != st.next) {
            st.release_all();
        }
        (st.playing, st.next) = (playing, pos + out.len() as u64);
        let coef = |ms: f32| 1.0 - (-1.0 / (ms * 0.001 * sr)).exp();
        let (att, dec, rel) = (coef(p[5]), coef(p[6]), coef(p[8]));
        let (wave, det, res_k, vol) = (p[0].round() as u8, 2f32.powf(p[1] / 1200.0), 2.0 - 2.0 * p[3], 10f32.powf(p[9] / 20.0));
        let mut ev = events.iter().peekable();
        for (i, o) in out.iter_mut().enumerate() {
            while let Some(&(_, e)) = ev.next_if(|e| e.0 <= i) {
                st.handle(e);
            }
            for v in st.voices.iter_mut().filter(|v| v.stage > 0) {
                match v.stage {
                    1 => {
                        v.env += (1.2 - v.env) * att;
                        if v.env >= 1.0 {
                            (v.env, v.stage) = (1.0, 2);
                        }
                    }
                    2 => v.env += (p[7] - v.env) * dec,
                    _ => {
                        v.env -= v.env * rel;
                        if v.env < 1e-4 {
                            (v.env, v.stage) = (0.0, 0);
                            continue;
                        }
                    }
                }
                let f = 440.0 * 2f32.powf((v.key as f32 - 69.0) / 12.0);
                if i % 16 == 0 {
                    let cutoff = (p[2] * 2f32.powf(p[4] * v.env * 5.0)).min(sr * 0.45);
                    v.g = (PI * cutoff / sr).tan();
                }
                let (a1, gain) = (1.0 / (1.0 + v.g * (v.g + res_k)), v.env * v.vel * vol);
                for (osc, out) in o.iter_mut().enumerate() {
                    let dt = f * if osc == 0 { 1.0 / det } else { det } / sr;
                    let t = v.phase[osc];
                    let x = match wave {
                        0 => 2.0 * t - 1.0 - poly_blep(t, dt),
                        1 => (if t < 0.5 { 1.0 } else { -1.0 }) + poly_blep(t, dt) - poly_blep((t + 0.5).fract(), dt),
                        2 => 1.0 - 4.0 * (t - 0.5).abs(),
                        _ => (TAU * t).sin(),
                    };
                    v.phase[osc] = (t + dt).fract();
                    // Filtro paso bajo SVF (topología TPT), estable con modulación.
                    let [ic1, ic2] = v.svf[osc];
                    let v3 = x - ic2;
                    let v1 = a1 * ic1 + v.g * a1 * v3;
                    let v2 = ic2 + v.g * v1;
                    v.svf[osc] = [2.0 * v1 - ic1, 2.0 * v2 - ic2];
                    *out += v2 * gain;
                }
            }
        }
        // Eventos que llegaran con offset fuera del bloque.
        ev.for_each(|&(_, e)| st.handle(e));
    }
}
