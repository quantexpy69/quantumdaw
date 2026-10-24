//! Licencias de los plugins QUANTUM Pro. Una clave es `QDAW1.<datos>.<firma>` (base64url): los datos
//! son JSON con los plugins que activa (bits), a nombre de quién y la fecha; la firma Ed25519 la hace
//! la web al vender. La app solo tiene la clave pública: verifica sin conexión.
use crate::*;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use egui::{RichText, vec2};

/// Clave pública de firma de quantumdaw.com.
const PUBLICA: [u8; 32] = [116, 162, 126, 151, 54, 61, 173, 53, 25, 114, 207, 153, 106, 195, 223, 191, 137, 252, 220, 28, 201, 222, 144, 154, 151, 160, 173, 99, 157, 34, 93, 76];

#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Licencia {
    /// Plugins que activa (bits de `FxKind::pro`).
    pub p: u32,
    /// A nombre de quién (o su correo).
    pub n: String,
    /// Fecha de la compra.
    #[serde(default)]
    pub f: String,
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        } as u32)
    };
    let datos: Vec<u8> = s.bytes().filter(|c| *c != b'=').collect();
    let mut out = Vec::with_capacity(datos.len() * 3 / 4);
    for trozo in datos.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in trozo.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..trozo.len()]);
    }
    Some(out)
}

/// Verifica una clave; devuelve la licencia si la firma es de quantumdaw.com.
pub fn verificar(clave: &str) -> Result<Licencia, &'static str> {
    let limpia: String = clave.split_whitespace().collect();
    let mut partes = limpia.split('.');
    let (Some("QDAW1"), Some(datos), Some(firma), None) = (partes.next(), partes.next(), partes.next(), partes.next()) else {
        return Err("La clave no tiene el formato de QUANTUM (QDAW1.…)");
    };
    let (Some(bytes), Some(firma)) = (base64url(datos), base64url(firma)) else {
        return Err("La clave está incompleta o tiene caracteres de más");
    };
    let firma: [u8; 64] = firma.try_into().map_err(|_| "La firma de la clave está incompleta")?;
    let publica = VerifyingKey::from_bytes(&PUBLICA).map_err(|_| "Clave pública inválida")?;
    publica.verify(&bytes, &Signature::from_bytes(&firma)).map_err(|_| "La clave no es válida (no la firmó quantumdaw.com)")?;
    serde_json::from_slice(&bytes).map_err(|_| "La clave no tiene datos de licencia")
}

/// Bits de todos los plugins con licencia entre las claves guardadas.
pub fn bits(claves: &[String]) -> u32 {
    claves.iter().filter_map(|c| verificar(c).ok()).fold(0, |a, l| a | l.p)
}

/// Qué pide a la app el panel de un plugin Pro.
#[cfg_attr(not(feature = "pro"), allow(dead_code))]
pub enum PedidoPro {
    Licencias,
}

/// Panel de un plugin Pro; en las compilaciones sin el módulo Pro, un aviso.
pub fn panel_pro(ui: &mut egui::Ui, fx: &engine::Fx) -> Option<PedidoPro> {
    #[cfg(feature = "pro")]
    return crate::pro::ui::panel(ui, fx);
    #[cfg(not(feature = "pro"))]
    {
        ui.label(RichText::new(format!("{} {}", tr(fx.kind.name()), tr("viene en la versión oficial de quantumdaw.com: esta compilación no lo incluye, así que el sonido pasa sin procesar."))).color(TEXT_DIM()));
        None
    }
}

/// Página de compra de un plugin.
#[cfg_attr(not(feature = "pro"), allow(dead_code))]
pub fn url_compra(kind: FxKind) -> String {
    url_compra_slug(kind.pro().map_or("", |p| p.1))
}

/// Página de compra de un producto por su identificador en la tienda.
pub fn url_compra_slug(slug: &str) -> String {
    format!("{}/tienda.html#{slug}", web::base())
}

/// Productos Pro: (nombre, bit de licencia, identificador en la tienda, precio en US$, color).
pub fn productos_pro() -> Vec<(&'static str, u32, &'static str, u32, Color32)> {
    let mut v: Vec<_> = FxKind::PRO.iter().filter_map(|&k| k.pro().map(|(bit, slug, precio)| (k.name(), bit, slug, precio, plugins::kind_color(Some(k))))).collect();
    v.push(("Q Drums Pro", engine::pro::BIT_DRUMMER, "drummer-pro", 8, Color32::from_rgb(0xFF, 0x6B, 0x3D)));
    v
}

impl App {
    /// Activa en el motor las licencias guardadas (al iniciar y al agregar una clave).
    pub fn aplicar_licencias(&self) {
        engine::pro::activar(bits(&self.config.licencias));
    }

