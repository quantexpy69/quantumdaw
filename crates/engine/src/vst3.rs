//! Plugins VST3 de terceros (efectos e instrumentos) con `vst3-host`.
//!
//! - **Catálogo**: se buscan bundles `.vst3` en las rutas estándar y en las carpetas propias, y se
//!   leen sus datos de `moduleinfo.json` sin cargar el plugin (rápido y sin riesgo de que un plugin
//!   roto cierre QUANTUM al buscar). Los que no traen ese archivo se muestran por el nombre del bundle.
//! - **Proceso**: el plugin vive en un `Arc<Mutex<Plugin>>`. El hilo de audio lo toma sin esperar
//!   en cada bloque (si está ocupado, ese bloque sale sin el plugin) y la ventana del plugin lo usa
//!   desde el hilo de la interfaz (ver `PluginWindow` y `servir_ventana`).
//! - **Estado**: el plugin guarda su estado (por ejemplo, la librería que tiene abierta Decent
//!   Sampler) en bytes que van en el proyecto.
use crate::{MidiEvent, lv2::Categoria};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use vst3_host::{AudioBuffers, Plugin, Vst3Host, midi::MidiEvent as Vst3Midi};

pub use vst3_host::PluginWindow;

/// Un plugin del catálogo (una clase de audio de un bundle).
#[derive(Debug)]
pub struct Info {
    pub ruta: PathBuf,
    /// Id de la clase (32 hex) si se conoce por `moduleinfo.json`.
    pub clase: String,
    pub nombre: String,
    pub marca: String,
    pub instrumento: bool,
    pub categoria: Categoria,
}

impl Info {
    /// Lo que se guarda en el proyecto: «vst3:<ruta del bundle>#<clase>».
    pub fn clave(&self) -> String {
        format!("vst3:{}#{}", self.ruta.display(), self.clase)
    }
}

static CARPETAS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static CATALOGO: Mutex<Option<Arc<Vec<Arc<Info>>>>> = Mutex::new(None);

/// Carpetas propias donde también se buscan bundles `.vst3`.
pub fn carpetas(dirs: &[PathBuf]) {
    if let Ok(mut c) = CARPETAS.lock() {
        *c = dirs.to_vec();
    }
}

/// Rutas estándar de VST3 en cada sistema.
fn rutas_estandar() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "linux") {
        vec![home.join(".vst3"), "/usr/lib/vst3".into(), "/usr/local/lib/vst3".into(), "/usr/lib64/vst3".into()]
    } else if cfg!(target_os = "macos") {
        vec![home.join("Library/Audio/Plug-Ins/VST3"), "/Library/Audio/Plug-Ins/VST3".into()]
    } else {
        let comunes = std::env::var_os("CommonProgramFiles").map(PathBuf::from).unwrap_or_else(|| "C:\\Program Files\\Common Files".into());
        vec![comunes.join("VST3"), std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default().join("Programs\\Common\\VST3")]
    }
}

/// Bundles `.vst3` dentro de `dir` (hasta 5 niveles; un bundle no se recorre por dentro).
fn bundles(dir: &Path, nivel: usize, v: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut hijos: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    hijos.sort();
    for p in hijos {
        let nombre = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if nombre.starts_with('.') || nombre == "__MACOSX" {
            continue;
        }
        if nombre.to_ascii_lowercase().ends_with(".vst3") {
            v.push(p);
        } else if p.is_dir() && nivel < 5 {
            bundles(&p, nivel + 1, v);
        }
    }
}

