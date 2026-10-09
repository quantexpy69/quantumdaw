mod analyzer;
mod audioedit;
mod chrome;
mod i18n;
mod icons;
mod instruments;
mod keys;
mod library;
mod mixer;
mod pianoroll;
mod plugins;
mod routing;
mod timeline;
mod video;
mod welcome;
mod widgets;

use eframe::egui::{self, Color32, Event, Key};
use engine::{AudioBuf, AudioConfig, Clip, DeviceInfo, Engine, Fx, FxKind, MidiConnections, MidiEvent, Node, Note, Params, Sampler, Source, Synth};
use i18n::tr;
use project::{AUDIO_DIR, ClipState, Format, FxState, GroupState, PALETTE, Project, RENDER_DIR, TrackKind, TrackState};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// Tokens de color: grises neutros oscuros al estilo de Logic, con buen contraste.
const BG: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1C);
const PANEL: Color32 = Color32::from_rgb(0x25, 0x25, 0x28);
const ELEVATED: Color32 = Color32::from_rgb(0x30, 0x30, 0x34);
const BORDER: Color32 = Color32::from_rgb(0x3E, 0x3E, 0x44);
const TEXT: Color32 = Color32::from_rgb(0xED, 0xED, 0xEF);
const TEXT_DIM: Color32 = Color32::from_rgb(0xA3, 0xA3, 0xAB);
const ACCENT: Color32 = Color32::from_rgb(0x5B, 0x8C, 0xFF);
const METER: [Color32; 3] = [Color32::from_rgb(0x3D, 0xDC, 0x84), Color32::from_rgb(0xFF, 0xB0, 0x20), Color32::from_rgb(0xFF, 0x4D, 0x4F)];
const AUDIO_EXT: [&str; 7] = ["wav", "flac", "ogg", "mp3", "m4a", "aac", "caf"];
const NOTE_NAMES: [&str; 12] = ["Do", "Do#", "Re", "Re#", "Mi", "Fa", "Fa#", "Sol", "Sol#", "La", "La#", "Si"];

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Carpeta por defecto de los proyectos: ~/Documentos/Quantum DAW (fuera del código fuente).
/// Fecha y hora local legibles («08/10/2026 19:45»).
fn now_text() -> String {
    std::process::Command::new("date").arg("+%d/%m/%Y %H:%M").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

/// Nombre del usuario del sistema (nombre completo si está configurado).
fn user_name() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let full = std::process::Command::new("getent").args(["passwd", &user]).output().ok().and_then(|o| String::from_utf8_lossy(&o.stdout).split(':').nth(4).map(|s| s.split(',').next().unwrap_or("").trim().to_string()));
    full.filter(|s| !s.is_empty()).unwrap_or(user)
}

fn projects_dir() -> PathBuf {
    let docs = ["Documentos", "Documents"].iter().map(|d| home().join(d)).find(|d| d.is_dir()).unwrap_or_else(home);
    docs.join("Quantum DAW")
}

/// Preferencias del usuario en ~/.config/quantum-daw/config.json.
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct Config {
    recent: Vec<PathBuf>,
    square: bool,
    show_hdmi: bool,
    midi_port: String,
    /// Altura del panel inferior (mixer, piano roll, analizador).
    bottom_h: f32,
    /// Atajos personalizados: comando → "Ctrl+Shift+S"; rueda: acción → modificadores.
    keys: HashMap<String, String>,
    wheel: HashMap<String, String>,
    /// Escala de la interfaz (0 = 1.0).
    ui_scale: f32,
    /// Biblioteca: orden de las secciones, secciones/categorías plegadas y previsualización manual.
    lib_order: Vec<String>,
    /// Secciones y categorías desplegadas (todo empieza cerrado).
    lib_open: Vec<String>,
    preview_manual: bool,
    hide_welcome: bool,
    /// Idioma de la interfaz (0 español, 1 inglés, 2 portugués).
    lang: u8,
    /// Ancho de la columna de cabeceras de pista (0 = por defecto).
    header_w: f32,
    /// Nombre de quien crea los proyectos (se recuerda).
    author: String,
    /// Plugins del sistema ocultados de la biblioteca.
    hidden_plugins: Vec<PathBuf>,
}

impl Config {
    fn path() -> PathBuf {
        home().join(".config/quantum-daw/config.json")
    }
    fn load() -> Self {
        fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }
    fn save(&self) {
        let _ = fs::create_dir_all(Self::path().parent().unwrap());
        let _ = fs::write(Self::path(), serde_json::to_string_pretty(self).unwrap_or_default());
    }
}

fn main() -> eframe::Result {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("Quantum DAW {}", welcome::VERSION);
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("Uso: quantum-daw [CARPETA_DEL_PROYECTO]\n  --version  muestra la versión\n  --help     muestra esta ayuda");
            return Ok(());
        }
        _ => {}
    }
    let config = Config::load();
    let welcome = std::env::args().nth(1).is_none() && !config.hide_welcome;
    let dir = std::env::args().nth(1).map(PathBuf::from).or_else(|| config.recent.iter().find(|d| d.join(project::FILE).exists()).cloned()).unwrap_or_else(|| projects_dir().join("Demo"));
    widgets::ROUND.store(!config.square, Relaxed);
    i18n::set(config.lang);
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1500.0, 940.0]).with_app_id("quantum-daw").with_title("Quantum DAW");
    if let Some(icon) = logo_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    // glow (OpenGL): con wgpu el redibujado se detenía en Wayland durante la reproducción.
    eframe::run_native(
        "Quantum DAW",
        options,
        Box::new(move |cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let mut app = App::new(dir, config, &cc.egui_ctx)?;
            app.welcome = welcome.then(Default::default);
            Ok(Box::new(app))
        }),
    )
}

/// Icono de la ventana: el logo SVG de Quantum rasterizado a 256 px.
fn logo_icon() -> Option<egui::IconData> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(include_bytes!("../../../assets/quantumlogo.svg"), &usvg::Options::default()).ok()?;
    let mut pixmap = tiny_skia::Pixmap::new(256, 256)?;
    let scale = 256.0 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(egui::IconData { rgba: pixmap.data().to_vec(), width: 256, height: 256 })
}

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}
fn rgb3(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}
fn safe(name: &str) -> String {
    name.replace(|c: char| !c.is_alphanumeric() && c != '-', "_")
}
fn stamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

/// Identificador estable de pista (los envíos apuntan a ids, no a posiciones).
fn new_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Relaxed)
}

fn clip_state(c: &Clip, sr: f64) -> ClipState {
    ClipState {
        file: c.buf().map(|b| b.file.clone()).unwrap_or_default(),
        start: c.start as f64 / sr,
        offset: c.offset as f64 / sr,
        len: c.len as f64 / sr,
        fade_in: c.fade_in as f64 / sr,
        fade_out: c.fade_out as f64 / sr,
        gain_db: widgets::db(c.gain),
        notes: c.notes().map(|n| n.iter().map(|n| (n.start as f64 / sr, n.len as f64 / sr, n.key, n.vel)).collect()).unwrap_or_default(),
    }
}

/// Carga una región guardada (decodificando su audio una sola vez gracias a `cache`).
fn load_clip(dir: &Path, rate: u32, c: &ClipState, midi: bool, cache: &mut HashMap<String, Arc<AudioBuf>>, missing: &mut Vec<String>) -> Option<Clip> {
    let f = |s: f64| (s * rate as f64) as u64;
    let src = if midi {
        Source::Midi(Arc::new(c.notes.iter().map(|&(s, l, key, vel)| Note { start: f(s), len: f(l), key, vel }).collect()))
    } else {
        let buf = match cache.get(&c.file) {
            Some(b) => b.clone(),
            None => match engine::decode(&dir.join(&c.file), rate) {
                Ok(frames) => cache.entry(c.file.clone()).or_insert(AudioBuf::new(c.file.clone(), frames)).clone(),
                Err(err) => {
                    missing.push(format!("{} ({err})", c.file));
                    return None;
                }
            },
        };
        Source::Audio(buf)
    };
    let (offset, mut len) = (f(c.offset), f(c.len));
    if let Source::Audio(b) = &src {
        len = len.min((b.frames.len() as u64).saturating_sub(offset));
    }
    let mut clip = Clip::new(src, f(c.start), len);
    (clip.offset, clip.fade_in, clip.fade_out, clip.gain) = (offset, f(c.fade_in), f(c.fade_out), 10f32.powf(c.gain_db / 20.0));
    Some(clip)
}

/// Reemplaza el tramo `[a, b)` del comp por la toma `k`.
fn set_segment(comp: &[(u64, u64, usize)], a: u64, b: u64, k: usize) -> Vec<(u64, u64, usize)> {
    let mut out: Vec<_> =
        comp.iter().flat_map(|&(s, e, l)| if e <= a || s >= b { vec![(s, e, l)] } else { [(s, a.max(s), l), (b.min(e), e, l)].into_iter().filter(|x| x.0 < x.1).collect() }).collect();
    out.push((a, b, k));
    out.sort_by_key(|x| x.0);
    out
}

/// Regiones resultantes del comp: cada tramo toma el material de su toma, con fundido en los cortes.
fn comp_clips(takes: &[Vec<Clip>], comp: &[(u64, u64, usize)], xf: u64) -> Vec<Clip> {
    comp.iter()
        .flat_map(|&(a, b, k)| {
            takes.get(k).into_iter().flatten().filter_map(move |c| {
                let mut t = c.trim(a, b)?;
                if t.start == a && a > c.start {
                    t.fade_in = t.fade_in.max(xf).min(t.len / 2);
                }
                if t.end() == b && b < c.end() {
                    t.fade_out = t.fade_out.max(xf).min(t.len / 2);
                }
                Some(t)
            })
        })
        .collect()
}

/// Botón del canal por índice: 0 = grabar, 1 = mute, 2 = solo, 3 = monitoreo de entrada.
fn flag(p: &Params, k: usize) -> &AtomicBool {
    [&p.arm, &p.mute, &p.solo, &p.monitor][k]
}

