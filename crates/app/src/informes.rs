//! Registro de errores e informes de fallos, como el «enviar informe» tradicional de los programas:
//! - `quantum-daw.log` en la carpeta de registros guarda inicios, errores y avisos del motor de audio.
//! - Si el programa se cierra por un fallo (panic) se guarda un informe con la pila de llamadas.
//! - Un vigilante detecta cuando la ventana deja de responder y guarda la pila de todos los hilos
//!   (en Linux, con eu-stack o gdb), que es lo que permite saber dónde se quedó colgado.
//!
//! Los informes pendientes se muestran al usuario, que decide si enviarlos a quantumdaw.com.
use crate::*;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const URL: &str = "https://www.quantumdaw.com/api/reporte.php";
/// Segundos sin dibujar un cuadro para considerar que la ventana está colgada.
#[cfg(target_os = "linux")]
const CUELGUE_SEG: u64 = 12;

static LOG: Mutex<Option<fs::File>> = Mutex::new(None);
/// Datos que acompañan a cada informe (GPU, audio, proyecto…).
static CONTEXTO: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());
static LATIDO: AtomicU64 = AtomicU64::new(0);
/// La ventana principal tenía el foco en el último cuadro dibujado.
static FOCO: AtomicBool = AtomicBool::new(false);
/// Hay un informe nuevo que mostrar (por ejemplo un fallo en otro hilo mientras el programa sigue abierto).
pub static NUEVO: AtomicBool = AtomicBool::new(false);

pub fn carpeta() -> PathBuf {
    data_dir().join("registros")
}

fn ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Abre el registro, instala el capturador de fallos y arranca el vigilante de cuelgues.
pub fn iniciar() {
    let dir = carpeta();
    let _ = fs::create_dir_all(&dir);
    let log = dir.join("quantum-daw.log");
    if fs::metadata(&log).is_ok_and(|m| m.len() > 2_000_000) {
        let _ = fs::rename(&log, dir.join("quantum-daw.anterior.log"));
    }
    *LOG.lock().unwrap_or_else(|e| e.into_inner()) = OpenOptions::new().create(true).append(true).open(&log).ok();
    registrar("INICIO", &format!("QUANTUM {VERSION} · {}", sistema()));
    let previo = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let hilo = std::thread::current().name().unwrap_or("sin nombre").to_string();
        let msg = info.payload().downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| info.payload().downcast_ref::<String>().cloned()).unwrap_or_default();
        let lugar = info.location().map(|l| l.to_string()).unwrap_or_default();
        // Al cerrar la sesión o reiniciar con el programa abierto, el servidor X se va y winit aborta al
        // perder la conexión: no es un fallo de QUANTUM, así que no se ofrece enviar un informe.
        if lugar.contains("platform_impl/linux/x11") && msg.contains("Connection(") {
            registrar("INFO", &format!("Se cerró la sesión gráfica con el programa abierto ({msg})"));
            return previo(info);
        }
        let resumen = format!("Fallo en el hilo «{hilo}»: {msg}");
        registrar("FALLO", &format!("{resumen} ({lugar})"));
        guardar("fallo", &resumen, &format!("Lugar: {lugar}\n\n{}", std::backtrace::Backtrace::force_capture()));
        previo(info);
    }));
    #[cfg(target_os = "linux")]
    if herramienta_pilas().is_some() {
        // Permite que eu-stack/gdb lean este proceso aunque ptrace esté restringido (Ubuntu y otros).
        unsafe { libc::prctl(libc::PR_SET_PTRACER, libc::PR_SET_PTRACER_ANY) };
        let _ = std::thread::Builder::new().name("vigilante".into()).spawn(vigilar);
    }
}

