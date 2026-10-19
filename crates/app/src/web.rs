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

/// Dirección de la web.
pub fn base() -> String {
    std::env::var("QUANTUM_WEB").unwrap_or_else(|_| "https://www.quantumdaw.com".into()).trim_end_matches('/').to_string()
}

fn cache() -> PathBuf {
    data_dir().join("web.json")
}

fn pedir<T: for<'de> Deserialize<'de>>(ruta: &str, campo: &str) -> Option<Vec<T>> {
    let salida = comando("curl").args(["-sfL", "--max-time", "8", &format!("{}/{ruta}", base())]).output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&salida.stdout).ok()?;
    serde_json::from_value(v.get(campo)?.clone()).ok()
}

/// Novedades y tienda (lo último descargado o la caché). La primera llamada lanza la descarga.
pub fn contenido() -> Option<Contenido> {
    if !PEDIDO.swap(true, Relaxed) {
        if let Some(c) = fs::read_to_string(cache()).ok().and_then(|s| serde_json::from_str::<Contenido>(&s).ok()) {
            *ESTADO.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
        }
        std::thread::spawn(|| {
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
            *ESTADO.lock().unwrap_or_else(|e| e.into_inner()) = Some(c);
        });
    }
    ESTADO.lock().ok()?.clone()
}

/// Precio legible («Gratis», «29,00 USD»).
pub fn precio(p: &Producto) -> String {
    if p.gratis { tr("Gratis").to_string() } else { format!("{:.2} {}", p.precio, p.moneda).replace('.', ",") }
}