#[derive(Clone)]
struct Track {
    name: String,
    color: Color32,
    kind: TrackKind,
    group: Option<usize>,
    params: Arc<Params>,
    clips: Vec<Clip>,
    fx: Vec<Arc<Fx>>,
    /// Instrumento (solo pistas MIDI): sintetizador y, si se eligió, un instrumento real (sampler).
    synth: Option<Arc<Synth>>,
    sampler: Option<Arc<Sampler>>,
    /// Id del catálogo de instrumentos o "synth:<preset>".
    instrument: String,
    selected: bool,
    /// Primer canal de entrada de la interfaz.
    input: u16,
    vol_auto: Vec<(u64, f32)>,
    pan_auto: Vec<(u64, f32)>,
    /// Curva visible/editable: 0 = volumen, 1 = panorama.
    auto_param: u8,
    /// Altura del carril en el timeline.
    height: f32,
    id: u64,
    /// Icono elegido ("" = automático según nombre/instrumento).
    icon: String,
    /// Pista de video: fotogramas y posición de inicio (frames).
    video: Option<Arc<video::VideoData>>,
    video_start: u64,
    /// Envíos post-fader (id de la pista destino, ganancia) y salida al master.
    sends: Vec<(u64, f32)>,
    to_master: bool,
    /// Tomas grabadas (comping) y tramos del comp: (inicio, fin, toma).
    takes: Vec<Vec<Clip>>,
    comp: Vec<(u64, u64, usize)>,
    show_takes: bool,
    /// Automatización: modo (0 Read, 1 Off, 2 Touch, 3 Latch, 4 Write), fader o panorama tocados en
    /// este cuadro, Latch enganchado y última posición escrita.
    auto_mode: u8,
    touching: bool,
    latched: bool,
    write_last: Option<u64>,
}

pub const TRACK_H: f32 = 100.0;

impl Track {
    fn new(name: String, kind: TrackKind, color: Color32) -> Self {
        let synth = (kind == TrackKind::Midi).then(|| Arc::new(Synth::default()));
        Self {
            name,
            color,
            kind,
            group: None,
            params: Default::default(),
            clips: vec![],
            fx: vec![],
            synth,
            sampler: None,
            instrument: String::new(),
            selected: false,
            input: 0,
            vol_auto: vec![],
            pan_auto: vec![],
            auto_param: 0,
            height: TRACK_H,
            id: new_id(),
            icon: String::new(),
            video: None,
            video_start: 0,
            sends: vec![],
            to_master: true,
            takes: vec![],
            comp: vec![],
            show_takes: false,
            auto_mode: 0,
            touching: false,
            latched: false,
            write_last: None,
        }
    }
    fn auto_mut(&mut self) -> &mut Vec<(u64, f32)> {
        if self.auto_param == 0 { &mut self.vol_auto } else { &mut self.pan_auto }
    }
    fn midi(&self) -> bool {
        self.kind == TrackKind::Midi
    }
}

/// Pista, región, muestras en edición y última muestra tocada por el lápiz.
type SampleEdit = (usize, usize, Vec<[f32; 2]>, Option<usize>);
type Automation = Vec<(u64, f32)>;
/// Carga de un instrumento real en segundo plano.
type SamplerLoad = std::sync::mpsc::Receiver<Result<Arc<Sampler>, String>>;

#[derive(Clone)]
struct Group {
    name: String,
    color: Color32,
    /// Edición vinculada: seleccionar regiones selecciona también las del resto del grupo.
    edit: bool,
}

/// Estado editable del proyecto; se clona para deshacer/rehacer.
#[derive(Clone, Default)]
struct Session {
    tracks: Vec<Track>,
    groups: Vec<Group>,
    /// Mapa de tempo (frame, BPM); el primer tramo empieza en 0.
    tempo: Vec<(u64, f32)>,
    /// Marcas de la línea de tiempo (frame, nombre), ordenadas.
    markers: Vec<(u64, String)>,
    /// Acordes y secciones del arreglo: (inicio, fin, nombre), ordenados.
    chords: Vec<(u64, u64, String)>,
    sections: Vec<(u64, u64, String)>,
}

/// Arrastre de un acorde o sección: tipo, índice, modo (0 mover, 1 borde izq., 2 borde der.), agarre y rango original.
type SpanDrag = (NameKind, usize, u8, i64, (u64, u64));

/// Qué se renombra en el diálogo de nombre.
#[derive(Clone, Copy, PartialEq)]
enum NameKind {
    Marker,
    Chord,
    Section,
    Group,
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Select,
    Pencil,
    Blade,
}

/// Operaciones del menú contextual sobre las regiones seleccionadas.
#[derive(Clone, Copy, PartialEq)]
enum RegionOp {
    Normalize,
    Reverse,
    Stretch(f64),
    Transpose(f32),
    ClearFades,
    Duplicate,
    Quantize,
}

enum Dialog {
    /// Nombre, tipo, cantidad de pistas a crear y color (None = automático).
    NewTrack(String, TrackKind, u32, Option<Color32>),
    /// Configuración, bits de grabación, dispositivos, puertos MIDI y puerto elegido.
    Settings(AudioConfig, u16, Vec<DeviceInfo>, Vec<String>, String),
    /// Formato, rango (0 proyecto, 1 loop, 2 selección), origen (0 mezcla, 1 una pista, 2 stems) y pista.
    Export(Format, u8, u8, usize),
    Groups,
    /// Editor de atajos (con la captura en curso).
    Keys(Option<keys::Capture>),
    /// Estirar (porcentaje de duración) o transponer (semitonos) las regiones seleccionadas.
    Stretch(f32),
    Transpose(f32),
    /// Tempo de un tramo: inicio, fin, BPM y si se adapta (estira) el audio y el MIDI.
    Tempo(u64, u64, f32, bool),
    /// Nombre de una marca, acorde, sección o grupo (índice y texto en edición).
    Name(NameKind, usize, String),
}