/// Categoría para la carátula y los filtros, según las subcategorías VST3 («Fx|Reverb», «Instrument|Sampler»…).
fn categoria(sub: &str, nombre: &str) -> Categoria {
    let s = format!("{sub} {nombre}").to_lowercase();
    match () {
        _ if s.contains("drum") => Categoria::Bateria,
        _ if s.contains("sampler") => Categoria::Sampler,
        _ if s.contains("organ") => Categoria::Organo,
        _ if s.contains("instrument") || s.contains("synth") => Categoria::Sintetizador,
        _ if s.contains("reverb") => Categoria::Reverb,
        _ if s.contains("delay") => Categoria::Delay,
        _ if s.contains("dynamics") || s.contains("comp") || s.contains("limit") => Categoria::Dinamica,
        _ if s.contains("eq") || s.contains("filter") => Categoria::Eq,
        _ if s.contains("distortion") || s.contains("saturat") => Categoria::Distorsion,
        _ if s.contains("amp") => Categoria::Amplificador,
        _ if s.contains("modulation") || s.contains("chorus") || s.contains("flanger") || s.contains("phaser") => Categoria::Modulacion,
        _ if s.contains("pitch") || s.contains("tune") => Categoria::Afinacion,
        _ if s.contains("analyzer") || s.contains("meter") => Categoria::Analisis,
        _ => Categoria::Utilidad,
    }
}

/// Clases de audio de un bundle según su `moduleinfo.json`; sin él, una entrada por el nombre.
fn leer_bundle(ruta: &Path) -> Vec<Info> {
    let base = ruta.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    match vst3_host::discovery::read_module_info(ruta) {
        Ok(Some(m)) => m
            .classes
            .iter()
            .filter(|c| c.category == "Audio Module Class")
            .map(|c| {
                let sub = c.sub_categories.join("|");
                Info {
                    ruta: ruta.to_path_buf(),
                    clase: c.class_id.clone(),
                    nombre: if c.name.is_empty() { base.clone() } else { c.name.clone() },
                    marca: if c.vendor.is_empty() { m.factory.vendor.clone() } else { c.vendor.clone() },
                    instrumento: sub.to_lowercase().contains("instrument"),
                    categoria: categoria(&sub, &c.name),
                }
            })
            .collect(),
        // Sin datos: por el nombre (se adivina si es instrumento).
        _ => {
            let n = base.to_lowercase();
            let instrumento = INSTRUMENTOS_CONOCIDOS.iter().any(|k| n.contains(k));
            vec![Info { ruta: ruta.to_path_buf(), clase: String::new(), nombre: base.clone(), marca: String::new(), instrumento, categoria: categoria("", &base) }]
        }
    }
}

/// Catálogo de plugins VST3 (se busca la primera vez y se recuerda hasta `recargar`).
pub fn catalogo() -> Arc<Vec<Arc<Info>>> {
    if let Some(c) = CATALOGO.lock().ok().and_then(|c| c.clone()) {
        return c;
    }
    let mut rutas = rutas_estandar();
    rutas.extend(CARPETAS.lock().map(|c| c.clone()).unwrap_or_default());
    let mut encontrados = vec![];
    for r in &rutas {
        bundles(r, 0, &mut encontrados);
    }
    encontrados.sort();
    encontrados.dedup();
    let mut lista: Vec<Arc<Info>> = encontrados.iter().flat_map(|b| leer_bundle(b)).map(Arc::new).collect();
    lista.sort_by_key(|i| i.nombre.to_lowercase());
    let c = Arc::new(lista);
    if let Ok(mut g) = CATALOGO.lock() {
        *g = Some(c.clone());
    }
    c
}

/// Vuelve a buscar los plugins (tras instalar alguno o cambiar las carpetas).
pub fn recargar() {
    if let Ok(mut g) = CATALOGO.lock() {
        *g = None;
    }
    catalogo();
}

/// Plugin del catálogo por su clave («vst3:<ruta>#<clase>»), o armado desde la ruta si ya no está
/// en el catálogo pero el bundle existe.
pub fn info(clave: &str) -> Option<Arc<Info>> {
    let resto = clave.strip_prefix("vst3:")?;
    let (ruta, clase) = resto.rsplit_once('#').unwrap_or((resto, ""));
    if let Some(i) = catalogo().iter().find(|i| i.ruta == Path::new(ruta) && i.clase == clase) {
        return Some(i.clone());
    }
    let ruta = Path::new(ruta);
    ruta.exists().then(|| leer_bundle(ruta).into_iter().find(|i| i.clase == clase).map(Arc::new)).flatten()
}

