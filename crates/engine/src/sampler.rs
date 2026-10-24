//! Sampler SFZ: reproduce instrumentos reales (guitarras, bajo, batería, cuerdas…) a partir de
//! muestras. Soporta el subconjunto de SFZ que usan las bibliotecas libres: zonas por nota y
//! velocidad, variaciones aleatorias, loops, afinación y release.
use crate::{AtomicF32, MidiEvent, decode};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};

const VOICES: usize = 32;

/// Muestra en 16 bits (la mitad de memoria que f32; las bibliotecas pesan cientos de MB).
type Pcm = Arc<Vec<[i16; 2]>>;

struct Zone {
    data: Pcm,
    lokey: u8,
    hikey: u8,
    root: f32,
    lovel: u8,
    hivel: u8,
    lorand: f32,
    hirand: f32,
    looped: Option<(usize, usize)>,
    release: f32,
    gain: f32,
    veltrack: f32,
    /// Panorama (-1 izquierda, 1 derecha).
    pan: f32,
    /// Caída hasta el silencio en segundos (0 = el sonido natural de la muestra).
    decay: f32,
    /// Suena entera aunque la nota termine antes (golpes de batería).
    one_shot: bool,
    /// Grupo de corte: una nota de un grupo apaga las voces cuyo `off_by` es ese grupo (hi-hat).
    group: u32,
    off_by: u32,
    /// Curva de velocidad (amp_velcurve_N): ganancia para cada fuerza de golpe, si la hay.
    curva: Option<Arc<[f32; 128]>>,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    zone: usize,
    key: u8,
    active: bool,
    releasing: bool,
    pos: f64,
    step: f64,
    gain: f32,
    env: f32,
    rel: f32,
    age: u32,
    /// Ataque propio de la nota: coeficiente por muestra (1 = inmediato).
    att: f32,
    attacking: bool,
    /// Caída por muestra (1 = sin caída) y ganancias de panorama.
    dec: f32,
    lr: [f32; 2],
}

#[derive(Default)]
struct State {
    voices: [Voice; VOICES],
    clock: u32,
    rng: u32,
    next: u64,
    playing: bool,
    /// Ganancia y ataque pedidos para la próxima nota (controladores 16 y 73).
    cc_gain: Option<u8>,
    cc_attack: Option<u8>,
}

pub struct Sampler {
    pub name: String,
    pub gain: AtomicF32,
    /// Nombres de teclas (p. ej. "Bombo", "Caja" en una batería), para el piano roll.
    pub key_names: Vec<(u8, String)>,
    zones: Vec<Zone>,
    state: Mutex<State>,
}

/// Nota SFZ: número MIDI o nombre ("c4" = 60, "f#3", "bb2").
fn note(v: &str) -> Option<f32> {
    if let Ok(n) = v.parse::<f32>() {
        return Some(n);
    }
    let v = v.to_lowercase();
    let base = [0, 2, 4, 5, 7, 9, 11]["cdefgab".find(v.chars().next()?)?] as f32;
    let (acc, rest) = match &v[1..] {
        r if r.starts_with('#') => (1.0, &r[1..]),
        r if r.starts_with('b') && r.len() > 1 => (-1.0, &r[1..]),
        r => (0.0, r),
    };
    Some((rest.parse::<f32>().ok()? + 1.0) * 12.0 + base + acc)
}