/// Agrega una línea al registro con fecha y hora.
pub fn registrar(nivel: &str, texto: &str) {
    if let Some(f) = LOG.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        let _ = writeln!(f, "{} [{nivel}] {texto}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

/// Dato que se incluye en los informes (se reemplaza si ya existe).
pub fn contexto(clave: &'static str, valor: String) {
    let mut c = CONTEXTO.lock().unwrap_or_else(|e| e.into_inner());
    c.retain(|(k, _)| *k != clave);
    c.push((clave, valor));
}

/// Dato del contexto de los informes (por ejemplo la tarjeta gráfica), para mostrarlo en Acerca de.
pub fn dato(clave: &str) -> Option<String> {
    CONTEXTO.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(k, _)| *k == clave).map(|(_, v)| v.clone())
}

/// La interfaz dibujó un cuadro: el vigilante sabe que no está colgada. Sin foco el sistema puede
/// dejar de pedir cuadros (ventana tapada, en otro escritorio o minimizada) y eso no es un cuelgue.
pub fn latido(con_foco: bool) {
    FOCO.store(con_foco, Relaxed);
    LATIDO.store(ms(), Relaxed);
}

/// Una ventana separada (plugin, mixer…) tiene el foco: también cuenta como «en uso».
pub fn foco_separada() {
    FOCO.store(true, Relaxed);
}

/// Sistema operativo y versión, para los informes.
pub fn sistema() -> String {
    #[cfg(target_os = "linux")]
    {
        let distro = fs::read_to_string("/etc/os-release").ok().and_then(|s| s.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string())));
        let kernel = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        let sesion = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        format!("{} · kernel {} · {sesion}", distro.unwrap_or_else(|| "Linux".into()), kernel.trim())
    }
    #[cfg(not(target_os = "linux"))]
    {
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Guarda un informe en la carpeta de registros para mostrarlo y, si el usuario quiere, enviarlo.
pub fn guardar(tipo: &str, resumen: &str, detalle: &str) {
    let ctx = CONTEXTO.lock().map(|c| c.iter().map(|(k, v)| format!("{k}: {v}\n")).collect::<String>()).unwrap_or_default();
    let log = fs::read_to_string(carpeta().join("quantum-daw.log")).unwrap_or_default();
    let cola: Vec<&str> = log.lines().rev().take(60).collect();
    let texto = format!(
        "QUANTUM {VERSION} — informe de {tipo}\nFecha: {}\nSistema: {}\n{ctx}Resumen: {resumen}\n\n{detalle}\n\nÚltimas líneas del registro:\n{}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        sistema(),
        cola.into_iter().rev().collect::<Vec<_>>().join("\n")
    );
    let nombre = format!("{tipo}-{}.txt", chrono::Local::now().format("%Y%m%d-%H%M%S"));
    if fs::write(carpeta().join(nombre), texto).is_ok() {
        NUEVO.store(true, Relaxed);
    }
}

/// Resultado del envío en curso: `None` mientras se envía.
type Envio = Arc<Mutex<Option<Result<(), String>>>>;

/// Informe guardado que el usuario todavía no envió ni descartó.
pub struct Pendiente {
    pub ruta: PathBuf,
    pub tipo: String,
    pub texto: String,
    pub comentario: String,
    pub envio: Option<Envio>,
}

/// El informe pendiente más reciente.
pub fn pendiente() -> Option<Pendiente> {
    let mut rutas: Vec<PathBuf> = fs::read_dir(carpeta()).ok()?.flatten().map(|e| e.path()).filter(|p| {
        let n = p.file_name().unwrap_or_default().to_string_lossy();
        ["fallo-", "cuelgue-", "error-"].iter().any(|t| n.starts_with(t)) && n.ends_with(".txt") && n.matches('.').count() == 1
    }).collect();
    rutas.sort();
    let ruta = rutas.pop()?;
    let texto = fs::read_to_string(&ruta).ok()?;
    let tipo = ruta.file_name()?.to_string_lossy().split('-').next()?.to_string();
    Some(Pendiente { ruta, tipo, texto, comentario: String::new(), envio: None })
}

/// Marca el informe como enviado o descartado (no se vuelve a mostrar, pero queda en la carpeta).
pub fn marcar(p: &Pendiente, como: &str) {
    let _ = fs::rename(&p.ruta, p.ruta.with_extension(format!("{como}.txt")));
}

/// Envía el informe a quantumdaw.com en segundo plano (con curl, sin bloquear la interfaz).
pub fn enviar(p: &mut Pendiente) {
    let resumen = p.texto.lines().find_map(|l| l.strip_prefix("Resumen: ")).unwrap_or("").to_string();
    let cuerpo = serde_json::json!({
        "tipo": p.tipo, "version": VERSION, "sistema": sistema(), "resumen": resumen,
        "texto": p.texto, "comentario": p.comentario,
    })
    .to_string();
    let estado: Envio = Arc::new(Mutex::new(None));
    p.envio = Some(estado.clone());
    std::thread::spawn(move || {
        let r = (|| {
            let mut hijo = comando("curl")
                .args(["-sS", "--fail-with-body", "--max-time", "30", "-H", "Content-Type: application/json", "--data-binary", "@-", URL])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| format!("No se pudo ejecutar curl: {e}"))?;
            hijo.stdin.take().map(|mut s| s.write_all(cuerpo.as_bytes()));
            let salida = hijo.wait_with_output().map_err(|e| e.to_string())?;
            if salida.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&salida.stderr).trim().to_string()) }
        })();
        registrar(if r.is_ok() { "INFO" } else { "ERROR" }, &format!("Envío del informe: {}", r.as_ref().map_or_else(|e| e.as_str(), |_| "enviado")));
        *estado.lock().unwrap_or_else(|e| e.into_inner()) = Some(r);
    });
}