/// Palabras de nombres de instrumentos, para los VST3 sin moduleinfo.json (un solo archivo .vst3,
/// habitual en Windows). Al abrirlos se confirma con su categoría real (ver `Instancia::nueva`).
const INSTRUMENTOS_CONOCIDOS: [&str; 40] = [
    "synth", "sampler", "piano", "drum", "organ", "instrument", "kontakt", "komplete", "battery", "massive", "fm8", "reaktor", "omnisphere", "keyscape", "trilian",
    "serum", "vital", "pigments", "analog lab", "diva", "repro", "zebra", "hive", "sylenth", "nexus", "spire", "halion", "groove agent", "superior drummer", "ezdrummer",
    "ezkeys", "addictive", "spitfire", "labs", "sampletank", "pianoteq", "dexed", "surge", "phase plant", "sforzando",
];

/// Un plugin abierto.
pub struct Instancia {
    pub info: Arc<Info>,
    pub plugin: Arc<Mutex<Plugin>>,
    bufs: Mutex<AudioBuffers>,
    /// Ya se avisó de un error al atender sus pedidos (para no repetirlo en cada cuadro).
    avisado: std::sync::atomic::AtomicBool,
    /// Bloques que salieron sin el plugin porque estaba ocupado (la ventana lo tenía).
    pub perdidos: std::sync::atomic::AtomicU32,
    /// Eventos MIDI de bloques que salieron sin el plugin: van al principio del siguiente bloque
    /// (perder un note on deja la tecla muda; perder un note off deja la nota colgada).
    pendientes: Mutex<Pendientes>,
    /// Lo que el plugin emite por su salida MIDI (sin bloquear): por ejemplo las teclas que se tocan
    /// en el teclado de su ventana (Kontakt). Se graban en la pista como las del teclado externo.
    salida_midi: Option<vst3_host::OutputMidiConsumer>,
    sample_rate: f32,
}

/// Un evento de la salida MIDI de un plugin en bytes MIDI (notas, controles, rueda de tono y presión).
fn a_bytes(e: Vst3Midi) -> Option<MidiEvent> {
    let c = |ch: vst3_host::midi::MidiChannel| ch.as_index() & 0x0F;
    Some(match e {
        Vst3Midi::NoteOn { channel, note, velocity } => [0x90 | c(channel), note & 0x7F, velocity.clamp(1, 127)],
        Vst3Midi::NoteOff { channel, note, velocity } => [0x80 | c(channel), note & 0x7F, velocity & 0x7F],
        Vst3Midi::ControlChange { channel, controller, value } => [0xB0 | c(channel), controller & 0x7F, value & 0x7F],
        Vst3Midi::PitchBend { channel, value } => [0xE0 | c(channel), (value & 0x7F) as u8, ((value >> 7) & 0x7F) as u8],
        Vst3Midi::ChannelAftertouch { channel, pressure } => [0xD0 | c(channel), pressure & 0x7F, 0],
        Vst3Midi::PolyAftertouch { channel, note, pressure } => [0xA0 | c(channel), note & 0x7F, pressure & 0x7F],
        _ => return None,
    })
}

/// Eventos MIDI guardados sin pedir memoria (se usa en el hilo de audio).
struct Pendientes {
    items: [MidiEvent; 256],
    len: usize,
}

impl Default for Pendientes {
    fn default() -> Self {
        Self { items: [[0; 3]; 256], len: 0 }
    }
}

/// Controles propios de las notas de QUANTUM (ganancia y ataque por nota, para sus instrumentos):
/// no se envían a los plugins, que los tomarían por otros (el CC73 es «tiempo de ataque»).
fn cc_propio(e: &MidiEvent) -> bool {
    e[0] & 0xF0 == 0xB0 && (e[1] == crate::CC_GANANCIA || e[1] == crate::CC_ATAQUE)
}

