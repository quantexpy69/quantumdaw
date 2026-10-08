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
}

#[derive(Default)]
struct State {
    voices: [Voice; VOICES],
    clock: u32,
    rng: u32,
    next: u64,
    playing: bool,
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
            });
        }
        anyhow::ensure!(!zones.is_empty(), "el instrumento no tiene muestras");
        let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        Ok(Self { name, gain: AtomicF32::new(1.0), key_names, zones, state: Mutex::new(State { rng: 0x9E37_79B9, ..Default::default() }) })
    }

    pub fn render(&self, out: &mut [[f32; 2]], events: &[(usize, MidiEvent)], sr: f32, offline: bool, pos: u64, playing: bool) {
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
                if v.releasing {
                    v.env *= v.rel;
                    if v.env < 1e-4 {
                        v.active = false;
                        continue;
                    }
                }
                let gain = v.gain * v.env * g;
                *o = [o[0] + s(0) * gain, o[1] + s(1) * gain];
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
                let slot = (0..VOICES).find(|&i| !st.voices[i].active).unwrap_or_else(|| (0..VOICES).min_by_key(|&i| st.voices[i].age).unwrap_or(0));
                let vel_gain = (1.0 - z.veltrack) + z.veltrack * (vel as f32 / 127.0).powi(2);
                st.voices[slot] = Voice {
                    zone,
                    key: e[1],
                    active: true,
                    releasing: false,
                    pos: 0.0,
                    step: 2f64.powf((e[1] as f64 - z.root as f64) / 12.0),
                    gain: z.gain * vel_gain,
                    env: 1.0,
                    rel: (-1.0 / (z.release * sr)).exp(),
                    age: st.clock,
                };
            }
            (0x80 | 0x90, _) => st.voices.iter_mut().filter(|v| v.active && v.key == e[1]).for_each(|v| v.releasing = true),
            (0xB0, _) if e[1] >= 120 => st.voices.iter_mut().for_each(|v| v.releasing = true),
            _ => {}
        }
    }
}
