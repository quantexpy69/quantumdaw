//! Actualizaciones desde la app: al arrancar se mira el último release de GitHub y, si hay una
//! versión nueva, se ofrece instalarla. Se descarga el paquete del sistema (instalador o .zip en
//! Windows, .dmg en macOS, AppImage / .deb / .rpm en Linux), se comprueba con SHA256SUMS y
//! se instala al cerrar QUANTUM, que se vuelve a abrir solo con la versión nueva.
//!
//! `QUANTUM_REPO` cambia el repositorio (dueño/nombre) para probar con otro.
use crate::*;
use egui::RichText;
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

const REPO: &str = "quantexpy69/quantumdaw";

#[derive(Clone)]
pub struct Version {
    pub etiqueta: String,
    pub notas: String,
    pub pagina: String,
    /// Paquete para este sistema: (nombre, url de descarga).
    pub paquete: Option<(String, String)>,
    /// URL de SHA256SUMS del release.
    pub sumas: Option<String>,
}

#[derive(Clone)]
pub enum Estado {
    Buscando,
    AlDia,
    Disponible(Version),
    Descargando(Version),
    /// Descargada y verificada: se instala al cerrar.
    Lista(Version),
    Error(String),
}

static ESTADO: Mutex<Option<Estado>> = Mutex::new(None);
static BUSCADO: AtomicBool = AtomicBool::new(false);
/// El aviso se cerró en esta sesión («Más tarde»).
static OCULTO: AtomicBool = AtomicBool::new(false);
/// Se lanzó el instalador: la app debe cerrarse.
static CERRAR: AtomicBool = AtomicBool::new(false);

fn poner(e: Estado) {
    *ESTADO.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
}

pub fn estado() -> Option<Estado> {
    ESTADO.lock().ok()?.clone()
}

fn repo() -> String {
    std::env::var("QUANTUM_REPO").unwrap_or_else(|_| REPO.into())
}

/// «v0.61.2» → [0, 61, 2].
fn numeros(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
}

/// `nueva` es posterior a `actual`.
pub fn es_mas_nueva(nueva: &str, actual: &str) -> bool {
    let (a, b) = (numeros(nueva), numeros(actual));
    !a.is_empty() && a > b
}

/// Ejecutable compilado en la carpeta `target/` de cargo: no se reemplaza (solo se avisa).
fn es_desarrollo() -> bool {
    std::env::current_exe().is_ok_and(|p| p.ancestors().any(|d| d.file_name().is_some_and(|n| n == "target")))
}

/// Cómo está instalado QUANTUM en este equipo.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Instalacion {
    WinInstalador,
    WinPortable,
    Mac,
    AppImage,
    LinuxPortable,
    Deb,
    Rpm,
}

fn instalacion() -> Instalacion {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    if cfg!(windows) {
        if dir.join("unins000.exe").exists() { Instalacion::WinInstalador } else { Instalacion::WinPortable }
    } else if cfg!(target_os = "macos") {
        Instalacion::Mac
    } else if std::env::var_os("APPIMAGE").is_some() {
        Instalacion::AppImage
    } else if exe.starts_with("/usr") {
        let hay = |p: &str| comando("sh").args(["-c", &format!("command -v {p}")]).output().is_ok_and(|o| o.status.success());
        if hay("dpkg") { Instalacion::Deb } else if hay("rpm") { Instalacion::Rpm } else { Instalacion::LinuxPortable }
    } else {
        Instalacion::LinuxPortable
    }
}

fn nombre_paquete(i: Instalacion) -> &'static str {
    match i {
        Instalacion::WinInstalador => "quantum-daw-windows-x64-setup.exe",
        Instalacion::WinPortable => "quantum-daw-windows-x64.zip",
        Instalacion::Mac => "quantum-daw-macos-universal.dmg",
        Instalacion::AppImage => "quantum-daw-x86_64.AppImage",
        // Ya no se publica: las copias instaladas desde el .tar.gz antiguo solo ven el enlace a la página.
        Instalacion::LinuxPortable => "quantum-daw-linux-x86_64.tar.gz",
        Instalacion::Deb => "quantum-daw-amd64.deb",
        Instalacion::Rpm => "quantum-daw-x86_64.rpm",
    }
}

fn curl() -> std::process::Command {
    let mut c = comando("curl");
    c.args(["-sfL", "-A", &format!("quantum-daw/{}", welcome::VERSION)]);
    c
}

