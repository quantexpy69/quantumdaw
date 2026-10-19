//! Novedades del blog y productos de la tienda de quantumdaw.com para la pantalla inicial.
//! Se piden una vez por sesión en segundo plano (con curl) y se guardan en caché para verlos sin
//! conexión. `QUANTUM_WEB` cambia la dirección (por ejemplo para probar con la web local).
use crate::*;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Serialize, Deserialize)]
pub struct Noticia {
    pub slug: String,
    pub titulo: String,
    pub resumen: String,
    pub fecha: String,
    #[serde(default)]
    pub imagen: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Producto {
    pub slug: String,
    pub nombre: String,
    pub resumen: String,
    pub tipo: String,
    pub precio: f64,
    pub moneda: String,
    pub gratis: bool,
    pub etiqueta: Option<String>,
    pub enlace: Option<String>,
    #[serde(default)]
    pub imagen: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Contenido {
    pub noticias: Vec<Noticia>,
    pub productos: Vec<Producto>,
    /// false = contenido guardado de una sesión anterior (sin conexión ahora).
    #[serde(skip)]
    pub en_linea: bool,
}

static ESTADO: Mutex<Option<Contenido>> = Mutex::new(None);
static PEDIDO: AtomicBool = AtomicBool::new(false);
/// Imágenes de las tarjetas ya decodificadas (ruta en la web → imagen) y sus texturas.
static IMAGENES: Mutex<Vec<(String, egui::ColorImage)>> = Mutex::new(Vec::new());
static TEXTURAS: Mutex<Vec<(String, egui::TextureHandle)>> = Mutex::new(Vec::new());

/// Dirección de la web.
pub fn base() -> String {
    std::env::var("QUANTUM_WEB").unwrap_or_else(|_| "https://www.quantumdaw.com".into()).trim_end_matches('/').to_string()
}

fn cache() -> PathBuf {
    data_dir().join("web.json")
}

/// Descarga (o lee de la caché) y decodifica las imágenes de noticias y productos.
fn cargar_imagenes(c: &Contenido) {
    let rutas: Vec<String> = c.noticias.iter().filter_map(|n| n.imagen.clone()).chain(c.productos.iter().filter_map(|p| p.imagen.clone())).collect();
    let dir = data_dir().join("web-imagenes");
    let _ = fs::create_dir_all(&dir);
    for ruta in rutas {
        if IMAGENES.lock().is_ok_and(|v| v.iter().any(|x| x.0 == ruta)) {
            continue;
        }
        let archivo = dir.join(ruta.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' { c } else { '_' }).collect::<String>());
        if !archivo.exists() {
            let url = if ruta.starts_with("http") { ruta.clone() } else { format!("{}/{ruta}", base()) };
            let _ = comando("curl").args(["-sfL", "--max-time", "10", "-o"]).arg(&archivo).arg(&url).status();
        }
        let Ok(datos) = fs::read(&archivo) else { continue };
        let Ok(img) = image::load_from_memory(&datos) else { continue };
        let img = img.thumbnail(720, 720).to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
        IMAGENES.lock().unwrap_or_else(|e| e.into_inner()).push((ruta, ci));
    }
}

/// Textura de la imagen de una tarjeta (None mientras se descarga o si no tiene).
pub fn imagen(ctx: &egui::Context, ruta: Option<&str>) -> Option<egui::TextureHandle> {
    let ruta = ruta?;
    let mut t = TEXTURAS.lock().ok()?;
    if let Some(x) = t.iter().find(|x| x.0 == ruta) {
        return Some(x.1.clone());
    }
    let Some(ci) = IMAGENES.lock().ok()?.iter().find(|x| x.0 == ruta).map(|x| x.1.clone()) else {
        // Todavía se descarga: se vuelve a mirar en un rato.
        ctx.request_repaint_after(std::time::Duration::from_millis(400));
        return None;
    };
    let h = ctx.load_texture(format!("web:{ruta}"), ci, egui::TextureOptions::LINEAR);
    t.push((ruta.to_string(), h.clone()));
    Some(h)
}

fn pedir<T: for<'de> Deserialize<'de>>(ruta: &str, campo: &str) -> Option<Vec<T>> {
    let salida = comando("curl").args(["-sfL", "--max-time", "8", &format!("{}/{ruta}", base())]).output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&salida.stdout).ok()?;
    serde_json::from_value(v.get(campo)?.clone()).ok()
}

/// Novedades y tienda (lo último descargado o la caché). La primera llamada lanza la descarga.
pub fn contenido() -> Option<Contenido> {
    if !PEDIDO.swap(true, Relaxed) {
        let guardado = fs::read_to_string(cache()).ok().and_then(|s| serde_json::from_str::<Contenido>(&s).ok());
        if let Some(c) = guardado.clone() {
            *ESTADO.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
        }
        std::thread::spawn(move || {
            // Primero las imágenes ya guardadas (se ven al momento, aun sin conexión).
            if let Some(c) = &guardado {
                cargar_imagenes(c);
            }
            let (noticias, productos) = (pedir::<Noticia>("api/noticias.php?limite=3", "noticias"), pedir::<Producto>("api/tienda.php", "productos"));
            if noticias.is_none() && productos.is_none() {
                informes::registrar("INFO", "Novedades y tienda: sin conexión con la web");
                // Sin conexión: queda lo guardado de otra sesión, o un contenido vacío que lo indica.
                let mut e = ESTADO.lock().unwrap_or_else(|e| e.into_inner());
                if e.is_none() {
                    *e = Some(Contenido::default());
                }
                return;
            }
            let c = Contenido { noticias: noticias.unwrap_or_default(), productos: productos.unwrap_or_default(), en_linea: true };
            let _ = fs::create_dir_all(data_dir());
            let _ = fs::write(cache(), serde_json::to_string(&c).unwrap_or_default());
            *ESTADO.lock().unwrap_or_else(|e| e.into_inner()) = Some(c.clone());
            cargar_imagenes(&c);
        });
    }
    ESTADO.lock().ok()?.clone()
}

/// Precio legible («Gratis», «29,00 USD»).
pub fn precio(p: &Producto) -> String {
    if p.gratis { tr("Gratis").to_string() } else { format!("{:.2} {}", p.precio, p.moneda).replace('.', ",") }
}
