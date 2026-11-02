//! Plugins LV2 de terceros (Linux): catálogo con lilv y proceso como efecto o instrumento.
//! El catálogo se arma una sola vez (tarda un momento con cientos de plugins). Los nombres y rangos
//! de los parámetros quedan fijos (`&'static`), igual que los de los efectos nativos, para que la
//! interfaz los trate del mismo modo.
// Fuera de Linux el catálogo está vacío y casi nada de esto se usa.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]
use crate::{AtomicF32, MidiEvent};
use std::sync::Arc;

/// Tipo de plugin, para el dibujo de su carátula y los filtros de la biblioteca.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Categoria {
    Reverb,
    Delay,
    Dinamica,
    Eq,
    Distorsion,
    Amplificador,
    Modulacion,
    Afinacion,
    Sintetizador,
    Bateria,
    Organo,
    Sampler,
    Analisis,
    Utilidad,
}

impl Categoria {
    pub fn nombre(self) -> &'static str {
        match self {
            Self::Reverb => "Reverb",
            Self::Delay => "Delay",
            Self::Dinamica => "Dinámica",
            Self::Eq => "EQ y filtros",
            Self::Distorsion => "Distorsión",
            Self::Amplificador => "Amplificador",
            Self::Modulacion => "Modulación",
            Self::Afinacion => "Afinación",
            Self::Sintetizador => "Sintetizador",
            Self::Bateria => "Batería",
            Self::Organo => "Órgano",
            Self::Sampler => "Sampler",
            Self::Analisis => "Análisis",
            Self::Utilidad => "Utilidad",
        }
    }
}

/// Cómo se muestra y se envía un parámetro.
#[derive(Default)]
pub struct Param {
    /// Valores enteros o de encendido (se redondean); con opciones, sus nombres por valor.
    pub entero: bool,
    pub conmutador: bool,
    pub opciones: Vec<(f32, String)>,
}

impl Param {
    /// Texto del valor: la opción que corresponde, Sí/No o el número.
    pub fn texto(&self, v: f32) -> String {
        if let Some((_, n)) = self.opciones.iter().min_by(|a, b| (a.0 - v).abs().total_cmp(&(b.0 - v).abs())) {
            return n.clone();
        }
        match (self.conmutador, self.entero) {
            (true, _) => if v >= 0.5 { "Sí" } else { "No" }.into(),
            (_, true) => format!("{:.0}", v.round()),
            _ if v.abs() >= 100.0 => format!("{v:.0}"),
            _ if v.abs() >= 10.0 => format!("{v:.1}"),
            _ => format!("{v:.2}"),
        }
    }
}

/// Un plugin del catálogo.
pub struct Info {
    pub uri: String,
    pub nombre: String,
    /// Colección o fabricante (LSP, Calf, Guitarix…).
    pub marca: String,
    pub instrumento: bool,
    pub categoria: Categoria,
    /// (nombre, mínimo, máximo, valor por defecto) de cada parámetro visible.
    pub defs: &'static [(&'static str, f32, f32, f32)],
    pub meta: &'static [Param],
    /// Puerto de control de cada parámetro visible.
    puertos: Vec<usize>,
    pub entradas: usize,
    pub salidas: usize,
}

/// Datos de un parámetro a partir de su nombre (el `&'static str` de `Info::defs`).
pub fn param(nombre: &str) -> Option<&'static Param> {
    imp::param(nombre)
}

/// Catálogo completo (lo arma la primera vez que se pide).
pub fn catalogo() -> &'static [Arc<Info>] {
    imp::catalogo()
}

/// Carpetas propias donde también se buscan bundles `.lv2` (además de las rutas estándar).
pub fn carpetas(dirs: &[std::path::PathBuf]) {
    if let Ok(mut c) = CARPETAS.lock() {
        *c = dirs.to_vec();
    }
}

static CARPETAS: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Vuelve a buscar los plugins (tras instalar alguno o cambiar las carpetas). Tarda: va en otro hilo.
/// Los plugins ya abiertos siguen funcionando con el catálogo anterior.
pub fn recargar() {
    imp::recargar()
}

/// Bundles `.lv2` dentro de las carpetas propias (hasta tres niveles de profundidad).
fn bundles_propios() -> Vec<std::path::PathBuf> {
    fn buscar(d: &std::path::Path, nivel: usize, v: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if p.extension().is_some_and(|x| x == "lv2") {
                v.push(p);
            } else if nivel < 3 {
                buscar(&p, nivel + 1, v);
            }
        }
    }
    let mut v = vec![];
    for d in CARPETAS.lock().map(|c| c.clone()).unwrap_or_default() {
        if d.extension().is_some_and(|x| x == "lv2") { v.push(d) } else { buscar(&d, 0, &mut v) }
    }
    v
}