/// eu-stack (elfutils) o gdb, para leer la pila de todos los hilos.
#[cfg(target_os = "linux")]
fn herramienta_pilas() -> Option<&'static str> {
    ["eu-stack", "gdb"].into_iter().find(|h| comando("which").arg(h).output().is_ok_and(|o| o.status.success()))
}

/// Pila de llamadas de todos los hilos de este proceso.
#[cfg(target_os = "linux")]
fn pilas() -> Option<String> {
    let pid = std::process::id().to_string();
    // Mientras se leen las pilas el proceso queda detenido: nada de descargar símbolos de internet
    // (debuginfod, activo por defecto en Fedora), que lo dejaría congelado durante segundos.
    let mut c = match herramienta_pilas()? {
        "eu-stack" => {
            let mut c = comando("eu-stack");
            c.args(["-p", &pid]);
            c
        }
        _ => {
            let mut c = comando("gdb");
            c.args(["-batch", "-iex", "set debuginfod enabled off", "-p", &pid, "-ex", "thread apply all bt"]);
            c
        }
    };
    let salida = c.env_remove("DEBUGINFOD_URLS").stdin(std::process::Stdio::null()).output().ok()?;
    Some(String::from_utf8_lossy(&salida.stdout).into_owned()).filter(|s| !s.trim().is_empty())
}

/// ¿El hilo principal está simplemente esperando eventos (ventana minimizada u oculta) y no colgado?
#[cfg(target_os = "linux")]
fn en_reposo(pilas: &str) -> bool {
    let pid = std::process::id();
    let principal = pilas.split("TID ").find(|b| b.starts_with(&format!("{pid}:"))).unwrap_or("");
    let antes_del_bucle: Vec<&str> = principal.lines().take_while(|l| !l.contains("winit")).collect();
    let ocupado = ["Swap", "egui", "eframe", "glow", "quantum_daw", "rfd"];
    principal.contains("winit") && !antes_del_bucle.iter().any(|l| ocupado.iter().any(|o| l.contains(o)))
}