/// Toma un mutex esperando como mucho hasta `limite` (en tiempo real no se bloquea sin fin: si no se
/// libera a tiempo, el bloque sigue sin el plugin).
fn tomar_antes<T>(m: &Mutex<T>, limite: std::time::Instant) -> Option<std::sync::MutexGuard<'_, T>> {
    let mut vueltas = 0u32;
    loop {
        match m.try_lock() {
            Ok(g) => return Some(g),
            Err(std::sync::TryLockError::Poisoned(p)) => return Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if std::time::Instant::now() >= limite {
            return None;
        }
        vueltas += 1;
        if vueltas % 64 == 0 {
            std::thread::yield_now();
        } else {
            std::hint::spin_loop();
        }
    }
}

/// Bloque más grande que se le pasa al plugin (el del motor).
const BLOQUE: usize = crate::BLOCK;

impl Instancia {
    /// Abre el plugin y lo deja procesando a `sample_rate`.
    pub fn nueva(info: &Arc<Info>, sample_rate: u32) -> anyhow::Result<Self> {
        let mut host = Vst3Host::builder().sample_rate(sample_rate as f64).block_size(BLOQUE).with_process_isolation(false).build()?;
        let mut plugin = if info.clase.is_empty() { host.load_plugin(&info.ruta)? } else { host.load_plugin_class(&info.ruta, &info.clase)? };
        plugin.start_processing()?;
        // Ya abierto, el plugin dice qué es: su categoría real («Instrument|Sampler») y si tiene
        // entrada de audio y de MIDI. Los que no traen moduleinfo.json (Kontakt en Windows, un solo
        // archivo .vst3) se catalogan por el nombre y pueden haberse tomado por efectos.
        let datos = plugin.info().clone();
        let por_categoria = datos.category.to_lowercase().contains("instrument");
        let es_instrumento = info.instrumento || por_categoria || (datos.audio_inputs == 0 && datos.has_midi_input);
        // Con la categoría real, también se corrige el tipo que se adivinó por el nombre.
        let info = if es_instrumento == info.instrumento && !por_categoria {
            info.clone()
        } else {
            let cat = if por_categoria { categoria(&datos.category, &info.nombre) } else { info.categoria };
            Arc::new(Info { ruta: info.ruta.clone(), clase: info.clase.clone(), nombre: info.nombre.clone(), marca: info.marca.clone(), instrumento: es_instrumento, categoria: cat })
        };
        let salidas = plugin.output_channel_count().max(2);
        let entradas = if info.instrumento { 0 } else { 2 };
        let mut bufs = AudioBuffers::new(entradas, salidas, BLOQUE, sample_rate as f64);
        bufs.clear();
        let salida_midi = plugin.output_midi_handle();
        Ok(Self { info, plugin: Arc::new(Mutex::new(plugin)), bufs: Mutex::new(bufs), avisado: Default::default(), perdidos: Default::default(), pendientes: Default::default(), salida_midi, sample_rate: sample_rate as f32 })
    }

