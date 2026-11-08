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
            let instrumento = ["synth", "sampler", "piano", "drum", "organ", "instrument"].iter().any(|k| n.contains(k));
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

/// Un plugin abierto.
pub struct Instancia {
    pub info: Arc<Info>,
    pub plugin: Arc<Mutex<Plugin>>,
    bufs: Mutex<AudioBuffers>,
    /// Ya se avisó de un error al atender sus pedidos (para no repetirlo en cada cuadro).
    avisado: std::sync::atomic::AtomicBool,
    /// Bloques que salieron sin el plugin porque estaba ocupado (la ventana lo tenía).
    pub perdidos: std::sync::atomic::AtomicU32,
    sample_rate: f32,
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
        let salidas = plugin.output_channel_count().max(2);
        let entradas = if info.instrumento { 0 } else { 2 };
        let mut bufs = AudioBuffers::new(entradas, salidas, BLOQUE, sample_rate as f64);
        bufs.clear();
        Ok(Self { info: info.clone(), plugin: Arc::new(Mutex::new(plugin)), bufs: Mutex::new(bufs), avisado: Default::default(), perdidos: Default::default(), sample_rate: sample_rate as f32 })
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
            return;
        };
        for &(at, e) in eventos {
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

    /// Estado del plugin para guardarlo en el proyecto.
    pub fn estado(&self) -> Option<Vec<u8>> {
        self.plugin.lock().ok()?.save_state().ok()
    }

    /// Restaura el estado guardado.
    pub fn cargar_estado(&self, datos: &[u8]) -> bool {
        !datos.is_empty() && self.plugin.lock().is_ok_and(|mut p| p.load_state(datos).is_ok())
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
}