/// URI `file://` de una carpeta (con los espacios y acentos codificados), terminada en `/`.
fn uri_archivo(p: &std::path::Path) -> String {
    let mut s = String::from("file://");
    for b in p.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => s.push(b as char),
            _ => s.push_str(&format!("%{b:02X}")),
        }
    }
    if !s.ends_with('/') {
        s.push('/');
    }
    s
}

/// Catálogo si ya está armado (la interfaz no espera a que termine).
pub fn listo() -> Option<&'static [Arc<Info>]> {
    imp::listo()
}

pub fn info(uri: &str) -> Option<Arc<Info>> {
    catalogo().iter().find(|i| i.uri == uri).cloned()
}

/// CC propios de las notas de QUANTUM (ganancia y ataque): no se envían a los plugins.
fn cc_propio(e: &MidiEvent) -> bool {
    e[0] & 0xF0 == 0xB0 && (e[1] == crate::CC_GANANCIA || e[1] == crate::CC_ATAQUE)
}

fn marca(uri: &str, autor: Option<&str>) -> String {
    const MARCAS: [(&str, &str); 12] = [
        ("lsp-plug.in", "LSP"),
        ("calf.sourceforge", "Calf"),
        ("guitarix", "Guitarix"),
        ("dragonfly", "Dragonfly"),
        ("zynaddsubfx", "ZynAddSubFX"),
        ("synthv1", "synthv1"),
        ("samplv1", "samplv1"),
        ("avldrums", "AVL Drums"),
        ("b_synth", "setBfree"),
        ("b_whirl", "setBfree"),
        ("gareus.org", "x42"),
        ("carla", "Carla"),
    ];
    MARCAS.iter().find(|m| uri.contains(m.0)).map(|m| m.1.to_string()).or_else(|| autor.map(String::from)).unwrap_or_else(|| uri.split('/').nth(2).unwrap_or(uri).to_string())
}