    /// Procesa un bloque: los eventos MIDI en su posición y el audio de `buf`. Los efectos
    /// reemplazan `buf`; los instrumentos le suman su sonido. En tiempo real, si la ventana del
    /// plugin lo está usando (al redibujarse, p. ej. el teclado de Decent Sampler al tocar), se
    /// espera hasta medio bloque a que lo suelte; saltar el bloque se oye como un clic.
    pub fn procesar(&self, buf: &mut [[f32; 2]], eventos: &[(usize, MidiEvent)], offline: bool) {
        let n = buf.len().min(BLOQUE);
        if n == 0 {
            return;
        }
        let (plugin, bufs) = if offline {
            (self.plugin.lock().ok(), self.bufs.lock().ok())
        } else {
            let limite = std::time::Instant::now() + std::time::Duration::from_secs_f32(0.5 * n as f32 / self.sample_rate.max(1.0));
            (tomar_antes(&self.plugin, limite), tomar_antes(&self.bufs, limite))
        };
        let (Some(mut plugin), Some(mut b)) = (plugin, bufs) else {
            self.perdidos.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // Las notas de este bloque no se pierden: esperan al siguiente.
            if let Ok(mut p) = self.pendientes.try_lock() {
                for &(_, e) in eventos.iter().filter(|(_, e)| !cc_propio(e)) {
                    if p.len < p.items.len() {
                        let k = p.len;
                        p.items[k] = e;
                        p.len += 1;
                    }
                }
            }
            return;
        };
        if let Ok(mut p) = self.pendientes.try_lock()
            && p.len > 0
        {
            for e in &p.items[..p.len] {
                if let Some(ev) = Vst3Midi::from_midi_bytes(e) {
                    let _ = plugin.send_midi_event_at(ev, 0);
                }
            }
            p.len = 0;
        }
        for &(at, e) in eventos.iter().filter(|(_, e)| !cc_propio(e)) {
            if let Some(ev) = Vst3Midi::from_midi_bytes(&e) {
                let _ = plugin.send_midi_event_at(ev, at.min(n - 1) as i32);
            }
        }
        // La capacidad ya está reservada: cambiar el largo no pide memoria en el hilo de audio.
        for (c, canal) in b.inputs.iter_mut().enumerate() {
            canal.resize(n, 0.0);
            canal.iter_mut().zip(buf.iter()).for_each(|(d, s)| *d = s[c.min(1)]);
        }
        for canal in b.outputs.iter_mut() {
            canal.resize(n, 0.0);
            canal.fill(0.0);
        }
        if plugin.process_audio(&mut b).is_err() {
            return;
        }
        let (l, r) = (&b.outputs[0], b.outputs.get(1).unwrap_or(&b.outputs[0]));
        let instrumento = b.inputs.is_empty();
        for (i, s) in buf.iter_mut().take(n).enumerate() {
            *s = if instrumento { [s[0] + l[i], s[1] + r[i]] } else { [l[i], r[i]] };
        }
    }

    /// Saca lo que el plugin emitió por su salida MIDI desde la última llamada (sin bloquear ni pedir
    /// memoria: se usa en el hilo de audio). Hay que llamarla siempre, grabe o no, para que no se
    /// acumule y aparezca de golpe al empezar a grabar.
    pub fn tomar_salida_midi(&self, mut f: impl FnMut(MidiEvent)) {
        if let Some(s) = &self.salida_midi {
            while let Some(e) = s.pop() {
                if let Some(b) = a_bytes(e) {
                    f(b);
                }
            }
        }
    }

    /// Estado del plugin para guardarlo en el proyecto.
    pub fn estado(&self) -> Option<Vec<u8>> {
        self.plugin.lock().ok()?.save_state().ok()
    }

    /// Restaura el estado guardado.
    pub fn cargar_estado(&self, datos: &[u8]) -> bool {
        !datos.is_empty() && self.plugin.lock().is_ok_and(|mut p| p.load_state(datos).is_ok())
    }

    /// Frecuencia de muestreo con la que se abrió.
    pub fn frecuencia(&self) -> u32 {
        self.sample_rate as u32
    }

    pub fn tiene_ventana(&self) -> bool {
        self.plugin.lock().is_ok_and(|p| p.has_editor())
    }
}

/// Tareas del plugin que van en el hilo que lo abrió (la interfaz), en cada cuadro, haya o no ventana:
/// los pedidos de reinicio (`restartComponent`: al cambiar de preset o de muestras, el plugin pide
/// que se lo detenga y se lo vuelva a activar; si no se atiende queda callado) y sus temporizadores.
/// Sin esperar: si el audio tiene el plugin, se atiende en el cuadro siguiente.
pub fn servir_plugin(inst: &Instancia) {
    if let Ok(mut p) = inst.plugin.try_lock() {
        if let Err(e) = p.service_host_requests()
            && !inst.avisado.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            crate::avisar(format!("{}: {e}", inst.info.nombre));
        }
        p.service_run_loop();
    }
    // Si algún bloque salió sin el plugin, queda en el registro (para diagnosticar clics).
    let perdidos = inst.perdidos.swap(0, std::sync::atomic::Ordering::Relaxed);
    if perdidos > 0 {
        crate::avisar(format!("{}: {perdidos} bloques de audio sin procesar (el plugin estaba ocupado)", inst.info.nombre));
    }
}