/// Lee el último release publicado.
fn ultimo() -> anyhow::Result<Version> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo());
    let salida = curl().args(["--max-time", "15", "-H", "Accept: application/vnd.github+json", &url]).output()?;
    anyhow::ensure!(salida.status.success(), "{}", tr("no se pudo consultar GitHub"));
    let v: serde_json::Value = serde_json::from_slice(&salida.stdout)?;
    let texto = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or_default().to_string();
    let assets = v.get("assets").and_then(|a| a.as_array()).cloned().unwrap_or_default();
    let asset = |nombre: &str| assets.iter().find(|a| a.get("name").and_then(|n| n.as_str()) == Some(nombre)).and_then(|a| a.get("browser_download_url")?.as_str().map(str::to_string));
    let nombre = nombre_paquete(instalacion());
    Ok(Version {
        etiqueta: texto("tag_name"),
        notas: texto("body"),
        pagina: texto("html_url"),
        paquete: asset(nombre).map(|u| (nombre.to_string(), u)),
        sumas: asset("SHA256SUMS"),
    })
}

/// Busca una versión nueva en segundo plano (una vez por sesión salvo que `forzar`).
pub fn buscar(forzar: bool) {
    if BUSCADO.swap(true, Relaxed) && !forzar {
        return;
    }
    if forzar {
        OCULTO.store(false, Relaxed);
    }
    poner(Estado::Buscando);
    std::thread::spawn(|| {
        let e = match ultimo() {
            Ok(v) if es_mas_nueva(&v.etiqueta, welcome::VERSION) => Estado::Disponible(v),
            Ok(_) => Estado::AlDia,
            Err(e) => {
                informes::registrar("INFO", &format!("Actualizaciones: {e}"));
                Estado::Error(e.to_string())
            }
        };
        poner(e);
    });
}

fn carpeta_descargas() -> PathBuf {
    data_dir().join("actualizaciones")
}