impl Sampler {
    /// Carga un archivo .sfz y decodifica sus muestras a la frecuencia del motor.
    pub fn load(path: &Path, rate: u32) -> anyhow::Result<Self> {
        let dir = path.parent().unwrap_or(Path::new("."));
        let text = std::fs::read_to_string(path)?;
        let (mut global, mut group, mut control) = (HashMap::new(), HashMap::new(), HashMap::new());
        let mut regions: Vec<HashMap<String, String>> = vec![];
        let mut key_names = vec![];
        let (mut current, mut header, mut comment) = (HashMap::new(), String::new(), String::new());
        let mut flush = |header: &str, map: &mut HashMap<String, String>, global: &mut HashMap<String, String>, group: &mut HashMap<String, String>, control: &mut HashMap<String, String>| {
            let map = std::mem::take(map);
            match header {
                "region" => {
                    let mut r = global.clone();
                    r.extend(group.clone());
                    r.extend(map);
                    r.extend(control.iter().filter(|(k, _)| *k == "default_path").map(|(k, v)| (k.clone(), v.clone())));
                    regions.push(r);
                }
                "group" | "master" => *group = map,
                "global" => *global = map,
                "control" => *control = map,
                _ => {}
            }
        };
        for line in text.lines() {
            let line = line.trim();
            if let Some(c) = line.strip_prefix("//").filter(|c| !c.starts_with('+')) {
                comment = c.trim().to_string();
                continue;
            }
            let line = line.split("//").next().unwrap_or("");
            let mut last: Option<String> = None;
            for tok in line.split_whitespace() {
                if let Some(h) = tok.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
                    flush(&header, &mut current, &mut global, &mut group, &mut control);
                    header = h.to_string();
                    last = None;
                    if h == "group" {
                        group.clear();
                    }
                } else if let Some((k, v)) = tok.split_once('=') {
                    current.insert(k.to_string(), v.to_string());
                    if k == "key"
                        && !comment.is_empty()
                        && let Some(n) = note(v)
                    {
                        key_names.push((n as u8, std::mem::take(&mut comment)));
                    }
                    last = Some(k.to_string());
                } else if let Some(k) = &last {
                    // Valores con espacios (rutas de muestras).
                    current.entry(k.clone()).and_modify(|v| *v = format!("{v} {tok}"));
                }
            }
        }
        flush(&header, &mut current, &mut global, &mut group, &mut control);
        key_names.dedup_by_key(|k| k.0);

        let mut cache: HashMap<String, Pcm> = HashMap::new();
        let mut zones = vec![];
        for r in &regions {
            let Some(sample) = r.get("sample") else {
                continue;
            };
            if r.get("trigger").is_some_and(|t| t != "attack") {
                continue;
            }
            let file = format!("{}{}", r.get("default_path").map_or("", |s| s.as_str()), sample).replace('\\', "/");
            let data = match cache.get(&file) {
                Some(d) => d.clone(),
                None => {
                    let frames = decode(&dir.join(&file), rate)?;
                    let pcm: Pcm = Arc::new(frames.iter().map(|s| [(s[0].clamp(-1.0, 1.0) * 32767.0) as i16, (s[1].clamp(-1.0, 1.0) * 32767.0) as i16]).collect());
                    cache.insert(file.clone(), pcm.clone());
                    pcm
                }
            };
            let num = |k: &str, d: f32| r.get(k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(d);
            let key = |k: &str| r.get(k).and_then(|v| note(v));
            let (lokey, hikey) = match key("key") {
                Some(k) => (k, k),
                None => (key("lokey").unwrap_or(0.0), key("hikey").unwrap_or(127.0)),
            };
            let root = key("pitch_keycenter").or(key("key")).unwrap_or(60.0) - num("transpose", 0.0) - num("tune", 0.0) / 100.0;
            let looped = matches!(r.get("loop_mode").map(String::as_str), Some("loop_continuous" | "loop_sustain"))
                .then(|| (num("loop_start", 0.0) as usize, num("loop_end", data.len() as f32 - 1.0) as usize))
                .filter(|(a, b)| a < b && *b < data.len());
            zones.push(Zone {
                data,
                lokey: lokey as u8,
                hikey: hikey as u8,
                root,
                lovel: num("lovel", 0.0) as u8,
                hivel: num("hivel", 127.0) as u8,
                lorand: num("lorand", 0.0),
                hirand: num("hirand", 1.0),
                looped,
                release: num("ampeg_release", 0.05).clamp(0.01, 10.0),
                gain: 10f32.powf(num("volume", 0.0) / 20.0),
                veltrack: num("amp_veltrack", 100.0) / 100.0,
                pan: (num("pan", 0.0) / 100.0).clamp(-1.0, 1.0),
                decay: num("ampeg_decay", 0.0).max(0.0),
                one_shot: r.get("loop_mode").is_some_and(|m| m == "one_shot"),
                group: num("group", 0.0) as u32,
                off_by: num("off_by", 0.0) as u32,
                curva: curva_velocidad(r),
            });
        }
        anyhow::ensure!(!zones.is_empty(), "el instrumento no tiene muestras");
        let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        Ok(Self { name, gain: AtomicF32::new(1.0), key_names, zones, state: Mutex::new(State { rng: 0x9E37_79B9, ..Default::default() }) })
    }