fn categoria(nombre: &str, clases: &[&str], instrumento: bool) -> Categoria {
    let n = nombre.to_lowercase();
    let es = |c: &str| clases.iter().any(|x| x.starts_with(c));
    if instrumento {
        return if n.contains("drum") || n.contains("percussion") {
            Categoria::Bateria
        } else if n.contains("organ") || n.contains("tonewheel") {
            Categoria::Organo
        } else if n.contains("sampler") || n.contains("fluidsynth") {
            Categoria::Sampler
        } else {
            Categoria::Sintetizador
        };
    }
    if es("Reverb") {
        Categoria::Reverb
    } else if es("Delay") {
        Categoria::Delay
    } else if es("Compressor") || es("Limiter") || es("Gate") || es("Expander") || es("Dynamics") || es("Envelope") {
        Categoria::Dinamica
    } else if es("Parametric EQ") || es("Multiband EQ") || es("EQ") || es("Filter") || es("Lowpass") || es("Highpass") || es("Bandpass") || es("Allpass") || es("Comb") {
        Categoria::Eq
    } else if es("Pitch Shifter") || n.contains("tune") {
        Categoria::Afinacion
    } else if es("Simulator") || n.contains("amp") {
        Categoria::Amplificador
    } else if es("Distortion") || es("Waveshaper") {
        Categoria::Distorsion
    } else if es("Modulator") || es("Chorus") || es("Flanger") || es("Phaser") {
        Categoria::Modulacion
    } else if es("Analyser") {
        Categoria::Analisis
    } else {
        Categoria::Utilidad
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use crate::BLOCK;
    use livi::{PortIndex, PortType, event::LV2AtomSequence};
    use std::{
        collections::{HashMap, HashSet},
        sync::{Mutex, RwLock},
    };

    /// Las rutas estándar en un mundo de lilv y cada bundle de las carpetas propias en el suyo
    /// (lilv solo amplía su búsqueda con LV2_PATH, que no se puede cambiar con el programa abierto).
    struct Mundo {
        mundos: Vec<(livi::World, Arc<livi::Features>)>,
        infos: Vec<Arc<Info>>,
    }

    /// El catálogo vigente. Al recargar, el anterior queda vivo (lo usan los plugins ya abiertos).
    static MUNDO: RwLock<Option<&'static Mundo>> = RwLock::new(None);
    static CARGANDO: Mutex<()> = Mutex::new(());
    /// Parámetro por la dirección de su nombre (de todos los catálogos armados).
    static PARAMS: RwLock<Option<HashMap<usize, &'static Param>>> = RwLock::new(None);

    fn actual() -> Option<&'static Mundo> {
        *MUNDO.read().ok()?
    }

    fn mundo() -> &'static Mundo {
        if let Some(m) = actual() {
            return m;
        }
        let _uno = CARGANDO.lock();
        actual().unwrap_or_else(|| {
            let m: &'static Mundo = Box::leak(Box::new(cargar()));
            if let Ok(mut w) = MUNDO.write() {
                *w = Some(m);
            }
            m
        })
    }

    pub fn recargar() {
        let _uno = CARGANDO.lock();
        let m: &'static Mundo = Box::leak(Box::new(cargar()));
        if let Ok(mut w) = MUNDO.write() {
            *w = Some(m);
        }
    }

    pub fn catalogo() -> &'static [Arc<Info>] {
        &mundo().infos
    }

    pub fn listo() -> Option<&'static [Arc<Info>]> {
        actual().map(|m| m.infos.as_slice())
    }

    pub fn param(nombre: &str) -> Option<&'static Param> {
        PARAMS.read().ok()?.as_ref()?.get(&(nombre.as_ptr() as usize)).copied()
    }

    fn cargar() -> Mundo {
        let mut mundos = vec![livi::World::new()];
        mundos.extend(bundles_propios().iter().map(|b| livi::World::with_load_bundle(&uri_archivo(b))));
        let mundos: Vec<_> = mundos
            .into_iter()
            .map(|w| {
                let f = w.try_build_features(livi::FeaturesBuilder { min_block_length: 1, max_block_length: BLOCK }).expect("BLOCK es un tamaño de bloque válido");
                (w, f)
            })
            .collect();
        let mut params = HashMap::new();
        let mut vistos = HashSet::new();
        let mut infos = vec![];
        for (world, _) in &mundos {
            for i in leer(world, &mut params) {
                if vistos.insert(i.uri.clone()) {
                    infos.push(i);
                }
            }
        }
        infos.sort_by_cached_key(|i| (i.marca.to_lowercase(), i.nombre.to_lowercase()));
        if let Ok(mut p) = PARAMS.write() {
            p.get_or_insert_with(HashMap::new).extend(params);
        }
        Mundo { mundos, infos }
    }

    fn leer(world: &livi::World, params: &mut HashMap<usize, &'static Param>) -> Vec<Arc<Info>> {
        let raw = world.raw();
        let prop = |uri: &str| raw.new_uri(uri);
        let (entero, conmutador, enumeracion, oculto) = (
            prop("http://lv2plug.in/ns/lv2core#integer"),
            prop("http://lv2plug.in/ns/lv2core#toggled"),
            prop("http://lv2plug.in/ns/lv2core#enumeration"),
            prop("http://lv2plug.in/ns/ext/port-props#notOnGUI"),
        );
        world
            .iter_plugins()
            .map(|p| {
                let (uri, nombre, instrumento) = (p.uri(), p.name(), p.is_instrument());
                let clases: Vec<&str> = p.classes().collect();
                let autor = p.raw().author_name().and_then(|n| n.as_str().map(String::from));
                let (mut defs, mut meta, mut puertos) = (vec![], vec![], vec![]);
                for port in p.ports_with_type(PortType::ControlInput) {
                    let Some(lp) = p.raw().port_by_index(port.index.0) else { continue };
                    let (min, max) = (port.min_value.unwrap_or(0.0), port.max_value.unwrap_or(1.0));
                    if lp.has_property(&oculto) || !(max > min) || port.name.is_empty() {
                        continue;
                    }
                    let opciones = if lp.has_property(&enumeracion) || lp.scale_points().count() > 0 {
                        lp.scale_points().iter().filter_map(|s| Some((s.value().as_float()?, s.label().as_str()?.to_string()))).collect()
                    } else {
                        vec![]
                    };
                    let nombre: &'static str = Box::leak(port.name.clone().into_boxed_str());
                    defs.push((nombre, min, max, port.default_value.clamp(min, max)));
                    meta.push(Param { entero: lp.has_property(&entero) || !opciones.is_empty(), conmutador: lp.has_property(&conmutador), opciones });
                    puertos.push(port.index.0);
                }
                let defs: &'static [_] = Box::leak(defs.into_boxed_slice());
                let meta: &'static [Param] = Box::leak(meta.into_boxed_slice());
                for (d, m) in defs.iter().zip(meta) {
                    params.insert(d.0.as_ptr() as usize, m);
                }
                let c = p.port_counts();
                Arc::new(Info {
                    marca: marca(&uri, autor.as_deref()),
                    categoria: categoria(&nombre, &clases, instrumento),
                    uri,
                    nombre,
                    instrumento,
                    defs,
                    meta,
                    puertos,
                    entradas: c.audio_inputs,
                    salidas: c.audio_outputs,
                })
            })
            // Solo lo que se puede usar en una pista: con salida de audio y, si no es instrumento, con entrada.
            .filter(|i| i.salidas > 0 && (i.instrumento || i.entradas > 0))
            .collect()
    }

    /// Un plugin funcionando, con sus buffers (el hilo de audio no reserva memoria).
    pub struct Instancia {
        inst: livi::Instance,
        info: Arc<Info>,
        ent: Vec<Vec<f32>>,
        sal: Vec<Vec<f32>>,
        seq_in: Vec<LV2AtomSequence>,
        seq_out: Vec<LV2AtomSequence>,
        midi: u32,
    }

    impl Instancia {
        pub fn nueva(info: &Arc<Info>, sample_rate: u32) -> Option<Self> {
            let (p, features) = mundo().mundos.iter().find_map(|(w, f)| Some((w.plugin_by_uri(&info.uri)?, f)))?;
            let inst = unsafe { p.instantiate(features.clone(), sample_rate as f64) }.ok()?;
            let c = inst.port_counts();
            let seq = |n| (0..n).map(|_| LV2AtomSequence::new(features, 16 * 1024)).collect();
            Some(Self {
                ent: vec![vec![0.0; BLOCK]; c.audio_inputs],
                sal: vec![vec![0.0; BLOCK]; c.audio_outputs],
                seq_in: seq(c.atom_sequence_inputs),
                seq_out: seq(c.atom_sequence_outputs),
                midi: features.midi_urid(),
                info: info.clone(),
                inst,
            })
        }

        /// Procesa `buf` con los valores de `params`. Un efecto reemplaza la señal; un instrumento
        /// toca `eventos` y su sonido se suma a lo que ya hay en `buf`.
        pub fn procesar(&mut self, buf: &mut [[f32; 2]], params: &[AtomicF32], eventos: &[(usize, MidiEvent)]) {
            let n = buf.len().min(BLOCK);
            if n == 0 {
                return;
            }
            for ((&puerto, a), m) in self.info.puertos.iter().zip(params).zip(self.info.meta) {
                let v = a.get();
                self.inst.set_control_input(PortIndex(puerto), if m.entero || m.conmutador { v.round() } else { v });
            }
            match self.ent.as_mut_slice() {
                [] => {}
                [mono] => mono.iter_mut().zip(buf.iter()).for_each(|(x, s)| *x = (s[0] + s[1]) * 0.5),
                [l, r, resto @ ..] => {
                    for (i, s) in buf[..n].iter().enumerate() {
                        (l[i], r[i]) = (s[0], s[1]);
                    }
                    resto.iter_mut().for_each(|c| c.fill(0.0));
                }
            }
            for (k, seq) in self.seq_in.iter_mut().enumerate() {
                seq.clear();
                if k == 0 {
                    for (at, e) in eventos.iter().filter(|(_, e)| !cc_propio(e)) {
                        let largo = if matches!(e[0] & 0xF0, 0xC0 | 0xD0) { 2 } else { 3 };
                        let _ = seq.push_midi_event::<3>((*at).min(n - 1) as i64, self.midi, &e[..largo]);
                    }
                }
            }
            self.seq_out.iter_mut().for_each(|s| s.clear_as_chunk());
            let ports = livi::EmptyPortConnections::new()
                .with_audio_inputs(self.ent.iter().map(|c| &c[..n]))
                .with_audio_outputs(self.sal.iter_mut().map(|c| &mut c[..n]))
                .with_atom_sequence_inputs(self.seq_in.iter())
                .with_atom_sequence_outputs(self.seq_out.iter_mut());
            if unsafe { self.inst.run(n, ports) }.is_err() {
                return;
            }
            let (l, r) = match self.sal.as_slice() {
                [] => return,
                [mono] => (mono, mono),
                [l, r, ..] => (l, r),
            };
            let instrumento = self.info.instrumento;
            for (i, s) in buf[..n].iter_mut().enumerate() {
                *s = if instrumento { [s[0] + l[i], s[1] + r[i]] } else { [l[i], r[i]] };
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    pub fn catalogo() -> &'static [Arc<Info>] {
        &[]
    }
    pub fn listo() -> Option<&'static [Arc<Info>]> {
        Some(&[])
    }
    pub fn param(_: &str) -> Option<&'static Param> {
        None
    }
    pub fn recargar() {}

    pub struct Instancia;

    impl Instancia {
        pub fn nueva(_: &Arc<Info>, _: u32) -> Option<Self> {
            None
        }
        pub fn procesar(&mut self, _: &mut [[f32; 2]], _: &[AtomicF32], _: &[(usize, MidiEvent)]) {}
    }
}

pub use imp::Instancia;