struct App {
    ctx: egui::Context,
    config: Config,
    engine: Arc<Engine>,
    streams: Vec<cpal::Stream>,
    midi_in: MidiConnections,
    /// El grafo anterior se libera aquí y no en el hilo de audio.
    old_graph: Option<Arc<Vec<Node>>>,
    dir: PathBuf,
    s: Session,
    undo: Vec<Session>,
    redo: Vec<Session>,
    dirty: bool,
    clipboard: Vec<(usize, Clip)>,
    time_sel: Option<(u64, u64)>,
    tool: Tool,
    snap: bool,
    show_auto: bool,
    /// Panel inferior (pestañas: 0 mixer, 1 piano roll, 2 analizador).
    show_mixer: bool,
    bottom_tab: u8,
    mixer_float: bool,
    show_lib: bool,
    lib_float: bool,
    /// Biblioteca: lugares/discos, árbol de carpetas y previsualización.
    places: Vec<(String, PathBuf)>,
    tree_cache: HashMap<PathBuf, Vec<PathBuf>>,
    tree_open: HashSet<PathBuf>,
    preview_file: Option<PathBuf>,
    preview_rx: Option<std::sync::mpsc::Receiver<Result<Vec<[f32; 2]>, String>>>,
    preview_data: Option<Arc<Vec<[f32; 2]>>>,
    preview_len: usize,
    /// Video: importaciones en curso, fotogramas en caché y visor.
    video_jobs: Vec<std::sync::mpsc::Receiver<Result<video::Imported, String>>>,
    video_cache: HashMap<(u64, u64), egui::TextureHandle>,
    show_video: bool,
    welcome: Option<welcome::Welcome>,
    /// Clic derecho en el carril de tempo: posición y marca cercana.
    tempo_ctx: Option<(u64, Option<usize>)>,
    /// Menú de marcas (frame, marca bajo el puntero) y marca que se arrastra.
    mark_ctx: Option<(u64, Option<usize>)>,
    mark_drag: Option<usize>,
    /// Ancho de cada fila de la barra de control (para centrarlas).
    bar_widths: Vec<f32>,
    bar_two_rows: bool,
    /// Editor de audio del panel inferior.
    aedit: Option<audioedit::AudioEdit>,
    /// Pista de video que muestra el visor.
    video_track: Option<u64>,
    /// Arrastre en los carriles de acordes/arreglo: tipo, índice, modo (0 mover, 1 borde izq., 2 borde der.), agarre y original.
    span_drag: Option<SpanDrag>,
    span_ctx: Option<(NameKind, u64, Option<usize>)>,
    show_chords: bool,
    show_arrange: bool,
    /// Contador de tempo: instantes de los toques y BPM estimado.
    taps: Vec<f64>,
    tempo_guess: Option<f32>,
    /// Pestaña visible de la biblioteca (índice en el orden guardado).
    lib_tab: usize,
    /// Datos de la canción y derechos de autor del proyecto abierto.
    meta: project::SongMeta,
    plugins: Vec<(String, &'static str, PathBuf)>,
    /// Desinstalación pendiente de confirmar: nombre, carpeta o archivo e instrumento (si lo es).
    uninstall: Option<(String, PathBuf, Option<&'static str>)>,
    /// Zoom en píxeles por segundo y desplazamiento del timeline (horizontal en f64 para zoom profundo).
    pps: f32,
    scroll: egui::Vec2,
    sx: f64,
    /// Pistas que abarca la selección de tiempo (razor); `None` = según la selección de pistas.
    razor_lanes: Option<(usize, usize)>,
    /// Edición de muestras con el lápiz: pista, región, muestras en edición y última muestra tocada.
    sample_edit: Option<SampleEdit>,
    route_window: Option<usize>,
    /// Alto de un canal del mixer sin contar el fader (se mide para que el mixer llene el panel).
    strip_extra: f32,
    strip_measured: f32,
    /// Alto del master sin su fader (cuadro anterior), para igualar su altura con la de las pistas.
    master_extra: f32,
    /// Fundido entre tramos de tomas (ms).
    xfade_ms: f32,
    /// Instrumentos reales: cargas en curso (pista, receptor), descargas y asignaciones pendientes.
    loading: Vec<(u64, SamplerLoad)>,
    downloads: HashMap<&'static str, Arc<std::sync::Mutex<String>>>,
    pending_inst: Vec<(u64, &'static str)>,
    /// Alcance del estirado manual: 0 región, 1 pista completa, 2 grupo.
    stretch_scope: u8,
    /// Última pista seleccionada (para Shift+clic por rango).
    last_track: Option<usize>,
    drag: Option<timeline::Drag>,
    /// Región bajo el último clic derecho (menú contextual).
    ctx_region: Option<(usize, usize)>,
    /// Pista y posición del último clic derecho en el timeline.
    ctx_lane: Option<(usize, u64)>,
    /// Último punto dibujado con el lápiz (pista, frame).
    pencil: Option<(usize, u64)>,
    /// Reordenar pistas arrastrando: (origen, destino).
    track_drag: Option<(usize, usize)>,
    strip_drag: Option<(usize, usize)>,
    strip_rects: Vec<egui::Rect>,
    /// Pistas a eliminar al terminar el cuadro (nunca mientras se dibujan).
    pending_delete: Vec<usize>,
    roll: pianoroll::Roll,
    analyzer: analyzer::Analyzer,
    /// Medidores con caída (pistas + master), picos retenidos y entradas.
    levels: Vec<f32>,
    peaks: Vec<f32>,
    in_levels: Vec<f32>,
    rec_data: Vec<f32>,
    rec_midi: Vec<(u64, MidiEvent)>,
    dialog: Option<Dialog>,
    fx_window: Option<usize>,
    audio: AudioConfig,
    /// Dispositivos de audio conocidos (para mostrar nombres legibles).
    devices: Vec<DeviceInfo>,
    rec_bits: u16,
    status: String,
}

impl App {
    fn new(dir: PathBuf, config: Config, ctx: &egui::Context) -> anyhow::Result<Self> {
        theme(ctx);
        ctx.set_zoom_factor(if config.ui_scale > 0.0 { config.ui_scale } else { 1.0 });
        let p = Project::open_or_demo(&dir)?;
        let (engine, streams) = Engine::start(&audio_config(&p))?;
        let mut app = Self {
            ctx: ctx.clone(),
            midi_in: engine::connect_midi(&config.midi_port, engine.midi_tx.clone()),
            config,
            engine,
            streams,
            old_graph: None,
            dir,
            s: Session::default(),
            undo: vec![],
            redo: vec![],
            dirty: true,
            clipboard: vec![],
            time_sel: None,
            tool: Tool::Select,
            snap: true,
            show_auto: false,
            show_mixer: true,
            bottom_tab: 0,
            mixer_float: false,
            show_lib: false,
            lib_float: false,
            places: vec![],
            tree_cache: HashMap::new(),
            tree_open: HashSet::new(),
            preview_file: None,
            preview_rx: None,
            preview_data: None,
            preview_len: 0,
            video_jobs: vec![],
            video_cache: HashMap::new(),
            show_video: true,
            welcome: None,
            lib_tab: 0,
            tempo_ctx: None,
            mark_ctx: None,
            mark_drag: None,
            bar_widths: vec![],
            bar_two_rows: false,
            aedit: None,
            video_track: None,
            span_drag: None,
            span_ctx: None,
            show_chords: true,
            show_arrange: true,
            taps: vec![],
            tempo_guess: None,
            meta: Default::default(),
            plugins: library::scan_plugins(),
            uninstall: None,
            pps: 60.0,
            scroll: egui::Vec2::ZERO,
            sx: 0.0,
            razor_lanes: None,
            sample_edit: None,
            route_window: None,
            strip_extra: 330.0,
            strip_measured: 0.0,
            master_extra: 0.0,
            xfade_ms: 10.0,
            loading: vec![],
            downloads: HashMap::new(),
            pending_inst: vec![],
            stretch_scope: 0,
            last_track: None,
            drag: None,
            ctx_region: None,
            ctx_lane: None,
            pencil: None,
            track_drag: None,
            strip_drag: None,
            strip_rects: vec![],
            pending_delete: vec![],
            roll: Default::default(),
            analyzer: Default::default(),
            levels: vec![],
            peaks: vec![],
            in_levels: vec![],
            rec_data: vec![],
            rec_midi: vec![],
            dialog: None,
            fx_window: None,
            audio: AudioConfig::default(),
            devices: engine::audio_devices(),
            rec_bits: 24,
            status: String::new(),
        };
        app.apply_project(p);
        Ok(app)
    }

    fn sr(&self) -> f64 {
        self.engine.sample_rate as f64
    }
    fn pos(&self) -> u64 {
        self.engine.pos.load(Relaxed)
    }
    fn playing(&self) -> bool {
        self.engine.playing.load(Relaxed)
    }
    fn recording(&self) -> bool {
        self.engine.recording.load(Relaxed)
    }
    fn project_end(&self) -> u64 {
        self.s.tracks.iter().flat_map(|t| &t.clips).map(|c| c.end()).max().unwrap_or(0)
    }
    fn bar_frames(&self) -> u64 {
        (self.engine.beat_frames() * self.engine.beats.load(Relaxed).max(1) as f64) as u64
    }
    /// Tiempos (beats) en el frame `f` según el mapa de tempo, y su inversa.
    fn beats(&self, f: f64) -> f64 {
        engine::beats_at(&self.s.tempo, self.sr(), self.engine.unit.load(Relaxed), f)
    }
    fn frame_of(&self, b: f64) -> f64 {
        engine::frame_at_beat(&self.s.tempo, self.sr(), self.engine.unit.load(Relaxed), b)
    }

    fn report<T>(&mut self, ok: impl Into<String>, r: anyhow::Result<T>) {
        self.status = r.map_or_else(|e| format!("Error: {e}"), |_| ok.into());
    }

    // ---------- Proyecto ----------

    fn apply_project(&mut self, p: Project) {
        let (e, sr) = (self.engine.clone(), self.sr());
        e.bpm.set(p.bpm);
        e.beats.store(p.signature.0, Relaxed);
        e.unit.store(p.signature.1, Relaxed);
        e.metronome.store(p.metronome, Relaxed);
        e.looping.store(p.looping, Relaxed);
        e.loop_start.store((p.loop_secs.0 * sr) as u64, Relaxed);
        e.loop_end.store((p.loop_secs.1 * sr) as u64, Relaxed);
        (self.audio, self.rec_bits) = (audio_config(&p), p.rec_bits);
        let (mut cache, mut missing) = (HashMap::<String, Arc<AudioBuf>>::new(), vec![]);
        let f = |s: f64| (s * sr) as u64;
        let frames = |v: &[(f64, f32)]| v.iter().map(|&(t, x)| (f(t), x)).collect();
        let mut tracks = vec![];
        for ts in &p.tracks {
            let mut t = Track::new(ts.name.clone(), ts.kind, rgb(ts.color));
            let pr = &t.params;
            pr.gain.set(ts.gain);
            pr.pan.set(ts.pan);
            pr.mute.store(ts.mute, Relaxed);
            pr.invert.store(ts.invert, Relaxed);
            t.auto_mode = ts.auto_mode.min(4);
            pr.solo.store(ts.solo, Relaxed);
            pr.monitor.store(ts.monitor, Relaxed);
            pr.in_gain.set(10f32.powf(ts.in_gain_db / 20.0));
            let midi = ts.kind == TrackKind::Midi;
            let mut load = |cs: &[ClipState]| cs.iter().filter_map(|c| load_clip(&self.dir, e.sample_rate, c, midi || c.file.is_empty(), &mut cache, &mut missing)).collect::<Vec<_>>();
            t.clips = load(&ts.clips);
            t.takes = ts.takes.iter().map(|take| load(take)).collect();
            t.comp = ts.comp.iter().map(|&(a, b, k)| (f(a), if b >= 1e300 { u64::MAX } else { f(b) }, k)).collect();
            (t.to_master, t.show_takes) = (!ts.no_master, ts.show_takes);
            t.fx = ts
                .fx
                .iter()
                .filter_map(|f| {
                    let fx = Fx::new(FxKind::from_name(&f.kind)?, e.sample_rate);
                    fx.params.iter().zip(&f.params).for_each(|(a, v)| a.set(*v));
                    fx.bypass.store(f.bypass, Relaxed);
                    Some(Arc::new(fx))
                })
                .collect();
            if let Some(s) = &t.synth {
                s.params.iter().zip(&ts.synth).for_each(|(a, v)| a.set(*v));
            }
            (t.instrument, t.icon) = (ts.instrument.clone(), ts.icon.clone());
            if ts.synth.is_empty()
                && let (Some(s), Some(name)) = (&t.synth, ts.instrument.strip_prefix("synth:"))
                && let Some((_, v)) = pianoroll::PRESETS.iter().find(|p| p.0 == name)
            {
                s.params.iter().zip(v).for_each(|(a, v)| a.set(*v));
            }
            if ts.kind == TrackKind::Video && !ts.video.is_empty() {
                match video::open(&self.dir, &ts.video) {
                    Ok(v) => t.video = Some(Arc::new(v)),
                    Err(err) => missing.push(format!("{} ({err})", ts.video)),
                }
                t.video_start = f(ts.video_start);
                // Proyectos anteriores: el video sin regiones pasa a ser una región en su inicio.
                if t.clips.is_empty()
                    && let Some(v) = &t.video
                {
                    t.clips.push(video::region(None, t.video_start, (v.duration() * sr) as u64));
                }
            }
            (t.group, t.input) = (ts.group.filter(|g| *g < p.groups.len()), ts.input);
            t.height = if ts.height > 0.0 { ts.height } else { TRACK_H };
            (t.vol_auto, t.pan_auto) = (frames(&ts.vol_auto), frames(&ts.pan_auto));
            tracks.push(t);
        }
        // Los envíos se guardan por posición de pista y en memoria se refieren por id.
        let ids: Vec<u64> = tracks.iter().map(|t| t.id).collect();
        for (t, ts) in tracks.iter_mut().zip(&p.tracks) {
            t.sends = ts.sends.iter().filter_map(|&(j, g)| Some((*ids.get(j)?, g))).collect();
        }
        self.xfade_ms = p.comp_xfade_ms;
        // Los instrumentos reales se cargan en segundo plano (o se descargan si faltan).
        self.loading.clear();
        self.pending_inst = tracks.iter().filter_map(|t| Some((t.id, instruments::find(&t.instrument)?.id))).collect();
        let groups = p.groups.iter().map(|g| Group { name: g.name.clone(), color: rgb(g.color), edit: g.edit }).collect();
        let tempo = match p.tempo_map.is_empty() {
            true => vec![(0, p.bpm)],
            false => p.tempo_map.iter().map(|&(s, b)| (f(s), b)).collect(),
        };
        let markers = p.markers.iter().map(|(s, n)| (f(*s), n.clone())).collect();
        let spans = |v: &[(f64, f64, String)]| v.iter().map(|(a, b, n)| (f(*a), f(*b), n.clone())).collect();
        let (chords, sections) = (spans(&p.chords), spans(&p.sections));
        self.s = Session { tracks, groups, tempo, markers, chords, sections };
        self.pin_video();
        self.meta = p.meta.clone();
        (self.undo, self.redo, self.dirty, self.time_sel, self.fx_window, self.route_window) = (vec![], vec![], true, None, None, None);
        self.roll.close();
        let name = self.dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!("Quantum DAW — {name}")));
        self.config.recent.retain(|d| d != &self.dir);
        self.config.recent.insert(0, self.dir.clone());
        self.config.recent.truncate(10);
        self.config.save();
        self.status = match (missing.is_empty(), &self.engine.input_error) {
            (false, _) => format!("Faltan archivos: {}", missing.join(", ")),
            (_, Some(err)) => format!("Proyecto «{name}» cargado · sin entrada de audio: {err}"),
            _ => format!("Proyecto «{name}» cargado · {}", self.dir.display()),
        };
    }

    fn to_project(&self) -> Project {
        let (e, sr) = (&self.engine, self.sr());
        let secs = |v: &[(u64, f32)]| v.iter().map(|&(f, x)| (f as f64 / sr, x)).collect();
        let index = |id: u64| self.s.tracks.iter().position(|t| t.id == id);
        let tracks = self.s.tracks.iter().map(|t| {
            let mut ts = TrackState::new(t.name.clone(), t.kind, rgb3(t.color));
            ts.sends = t.sends.iter().filter_map(|&(id, g)| Some((index(id)?, g))).collect();
            (ts.no_master, ts.show_takes, ts.invert) = (!t.to_master, t.show_takes, t.params.invert.load(Relaxed));
            ts.auto_mode = t.auto_mode;
            ts.takes = t.takes.iter().map(|take| take.iter().map(|c| clip_state(c, sr)).collect()).collect();
            ts.comp = t.comp.iter().map(|&(a, b, k)| (a as f64 / sr, if b == u64::MAX { f64::MAX } else { b as f64 / sr }, k)).collect();
            let p = &t.params;
            (ts.group, ts.gain, ts.pan, ts.input) = (t.group, p.gain.get(), p.pan.get(), t.input);
            (ts.mute, ts.solo, ts.monitor) = (p.mute.load(Relaxed), p.solo.load(Relaxed), p.monitor.load(Relaxed));
            ts.in_gain_db = widgets::db(p.in_gain.get());
            ts.clips = t.clips.iter().map(|c| clip_state(c, sr)).collect();
            ts.fx = t.fx.iter().map(|f| FxState { kind: f.kind.name().into(), params: f.params.iter().map(|p| p.get()).collect(), bypass: f.bypass.load(Relaxed) }).collect();
            ts.synth = t.synth.as_ref().map(|s| s.params.iter().map(|p| p.get()).collect()).unwrap_or_default();
            (ts.instrument, ts.icon) = (t.instrument.clone(), t.icon.clone());
            if let Some(v) = &t.video {
                (ts.video, ts.video_start) = (v.file.clone(), t.video_start as f64 / sr);
            }
            (ts.vol_auto, ts.pan_auto) = (secs(&t.vol_auto), secs(&t.pan_auto));
            ts.height = t.height;
            ts
        });
        Project {
            output_device: self.audio.output.clone(),
            input_device: self.audio.input.clone(),
            sample_rate: self.audio.rate,
            buffer: self.audio.buffer,
            rec_bits: self.rec_bits,
            bpm: self.s.tempo.first().map_or(e.bpm.get(), |t| t.1),
            tempo_map: self.s.tempo.iter().map(|&(f, b)| (f as f64 / sr, b)).collect(),
            signature: (e.beats.load(Relaxed), e.unit.load(Relaxed)),
            metronome: e.metronome.load(Relaxed),
            looping: e.looping.load(Relaxed),
            loop_secs: (e.loop_start.load(Relaxed) as f64 / sr, e.loop_end.load(Relaxed) as f64 / sr),
            groups: self.s.groups.iter().map(|g| GroupState { name: g.name.clone(), color: rgb3(g.color), edit: g.edit }).collect(),
            tracks: tracks.collect(),
            comp_xfade_ms: self.xfade_ms,
            meta: self.meta.clone(),
            markers: self.s.markers.iter().map(|(f, n)| (*f as f64 / sr, n.clone())).collect(),
            chords: self.s.chords.iter().map(|(a, b, n)| (*a as f64 / sr, *b as f64 / sr, n.clone())).collect(),
            sections: self.s.sections.iter().map(|(a, b, n)| (*a as f64 / sr, *b as f64 / sr, n.clone())).collect(),
            ..Default::default()
        }
    }

    /// Reinicia el motor (dispositivos, frecuencia, buffer o proyecto nuevo) y carga `p`.
    fn reload(&mut self, dir: PathBuf, p: Project) -> anyhow::Result<()> {
        self.stop();
        (self.streams, self.old_graph, self.midi_in) = (vec![], None, vec![]);
        let (engine, streams) = Engine::start(&audio_config(&p))?;
        self.midi_in = engine::connect_midi(&self.config.midi_port, engine.midi_tx.clone());
        (self.engine, self.streams, self.dir) = (engine, streams, dir);
        self.apply_project(p);
        Ok(())
    }

    fn save(&mut self) {
        self.meta.saved_at = now_text();
        let r = self.to_project().save(&self.dir);
        self.report(format!("Proyecto guardado en {}", self.dir.display()), r);
    }

    fn save_as(&mut self) -> anyhow::Result<()> {
        let dialog = rfd::FileDialog::new().set_title("Guardar proyecto en carpeta").set_directory(projects_dir());
        let Some(dir) = dialog.pick_folder() else {
            return Ok(());
        };
        for b in self.s.tracks.iter().flat_map(|t| &t.clips).filter_map(|c| c.buf()) {
            let to = dir.join(&b.file);
            if !to.exists() {
                fs::create_dir_all(to.parent().unwrap())?;
                fs::copy(self.dir.join(&b.file), to)?;
            }
        }
        self.dir = dir;
        self.to_project().save(&self.dir)?;
        self.apply_project(self.to_project());
        Ok(())
    }

    fn open_dir(&mut self, dir: PathBuf) -> anyhow::Result<()> {
        let p = if dir.join(project::FILE).exists() { Project::load(&dir)? } else { Project::default() };
        p.save(&dir)?;
        self.reload(dir, p)
    }

    fn open_dialog(&mut self, new: bool) -> anyhow::Result<()> {
        let title = if new { "Elige o crea la carpeta del nuevo proyecto" } else { "Abrir carpeta de proyecto" };
        let _ = fs::create_dir_all(projects_dir());
        let Some(dir) = rfd::FileDialog::new().set_title(title).set_directory(projects_dir()).pick_folder() else {
            return Ok(());
        };
        self.open_dir(dir)
    }

    fn import(&mut self, paths: Vec<PathBuf>) {
        let at = self.pos();
        for src in paths {
            let r = self.import_one(&src, at, None);
            self.report(format!("Importado: {}", src.display()), r);
        }
    }

    /// Copia el archivo a "Audio Files/" (sin sobrescribir) y lo coloca en `into` o en una pista nueva.
    fn import_one(&mut self, src: &Path, at: u64, into: Option<usize>) -> anyhow::Result<()> {
        let frames = engine::decode(src, self.engine.sample_rate)?;
        let (stem, ext) = (src.file_stem().unwrap_or_default().to_string_lossy(), src.extension().unwrap_or_default().to_string_lossy());
        let audio_dir = self.dir.join(AUDIO_DIR);
        fs::create_dir_all(&audio_dir)?;
        let mut name = format!("{stem}.{ext}");
        if src.parent() != Some(audio_dir.as_path()) {
            let mut n = 1;
            while audio_dir.join(&name).exists() {
                (name, n) = (format!("{stem}-{n}.{ext}"), n + 1);
            }
            fs::copy(src, audio_dir.join(&name))?;
        }
        let i = match into.filter(|&i| self.s.tracks.get(i).is_some_and(|t| !t.midi())) {
            Some(i) => {
                self.edit();
                i
            }
            None => {
                self.add_track(stem.into(), TrackKind::AudioStereo);
                self.s.tracks.len() - 1
            }
        };
        self.s.tracks[i].clips.push(Clip::audio(AudioBuf::new(format!("{AUDIO_DIR}/{name}"), frames), at));
        Ok(())
    }

    /// Guarda audio procesado como archivo nuevo del proyecto (las ediciones nunca tocan el original).
    fn write_audio(&self, base: &str, tag: &str, frames: Vec<[f32; 2]>) -> anyhow::Result<Arc<AudioBuf>> {
        let stem = Path::new(base).file_stem().unwrap_or_default().to_string_lossy().to_string();
        let file = format!("{AUDIO_DIR}/{}-{tag}-{}.wav", safe(&stem), stamp());
        let samples: Vec<f32> = frames.iter().flatten().copied().collect();
        project::write_wav(&self.dir.join(&file), self.engine.sample_rate, 2, 32, &samples)?;
        Ok(AudioBuf::new(file, frames))
    }

    fn add_fx(&mut self, i: usize, kind: FxKind) {
        self.edit();
        self.s.tracks[i].fx.push(Arc::new(Fx::new(kind, self.engine.sample_rate)));
        self.fx_window = Some(i);
    }

    fn export_range(&self, range: u8) -> (u64, u64) {
        let e = &self.engine;
        match range {
            1 => (e.loop_start.load(Relaxed), e.loop_end.load(Relaxed)),
            2 => self.time_sel.unwrap_or_default(),
            _ => (0, self.project_end()),
        }
    }

    fn write_export(&self, path: &Path, format: Format, (a, b): (u64, u64), only: Option<&Arc<Params>>) -> anyhow::Result<()> {
        let (mut samples, mut rate) = (self.engine.bounce(a, b, only), self.engine.sample_rate);
        if format == Format::Mp3 && rate > 48_000 {
            let frames = samples.chunks(2).map(|c| [c[0], c[1]]).collect();
            (samples, rate) = (engine::resample(frames, rate, 48_000).into_iter().flatten().collect(), 48_000);
        }
        project::export(path, format, rate, &samples)
    }

    /// Exporta la mezcla, una sola pista o cada pista por separado (stems).
    fn export(&mut self, format: Format, range: u8, source: u8, track: usize) -> anyhow::Result<String> {
        let r = self.export_range(range);
        anyhow::ensure!(r.1 > r.0, "el rango de exportación está vacío");
        self.stop();
        let ext = format.ext();
        if source == 2 {
            let dir = rfd::FileDialog::new().set_title("Carpeta para los stems").set_directory(self.dir.join(RENDER_DIR)).pick_folder();
            let dir = dir.ok_or_else(|| anyhow::anyhow!("exportación cancelada"))?;
            for t in &self.s.tracks {
                self.write_export(&dir.join(format!("{}.{ext}", safe(&t.name))), format, r, Some(&t.params))?;
            }
            return Ok(format!("{} stems exportados en {}", self.s.tracks.len(), dir.display()));
        }
        let only = (source == 1).then(|| self.s.tracks.get(track).map(|t| t.params.clone())).flatten();
        let name = if only.is_some() { safe(&self.s.tracks[track].name) } else { "mix".into() };
        let dialog = rfd::FileDialog::new().set_directory(self.dir.join(RENDER_DIR)).set_file_name(format!("{name}.{ext}"));
        let path = dialog.save_file().ok_or_else(|| anyhow::anyhow!("exportación cancelada"))?;
        self.write_export(&path, format, r, only.as_ref())?;
        Ok(format!("Exportado: {}", path.display()))
    }

    // ---------- Edición ----------

    /// Guarda un punto de deshacer antes de un cambio estructural.
    fn edit(&mut self) {
        self.undo.push(self.s.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        (self.redo, self.dirty) = (vec![], true);
    }

    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.s, s));
            self.dirty = true;
        }
    }

    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.s, s));
            self.dirty = true;
        }
    }

    /// Publica el estado actual al hilo de audio.
    /// Publica el estado actual al hilo de audio, en orden de proceso (orígenes antes que sus buses).
    fn sync(&mut self) {
        let ich = self.engine.in_channels;
        let order = self.process_order();
        let slot = |id: u64| order.iter().position(|&i| self.s.tracks[i].id == id);
        let graph = order.iter().map(|&i| {
            let t = &self.s.tracks[i];
            let l = t.input as usize;
            let r = if t.kind == TrackKind::AudioStereo { (l + 1).min(ich.saturating_sub(1)) } else { l };
            let audio_in = matches!(t.kind, TrackKind::AudioMono | TrackKind::AudioStereo);
            Node {
                params: t.params.clone(),
                clips: t.clips.clone(),
                fx: t.fx.clone(),
                synth: t.synth.clone(),
                sampler: t.sampler.clone(),
                input: (audio_in && l < ich).then_some((l, r)),
                vol_auto: t.vol_auto.clone(),
                pan_auto: t.pan_auto.clone(),
                sends: t.sends.iter().filter_map(|&(id, g)| Some((slot(id)?, g))).collect(),
                to_master: t.to_master,
                bus: self.s.tracks.iter().any(|o| o.sends.iter().any(|s| s.0 == t.id)),
                click: t.kind == TrackKind::Click,
                recv: Node::recv_buffer(),
            }
        });
        self.old_graph = Some(self.engine.graph.swap(Arc::new(graph.collect())));
        self.engine.tempo.store(Arc::new(self.s.tempo.clone()));
        self.dirty = false;
    }

    /// Asigna un instrumento real del catálogo a una pista MIDI (lo descarga si hace falta).
    fn set_instrument(&mut self, i: usize, id: &'static str) {
        let t = &mut self.s.tracks[i];
        (t.instrument, t.sampler) = (id.to_string(), None);
        self.pending_inst.retain(|p| p.0 != t.id);
        self.pending_inst.push((t.id, id));
        self.dirty = true;
    }

    /// Avanza descargas y cargas de instrumentos; cuando un sampler está listo se asigna a su pista.
    fn poll_instruments(&mut self) {
        let rate = self.engine.sample_rate;
        for (track, id) in std::mem::take(&mut self.pending_inst) {
            if instruments::installed(id) {
                self.loading.push((track, instruments::load(id, rate)));
                self.status = format!("Cargando {}…", instruments::find(id).map_or(id, |e| e.name));
            } else {
                let status = self.downloads.entry(id).or_insert_with(|| {
                    let s = Arc::new(std::sync::Mutex::new(String::new()));
                    instruments::install(instruments::find(id).unwrap(), s.clone());
                    s
                });
                let st = status.lock().map(|s| s.clone()).unwrap_or_default();
                if st.starts_with("Error") {
                    self.status = format!("{}: {st}", instruments::find(id).map_or(id, |e| e.name));
                    self.downloads.remove(id);
                } else {
                    self.pending_inst.push((track, id));
                }
            }
        }
        let mut done = vec![];
        for (k, (track, rx)) in self.loading.iter().enumerate() {
            if let Ok(r) = rx.try_recv() {
                done.push(k);
                match (r, self.s.tracks.iter_mut().find(|t| t.id == *track)) {
                    (Ok(s), Some(t)) => {
                        self.status = format!("Instrumento listo: {} en «{}»", instruments::find(&t.instrument).map_or("", |e| e.name), t.name);
                        t.sampler = Some(s);
                        self.dirty = true;
                    }
                    (Err(e), _) => self.status = format!("No se pudo cargar el instrumento: {e}"),
                    _ => {}
                }
            }
        }
        done.into_iter().rev().for_each(|k| _ = self.loading.remove(k));
    }

    fn xfade(&self) -> u64 {
        (self.xfade_ms as f64 / 1000.0 * self.sr()) as u64
    }

    /// Elige la toma `k` para el tramo `[a, b)` de la pista `i` y recalcula sus regiones.
    fn comp_take(&mut self, i: usize, a: u64, b: u64, k: usize) {
        let xf = self.xfade();
        let t = &mut self.s.tracks[i];
        t.comp = set_segment(&t.comp, a, b, k);
        t.clips = comp_clips(&t.takes, &t.comp, xf);
        self.dirty = true;
    }

    fn deselect_clips(&mut self) {
        self.s.tracks.iter_mut().flat_map(|t| &mut t.clips).for_each(|c| c.selected = false);
    }

    fn select_all(&mut self) {
        self.s.tracks.iter_mut().flat_map(|t| &mut t.clips).for_each(|c| c.selected = true);
    }

    /// Extrae la selección: el rango de tiempo (en pistas seleccionadas o todas) o los clips seleccionados.
    fn extract(&mut self, remove: bool) -> Vec<(usize, Clip)> {
        let any_track = self.s.tracks.iter().any(|t| t.selected);
        let razor = self.razor_lanes;
        let mut taken = vec![];
        for (i, t) in self.s.tracks.iter_mut().enumerate() {
            let in_razor = razor.map_or(!any_track || t.selected, |(l0, l1)| (l0..=l1).contains(&i));
            match self.time_sel {
                Some((a, b)) if in_razor => {
                    taken.extend(t.clips.iter().filter_map(|c| c.trim(a, b)).map(|c| (i, c)));
                    if remove {
                        t.clips = t.clips.iter().flat_map(|c| [c.trim(0, a), c.trim(b, u64::MAX)]).flatten().collect();
                        // Razor: también se borran los puntos de automatización del rango.
                        t.vol_auto.retain(|p| p.0 < a || p.0 >= b);
                        t.pan_auto.retain(|p| p.0 < a || p.0 >= b);
                    }
                }
                Some(_) => {}
                None => {
                    taken.extend(t.clips.iter().filter(|c| c.selected).map(|c| (i, c.clone())));
                    if remove {
                        t.clips.retain(|c| !c.selected);
                    }
                }
            }
        }
        taken
    }

    fn copy(&mut self, cut: bool) {
        if cut {
            self.edit();
        }
        self.clipboard = self.extract(cut);
        // egui solo emite el evento de pegar si el portapapeles del sistema tiene texto.
        self.ctx.copy_text("quantum-daw:clips".into());
        let n = self.clipboard.len();
        self.status = if n == 0 { "Nada seleccionado: haz clic en una región o arrastra una selección de tiempo".into() } else { format!("{n} región(es) en el portapapeles") };
    }

    fn delete(&mut self) {
        self.edit();
        self.extract(true);
    }

    /// Pega en el cursor, en las pistas de origen, y mueve el cursor al final.
    fn paste(&mut self) {
        let Some(base) = self.clipboard.iter().map(|c| c.1.start).min() else {
            return;
        };
        self.edit();
        self.deselect_clips();
        let (pos, mut end) = (self.pos(), self.pos());
        for (i, c) in self.clipboard.clone() {
            if let Some(t) = self.s.tracks.get_mut(i) {
                let c = Clip { start: pos + c.start - base, selected: true, ..c };
                end = end.max(c.end());
                t.clips.push(c);
            }
        }
        self.engine.pos.store(end, Relaxed);
    }

    /// Razor: mueve el contenido del rango (regiones y automatización) `delta` frames en las mismas pistas.
    fn move_razor(&mut self, delta: i64) {
        let Some((a, b)) = self.time_sel else { return };
        self.edit();
        let lanes = self.razor_lanes;
        let shift = |f: u64| (f as i64 + delta).max(0) as u64;
        let autos: Vec<(usize, Automation, Automation)> = self
            .s
            .tracks
            .iter()
            .enumerate()
            .filter(|(i, t)| lanes.map_or(t.selected || !self.s.tracks.iter().any(|t| t.selected), |(l0, l1)| (l0..=l1).contains(i)))
            .map(|(i, t)| (i, t.vol_auto.iter().filter(|p| p.0 >= a && p.0 < b).copied().collect(), t.pan_auto.iter().filter(|p| p.0 >= a && p.0 < b).copied().collect()))
            .collect();
        let taken = self.extract(true);
        for (i, mut c) in taken {
            c.start = shift(c.start);
            self.s.tracks[i].clips.push(c);
        }
        for (i, vol, pan) in autos {
            let t = &mut self.s.tracks[i];
            for (dst, pts) in [(&mut t.vol_auto, vol), (&mut t.pan_auto, pan)] {
                dst.extend(pts.into_iter().map(|(f, v)| (shift(f), v)));
                dst.sort_by_key(|p| p.0);
            }
        }
        self.time_sel = Some((shift(a), shift(b)));
    }

    /// Edición vinculada: al seleccionar una región, se seleccionan las que coinciden en el tiempo en su grupo.
    fn select_linked(&mut self, i: usize, j: usize) {
        let Some(g) = self.s.tracks[i].group.filter(|&g| self.s.groups.get(g).is_some_and(|g| g.edit)) else {
            return;
        };
        let (a, b) = (self.s.tracks[i].clips[j].start, self.s.tracks[i].clips[j].end());
        for t in self.s.tracks.iter_mut().filter(|t| t.group == Some(g)) {
            t.clips.iter_mut().filter(|c| c.start < b && c.end() > a).for_each(|c| c.selected = true);
        }
    }

    /// Divide en `at` los clips seleccionados; si no hay, los de las pistas seleccionadas (o todas).
    fn split(&mut self, at: u64) {
        let any_clip = self.s.tracks.iter().flat_map(|t| &t.clips).any(|c| c.selected);
        let any_track = self.s.tracks.iter().any(|t| t.selected);
        self.edit();
        for t in self.s.tracks.iter_mut() {
            let track_on = !any_track || t.selected;
            let parts = t.clips.iter().flat_map(|c| if (any_clip && c.selected) || (!any_clip && track_on) { [c.trim(0, at), c.trim(at, u64::MAX)] } else { [Some(c.clone()), None] });
            t.clips = parts.flatten().collect();
        }
    }

    /// Aplica una operación a las regiones seleccionadas. El audio procesado se guarda como archivo nuevo.
    fn region_op(&mut self, op: RegionOp) -> anyhow::Result<()> {
        self.edit();
        self.region_op_no_undo(op)
    }

    /// Cambia el tempo del tramo `[a, b)`. Si `adapt`, el audio y el MIDI de todas las pistas en
    /// ese tramo se estiran al nuevo tempo y lo que viene después se desplaza; si no, solo cambia
    /// la rejilla (y el metrónomo) en ese tramo.
    fn apply_tempo(&mut self, a: u64, b: u64, bpm: f32, adapt: bool) -> anyhow::Result<()> {
        anyhow::ensure!(b > a, "el tramo de tempo está vacío");
        self.edit();
        let (sr, unit) = (self.sr(), self.engine.unit.load(Relaxed));
        let old = self.s.tempo.clone();
        let beats = engine::beats_at(&old, sr, unit, b as f64) - engine::beats_at(&old, sr, unit, a as f64);
        let after = engine::bpm_at(&old, b as f64);
        let new_len = if adapt { (beats * engine::beat_len(sr, bpm, unit)) as u64 } else { b - a };
        let delta = new_len as i64 - (b - a) as i64;
        let shift = |f: u64| (f as i64 + delta).max(0) as u64;
        let mut map: Vec<(u64, f32)> = old.iter().copied().filter(|s| s.0 < a).collect();
        map.push((a, bpm));
        if !old.iter().any(|s| s.0 == b) {
            map.push((shift(b), after));
        }
        map.extend(old.iter().filter(|s| s.0 >= b).map(|&(f, t)| (shift(f), t)));
        map.dedup_by_key(|s| s.0);
        self.s.tempo = map;
        if adapt && delta != 0 {
            let factor = new_len as f64 / (b - a) as f64;
            let scale = |f: u64| {
                if f < a {
                    f
                } else if f < b {
                    a + ((f - a) as f64 * factor) as u64
                } else {
                    shift(f)
                }
            };
            for t in self.s.tracks.iter_mut() {
                t.clips = t.clips.iter().flat_map(|c| [c.trim(0, a), c.trim(a, b), c.trim(b, u64::MAX)]).flatten().collect();
                t.clips.iter_mut().for_each(|c| c.selected = c.start >= a && c.start < b);
                t.clips.iter_mut().filter(|c| c.start >= b).for_each(|c| c.start = shift(c.start));
                for pts in [&mut t.vol_auto, &mut t.pan_auto] {
                    pts.iter_mut().for_each(|p| p.0 = scale(p.0));
                }
                t.video_start = scale(t.video_start);
            }
            if self.s.tracks.iter().flat_map(|t| &t.clips).any(|c| c.selected) {
                self.region_op_no_undo(RegionOp::Stretch(factor))?;
                for c in self.s.tracks.iter_mut().flat_map(|t| &mut t.clips).filter(|c| c.selected) {
                    c.start = a + ((c.start - a) as f64 * factor) as u64;
                }
            }
            for at in [&self.engine.loop_start, &self.engine.loop_end] {
                at.store(scale(at.load(Relaxed)), Relaxed);
            }
        }
        self.dirty = true;
        Ok(())
    }

    /// Añade una marca en `at` con un nombre numerado y abre el diálogo para nombrarla.
    fn add_marker(&mut self, at: u64) {
        self.edit();
        let name = format!("{} {}", tr("Marca"), self.s.markers.len() + 1);
        self.s.markers.push((at, name.clone()));
        self.s.markers.sort_by_key(|m| m.0);
        let k = self.s.markers.iter().position(|m| m.0 == at && m.1 == name).unwrap_or(0);
        self.dialog = Some(Dialog::Name(NameKind::Marker, k, name));
    }

    /// Añade un acorde o una sección en `[a, b)` y abre el diálogo para nombrarlo.
    fn add_span(&mut self, kind: NameKind, a: u64, b: u64) {
        self.edit();
        let (list, name) = match kind {
            NameKind::Chord => (&mut self.s.chords, "C".to_string()),
            _ => (&mut self.s.sections, tr("Estrofa").to_string()),
        };
        list.push((a, b, name.clone()));
        list.sort_by_key(|s| s.0);
        let k = list.iter().position(|s| s.0 == a).unwrap_or(0);
        self.dialog = Some(Dialog::Name(kind, k, name));
    }

    /// Ancho de la columna de cabeceras (ajustable arrastrando el divisor).
    fn header_w(&self) -> f32 {
        if self.config.header_w > 0.0 { self.config.header_w.clamp(timeline::HEADER_MIN, timeline::HEADER_MAX) } else { timeline::HEADER_W }
    }

    /// Las pistas de video van siempre arriba de todo (en su orden).
    fn pin_video(&mut self) {
        self.s.tracks.sort_by_key(|t| t.kind != TrackKind::Video);
    }

    /// Cambia el BPM del tramo de tempo donde está el cursor (desde el LCD).
    fn set_tempo_here(&mut self, bpm: f32) {
        let pos = self.pos();
        if let Some(seg) = self.s.tempo.iter_mut().rev().find(|s| s.0 <= pos) {
            seg.1 = bpm;
        }
        self.dirty = true;
    }

    fn region_op_no_undo(&mut self, op: RegionOp) -> anyhow::Result<()> {
        let grid = self.engine.beat_frames() / 4.0;
        let targets: Vec<(usize, usize)> = self.s.tracks.iter().enumerate().flat_map(|(i, t)| t.clips.iter().enumerate().filter(|c| c.1.selected).map(move |(j, _)| (i, j))).collect();
        anyhow::ensure!(!targets.is_empty(), "selecciona una o más regiones");
        let mut added = vec![];
        for &(i, j) in &targets {
            let c = self.s.tracks[i].clips[j].clone();
            let new = match (&c.src, op) {
                (_, RegionOp::Duplicate) => {
                    added.push((i, Clip { start: c.end(), selected: false, ..c.clone() }));
                    continue;
                }
                (_, RegionOp::ClearFades) => Clip { fade_in: 0, fade_out: 0, ..c },
                (Source::Audio(b), _) => {
                    let part = &b.frames[c.offset as usize..(c.offset + c.len) as usize];
                    let (tag, frames) = match op {
                        RegionOp::Normalize => {
                            let g = 0.966 / engine::peak(part).max(1e-6);
                            self.s.tracks[i].clips[j].gain = g;
                            continue;
                        }
                        RegionOp::Reverse => ("invertido", part.iter().rev().copied().collect()),
                        RegionOp::Stretch(f) => ("estirado", engine::stretch(part, f)),
                        RegionOp::Transpose(st) => ("transpuesto", engine::pitch_shift(part, st)),
                        _ => continue,
                    };
                    let buf = self.write_audio(&b.file, tag, frames)?;
                    let scale = buf.frames.len() as f64 / c.len.max(1) as f64;
                    let fade = |f: u64| (f as f64 * scale) as u64;
                    Clip { fade_in: fade(c.fade_in), fade_out: fade(c.fade_out), gain: c.gain, selected: true, ..Clip::audio(buf, c.start) }
                }
                (Source::Midi(notes), _) => {
                    let notes: Vec<Note> = notes
                        .iter()
                        .map(|n| match op {
                            RegionOp::Stretch(f) => Note { start: (n.start as f64 * f) as u64, len: (n.len as f64 * f) as u64, ..*n },
                            RegionOp::Transpose(st) => Note { key: (n.key as i32 + st.round() as i32).clamp(0, 127) as u8, ..*n },
                            RegionOp::Quantize => Note { start: ((n.start as f64 / grid).round() * grid) as u64, ..*n },
                            _ => *n,
                        })
                        .collect();
                    let len = if let RegionOp::Stretch(f) = op { (c.len as f64 * f) as u64 } else { c.len };
                    let offset = if let RegionOp::Stretch(f) = op { (c.offset as f64 * f) as u64 } else { c.offset };
                    Clip { src: Source::Midi(Arc::new(notes)), len, offset, ..c }
                }
            };
            self.s.tracks[i].clips[j] = new;
        }
        added.into_iter().for_each(|(i, c)| self.s.tracks[i].clips.push(c));
        Ok(())
    }

    /// Crea una región MIDI vacía de 4 compases y la abre en el piano roll.
    fn new_midi_region(&mut self, i: usize, at: u64) {
        self.edit();
        let clip = Clip::new(Source::Midi(Arc::new(vec![])), at, self.bar_frames() * 4);
        self.s.tracks[i].clips.push(clip);
        self.open_roll(i, self.s.tracks[i].clips.len() - 1);
    }

    fn open_roll(&mut self, i: usize, j: usize) {
        self.roll.open(i, j);
        (self.show_mixer, self.bottom_tab) = (true, 1);
        // El teclado del piano roll y los controladores MIDI suenan en esta pista.
        self.s.tracks[i].params.monitor.store(true, Relaxed);
    }

    /// Lápiz: escribe un punto y borra los que quedan entre el trazo anterior y este.
    fn draw_point(&mut self, i: usize, f: u64, v: f32) {
        let (a, b) = match self.pencil {
            Some((li, lf)) if li == i => (lf.min(f), lf.max(f)),
            _ => (f, f),
        };
        let pts = self.s.tracks[i].auto_mut();
        pts.retain(|p| !(p.0 > a && p.0 < b) && p.0 != f);
        let k = pts.partition_point(|p| p.0 < f);
        pts.insert(k, (f, v));
        (self.pencil, self.dirty) = (Some((i, f)), true);
    }

    // ---------- Pistas y grupos ----------

    fn add_track(&mut self, name: String, kind: TrackKind) {
        self.edit();
        self.s.tracks.iter_mut().for_each(|t| t.selected = false);
        let mut t = Track::new(name, kind, rgb(PALETTE[self.s.tracks.len() % PALETTE.len()]));
        t.selected = true;
        self.s.tracks.push(t);
    }

    fn duplicate_track(&mut self, i: usize) {
        self.edit();
        let mut t = self.s.tracks[i].clone();
        let p = Params::default();
        (1..3).for_each(|k| flag(&p, k).store(flag(&t.params, k).load(Relaxed), Relaxed));
        p.gain.set(t.params.gain.get());
        p.pan.set(t.params.pan.get());
        p.in_gain.set(t.params.in_gain.get());
        t.params = Arc::new(p);
        let sr = self.engine.sample_rate;
        t.fx =
            t.fx.iter()
                .map(|f| {
                    let n = Fx::new(f.kind, sr);
                    n.params.iter().zip(&f.params).for_each(|(a, b)| a.set(b.get()));
                    Arc::new(n)
                })
                .collect();
        t.synth = t.synth.as_ref().map(|s| {
            let n = Synth::default();
            n.params.iter().zip(&s.params).for_each(|(a, b)| a.set(b.get()));
            Arc::new(n)
        });
        (t.id, t.takes, t.comp) = (new_id(), vec![], vec![]);
        t.name += " (copia)";
        self.s.tracks.insert(i + 1, t);
    }

    /// Marca pistas para eliminar; se borran en `apply_deletes`, al final del cuadro.
    fn delete_tracks(&mut self, which: impl Fn(usize, &Track) -> bool) {
        self.pending_delete.extend(self.s.tracks.iter().enumerate().filter(|(i, t)| which(*i, t)).map(|(i, _)| i));
    }

    fn apply_deletes(&mut self) {
        if self.pending_delete.is_empty() {
            return;
        }
        self.edit();
        let del = std::mem::take(&mut self.pending_delete);
        let mut i = 0;
        self.s.tracks.retain(|_| {
            i += 1;
            !del.contains(&(i - 1))
        });
        (self.fx_window, self.ctx_region, self.ctx_lane, self.track_drag, self.strip_drag) = (None, None, None, None, None);
        self.roll.close();
        self.status = format!("{} pista(s) eliminada(s) · Ctrl+Z para deshacer", del.len());
    }

    /// Mueve la pista `from` a la posición `to` (índice de inserción, 0..=n).
    fn move_track(&mut self, from: usize, to: usize) {
        if to == from || to == from + 1 {
            return;
        }
        self.edit();
        let t = self.s.tracks.remove(from);
        self.s.tracks.insert(if to > from { to - 1 } else { to }, t);
        self.pin_video();
        self.fx_window = None;
        self.roll.close();
    }

    fn group_members(&self, i: usize) -> Vec<usize> {
        match self.s.tracks[i].group {
            Some(g) => (0..self.s.tracks.len()).filter(|&j| self.s.tracks[j].group == Some(g)).collect(),
            None => vec![i],
        }
    }

    /// Pistas afectadas por un botón: las seleccionadas si la pista lo está; si no, solo ella
    /// (mute y solo también afectan a su grupo).
    fn flag_targets(&self, i: usize, k: usize) -> Vec<usize> {
        match (self.s.tracks[i].selected, k) {
            (true, _) => (0..self.s.tracks.len()).filter(|&j| self.s.tracks[j].selected).collect(),
            (false, 1 | 2) => self.group_members(i),
            _ => vec![i],
        }
    }

    /// Clic: solo esta pista · Ctrl+clic: añadir/quitar · Shift+clic: rango desde la última seleccionada.
    fn select_track(&mut self, i: usize, m: egui::Modifiers) {
        match (m.shift, self.last_track.filter(|&l| l < self.s.tracks.len())) {
            (true, Some(l)) => (l.min(i)..=l.max(i)).for_each(|k| self.s.tracks[k].selected = true),
            _ if m.command || m.shift => self.s.tracks[i].selected = !self.s.tracks[i].selected,
            _ => {
                self.s.tracks.iter_mut().for_each(|t| t.selected = false);
                self.s.tracks[i].selected = true;
            }
        }
        self.last_track = Some(i);
    }

    /// Estira la región `(i, j)` por `factor` y, según el alcance, también todas las regiones de su
    /// pista o de su grupo (sus posiciones se escalan desde la primera región).
    fn stretch_scoped(&mut self, i: usize, j: usize, factor: f64) -> anyhow::Result<()> {
        let tracks = match self.stretch_scope {
            0 => vec![],
            1 => vec![i],
            _ => self.group_members(i),
        };
        self.deselect_clips();
        if tracks.is_empty() {
            self.s.tracks[i].clips[j].selected = true;
        }
        for &k in &tracks {
            self.s.tracks[k].clips.iter_mut().for_each(|c| c.selected = true);
        }
        let anchor = self.s.tracks.iter().flat_map(|t| &t.clips).filter(|c| c.selected).map(|c| c.start).min().unwrap_or(0);
        self.region_op(RegionOp::Stretch(factor))?;
        for c in self.s.tracks.iter_mut().flat_map(|t| &mut t.clips).filter(|c| c.selected) {
            c.start = anchor + ((c.start - anchor) as f64 * factor) as u64;
        }
        Ok(())
    }

    fn group_selected(&mut self) {
        if !self.s.tracks.iter().any(|t| t.selected) {
            self.status = "Selecciona pistas para agruparlas (Ctrl+clic en las cabeceras)".into();
            return;
        }
        self.edit();
        let g = self.s.groups.len();
        self.s.groups.push(Group { name: format!("Grupo {}", g + 1), color: rgb(PALETTE[(g + 6) % PALETTE.len()]), edit: true });
        self.s.tracks.iter_mut().filter(|t| t.selected).for_each(|t| t.group = Some(g));
    }

    /// Escribe la automatización de volumen y panorama mientras se reproduce: Touch mientras se
    /// mueve el fader o el panorama, Latch desde que se toca hasta parar, Write siempre.
    fn write_automation(&mut self) {
        let (playing, pos) = (self.playing(), self.pos());
        let writing: Vec<bool> = self
            .s
            .tracks
            .iter_mut()
            .map(|t| {
                let touching = std::mem::take(&mut t.touching);
                t.latched = playing && (t.latched || (t.auto_mode == 3 && touching));
                playing
                    && match t.auto_mode {
                        2 => touching,
                        3 => t.latched,
                        4 => true,
                        _ => false,
                    }
            })
            .collect();
        if writing.iter().zip(&self.s.tracks).any(|(w, t)| *w && t.write_last.is_none()) {
            self.edit();
        }
        for (t, &w) in self.s.tracks.iter_mut().zip(&writing) {
            t.params.auto_read.store(t.auto_mode != 1 && !w, Relaxed);
            if !w {
                // Al terminar de escribir, el motor recibe la curva nueva.
                self.dirty |= t.write_last.take().is_some();
                continue;
            }
            let from = t.write_last.unwrap_or(pos);
            let (a, b) = (from.min(pos), from.max(pos));
            for (pts, v) in [(&mut t.vol_auto, engine::fader_pos(t.params.gain.get())), (&mut t.pan_auto, t.params.pan.get())] {
                pts.retain(|p| p.0 < a || p.0 > b);
                let k = pts.partition_point(|p| p.0 < pos);
                pts.insert(k, (pos, v));
            }
            t.write_last = Some(pos);
        }
    }

    fn remove_group(&mut self, g: usize) {
        self.edit();
        self.s.groups.remove(g);
        for t in &mut self.s.tracks {
            t.group = t.group.and_then(|x| if x == g { None } else { Some(x - (x > g) as usize) });
        }
    }

    // ---------- Transporte ----------

    fn toggle_play(&mut self) {
        if self.playing() { self.stop() } else { self.engine.playing.store(true, Relaxed) }
    }

    fn stop(&mut self) {
        self.engine.playing.store(false, Relaxed);
        self.finish_recording();
    }

    fn go(&mut self, frame: u64) {
        self.engine.pos.store(frame, Relaxed)
    }

    fn toggle_loop(&mut self) {
        let e = &self.engine;
        if e.loop_end.load(Relaxed) <= e.loop_start.load(Relaxed) {
            self.loop_all();
        } else {
            e.looping.fetch_xor(true, Relaxed);
        }
    }

    fn loop_all(&mut self) {
        self.set_loop(0, self.project_end())
    }

    fn set_loop(&mut self, a: u64, b: u64) {
        self.engine.loop_start.store(a, Relaxed);
        self.engine.loop_end.store(b, Relaxed);
        self.engine.looping.store(b > a, Relaxed);
    }

    fn drain_recording(&mut self) {
        if let Ok(mut rx) = self.engine.rec_rx.lock() {
            while let Ok(s) = rx.pop() {
                self.rec_data.push(s);
            }
        }
        if let Ok(mut rx) = self.engine.midi_rec_rx.lock() {
            while let Ok(e) = rx.pop() {
                self.rec_midi.push(e);
            }
        }
    }

    fn armed(&self, midi: bool) -> bool {
        self.s.tracks.iter().any(|t| t.params.arm.load(Relaxed) && t.midi() == midi)
    }

    fn toggle_record(&mut self) {
        if self.recording() {
            return self.stop();
        }
        if !self.armed(false) && !self.armed(true) {
            self.status = "Arma una pista con el botón R para grabar".into();
            return;
        }
        if self.armed(false) && self.engine.in_channels == 0 {
            self.status = "No hay entrada de audio: elígela en Herramientas → Configuración de audio".into();
            return;
        }
        self.drain_recording();
        (self.rec_data, self.rec_midi) = (vec![], vec![]);
        self.engine.looping.store(false, Relaxed);
        self.engine.recording.store(true, Relaxed);
        self.engine.playing.store(true, Relaxed);
        self.status = "Grabando…".into();
    }

    /// Crea las tomas: audio (con la ganancia de entrada de cada pista) y notas MIDI en las pistas armadas.
    fn finish_recording(&mut self) {
        if !self.engine.recording.swap(false, Relaxed) {
            return;
        }
        self.drain_recording();
        self.edit();
        let (sr, bits, ich) = (self.engine.sample_rate, self.rec_bits, self.engine.in_channels.max(1));
        let (start, end) = (self.engine.rec_start.load(Relaxed), self.pos());
        let (data, midi) = (std::mem::take(&mut self.rec_data), std::mem::take(&mut self.rec_midi));
        let mut notes = vec![];
        let mut held: HashMap<u8, (u64, u8)> = HashMap::new();
        for &(at, e) in &midi {
            let rel = at.saturating_sub(start);
            match (e[0] & 0xF0, e[2]) {
                (0x90, v) if v > 0 => _ = held.insert(e[1], (rel, v)),
                (0x80 | 0x90, _) => {
                    if let Some((s, v)) = held.remove(&e[1]) {
                        notes.push(Note { start: s, len: (rel - s).max(1), key: e[1], vel: v });
                    }
                }
                _ => {}
            }
        }
        notes.extend(held.into_iter().map(|(key, (s, vel))| Note { start: s, len: end.saturating_sub(start + s).max(1), key, vel }));
        let (mut errors, xf) = (vec![], self.xfade());
        let ts = stamp();
        let mut open_roll = None;
        for (ti, t) in self.s.tracks.iter_mut().enumerate().filter(|(_, t)| t.params.arm.load(Relaxed)) {
            if t.midi() {
                if notes.is_empty() {
                    continue;
                }
                // Si la grabación empieza dentro de una región MIDI, las notas se suman a ella
                // (así aparecen en el piano roll); si no, se crea una región nueva.
                let j = match t.clips.iter().position(|c| c.notes().is_some() && c.start <= start && start < c.end()) {
                    Some(j) => {
                        let c = &mut t.clips[j];
                        let shift = (start as i64 - c.origin()).max(0) as u64;
                        let mut merged = c.notes().map(|n| (**n).clone()).unwrap_or_default();
                        merged.extend(notes.iter().map(|n| Note { start: n.start + shift, ..*n }));
                        c.len = c.len.max(end.saturating_sub(c.start));
                        c.src = Source::Midi(Arc::new(merged));
                        j
                    }
                    None => {
                        t.clips.push(Clip::new(Source::Midi(Arc::new(notes.clone())), start, end.saturating_sub(start).max(1)));
                        t.clips.len() - 1
                    }
                };
                open_roll = Some((ti, j));
                continue;
            }
            let mono = t.kind == TrackKind::AudioMono;
            let (l, g) = ((t.input as usize).min(ich - 1), t.params.in_gain.get());
            let r = if mono { l } else { (l + 1).min(ich - 1) };
            let frames: Vec<[f32; 2]> = data.chunks_exact(ich).map(|c| [c[l] * g, c[r] * g]).collect();
            if frames.is_empty() {
                continue;
            }
            let file = format!("{AUDIO_DIR}/{}-{ts}.wav", safe(&t.name));
            let samples: Vec<f32> = if mono { frames.iter().map(|f| f[0]).collect() } else { frames.iter().flatten().copied().collect() };
            match project::write_wav(&self.dir.join(&file), sr, 2 - mono as u16, bits, &samples) {
                Ok(()) => {
                    let nc = Clip::audio(AudioBuf::new(file, frames), start);
                    // Grabar sobre material existente crea una toma nueva (comping), como en REAPER.
                    if t.takes.is_empty() && !t.clips.iter().any(|c| c.start < nc.end() && c.end() > nc.start) {
                        t.clips.push(nc);
                        continue;
                    }
                    if t.takes.is_empty() {
                        (t.takes, t.comp) = (vec![t.clips.clone()], vec![(0, u64::MAX, 0)]);
                    }
                    t.takes.push(vec![nc.clone()]);
                    t.comp = set_segment(&t.comp, nc.start, nc.end(), t.takes.len() - 1);
                    (t.clips, t.show_takes) = (comp_clips(&t.takes, &t.comp, xf), true);
                }
                Err(e) => errors.push(e.to_string()),
            }
        }
        if let Some((i, j)) = open_roll.filter(|_| self.roll.target.is_none_or(|(ti, _)| ti == open_roll.map_or(0, |o| o.0))) {
            self.roll.open(i, j);
        }
        self.status = if errors.is_empty() { "Grabación guardada".into() } else { format!("Error al guardar la toma: {}", errors.join(", ")) };
    }

    /// Zoom horizontal: desde ver horas de proyecto hasta 60 píxeles por muestra.
    fn zoom(&mut self, factor: f32) {
        self.pps = (self.pps * factor).clamp(2.0, self.engine.sample_rate as f32 * 60.0)
    }

    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        if tool == Tool::Pencil {
            self.show_auto = true;
        }
    }

    /// Actualiza los medidores con caída y retención de picos.
    fn update_meters(&mut self) {
        let n = self.s.tracks.len();
        self.levels.resize(n + 1, 0.0);
        self.peaks.resize(n + 1, 0.0);
        self.in_levels.resize(n, 0.0);
        for i in 0..=n {
            let p = self.s.tracks.get(i).map_or(&self.engine.master, |t| &t.params);
            self.levels[i] = p.meter.take().max(self.levels[i] * 0.9);
            self.peaks[i] = self.peaks[i].max(self.levels[i]);
            if i < n {
                self.in_levels[i] = p.in_meter.take().max(self.in_levels[i] * 0.9);
            }
        }
    }

    /// Panel inferior con pestañas: mixer, piano roll y analizador.
    fn bottom_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (k, label) in [(0, "Mixer"), (1, "Piano roll"), (2, "Analizador"), (3, "Editor de audio")] {
                ui.selectable_value(&mut self.bottom_tab, k, egui::RichText::new(tr(label)).strong());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").on_hover_text(tr("Ocultar panel (X)")).clicked() {
                    self.show_mixer = false;
                }
                if self.bottom_tab == 0 && ui.button(if self.mixer_float { "Acoplar mixer" } else { "Mixer flotante" }).clicked() {
                    self.mixer_float = !self.mixer_float;
                }
            });
        });
        match self.bottom_tab {
            0 if self.mixer_float => _ = ui.label(egui::RichText::new(tr("El mixer está en una ventana flotante.")).color(TEXT_DIM)),
            0 => self.mixer(ui),
            1 => self.piano_roll(ui),
            2 => self.analyzer(ui),
            _ => self.audio_editor(ui),
        }
    }
}