/// Windows: entrega los temporizadores (`WM_TIMER`) pendientes del hilo de la interfaz. Llamar en
/// cada cuadro mientras haya plugins VST3, sin tener el plugin tomado.
///
/// Windows solo saca un `WM_TIMER` de la cola cuando no queda ningún otro mensaje ni repintado
/// pendiente, y la interfaz de QUANTUM pide repintar seguido: los temporizadores no llegaban nunca.
/// Los plugins hechos con Qt (Kontakt 8 y otros de Native Instruments) procesan sus eventos con un
/// temporizador cuando corren dentro de otro programa, así que se quedaban congelados: no
/// respondían a los clics ni terminaban de cargar. Pedirlos con el filtro `WM_TIMER` los saca
/// aunque haya repintados pendientes.
pub fn atender_temporizadores() {
    #[cfg(windows)]
    {
        #[repr(C)]
        struct Msg {
            hwnd: isize,
            message: u32,
            wparam: usize,
            lparam: isize,
            time: u32,
            x: i32,
            y: i32,
            private: u32,
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn PeekMessageW(m: *mut Msg, h: isize, min: u32, max: u32, quitar: u32) -> i32;
            fn DispatchMessageW(m: *const Msg) -> isize;
        }
        const WM_TIMER: u32 = 0x0113;
        const PM_REMOVE: u32 = 1;
        let mut m = Msg { hwnd: 0, message: 0, wparam: 0, lparam: 0, time: 0, x: 0, y: 0, private: 0 };
        // Con tope: un temporizador que vence en cada vuelta no debe dejar la interfaz atrapada aquí.
        for _ in 0..64 {
            // SAFETY: `m` es un MSG válido (mismo diseño que el de Win32) y vive durante la llamada.
            if unsafe { PeekMessageW(&mut m, 0, WM_TIMER, WM_TIMER, PM_REMOVE) } == 0 {
                break;
            }
            // SAFETY: el mensaje lo acaba de entregar Windows para una ventana de este hilo.
            unsafe { DispatchMessageW(&m) };
        }
    }
}