    /// Voces sonando ahora (para el contador de la interfaz).
    pub fn voces_activas(&self) -> usize {
        self.state.try_lock().map_or(0, |st| st.voices.iter().filter(|v| v.active).count())
    }

    pub fn render(&self, out: &mut [[f32; 2]], events: &[(usize, MidiEvent)], sr: f32, offline: bool, pos: u64, playing: bool) {
        self.render_salidas(out, &mut [], &[0; 128], events, sr, offline, pos, playing);
    }

    /// Como `render`, pero cada nota con `ruta[nota] = k > 0` suena en `extra[k - 1]` en vez de en
    /// `out` (multisalida de una batería: cada pieza en su canal del mezclador).
    #[allow(clippy::too_many_arguments)]
    pub fn render_salidas(&self, out: &mut [[f32; 2]], extra: &mut [Vec<[f32; 2]>], ruta: &[u8; 128], events: &[(usize, MidiEvent)], sr: f32, offline: bool, pos: u64, playing: bool) {
        let Some(mut st) = (if offline { self.state.lock().ok() } else { self.state.try_lock().ok() }) else {
            return;
        };
        let st = &mut *st;
        // Stop, loop o salto del cursor: se liberan las notas.
        if (st.playing && !playing) || (playing && pos != st.next) {
            st.voices.iter_mut().filter(|v| v.active).for_each(|v| v.releasing = true);
        }
        (st.playing, st.next) = (playing, pos + out.len() as u64);
        let g = self.gain.get();
        let mut ev = events.iter().peekable();
        for (i, o) in out.iter_mut().enumerate() {
            while let Some(&(_, e)) = ev.next_if(|e| e.0 <= i) {
                self.handle(st, e, sr);
            }
            for v in st.voices.iter_mut().filter(|v| v.active) {
                let z = &self.zones[v.zone];
                let k = v.pos as usize;
                let (Some(a), Some(b)) = (z.data.get(k), z.data.get(k + 1).or(z.data.get(k))) else {
                    v.active = false;
                    continue;
                };
                let f = (v.pos - k as f64) as f32;
                let s = |c: usize| (a[c] as f32 + (b[c] as f32 - a[c] as f32) * f) / 32768.0;
                if v.attacking && !v.releasing {
                    v.env += (1.0 - v.env) * v.att;
                    if v.env >= 0.999 {
                        (v.env, v.attacking) = (1.0, false);
                    }
                }
                if v.releasing || v.dec < 1.0 {
                    v.env *= if v.releasing { v.rel } else { v.dec };
                    if v.env < 1e-4 {
                        v.active = false;
                        continue;
                    }
                }
                let gain = v.gain * v.env * g;
                let (l, r) = (s(0) * gain * v.lr[0], s(1) * gain * v.lr[1]);
                match ruta[v.key as usize & 127] {
                    0 => *o = [o[0] + l, o[1] + r],
                    k => {
                        if let Some(d) = extra.get_mut(k as usize - 1).and_then(|b| b.get_mut(i)) {
                            *d = [d[0] + l, d[1] + r];
                        }
                    }
                }
                v.pos += v.step;
                if let Some((ls, le)) = z.looped.filter(|(_, le)| v.pos >= *le as f64) {
                    v.pos -= (le - ls) as f64;
                }
            }
        }
        ev.for_each(|&(_, e)| self.handle(st, e, sr));
    }