fn audio_config(p: &Project) -> AudioConfig {
    AudioConfig { output: p.output_device.clone(), input: p.input_device.clone(), rate: p.sample_rate, buffer: p.buffer }
}

fn theme(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    (v.panel_fill, v.window_fill, v.extreme_bg_color, v.faint_bg_color) = (PANEL, PANEL, BG, ELEVATED);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT;
    v.widgets.inactive.weak_bg_fill = ELEVATED;
    v.widgets.inactive.bg_fill = BORDER;
    v.widgets.hovered.weak_bg_fill = BORDER;
    v.window_stroke.color = BORDER;
    ctx.set_visuals(v);
    let r = widgets::rr(8.0) as u8;
    ctx.all_styles_mut(|s| {
        let w = &mut s.visuals.widgets;
        for st in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            st.corner_radius = (r - r / 4).into();
        }
        (s.visuals.window_corner_radius, s.visuals.menu_corner_radius) = ((r + 4).into(), r.into());
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        // Las etiquetas no se seleccionan como texto: el puntero sigue siendo la flecha.
        s.interaction.selectable_labels = false;
        // Texto algo más grande que el de egui por defecto, legible en cualquier pantalla.
        use egui::{FontId, TextStyle};
        for (style, size) in [(TextStyle::Body, 13.5), (TextStyle::Button, 13.5), (TextStyle::Small, 10.5), (TextStyle::Heading, 19.0), (TextStyle::Monospace, 13.0)] {
            let family = if style == TextStyle::Monospace { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
            s.text_styles.insert(style, FontId::new(size, family));
        }
    });
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() {
            self.import(dropped);
        }
        if self.recording() {
            self.drain_recording();
        }
        self.update_meters();
        self.poll_instruments();
        self.poll_video();
        // El resto de la app (rejilla del piano roll, cuantizar…) usa el tempo bajo el cursor.
        self.engine.bpm.set(engine::bpm_at(&self.s.tempo, self.pos() as f64));
        if !self.pending_inst.is_empty() || !self.loading.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        self.engine.scope_on.store(self.show_mixer && self.bottom_tab == 2, Relaxed);
        // Franja del logo separada del menú; la barra de control crece según sus filas de cajas.
        egui::Panel::top("title").exact_size(40.0).frame(egui::Frame::new().fill(BG).inner_margin(egui::Margin { left: 14, right: 14, top: 8, bottom: 6 })).show(ui, |ui| self.title_bar(ui));
        egui::Panel::top("menu").frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(10, 4))).show(ui, |ui| self.menu_bar(ui));
        let bar_h = self.bar_rows().len() as f32 * (chrome::BAR_ROW_H + 6.0) + 10.0;
        egui::Panel::top("control").exact_size(bar_h).show(ui, |ui| self.control_bar(ui));
        egui::Panel::bottom("status").exact_size(24.0).show(ui, |ui| self.status_bar(ui));
        if self.show_mixer {
            let default = if self.config.bottom_h > 0.0 { self.config.bottom_h } else { 440.0 };
            let panel = egui::Panel::bottom("bottom").default_size(default).min_size(140.0).resizable(true).show(ui, |ui| self.bottom_panel(ui));
            // Recordar la altura elegida (al soltar el ratón) para la próxima vez.
            let h = panel.response.rect.height();
            if (h - self.config.bottom_h).abs() > 2.0 && !ctx.input(|i| i.pointer.any_down()) {
                self.config.bottom_h = h;
                self.config.save();
            }
        }
        if self.show_lib && !self.lib_float {
            egui::Panel::right("library").default_size(280.0).resizable(true).show(ui, |ui| self.library(ui));
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| self.timeline(ui));
        self.floating_windows(&ctx);
        self.fx_window(&ctx);
        self.route_window(&ctx);
        self.dialogs(&ctx);
        self.uninstall_dialog(&ctx);
        self.video_window(&ctx);
        self.welcome_window(&ctx);
        self.apply_deletes();
        self.write_automation();
        if self.dirty {
            self.sync();
        }
        if self.playing() || self.engine.scope_on.load(Relaxed) { ctx.request_repaint() } else { ctx.request_repaint_after(Duration::from_millis(40)) }
    }
}