fn sha256(ruta: &Path) -> anyhow::Result<String> {
    let mut h = Sha256::new();
    let mut f = fs::File::open(ruta)?;
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Descarga el paquete y lo verifica; deja el instalador listo para cuando se cierre la app.
fn descargar(v: &Version) -> anyhow::Result<PathBuf> {
    let (nombre, url) = v.paquete.clone().ok_or_else(|| anyhow::anyhow!("{}", tr("el release no trae paquete para este sistema")))?;
    let dir = carpeta_descargas().join(&v.etiqueta);
    fs::create_dir_all(&dir)?;
    let destino = dir.join(&nombre);
    let ok = curl().args(["--max-time", "1800", "-o"]).arg(&destino).arg(&url).status()?.success();
    anyhow::ensure!(ok, "{}", tr("falló la descarga"));
    if let Some(sumas) = &v.sumas {
        let salida = curl().args(["--max-time", "30", sumas]).output()?;
        let texto = String::from_utf8_lossy(&salida.stdout);
        let esperada = texto.lines().find_map(|l| {
            let mut p = l.split_whitespace();
            let (suma, archivo) = (p.next()?, p.next()?);
            (archivo.trim_start_matches('*') == nombre).then(|| suma.to_lowercase())
        });
        if let Some(esperada) = esperada {
            let real = sha256(&destino)?;
            if real != esperada {
                let _ = fs::remove_file(&destino);
                anyhow::bail!("{}", tr("la descarga está dañada (la suma SHA-256 no coincide)"));
            }
        }
    }
    Ok(destino)
}

/// Texto entre comillas simples para PowerShell.
#[cfg_attr(not(windows), allow(dead_code))]
fn ps(s: &Path) -> String {
    format!("'{}'", s.to_string_lossy().replace('\'', "''"))
}

/// Texto entre comillas simples para sh.
#[cfg_attr(windows, allow(dead_code))]
fn sh(s: &Path) -> String {
    format!("'{}'", s.to_string_lossy().replace('\'', r"'\''"))
}

/// Escribe y lanza (sin esperar) el guion que espera a que QUANTUM se cierre, instala y lo reabre.
fn lanzar_instalacion(paquete: &Path) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    let pid = std::process::id();
    let dir = paquete.parent().unwrap_or(Path::new(".")).to_path_buf();
    let tipo = instalacion();
    if cfg!(windows) {
        let pasos = match tipo {
            Instalacion::WinInstalador => format!("Start-Process -FilePath {} -ArgumentList '/SILENT','/SP-','/SUPPRESSMSGBOXES','/NORESTART','/CLOSEAPPLICATIONS' -Wait", ps(paquete)),
            _ => {
                let x = dir.join("extraido");
                format!(
                    "Remove-Item -Recurse -Force {x} -ErrorAction SilentlyContinue\nExpand-Archive -Path {zip} -DestinationPath {x} -Force\n$nuevo = Get-ChildItem -Path {x} -Recurse -Filter 'quantum-daw.exe' | Select-Object -First 1\nif ($nuevo) {{ for ($i = 0; $i -lt 20; $i++) {{ try {{ Copy-Item $nuevo.FullName {exe} -Force -ErrorAction Stop; break }} catch {{ Start-Sleep -Milliseconds 500 }} }} }}",
                    x = ps(&x),
                    zip = ps(paquete),
                    exe = ps(&exe)
                )
            }
        };
        let guion = format!("Wait-Process -Id {pid} -ErrorAction SilentlyContinue\nStart-Sleep -Milliseconds 500\n{pasos}\nStart-Process -FilePath {}\n", ps(&exe));
        let ruta = dir.join("instalar.ps1");
        fs::write(&ruta, guion)?;
        comando("powershell").args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"]).arg(&ruta).spawn()?;
    } else {
        let espera = format!("while kill -0 {pid} 2>/dev/null; do sleep 0.5; done\n");
        let (pasos, abrir) = match tipo {
            Instalacion::Mac => {
                // .../Quantum DAW.app/Contents/MacOS/quantum-daw → la carpeta .app
                let app = exe.ancestors().find(|d| d.extension().is_some_and(|e| e == "app")).map(Path::to_path_buf);
                let Some(app) = app else {
                    comando("open").arg(paquete).spawn()?;
                    return Ok(());
                };
                let mnt = dir.join("montado");
                // El copiado va en su propio guion: si /Applications pide permisos, se repite como
                // administrador (el sistema muestra su diálogo de contraseña).
                let copiar = dir.join("copiar.sh");
                fs::write(
                    &copiar,
                    format!("APP=$(ls -d {m}/*.app | head -n 1)\nrm -rf {n} && ditto \"$APP\" {n} && rm -rf {a} && mv {n} {a}\n", m = sh(&mnt), n = sh(&app.with_extension("nueva")), a = sh(&app)),
                )?;
                let pasos = format!(
                    "mkdir -p {m}\nhdiutil attach -nobrowse -readonly -quiet -mountpoint {m} {dmg}\nsh {c} || osascript -e \"do shell script \\\"sh '{cr}'\\\" with administrator privileges\"\nhdiutil detach {m} -quiet\nxattr -dr com.apple.quarantine {a} 2>/dev/null\n",
                    m = sh(&mnt),
                    dmg = sh(paquete),
                    c = sh(&copiar),
                    cr = copiar.to_string_lossy(),
                    a = sh(&app)
                );
                (pasos, format!("open {}", sh(&app)))
            }
            Instalacion::AppImage => {
                let destino = PathBuf::from(std::env::var_os("APPIMAGE").unwrap_or_default());
                (format!("chmod +x {p} && mv -f {p} {d}\n", p = sh(paquete), d = sh(&destino)), format!("{} &", sh(&destino)))
            }
            Instalacion::Deb | Instalacion::Rpm => {
                let instalar = if tipo == Instalacion::Deb { format!("apt-get install -y {}", sh(paquete)) } else { format!("rpm -U --force {}", sh(paquete)) };
                (format!("if command -v pkexec >/dev/null; then pkexec sh -c {}; else xdg-open {}; exit 0; fi\n", sh(Path::new(&instalar)), sh(paquete)), format!("{} &", sh(&exe)))
            }
            _ => {
                let x = dir.join("extraido");
                (
                    format!("rm -rf {x} && mkdir -p {x} && tar -xzf {p} -C {x}\nNUEVO=$(find {x} -type f -name quantum-daw | head -n 1)\n[ -n \"$NUEVO\" ] && cp -f \"$NUEVO\" {e}.nuevo && chmod +x {e}.nuevo && mv -f {e}.nuevo {e}\n", x = sh(&x), p = sh(paquete), e = sh(&exe)),
                    format!("{} &", sh(&exe)),
                )
            }
        };
        let guion = format!("#!/bin/sh\n{espera}{pasos}{abrir}\n");
        let ruta = dir.join("instalar.sh");
        fs::write(&ruta, guion)?;
        comando("sh").arg(&ruta).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()?;
    }
    Ok(())
}

/// Borra las descargas de versiones anteriores (al arrancar ya con la nueva).
pub fn limpiar() {
    let Ok(dirs) = fs::read_dir(carpeta_descargas()) else { return };
    for d in dirs.flatten() {
        if !es_mas_nueva(&d.file_name().to_string_lossy(), welcome::VERSION) {
            let _ = fs::remove_dir_all(d.path());
        }
    }
}