    fn handle(&self, st: &mut State, e: MidiEvent, sr: f32) {
        st.clock += 1;
        match (e[0] & 0xF0, e[2]) {
            (0x90, vel) if vel > 0 => {
                st.rng ^= st.rng << 13;
                st.rng ^= st.rng >> 17;
                st.rng ^= st.rng << 5;
                let r = st.rng as f32 / u32::MAX as f32;
                let zone = self
                    .zones
                    .iter()
                    .position(|z| (z.lokey..=z.hikey).contains(&e[1]) && (z.lovel..=z.hivel).contains(&vel) && r >= z.lorand && r < z.hirand.max(z.lorand + 1e-6))
                    .or_else(|| self.zones.iter().position(|z| (z.lokey..=z.hikey).contains(&e[1])));
                let Some(zone) = zone else { return };
                let z = &self.zones[zone];
                // Grupo de corte: el hi-hat cerrado apaga al abierto (con un fundido muy corto).
                if z.group > 0 {
                    let rapido = (-1.0 / (0.004 * sr)).exp();
                    for v in st.voices.iter_mut().filter(|v| v.active && self.zones[v.zone].off_by == z.group) {
                        (v.releasing, v.rel) = (true, rapido);
                    }
                }
                let slot = (0..VOICES).find(|&i| !st.voices[i].active).unwrap_or_else(|| (0..VOICES).min_by_key(|&i| st.voices[i].age).unwrap_or(0));
                let vel_gain = z.curva.as_ref().map_or_else(|| (1.0 - z.veltrack) + z.veltrack * (vel as f32 / 127.0).powi(2), |c| c[vel as usize]);
                let nota_gain = st.cc_gain.take().map_or(1.0, crate::ganancia_nota);
                let ataque = st.cc_attack.take().map_or(0.0, crate::ataque_ms);
                st.voices[slot] = Voice {
                    zone,
                    key: e[1],
                    active: true,
                    releasing: false,
                    pos: 0.0,
                    step: 2f64.powf((e[1] as f64 - z.root as f64) / 12.0),
                    gain: z.gain * vel_gain * nota_gain,
                    env: if ataque > 0.0 { 0.0 } else { 1.0 },
                    att: if ataque > 0.0 { 1.0 - (-1.0 / (ataque * 0.001 * sr)).exp() } else { 1.0 },
                    attacking: ataque > 0.0,
                    rel: (-1.0 / (z.release * sr)).exp(),
                    age: st.clock,
                    // Caída: -40 dB al cabo de `decay` segundos.
                    dec: if z.decay > 0.0 { (-4.6 / (z.decay * sr)).exp() } else { 1.0 },
                    lr: [(1.0 - z.pan).min(1.0), (1.0 + z.pan).min(1.0)],
                };
            }
            (0x80 | 0x90, _) => st.voices.iter_mut().filter(|v| v.active && v.key == e[1] && !self.zones[v.zone].one_shot).for_each(|v| v.releasing = true),
            (0xB0, _) if e[1] == crate::CC_GANANCIA => st.cc_gain = Some(e[2]),
            (0xB0, _) if e[1] == crate::CC_ATAQUE => st.cc_attack = Some(e[2]),
            (0xB0, _) if e[1] >= 120 => st.voices.iter_mut().for_each(|v| v.releasing = true),
            _ => {}
        }
    }
}

/// Curva de velocidad SFZ (`amp_velcurve_N=ganancia`, N de 1 a 127): se interpola entre los puntos
/// dados, con 0 en fuerza 0 y 1 en 127 si no se dan.
fn curva_velocidad(r: &HashMap<String, String>) -> Option<Arc<[f32; 128]>> {
    let mut puntos: Vec<(usize, f32)> = r.iter().filter_map(|(k, v)| Some((k.strip_prefix("amp_velcurve_")?.parse::<usize>().ok()?.min(127), v.parse::<f32>().ok()?))).collect();
    if puntos.is_empty() {
        return None;
    }
    if !puntos.iter().any(|p| p.0 == 0) {
        puntos.push((0, 0.0));
    }
    if !puntos.iter().any(|p| p.0 == 127) {
        puntos.push((127, 1.0));
    }
    puntos.sort_by_key(|p| p.0);
    let mut t = [0f32; 128];
    for w in puntos.windows(2) {
        let ((a, ga), (b, gb)) = (w[0], w[1]);
        for (v, g) in t.iter_mut().enumerate().take(b + 1).skip(a) {
            *g = ga + (gb - ga) * (v - a) as f32 / (b - a).max(1) as f32;
        }
    }
    Some(Arc::new(t))
}