/// Atiende una ventana de plugin abierta (llamar en cada cuadro de la interfaz): eventos de la
/// ventana y temporizadores del plugin (Linux), sin esperar si el audio tiene el plugin.
/// Devuelve `false` cuando el usuario la cerró.
pub fn servir_ventana(ventana: &PluginWindow, plugin: &Mutex<Plugin>) -> bool {
    if ventana.closed_by_user() || !ventana.is_open() {
        return false;
    }
    let _ = ventana.service_platform_events();
    if let Ok(mut p) = plugin.try_lock() {
        p.service_run_loop();
    }
    true
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Los CC de ganancia y ataque por nota son de QUANTUM; los del teclado (rueda, expresión,
    /// pedal) siguen yendo al plugin.
    #[test]
    fn controles_propios_no_van_al_plugin() {
        assert!(cc_propio(&[0xB0, crate::CC_GANANCIA, 90]));
        assert!(cc_propio(&[0xB3, crate::CC_ATAQUE, 10]));
        assert!(!cc_propio(&[0xB0, 1, 127]));
        assert!(!cc_propio(&[0xB0, 11, 100]));
        assert!(!cc_propio(&[0xB0, 64, 127]));
        assert!(!cc_propio(&[0x90, crate::CC_GANANCIA, 100]));
    }

    #[test]
    fn categorias_y_clave() {
        assert_eq!(categoria("Instrument|Sampler", "Decent Sampler"), Categoria::Sampler);
        assert_eq!(categoria("Fx|Reverb", "Room"), Categoria::Reverb);
        assert_eq!(categoria("Fx|Dynamics", "Comp"), Categoria::Dinamica);
        let i = Info { ruta: "/x/A.vst3".into(), clase: "ABC".into(), nombre: "A".into(), marca: String::new(), instrumento: false, categoria: Categoria::Utilidad };
        assert_eq!(i.clave(), "vst3:/x/A.vst3#ABC");
    }

    /// Con Decent Sampler instalado en ~/.vst3: aparece como instrumento, se abre, procesa notas sin
    /// fallar y guarda/restaura su estado.
    #[test]
    #[ignore]
    fn decent_sampler_instalado() {
        let cat = catalogo();
        let Some(ds) = cat.iter().find(|i| i.nombre.contains("Decent")) else {
            eprintln!("Decent Sampler no está instalado");
            return;
        };
        println!("{ds:?}");
        assert!(ds.instrumento);
        let inst = Instancia::nueva(ds, 48_000).unwrap();
        let mut buf = vec![[0f32; 2]; 256];
        for b in 0..50 {
            buf.iter_mut().for_each(|s| *s = [0.0; 2]);
            let ev: &[(usize, MidiEvent)] = if b == 0 { &[(10, [0x90, 60, 100])] } else { &[] };
            inst.procesar(&mut buf, ev, true);
            assert!(buf.iter().all(|s| s[0].is_finite()));
        }
        // Los pedidos del plugin se atienden sin error desde el hilo que lo abrió.
        servir_plugin(&inst);
        assert!(!inst.avisado.load(std::sync::atomic::Ordering::Relaxed));
        let estado = inst.estado().unwrap();
        println!("estado: {} bytes · ventana: {}", estado.len(), inst.tiene_ventana());
        assert!(inst.cargar_estado(&estado));
    }

    /// Decent Sampler con su ventana abierta, audio en otro hilo y la ventana atendida como en la
    /// aplicación (cada cuadro), guardando el estado cada pocos segundos. `QDAW_SEGUNDOS` alarga la prueba.
    #[test]
    #[ignore]
    fn decent_sampler_con_ventana_y_audio() {
        let Some(ds) = catalogo().iter().find(|i| i.nombre.contains("Decent")).cloned() else { return };
        let inst = Arc::new(Instancia::nueva(&ds, 48_000).unwrap());
        // QDAW_PRESET: un .dspreset que se carga como estado (el XML del preset, como lo guarda DS).
        if let Ok(ruta) = std::env::var("QDAW_PRESET") {
            let esc = |s: &str| s.replace('&', "&amp;").replace('"', "&quot;");
            let xml = std::fs::read_to_string(&ruta).unwrap();
            let xml = xml.trim_start_matches('\u{feff}');
            let cuerpo = xml.find("<DecentSampler").map_or(xml, |i| &xml[i..]);
            let xml = cuerpo.replacen("<DecentSampler", &format!("<DecentSampler _presetName=\"{0}\" _samplePath=\"{1}\"", esc(&ruta), esc(&std::path::Path::new(&ruta).parent().unwrap().to_string_lossy())), 1);
            let mut blob = b"VC2!".to_vec();
            blob.extend_from_slice(&(xml.len() as u32 + 1).to_le_bytes());
            blob.extend_from_slice(xml.as_bytes());
            blob.push(0);
            println!("preset cargado: {}", inst.cargar_estado(&blob));
        }
        let mut ventana = PluginWindow::new(inst.plugin.clone());
        ventana.open().unwrap();
        let fin = std::time::Instant::now() + std::time::Duration::from_secs(std::env::var("QDAW_SEGUNDOS").ok().and_then(|s| s.parse().ok()).unwrap_or(20));
        let audio = {
            let inst = inst.clone();
            std::thread::spawn(move || {
                let mut buf = vec![[0f32; 2]; 256];
                let mut k = 0u32;
                while std::time::Instant::now() < fin {
                    let ev = [(0usize, if k % 40 == 0 { [0x90, 48 + (k / 40 % 24) as u8, 100] } else { [0x80, 48 + ((k / 40 + 23) % 24) as u8, 0] })];
                    inst.procesar(&mut buf, if k % 20 == 0 { &ev } else { &[] }, false);
                    k += 1;
                    std::thread::sleep(std::time::Duration::from_micros(5300));
                }
            })
        };
        let mut ultimo = std::time::Instant::now();
        while std::time::Instant::now() < fin && servir_ventana(&ventana, &inst.plugin) {
            servir_plugin(&inst);
            if ultimo.elapsed().as_secs() >= 3 {
                ultimo = std::time::Instant::now();
                println!("estado: {} bytes", inst.estado().map_or(0, |e| e.len()));
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        audio.join().unwrap();
        ventana.close();
    }

    /// Kontakt (o el VST3 de `QDAW_VST3`) con su ventana, audio en otro hilo y un bucle de mensajes
    /// Win32 puro, sin la interfaz de QUANTUM, durante 40 s: para comparar su interfaz fuera de la
    /// aplicación. `QDAW_ESTADO`: archivo con un estado guardado del plugin para cargarlo antes.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn ventana_con_bucle_win32() {
        #[repr(C)]
        struct Msg { h: isize, m: u32, w: usize, l: isize, t: u32, x: i32, y: i32, p: u32 }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn PeekMessageW(m: *mut Msg, h: isize, a: u32, b: u32, r: u32) -> i32;
            fn TranslateMessage(m: *const Msg) -> i32;
            fn DispatchMessageW(m: *const Msg) -> isize;
        }
        let nombre = std::env::var("QDAW_VST3").unwrap_or_else(|_| "Kontakt".into());
        let Some(i) = catalogo().iter().find(|i| i.nombre.contains(&nombre)).cloned() else { return };
        let inst = Arc::new(Instancia::nueva(&i, 48_000).unwrap());
        if let Ok(ruta) = std::env::var("QDAW_ESTADO") {
            println!("estado cargado: {}", inst.cargar_estado(&std::fs::read(ruta).unwrap()));
        }
        let mut ventana = PluginWindow::new(inst.plugin.clone());
        ventana.open().unwrap();
        let fin = std::time::Instant::now() + std::time::Duration::from_secs(std::env::var("QDAW_SEGUNDOS").ok().and_then(|s| s.parse().ok()).unwrap_or(40));
        let audio = {
            let inst = inst.clone();
            std::thread::spawn(move || {
                // QDAW_NOTAS: cada 2 s una nota (Do central) de 1 s, y el pico que dio el plugin.
                // QDAW_CC1=<valor>: antes de cada nota, el control 1 (rueda de modulación).
                let notas = std::env::var_os("QDAW_NOTAS").is_some();
                let cc1: Option<u8> = std::env::var("QDAW_CC1").ok().and_then(|v| v.parse().ok());
                let mut buf = vec![[0f32; 2]; 512];
                let (mut k, mut pico) = (0u32, 0f32);
                while std::time::Instant::now() < fin {
                    buf.iter_mut().for_each(|s| *s = [0.0; 2]);
                    let mut ev: Vec<(usize, MidiEvent)> = vec![];
                    if notas && k % 188 == 0 {
                        if let Some(v) = cc1 {
                            ev.push((0, [0xB0, 1, v]));
                        }
                        ev.push((0, [0x90, 60, 100]));
                    }
                    if notas && k % 188 == 94 {
                        ev.push((0, [0x80, 60, 0]));
                    }
                    inst.procesar(&mut buf, &ev, false);
                    pico = buf.iter().fold(pico, |m, s| m.max(s[0].abs()).max(s[1].abs()));
                    if notas && k % 188 == 187 {
                        println!("nota {}: pico {pico:.4}", k / 188);
                        pico = 0.0;
                    }
                    k += 1;
                    std::thread::sleep(std::time::Duration::from_micros(10_600));
                }
            })
        };
        // Lo que el plugin emite por su salida MIDI (p. ej. las teclas de su propio teclado).
        let salida_midi = inst.plugin.lock().unwrap().output_midi_handle();
        println!("salida MIDI del plugin: {}", salida_midi.is_some());
        while std::time::Instant::now() < fin && servir_ventana(&ventana, &inst.plugin) {
            while let Some(e) = salida_midi.as_ref().and_then(|s| s.pop()) {
                println!("salida: {e:?}");
            }
            let mut m = Msg { h: 0, m: 0, w: 0, l: 0, t: 0, x: 0, y: 0, p: 0 };
            // SAFETY: `m` tiene el diseño de MSG y los mensajes son de ventanas de este hilo.
            unsafe {
                while PeekMessageW(&mut m, 0, 0, 0, 1) != 0 {
                    TranslateMessage(&m);
                    DispatchMessageW(&m);
                }
            }
            servir_plugin(&inst);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        audio.join().unwrap();
        ventana.close();
    }
}