    /// Agrega una clave: la verifica, la guarda y activa sus plugins.
    pub fn agregar_licencia(&mut self, clave: &str) -> Result<Licencia, &'static str> {
        let l = verificar(clave)?;
        let limpia: String = clave.split_whitespace().collect();
        if !self.config.licencias.contains(&limpia) {
            self.config.licencias.push(limpia);
            self.config.save();
        }
        self.aplicar_licencias();
        Ok(l)
    }

    /// Ventana para pegar una clave de licencia y ver las activas.
    pub fn ventana_licencias(&mut self, ctx: &egui::Context) {
        if !self.licencias_abierta {
            return;
        }
        let mut abierta = true;
        egui::Window::new(tr("Licencias QUANTUM Pro")).open(&mut abierta).collapsible(false).resizable(false).default_width(520.0).anchor(egui::Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.label(RichText::new(tr("Pega aquí la clave que recibiste al comprar en quantumdaw.com. Se verifica sin conexión.")).color(TEXT_DIM()));
            ui.add(egui::TextEdit::multiline(&mut self.licencia_texto).hint_text("QDAW1.…").desired_rows(3).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            ui.horizontal(|ui| {
                let b = egui::Button::new(RichText::new(tr("Activar")).strong().color(BG())).fill(ACCENT());
                if ui.add_enabled(!self.licencia_texto.trim().is_empty(), b).clicked() {
                    let texto = self.licencia_texto.clone();
                    self.licencia_estado = match self.agregar_licencia(&texto) {
                        Ok(l) => {
                            self.licencia_texto.clear();
                            let nombres: Vec<&str> = productos_pro().into_iter().filter(|p| l.p & p.1 != 0).map(|p| p.0).collect();
                            format!("✔ {} {} · {}", tr("Activado:"), nombres.join(", "), l.n)
                        }
                        Err(e) => tr(e).to_string(),
                    };
                }
                if !self.licencia_estado.is_empty() {
                    let ok = self.licencia_estado.starts_with('✔');
                    ui.label(RichText::new(&self.licencia_estado).color(if ok { METER[0] } else { METER[2] }));
                }
            });
            ui.separator();
            let activos = bits(&self.config.licencias);
            for (nombre, bit, slug, precio, color) in productos_pro() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("●").color(color));
                    ui.label(RichText::new(tr(nombre)).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if activos & bit != 0 {
                            ui.label(RichText::new(tr("Con licencia")).color(METER[0]));
                        } else {
                            if ui.button(format!("{} · {precio} US$", tr("Comprar"))).clicked() {
                                ui.ctx().open_url(egui::OpenUrl::new_tab(url_compra_slug(slug)));
                            }
                            ui.label(RichText::new(tr("Modo demo")).color(METER[1]));
                        }
                    });
                });
            }
            let titulares: Vec<String> = self.config.licencias.iter().filter_map(|c| verificar(c).ok()).map(|l| format!("{} ({})", l.n, l.f)).collect();
            if !titulares.is_empty() {
                ui.label(RichText::new(format!("{} {}", tr("A nombre de:"), titulares.join(", "))).size(11.0).color(TEXT_DIM()));
            }
            ui.label(RichText::new(tr("Sin licencia, los plugins Pro suenan completos pero hacen un silencio de un segundo cada 45 s, y sus ajustes no se guardan en el proyecto.")).size(11.0).color(TEXT_DIM()));
        });
        if !abierta {
            self.licencias_abierta = false;
            self.licencia_estado.clear();
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn rechaza_claves_falsas() {
        assert!(verificar("hola").is_err());
        assert!(verificar("QDAW1.e30.AAAA").is_err());
        // Datos válidos con una firma inventada.
        let datos = "eyJwIjoxNSwibiI6InlvIn0";
        let firma = "A".repeat(86);
        assert_eq!(verificar(&format!("QDAW1.{datos}.{firma}")), Err("La clave no es válida (no la firmó quantumdaw.com)"));
        assert_eq!(base64url("TWFu"), Some(b"Man".to_vec()));
        // Una clave real de quantumdaw.com que no activa ningún plugin (p = 0).
        let real = "QDAW1.eyJwIjowLCJuIjoiUHJ1ZWJhIFFVQU5UVU0iLCJmIjoiMjAyNi0xMC0xOSJ9.BADByR2ZLOIxDvrYSIkoAH_7t1i-VzYEYYKKQQYBckqx_WpYRlRT_XLgmZdfTlTpe7fPUWphGAoatn6lDOQtCg";
        assert_eq!(verificar(real), Ok(Licencia { p: 0, n: "Prueba QUANTUM".into(), f: "2026-10-19".into() }));
        // Con saltos de línea (copiada de un correo) también vale; cambiando un dato, ya no.
        assert!(verificar(&real.replace(".BAD", ".\nBAD")).is_ok());
        assert!(verificar(&real.replace("eyJwIjow", "eyJwIjox")).is_err());
        assert_eq!(base64url("TWE"), Some(b"Ma".to_vec()));
    }
}