#[cfg(test)]
mod pruebas_bateria {
    use super::*;

    /// WAV de 16 bits mono con un tono constante (para probar el sampler sin archivos externos).
    fn wav(ruta: &Path, n: usize) {
        let mut b: Vec<u8> = vec![];
        let datos = (n * 2) as u32;
        b.extend(b"RIFF");
        b.extend((36 + datos).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(48_000u32.to_le_bytes());
        b.extend(96_000u32.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(datos.to_le_bytes());
        for i in 0..n {
            b.extend((((i as f32 * 0.05).sin() * 16_000.0) as i16).to_le_bytes());
        }
        std::fs::write(ruta, b).unwrap();
    }

    fn pico(s: &Sampler, ev: &[(usize, MidiEvent)]) -> f32 {
        let mut out = vec![[0f32; 2]; 4800];
        s.render(&mut out, ev, 48_000.0, true, 0, true);
        out.iter().map(|x| x[0].abs().max(x[1].abs())).fold(0.0, f32::max)
    }

    #[test]
    fn golpe_entero_y_corte_del_hihat() {
        let dir = std::env::temp_dir().join(format!("qdaw-sampler-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        wav(&dir.join("a.wav"), 48_000);
        std::fs::write(dir.join("k.sfz"), "<global> loop_mode=one_shot\n<group> key=42 group=2\n<region> sample=a.wav\n<group> key=46 group=1 off_by=2 pan=-100\n<region> sample=a.wav\n").unwrap();
        let s = Sampler::load(&dir.join("k.sfz"), 48_000).unwrap();
        // Hi-hat abierto (todo a la izquierda): soltar la nota no lo corta.
        let mut out = vec![[0f32; 2]; 4800];
        s.render(&mut out, &[(0, [0x90, 46, 120]), (10, [0x80, 46, 0])], 48_000.0, true, 0, true);
        assert!(out[4000][1].abs() < 1e-6, "panorama a la izquierda");
        assert!(pico(&s, &[]) > 0.1, "one shot: sigue sonando");
        // El cerrado corta al abierto.
        s.render(&mut vec![[0f32; 2]; 4800], &[(0, [0x90, 42, 1])], 48_000.0, true, 0, true);
        let mut out = vec![[0f32; 2]; 4800];
        s.render(&mut out, &[], 48_000.0, true, 0, true);
        let izquierda = out.iter().map(|x| x[0].abs() - x[1].abs()).fold(0.0, f32::max);
        assert!(izquierda < 0.01, "el hi-hat abierto se cortó: {izquierda}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn multisalida_por_pieza() {
        let dir = std::env::temp_dir().join(format!("qdaw-salidas-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        wav(&dir.join("a.wav"), 48_000);
        std::fs::write(dir.join("k.sfz"), "<global> loop_mode=one_shot\n<group> key=36\n<region> sample=a.wav\n<group> key=38\n<region> sample=a.wav\n").unwrap();
        let s = Sampler::load(&dir.join("k.sfz"), 48_000).unwrap();
        // La caja va al canal 1; el bombo queda en la salida de la pista.
        let mut ruta = [0u8; 128];
        ruta[38] = 1;
        let (mut out, mut extra) = (vec![[0f32; 2]; 1024], vec![vec![[0f32; 2]; 1024]]);
        s.render_salidas(&mut out, &mut extra, &ruta, &[(0, [0x90, 38, 120])], 48_000.0, true, 0, true);
        let pico = |b: &[[f32; 2]]| b.iter().map(|x| x[0].abs()).fold(0.0, f32::max);
        assert!(pico(&out) < 1e-6 && pico(&extra[0]) > 0.1, "la caja suena solo en su canal");
        let (mut out, mut extra) = (vec![[0f32; 2]; 1024], vec![vec![[0f32; 2]; 1024]]);
        s.render_salidas(&mut out, &mut extra, &ruta, &[(0, [0x90, 36, 120])], 48_000.0, true, 1024, true);
        assert!(pico(&out) > 0.1, "el bombo sigue en la pista");
        let _ = std::fs::remove_dir_all(dir);
    }
}