impl App {
    /// Aviso de versión nueva (esquina inferior derecha) y el cierre para instalarla.
    pub fn aviso_actualizacion(&mut self, ctx: &egui::Context) {
        buscar(false);
        if CERRAR.swap(false, Relaxed) {
            // Cierre normal: si hay cambios sin guardar, se pregunta antes.
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let Some(e) = estado() else { return };
        let v = match &e {
            Estado::Disponible(v) | Estado::Descargando(v) | Estado::Lista(v) => v.clone(),
            _ => return,
        };
        if OCULTO.load(Relaxed) || (matches!(e, Estado::Disponible(_)) && self.config.omitir_version == v.etiqueta) {
            return;
        }
        let mut accion = None;
        egui::Window::new(tr("Actualización disponible"))
            .id(egui::Id::new("actualizacion"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -40.0])
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(format!("QUANTUM {} → {}", welcome::VERSION, v.etiqueta)).strong().size(15.0));
                let notas: String = v.notas.lines().filter(|l| !l.trim().is_empty()).take(6).collect::<Vec<_>>().join("\n");
                if !notas.is_empty() {
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| ui.label(RichText::new(notas).size(12.0).color(TEXT_DIM())));
                }
                ui.add_space(4.0);
                match &e {
                    Estado::Descargando(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(tr("Descargando la actualización…"));
                        });
                        ctx.request_repaint_after(std::time::Duration::from_millis(300));
                    }
                    Estado::Lista(_) => {
                        ui.label(tr("Lista. Se instala al cerrar QUANTUM y se vuelve a abrir solo."));
                        ui.horizontal(|ui| {
                            if ui.button(tr("Cerrar e instalar ahora")).clicked() {
                                accion = Some(2);
                            }
                            if ui.button(tr("Al salir")).clicked() {
                                accion = Some(3);
                            }
                        });
                    }
                    _ if es_desarrollo() || v.paquete.is_none() => {
                        ui.label(RichText::new(tr("Esta copia no se puede actualizar sola: descarga la versión nueva desde GitHub.")).size(12.0));
                        ui.horizontal(|ui| {
                            if ui.button(tr("Abrir la página")).clicked() {
                                accion = Some(4);
                            }
                            if ui.button(tr("Más tarde")).clicked() {
                                accion = Some(3);
                            }
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new(tr("Actualizar")).strong()).clicked() {
                                accion = Some(1);
                            }
                            if ui.button(tr("Más tarde")).clicked() {
                                accion = Some(3);
                            }
                            if ui.button(tr("Omitir esta versión")).clicked() {
                                accion = Some(5);
                            }
                        });
                    }
                }
            });
        match accion {
            Some(1) => {
                poner(Estado::Descargando(v.clone()));
                std::thread::spawn(move || {
                    let r = descargar(&v).and_then(|p| lanzar_instalacion(&p));
                    match r {
                        Ok(()) => poner(Estado::Lista(v)),
                        Err(e) => {
                            informes::registrar("ERROR", &format!("Actualización: {e}"));
                            poner(Estado::Error(e.to_string()));
                        }
                    }
                });
            }
            Some(2) => CERRAR.store(true, Relaxed),
            Some(3) => OCULTO.store(true, Relaxed),
            Some(4) => {
                let _ = comando(if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" }).arg(&v.pagina).spawn();
                OCULTO.store(true, Relaxed);
            }
            Some(5) => {
                self.config.omitir_version = v.etiqueta.clone();
                self.config.save();
            }
            _ => {}
        }
        if let Some(Estado::Error(e)) = estado() {
            self.status = format!("{} {e}", tr("Actualización:"));
            poner(Estado::AlDia);
        }
    }

    /// Menú Herramientas → Buscar actualizaciones: busca y dice el resultado en la barra de estado.
    pub fn buscar_actualizaciones(&mut self) {
        self.config.omitir_version.clear();
        buscar(true);
        self.status = tr("Buscando actualizaciones…").into();
        self.avisar_al_dia = true;
    }

    /// Tras una búsqueda manual, informa si ya se tiene la última versión.
    pub fn resultado_busqueda(&mut self) {
        if !self.avisar_al_dia {
            return;
        }
        match estado() {
            Some(Estado::AlDia) => {
                self.status = format!("{} ({})", tr("Ya tienes la última versión"), welcome::VERSION);
                self.avisar_al_dia = false;
            }
            Some(Estado::Buscando) => {}
            _ => self.avisar_al_dia = false,
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn compara_versiones() {
        assert!(es_mas_nueva("v0.61.0", "0.60.1"));
        assert!(es_mas_nueva("v1.0.0", "0.99.9"));
        assert!(es_mas_nueva("0.60.10", "0.60.9"));
        assert!(!es_mas_nueva("v0.60.1", "0.60.1"));
        assert!(!es_mas_nueva("v0.59.9", "0.60.1"));
        assert!(!es_mas_nueva("nightly", "0.60.1"));
    }
}