#[cfg(test)]
mod tests {
    /// Cada formato de exportación debe poder volver a decodificarse con la duración correcta.
    #[test]
    fn export_roundtrip() {
        let dir = std::env::temp_dir().join("quantum-daw-test");
        std::fs::create_dir_all(&dir).unwrap();
        let samples: Vec<f32> = (0..48_000).flat_map(|i| [(i as f32 * 0.05).sin() * 0.5; 2]).collect();
        for f in project::Format::ALL {
            let path = dir.join(format!("{f:?}.{}", f.ext()));
            project::export(&path, f, 48_000, &samples).unwrap();
            let frames = engine::decode(&path, 48_000).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            assert!((frames.len() as i64 - 48_000).abs() < 3000, "{f:?}: {} frames", frames.len());
        }
    }

    /// Automatización: interpolación lineal y valores constantes fuera de los puntos.
    #[test]
    fn automation_interpolates() {
        let pts = [(100, 0.0), (200, 1.0)];
        assert_eq!(engine::automation_at(&pts, 0), Some(0.0));
        assert_eq!(engine::automation_at(&pts, 150), Some(0.5));
        assert_eq!(engine::automation_at(&pts, 900), Some(1.0));
        assert_eq!(engine::automation_at(&[], 5), None);
        assert!((engine::fader_gain(engine::fader_pos(0.5)) - 0.5).abs() < 1e-4);
    }