#[cfg(target_os = "linux")]
fn vigilar() {
    // `revisado`: latido ya examinado (ventana oculta o minimizada); no se vuelve a mirar hasta que cambie.
    let (mut avisado, mut revisado) = (false, 0);
    loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let ultimo = LATIDO.load(Relaxed);
        if ultimo == 0 || ultimo == revisado || !FOCO.load(Relaxed) {
            continue;
        }
        let parado = ms().saturating_sub(ultimo) / 1000;
        if parado < 3 {
            avisado = false;
        }
        if parado < CUELGUE_SEG || avisado {
            continue;
        }
        let Some(p) = pilas() else { continue };
        // Un diálogo de archivos abierto o la ventana minimizada no son cuelgues.
        if p.contains("rfd") || en_reposo(&p) {
            revisado = ultimo;
            continue;
        }
        avisado = true;
        let resumen = format!("La ventana dejó de responder durante más de {parado} segundos");
        registrar("CUELGUE", &resumen);
        guardar("cuelgue", &resumen, &format!("Pilas de llamadas de todos los hilos:\n{p}"));
    }
}

impl App {
    /// Ventana «Informe de error»: muestra el informe pendiente y deja enviarlo o descartarlo.
    pub fn ventana_informe(&mut self, ctx: &egui::Context) {
        if NUEVO.swap(false, Relaxed) && self.informe.is_none() {
            self.informe = pendiente();
        }
        let Some(p) = &mut self.informe else { return };
        let (mut cerrar, mut abrir) = (None, false);
        let titulo = match p.tipo.as_str() {
            "fallo" => tr("QUANTUM tuvo un fallo"),
            "error" => tr("Informe de error"),
            _ => tr("QUANTUM dejó de responder"),
        };
        egui::Window::new(egui::RichText::new(titulo).strong()).id(egui::Id::new("informe")).collapsible(false).default_size([640.0, 520.0]).show(ctx, |ui| {
            ui.label(tr("Se guardó un informe con lo que estaba pasando. Enviarlo nos ayuda a encontrar y corregir el problema. Solo se envía este texto (sin tus audios ni proyectos)."));
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                ui.add(egui::TextEdit::multiline(&mut p.texto.as_str()).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY));
            });
            ui.add_space(6.0);
            ui.label(tr("¿Qué estabas haciendo? (opcional)"));
            ui.add(egui::TextEdit::multiline(&mut p.comentario).desired_rows(2).desired_width(f32::INFINITY));
            let estado = p.envio.as_ref().map(|e| e.lock().map(|g| g.clone()).unwrap_or(None));
            ui.horizontal(|ui| {
                match &estado {
                    None => {
                        if ui.add(egui::Button::new(egui::RichText::new(tr("Enviar informe")).color(BG()).strong()).fill(ACCENT())).clicked() {
                            enviar(p);
                        }
                        if ui.button(tr("No enviar")).clicked() {
                            cerrar = Some("visto");
                        }
                    }
                    Some(None) => {
                        ui.spinner();
                        ui.label(tr("Enviando…"));
                    }
                    Some(Some(Ok(()))) => {
                        ui.label(egui::RichText::new(tr("Informe enviado. ¡Gracias!")).color(ACCENT()));
                        if ui.button(tr("Cerrar")).clicked() {
                            cerrar = Some("enviado");
                        }
                    }
                    Some(Some(Err(e))) => {
                        ui.label(egui::RichText::new(format!("{}: {e}", tr("No se pudo enviar"))).color(Color32::from_rgb(255, 90, 95)));
                        if ui.button(tr("Reintentar")).clicked() {
                            enviar(p);
                        }
                        if ui.button(tr("Cerrar")).clicked() {
                            cerrar = Some("visto");
                        }
                    }
                }
                if ui.button(tr("Abrir carpeta de registros")).clicked() {
                    abrir = true;
                }
            });
        });
        if abrir {
            abrir_carpeta(&carpeta());
        }
        if let Some(como) = cerrar {
            marcar(p, como);
            self.informe = pendiente();
        }
    }
}

/// Abre una carpeta en el explorador de archivos del sistema.
pub fn abrir_carpeta(dir: &Path) {
    let programa = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = comando(programa).arg(dir).spawn();
}