    /// Time-stretch y transposición conservan la duración esperada.
    #[test]
    fn stretch_and_transpose_lengths() {
        let tone: Vec<[f32; 2]> = (0..48_000).map(|i| [(i as f32 * 0.06).sin(); 2]).collect();
        assert_eq!(engine::stretch(&tone, 1.5).len(), 72_000);
        assert_eq!(engine::pitch_shift(&tone, 3.0).len(), 48_000);
        assert!(engine::peak(&engine::stretch(&tone, 0.75)) > 0.5);
    }

    /// El sintetizador suena con una nota y QUANTUM Tune detecta un La3 (220 Hz) desafinado.
    #[test]
    fn synth_and_tune() {
        let synth = engine::Synth::default();
        let mut buf = [[0f32; 2]; 1024];
        synth.render(&mut buf, &[(0, [0x90, 60, 100])], 48_000.0, true, 0, false);
        assert!(engine::peak(&buf) > 0.01);
        let tune = engine::Fx::new(engine::FxKind::Tune, 48_000);
        let mut tone: Vec<[f32; 2]> = (0..9600).map(|i| [(i as f32 * std::f32::consts::TAU * 226.0 / 48_000.0).sin() * 0.5; 2]).collect();
        tone.chunks_mut(1024).for_each(|c| tune.process(c, 48_000.0, 120.0, true));
        assert!((tune.readout[0].get() - 57.46).abs() < 0.3, "detectado {}", tune.readout[0].get());
        assert_eq!(tune.readout[1].get(), 57.0);
    }

    /// Comping: elegir un tramo de otra toma divide el comp y aplica fundidos en los cortes.
    #[test]
    fn comping_segments() {
        let comp = super::set_segment(&[(0, u64::MAX, 0)], 100, 200, 1);
        assert_eq!(comp, vec![(0, 100, 0), (100, 200, 1), (200, u64::MAX, 0)]);
        let buf = engine::AudioBuf::new("a".into(), vec![[0.1; 2]; 1000]);
        let takes = vec![vec![engine::Clip::audio(buf.clone(), 0)], vec![engine::Clip::audio(buf, 0)]];
        let clips = super::comp_clips(&takes, &comp, 10);
        assert_eq!(clips.len(), 3);
        assert_eq!((clips[1].start, clips[1].len, clips[1].fade_in, clips[1].fade_out), (100, 100, 10, 10));
        assert_eq!(clips[0].fade_in, 0);
    }
}
