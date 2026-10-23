//! Partitura: las pistas MIDI escritas en pentagramas, alineadas y con llave como una partitura
//! impresa. Clave de sol o de fa según el registro, pentagrama de percusión para la batería,
//! instrumentos transpositores escritos como los leen sus intérpretes, armadura detectada, cifrado
//! de acordes encima y páginas A4 para imprimir o guardar en PDF.
//!
//! El diseño produce una lista de dibujo (`Elem`) que se pinta en la app o se exporta a SVG.
//! Los símbolos usan la fuente Bravura (SMuFL, licencia SIL OFL).
use crate::*;
use egui::{Pos2, pos2, vec2};
pub use project::partitura::Clave;

pub const FUENTE: &[u8] = include_bytes!("../../../assets/fuentes/Bravura.otf");

// Glifos SMuFL.
const CLAVE_SOL: char = '\u{E050}';
const CLAVE_FA: char = '\u{E062}';
const CLAVE_PERC: char = '\u{E069}';
const CLAVE_DO: char = '\u{E05C}';
const CLAVE_SOL_8VB: char = '\u{E052}';
const CABEZA_REDONDA: char = '\u{E0A2}';
const CABEZA_BLANCA: char = '\u{E0A3}';
const CABEZA_NEGRA: char = '\u{E0A4}';
const CABEZA_X: char = '\u{E0A9}';
const SOSTENIDO: char = '\u{E262}';
const BEMOL: char = '\u{E260}';
const BECUADRO: char = '\u{E261}';
const PUNTILLO: char = '\u{E1E7}';
/// Corchetes (arriba, abajo) de corchea, semicorchea, fusa y semifusa.
const CORCHETE: [[char; 2]; 4] = [['\u{E240}', '\u{E241}'], ['\u{E242}', '\u{E243}'], ['\u{E244}', '\u{E245}'], ['\u{E246}', '\u{E247}']];
const SILENCIOS: [(f64, char); 7] = [(4.0, '\u{E4E3}'), (2.0, '\u{E4E4}'), (1.0, '\u{E4E5}'), (0.5, '\u{E4E6}'), (0.25, '\u{E4E7}'), (0.125, '\u{E4E8}'), (0.0625, '\u{E4E9}')];
/// Figuras simples en negras: redonda, blanca, negra, corchea, semicorchea, fusa y semifusa.
pub const FIGURAS_BASE: [f64; 7] = [4.0, 2.0, 1.0, 0.5, 0.25, 0.125, 0.0625];

/// Figura simple y puntillos (0, 1 o 2) de una duración escribible de una sola figura.
pub fn base_y_puntos(d: f64) -> Option<(f64, u8)> {
    FIGURAS_BASE.iter().find_map(|&b| [(1.0, 0), (1.5, 1), (1.75, 2)].iter().find(|(f, _)| (d - b * f).abs() < 1e-6).map(|&(_, n)| (b, n)))
}
const DIGITO_COMPAS: u32 = 0xE080;

/// Elemento de dibujo en píxeles; `Glifo` y `Texto` se ubican por su línea base.
pub enum Elem {
    Linea(Pos2, Pos2, f32),
    Glifo(Pos2, char, f32),
    Texto(Pos2, String, f32, bool, u8),
    Curva([Pos2; 4], f32),
}

/// Una parte (pentagrama): nombre completo y abreviado, clave y notas escritas en negras.
pub struct Parte {
    pub nombre: String,
    pub abrev: String,
    pub clave: Clave,
    /// (inicio, duración, nota escrita, cabeza en x, identificador de la nota en su documento).
    pub notas: Vec<(f64, f64, u8, bool, usize)>,
    /// Llave con la parte siguiente (sistema de piano).
    pub llave: bool,
}

/// Ajustes del diseño: compás, armadura, cifrado de acordes y compases mínimos (para editar).
pub struct Conf {
    pub num: u32,
    pub den: u32,
    pub quintas: i32,
    pub acordes: Vec<(f64, String)>,
    pub compases_min: usize,
    /// Anotaciones: (parte, inicio en negras, espacios sobre la quinta línea, texto).
    pub anotaciones: Vec<(usize, f64, f32, String)>,
}

/// Compás dibujado: (número, x inicial y final de la música, puntos (tiempo relativo, x) de cada figura).
pub type MapaCompas = (usize, f32, f32, Vec<(f64, f32)>);

/// Dónde quedó cada cosa en un sistema, para editar con el ratón.
#[derive(Default, Clone)]
pub struct MapaSistema {
    /// Por parte: y de la primera línea (abajo) y clave.
    pub partes: Vec<(f32, Clave)>,
    /// Por compás: (número, x inicial y final de la música, puntos (tiempo relativo, x) de cada figura).
    pub compases: Vec<MapaCompas>,
    /// Cabezas de nota: (posición, parte, identificador).
    pub cabezas: Vec<(Pos2, usize, usize)>,
    /// Cuándo suena cada cabeza (inicio y duración en negras), en el mismo orden que `cabezas`.
    pub sonido: Vec<(f64, f64)>,
    /// Anotaciones: (rectángulo del texto, índice).
    pub anotaciones: Vec<(egui::Rect, usize)>,
}

/// Partitura diseñada: elementos de cada sistema, alto de cada uno y sus mapas.
pub struct Diseno {
    pub sistemas: Vec<Vec<Elem>>,
    pub alto: f32,
    pub mapas: Vec<MapaSistema>,
}

/// Instrumentos transpositores: (palabras del nombre, semitonos escritos sobre el sonido, abreviatura).
pub(crate) const TRANSPOSITORES: [(&[&str], i32, &str); 6] = [
    (&["clarinete", "clarinet"], 2, "Cl. (Sib)"),
    (&["trompeta", "trumpet"], 2, "Tpt. (Sib)"),
    (&["saxo alto", "alto sax"], 9, "Sax. A. (Mib)"),
    (&["saxo tenor", "tenor sax"], 14, "Sax. T. (Sib)"),
    (&["saxo barítono", "saxo baritono", "baritone sax"], 21, "Sax. B. (Mib)"),
    (&["trompa", "corno", "horn"], 7, "Tpa. (Fa)"),
];

/// Batería (General MIDI) en el pentagrama de percusión: (notas, posición en medios espacios desde la
/// primera línea, cabeza en x).
pub(crate) const BATERIA: [(&[u8], i32, bool); 9] = [
    (&[35, 36], 1, false),
    (&[37, 38, 39, 40], 5, false),
    (&[41, 43], 2, false),
    (&[45, 47], 6, false),
    (&[48, 50], 7, false),
    (&[42, 44], 9, true),
    (&[46], 9, true),
    (&[51, 53, 59], 8, true),
    (&[49, 52, 55, 57], 10, true),
];

/// Ortografía de cada clase de nota: (letra 0=Do…6=Si, alteración) con sostenidos o con bemoles.
const CON_SOSTENIDOS: [(i32, i32); 12] = [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)];
const CON_BEMOLES: [(i32, i32); 12] = [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)];
/// Orden de las alteraciones de la armadura (letras) y su posición en clave de sol.
const ORDEN_SOST: [(i32, i32); 7] = [(3, 8), (0, 5), (4, 9), (1, 6), (5, 3), (2, 7), (6, 4)];
const ORDEN_BEM: [(i32, i32); 7] = [(6, 4), (2, 7), (5, 3), (1, 6), (4, 2), (0, 5), (3, 1)];

/// Armadura más probable (quintas: -6 a 6) según cuánto tiempo suena cada clase de nota.
pub(crate) fn detectar_armadura(partes: &[Parte]) -> i32 {
    let mut peso = [0f64; 12];
    for p in partes.iter().filter(|p| p.clave != Clave::Percusion) {
        for n in &p.notas {
            peso[(n.2 % 12) as usize] += n.1;
        }
    }
    const MAYOR: [usize; 7] = [0, 2, 4, 5, 7, 9, 11];
    let quintas = |raiz: usize| -> i32 { [0, -5, 2, -3, 4, -1, 6, 1, -4, 3, -2, 5][raiz] };
    (0..12)
        .map(|r| (MAYOR.iter().map(|i| peso[(r + i) % 12]).sum::<f64>(), quintas(r)))
        .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.abs().cmp(&a.1.abs())))
        .map_or(0, |m| m.1)
}

/// Divide una duración (en negras) en figuras escribibles: redonda, blanca con puntillo, blanca…
fn figuras(mut d: f64) -> Vec<f64> {
    // Una figura con doble puntillo elegida a propósito se respeta; si no, se divide con ligaduras.
    if base_y_puntos(d).is_some() {
        return vec![d];
    }
    let mut v = vec![];
    for f in [4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.375, 0.25, 0.1875, 0.125, 0.09375, 0.0625] {
        while d + 1e-6 >= f {
            v.push(f);
            d -= f;
        }
    }
    v
}

impl App {
    /// Partes de la partitura: pistas MIDI seleccionadas (o todas si no hay ninguna seleccionada).
    fn partes(&self) -> Vec<Parte> {
        let a_negras = 4.0 / self.engine.unit.load(Relaxed).max(1) as f64;
        let algunas = !self.partitura_todas && self.s.tracks.iter().any(|t| t.selected && t.midi());
        self.s
            .tracks
            .iter()
            .filter(|t| t.midi() && (!algunas || t.selected))
            .map(|t| {
                let nombre_min = t.name.to_lowercase();
                let bateria = t.instrument == "bateria" || nombre_min.contains("bater") || nombre_min.contains("drum");
                let transp = TRANSPOSITORES.iter().find(|x| x.0.iter().any(|k| nombre_min.contains(k)));
                let mut notas = vec![];
                for c in &t.clips {
                    let Some(ns) = c.notes() else { continue };
                    for n in ns.iter() {
                        let ini = c.origin() + n.start as i64;
                        if ini < c.start as i64 || ini >= c.end() as i64 {
                            continue;
                        }
                        let fin = (ini + n.len as i64).min(c.end() as i64);
                        // A negras y cuantizado a semicorcheas.
                        let q = |f: i64| (self.beats(f as f64) * a_negras * 4.0).round() / 4.0;
                        let (a, b) = (q(ini), q(fin).max(q(ini) + 0.25));
                        let escrita = (n.key as i32 + transp.map_or(0, |x| x.1)).clamp(0, 127) as u8;
                        notas.push((a, b - a, escrita, false, usize::MAX));
                    }
                }
                notas.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
                let clave = if bateria {
                    Clave::Percusion
                } else {
                    let mut alturas: Vec<u8> = notas.iter().map(|n| n.2).collect();
                    alturas.sort();
                    if alturas.get(alturas.len() / 2).is_some_and(|&m| m < 57) { Clave::Fa } else { Clave::Sol }
                };
                let abrev = transp.map_or_else(|| t.name.chars().take(4).collect::<String>() + ".", |x| x.2.to_string());
                let nombre = transp.map_or_else(|| t.name.clone(), |x| format!("{} ({})", t.name, &x.2[x.2.find('(').map_or(0, |i| i + 1)..x.2.len().saturating_sub(1)]));
                Parte { nombre, abrev, clave, notas, llave: false }
            })
            .filter(|p| !p.notas.is_empty())
            .collect()
    }

    /// Diseña las pistas MIDI como partitura (vista PARTITURA e impresión).
    pub fn disenar_partitura(&self, ancho: f32, sp: f32) -> Diseno {
        let partes = self.partes();
        if partes.is_empty() {
            return Diseno { sistemas: vec![], alto: 0.0, mapas: vec![] };
        }
        let den = self.engine.unit.load(Relaxed).max(1);
        let a_negras = 4.0 / den as f64;
        let conf = Conf {
            num: self.engine.beats.load(Relaxed).max(1),
            den,
            quintas: detectar_armadura(&partes),
            acordes: self.s.chords.iter().map(|(a, _, n)| ((self.beats(*a as f64) * a_negras * 4.0).round() / 4.0, n.clone())).collect(),
            compases_min: 1,
            anotaciones: vec![],
        };
        disenar(&partes, &conf, ancho, sp)
    }

    /// Posición del cursor de reproducción en negras (para seguirlo en la partitura).
    pub fn negras_en_cursor(&self) -> f64 {
        self.beats(self.pos() as f64) * 4.0 / self.engine.unit.load(Relaxed).max(1) as f64
    }

    /// Vista de la partitura (pestaña del panel inferior).
    pub fn vista_partitura(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr("Partitura")).strong());
            ui.selectable_value(&mut self.partitura_todas, true, tr("Todas las pistas MIDI"));
            ui.selectable_value(&mut self.partitura_todas, false, tr("Solo las seleccionadas"));
            ui.add(egui::Slider::new(&mut self.partitura_sp, 5.0..=14.0).show_value(false).text(tr("Tamaño")));
            if ui.add(egui::Button::new(egui::RichText::new(tr("Abrir QUANTUM Partitura")).strong())).on_hover_text(tr("Editor de partituras: escribe, edita, cambia claves y transcribe lo que tocas")).clicked() {
                self.partitura_app.abierto = true;
            }
            if ui.add(egui::Button::new(egui::RichText::new(tr("Imprimir o guardar PDF…")).color(BG()).strong()).fill(ACCENT())).on_hover_text("Ctrl+P").clicked() || ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
                let r = self.imprimir_partitura();
                self.report(tr("Partitura abierta en el navegador: imprímela o guárdala como PDF"), r);
            }
        });
        let sp = self.partitura_sp;
        egui::Frame::new().fill(Color32::from_rgb(0xFA, 0xF8, 0xF2)).corner_radius(widgets::rr(6.0)).inner_margin(sp * 2.0).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                let ancho = ui.available_width().max(400.0);
                let d = self.disenar_partitura(ancho, sp);
                if d.sistemas.is_empty() {
                    ui.label(egui::RichText::new(tr("No hay notas MIDI para escribir. Selecciona pistas MIDI con notas.")).color(Color32::from_gray(90)));
                    return;
                }
                let (r, _) = ui.allocate_exact_size(vec2(ancho, d.alto * d.sistemas.len() as f32), egui::Sense::hover());
                let p = ui.painter_at(r);
                for (k, s) in d.sistemas.iter().enumerate() {
                    pintar(&p, s, r.min.to_vec2() + vec2(0.0, k as f32 * d.alto), Color32::from_gray(20));
                }
                // Lo que suena: cursor y notas resaltadas; al reproducir, la vista sigue al cursor.
                let compas = self.engine.beats.load(Relaxed).max(1) as f64 * 4.0 / self.engine.unit.load(Relaxed).max(1) as f64;
                let cursor = pintar_reproduccion(&p, &d, r.min, self.negras_en_cursor(), compas, sp);
                if self.playing() {
                    if let Some(c) = cursor {
                        ui.scroll_to_rect(c, Some(egui::Align::Center));
                    }
                    ui.ctx().request_repaint();
                }
            });
        });
    }

    /// Partitura de las pistas en páginas A4 para imprimir o guardar como PDF.
    fn imprimir_partitura(&mut self) -> anyhow::Result<()> {
        let titulo = if self.meta.title.is_empty() { self.dir.file_name().unwrap_or_default().to_string_lossy().to_string() } else { self.meta.title.clone() };
        let compositor = self.meta.composer.clone();
        let html = pagina_html(&titulo, &compositor, |ancho, sp| self.disenar_partitura(ancho, sp))?;
        self.abrir_html_partitura(&titulo, html)
    }

    /// Guarda el HTML de una partitura en Rendered/ y lo abre en el navegador.
    pub(crate) fn abrir_html_partitura(&self, titulo: &str, html: String) -> anyhow::Result<()> {
        let ruta = self.dir.join(project::RENDER_DIR).join(format!("{} - partitura.html", safe(titulo)));
        fs::create_dir_all(ruta.parent().unwrap_or(&self.dir))?;
        fs::write(&ruta, html)?;
        informes::abrir_carpeta(&ruta);
        Ok(())
    }
}

/// Diseña una partitura (de las pistas o de un documento de QUANTUM Partitura) en sistemas de
/// `ancho` píxeles con espacio de pentagrama `sp`.
pub fn disenar(partes: &[Parte], conf: &Conf, ancho: f32, sp: f32) -> Diseno {
        if partes.is_empty() {
            return Diseno { sistemas: vec![], alto: 0.0, mapas: vec![] };
        }
        let (num, den, quintas) = (conf.num, conf.den, conf.quintas);
        let compas = num as f64 * 4.0 / den as f64;
        let fin = partes.iter().flat_map(|p| p.notas.iter().map(|n| n.0 + n.1)).fold(0.0, f64::max);
        let n_compases = ((fin / compas - 1e-9).ceil().max(1.0) as usize).max(conf.compases_min);
        let acordes = &conf.acordes;
        // Eventos por parte y compás: (inicio en el compás, duración, notas (altura, x), ¿ligada a la siguiente?).
        type Evento = (f64, f64, Vec<(u8, bool, usize)>, bool);
        // Notas que empiezan juntas: (inicio, duración, alturas).
        type Grupo = (f64, f64, Vec<(u8, bool, usize)>);
        let eventos: Vec<Vec<Vec<Evento>>> = partes
            .iter()
            .map(|p| {
                let mut por_compas: Vec<Vec<Evento>> = vec![vec![]; n_compases];
                // Una voz: las notas que empiezan juntas forman un acorde y duran hasta la siguiente.
                let mut grupos: Vec<Grupo> = vec![];
                for &(a, d, k, x, id) in &p.notas {
                    match grupos.last_mut() {
                        Some(g) if (g.0 - a).abs() < 1e-6 => {
                            g.1 = g.1.max(d);
                            g.2.push((k, x, id));
                        }
                        _ => grupos.push((a, d, vec![(k, x, id)])),
                    }
                }
                for k in 0..grupos.len() {
                    if let Some(sig) = grupos.get(k + 1).map(|g| g.0) {
                        grupos[k].1 = grupos[k].1.min(sig - grupos[k].0);
                    }
                }
                let mut t = 0.0;
                let agregar = |desde: f64, hasta: f64, alturas: Vec<(u8, bool, usize)>, por_compas: &mut Vec<Vec<Evento>>| {
                    let mut a = desde;
                    while a + 1e-6 < hasta {
                        let m = (a / compas).floor() as usize;
                        let fin_compas = (m as f64 + 1.0) * compas;
                        let b = hasta.min(fin_compas);
                        let figs = figuras(b - a);
                        let mut x = a;
                        for (i, f) in figs.iter().enumerate() {
                            let ligada = !alturas.is_empty() && (i + 1 < figs.len() || b < hasta - 1e-6);
                            if let Some(c) = por_compas.get_mut(m) {
                                c.push((x - m as f64 * compas, *f, alturas.clone(), ligada));
                            }
                            x += f;
                        }
                        a = b;
                    }
                };
                for (a, d, alturas) in grupos {
                    if a > t + 1e-6 {
                        agregar(t, a, vec![], &mut por_compas);
                    }
                    agregar(a, a + d, alturas, &mut por_compas);
                    t = a + d;
                }
                if t < n_compases as f64 * compas {
                    agregar(t, n_compases as f64 * compas, vec![], &mut por_compas);
                }
                por_compas
            })
            .collect();

        // Ancho natural de cada compás: el de la parte con más figuras.
        let ancho_ev = |d: f64| sp * (2.6 + 1.8 * ((d * 4.0).log2().max(0.0)) as f32 + if base_y_puntos(d).is_some_and(|b| b.1 > 0) { 0.6 } else { 0.0 });
        let naturales: Vec<f32> = (0..n_compases).map(|m| eventos.iter().map(|p| p[m].iter().map(|e| ancho_ev(e.1)).sum::<f32>()).fold(0.0, f32::max) + sp * 2.0).collect();
        let nombre_w = sp * 13.0;
        let cabecera = sp * (4.0 + 1.2 * quintas.unsigned_abs() as f32 + 3.0);
        let alto_parte = sp * 4.0;
        let separacion = sp * 9.0;
        let alto_sistema = partes.len() as f32 * alto_parte + (partes.len() as f32 - 1.0) * separacion;
        let mut sistemas = vec![];
        let mut mapas = vec![];
        let mut m0 = 0;
        while m0 < n_compases {
            let disponible = ancho - nombre_w - cabecera - sp;
            let mut m1 = m0;
            let mut usado = 0.0;
            while m1 < n_compases && (m1 == m0 || usado + naturales[m1] <= disponible) {
                usado += naturales[m1];
                m1 += 1;
            }
            let estirar = if m1 < n_compases { disponible / usado } else { 1.0f32.max((disponible / usado).min(1.4)) };
            let mut el = vec![];
            let mut mapa = MapaSistema::default();
            let top = sp * 5.0;
            let x_ini = nombre_w;
            // Llave que une los pentagramas y línea inicial.
            let (y_arriba, y_abajo) = (top, top + alto_sistema);
            el.push(Elem::Linea(pos2(x_ini, y_arriba), pos2(x_ini, y_abajo), sp * 0.16));
            let grupos = partes.len() - partes.iter().filter(|p| p.llave).count();
            if grupos > 1 {
                el.push(Elem::Linea(pos2(x_ini - sp * 0.8, y_arriba - sp * 0.4), pos2(x_ini - sp * 0.8, y_abajo + sp * 0.4), sp * 0.5));
            }
            let x_llave = x_ini - if grupos > 1 { sp * 1.5 } else { sp * 0.35 };
            for (pi, p) in partes.iter().enumerate() {
                let y0 = top + pi as f32 * (alto_parte + separacion);
                let base = y0 + alto_parte; // primera línea (abajo)
                let y_pos = |pos: i32| base - pos as f32 * sp / 2.0;
                mapa.partes.push((base, p.clave));
                let ultimo_x = x_ini + cabecera + usado * estirar;
                for l in 0..5 {
                    el.push(Elem::Linea(pos2(x_ini, y_pos(l * 2)), pos2(ultimo_x, y_pos(l * 2)), sp * 0.11));
                }
                let nombre = if m0 == 0 { p.nombre.clone() } else { p.abrev.clone() };
                let con_llave = p.llave && pi + 1 < partes.len();
                let bajo_llave = pi > 0 && partes[pi - 1].llave;
                if con_llave {
                    // Llave del piano y nombre centrado entre los dos pentagramas.
                    let (a, b) = (y_pos(8), base + separacion + alto_parte);
                    llave(&mut el, x_llave, a, b, sp);
                    el.push(Elem::Texto(pos2(x_llave - sp * 2.0, (a + b) / 2.0 + sp * 0.4), nombre, sp * 1.25, false, 2));
                } else if !bajo_llave {
                    el.push(Elem::Texto(pos2(x_ini - sp * 1.6, y_pos(4) + sp * 0.4), nombre, sp * 1.25, false, 2));
                }
                // Clave, armadura y compás (este solo en el primer sistema).
                let (glifo, pos_clave) = match p.clave {
                    Clave::Sol => (CLAVE_SOL, 2),
                    Clave::Sol8vb => (CLAVE_SOL_8VB, 2),
                    Clave::Fa => (CLAVE_FA, 6),
                    Clave::Alto => (CLAVE_DO, 4),
                    Clave::Tenor => (CLAVE_DO, 6),
                    Clave::Percusion => (CLAVE_PERC, 4),
                };
                // La armadura se escribe en la misma letra que en clave de sol, dentro del pentagrama.
                let d = 30 - p.clave.paso_primera_linea();
                let desplaza = d - 7 * (d as f32 / 7.0).round() as i32;
                el.push(Elem::Glifo(pos2(x_ini + sp * 0.8, y_pos(pos_clave)), glifo, sp * 4.0));
                let mut x = x_ini + sp * 4.0;
                if p.clave != Clave::Percusion {
                    let orden = if quintas >= 0 { &ORDEN_SOST } else { &ORDEN_BEM };
                    for &(_, pos) in orden.iter().take(quintas.unsigned_abs() as usize) {
                        el.push(Elem::Glifo(pos2(x, y_pos(pos + desplaza)), if quintas >= 0 { SOSTENIDO } else { BEMOL }, sp * 4.0));
                        x += sp * 1.2;
                    }
                }
                if m0 == 0 {
                    let cifra = |n: u32| n.to_string().chars().filter_map(|c| char::from_u32(DIGITO_COMPAS + c.to_digit(10).unwrap_or(0))).collect::<String>();
                    el.push(Elem::Texto(pos2(x + sp * 0.3, y_pos(6)), cifra(num), sp * 4.0, true, 9));
                    el.push(Elem::Texto(pos2(x + sp * 0.3, y_pos(2)), cifra(den), sp * 4.0, true, 9));
                }
                // Compases.
                let mut xc = x_ini + cabecera;
                for m in m0..m1 {
                    let w = naturales[m] * estirar;
                    if pi == 0 {
                        el.push(Elem::Texto(pos2(xc + sp * 0.2, y_pos(8) - sp * 3.6), (m + 1).to_string(), sp * 1.0, false, 0));
                        for (b, nombre) in acordes.iter().filter(|(b, _)| (*b / compas).floor() as usize == m) {
                            let rel = (b - m as f64 * compas) / compas;
                            el.push(Elem::Texto(pos2(xc + sp + rel as f32 * (w - sp * 2.0), y_pos(8) - sp * 2.0), nombre.clone(), sp * 1.6, true, 0));
                        }
                    }
                    let mut alteraciones: std::collections::HashMap<i32, i32> = Default::default();
                    let mut anterior: Option<(f32, Vec<f32>)> = None;
                    let total: f32 = eventos[pi][m].iter().map(|e| ancho_ev(e.1)).sum::<f32>().max(1.0);
                    let mut xe = xc + sp;
                    if pi == 0 {
                        mapa.compases.push((m, xc + sp, xc + w - sp, vec![]));
                    }
                    let k_compas = mapa.compases.iter().position(|c| c.0 == m);
                    for &(t_rel, dur, ref alturas, ligada) in &eventos[pi][m] {
                        let paso = ancho_ev(dur) / total * (w - sp * 2.0);
                        if let Some(c) = k_compas.and_then(|k| mapa.compases.get_mut(k)) {
                            c.3.push((t_rel, xe));
                        }
                        if alturas.is_empty() {
                            // Silencio (redonda si ocupa el compás entero).
                            let (valor, glifo) = SILENCIOS.iter().copied().find(|(v, _)| dur + 1e-6 >= *v).unwrap_or(SILENCIOS[6]);
                            let entero = (dur - compas).abs() < 1e-6;
                            let pos = if valor >= 4.0 || entero { 6 } else { 4 };
                            let xr = if entero { xc + w / 2.0 - sp * 0.6 } else { xe };
                            el.push(Elem::Glifo(pos2(xr, y_pos(pos)), if entero { SILENCIOS[0].1 } else { glifo }, sp * 4.0));
                            let puntos = if entero { 0 } else { base_y_puntos(dur).map_or(0, |b| b.1) };
                            for k in 0..puntos {
                                el.push(Elem::Glifo(pos2(xr + sp * (1.6 + 0.6 * k as f32), y_pos(5)), PUNTILLO, sp * 4.0));
                            }
                            anterior = None;
                            xe += paso;
                            continue;
                        }
                        let (base_dur, puntos) = base_y_puntos(dur).unwrap_or((dur, 0));
                        let cabeza_ch = match base_dur {
                            d if d >= 4.0 - 1e-6 => CABEZA_REDONDA,
                            d if d >= 2.0 - 1e-6 => CABEZA_BLANCA,
                            _ => CABEZA_NEGRA,
                        };
                        // Posición de cada nota en el pentagrama y su alteración.
                        let posiciones: Vec<(i32, Option<char>, bool)> = alturas
                            .iter()
                            .map(|&(k, _, _)| {
                                if p.clave == Clave::Percusion {
                                    let (pos, x) = BATERIA.iter().find(|b| b.0.contains(&k)).map_or((4, false), |b| (b.1, b.2));
                                    return (pos, None, x);
                                }
                                let (letra, alt) = if quintas >= 0 { CON_SOSTENIDOS } else { CON_BEMOLES }[(k % 12) as usize];
                                let paso = (k as i32 / 12 - 1) * 7 + letra;
                                let pos = paso - p.clave.paso_primera_linea();
                                let en_armadura = {
                                    let orden = if quintas >= 0 { &ORDEN_SOST } else { &ORDEN_BEM };
                                    if orden.iter().take(quintas.unsigned_abs() as usize).any(|o| o.0 == letra) { quintas.signum() } else { 0 }
                                };
                                let vigente = *alteraciones.get(&paso).unwrap_or(&en_armadura);
                                let signo = (alt != vigente).then_some(match alt {
                                    1 => SOSTENIDO,
                                    -1 => BEMOL,
                                    _ => BECUADRO,
                                });
                                alteraciones.insert(paso, alt);
                                (pos, signo, false)
                            })
                            .collect();
                        let mut ys = vec![];
                        for (n_alt, &(pos, signo, x_cab)) in posiciones.iter().enumerate() {
                            let y = y_pos(pos);
                            ys.push(y);
                            mapa.cabezas.push((pos2(xe + sp * 0.6, y), pi, alturas[n_alt].2));
                            mapa.sonido.push((m as f64 * compas + t_rel, dur));
                            el.push(Elem::Glifo(pos2(xe, y), if x_cab { CABEZA_X } else { cabeza_ch }, sp * 4.0));
                            if let Some(s) = signo {
                                el.push(Elem::Glifo(pos2(xe - sp * 1.3, y), s, sp * 4.0));
                            }
                            let yd = if pos % 2 == 0 { y_pos(pos + 1) } else { y };
                            for k in 0..puntos {
                                el.push(Elem::Glifo(pos2(xe + sp * (1.6 + 0.6 * k as f32), yd), PUNTILLO, sp * 4.0));
                            }
                            // Líneas adicionales.
                            for l in (pos..=-2).filter(|l| l % 2 == 0).chain((10..=pos).filter(|l| l % 2 == 0)) {
                                el.push(Elem::Linea(pos2(xe - sp * 0.4, y_pos(l)), pos2(xe + sp * 1.6, y_pos(l)), sp * 0.12));
                            }
                        }
                        // Plica y corchetes (corcheas y semicorcheas).
                        if base_dur < 4.0 - 1e-6 {
                            let media = posiciones.iter().map(|p| p.0).sum::<i32>() as f32 / posiciones.len() as f32;
                            // La batería va con las plicas hacia arriba.
                            let arriba = media < 4.0 || p.clave == Clave::Percusion;
                            let (ymin, ymax) = (ys.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max));
                            let xs = if arriba { xe + sp * 1.18 } else { xe + sp * 0.06 };
                            // Corchete: 0 corchea, 1 semicorchea, 2 fusa, 3 semifusa (la plica se alarga con cada uno).
                            let corchete = [0.5, 0.25, 0.125, 0.0625].iter().position(|v| (base_dur - v).abs() < 1e-6);
                            let largo = sp * (3.5 + 0.75 * corchete.unwrap_or(0).saturating_sub(1) as f32);
                            let (y1, y2) = if arriba { (ymax, ymin - largo) } else { (ymin, ymax + largo) };
                            el.push(Elem::Linea(pos2(xs, y1), pos2(xs, y2), sp * 0.12));
                            if let Some(c) = corchete {
                                el.push(Elem::Glifo(pos2(xs - sp * 0.06, y2), CORCHETE[c][if arriba { 0 } else { 1 }], sp * 4.0));
                            }
                        }
                        // Ligadura con la figura anterior.
                        if let Some((xa, ya)) = anterior.take() {
                            for (y_a, y_b) in ya.iter().zip(&ys) {
                                let (a, b) = (pos2(xa + sp * 1.3, *y_a + sp * 0.6), pos2(xe - sp * 0.1, *y_b + sp * 0.6));
                                el.push(Elem::Curva([a, a + vec2(sp, sp * 0.9), b + vec2(-sp, sp * 0.9), b], sp * 0.14));
                            }
                        }
                        if ligada {
                            anterior = Some((xe, ys));
                        }
                        xe += paso;
                    }
                    xc += w;
                    // En el piano la barra cruza de un pentagrama al otro.
                    let y_fin = if con_llave { base + separacion } else { y_pos(0) };
                    el.push(Elem::Linea(pos2(xc, y_fin), pos2(xc, y_pos(8)), sp * 0.12));
                }
                if m1 == n_compases {
                    // Doble barra final.
                    let y_fin = if con_llave { base + separacion } else { y_pos(0) };
                    el.push(Elem::Linea(pos2(xc + sp * 0.5, y_fin), pos2(xc + sp * 0.5, y_pos(8)), sp * 0.5));
                }
            }
            // Anotaciones de este sistema, en el tiempo y la altura donde se pusieron.
            for (k, (pi, t, alto, texto)) in conf.anotaciones.iter().enumerate() {
                let m = (t / compas).floor().max(0.0) as usize;
                let Some(c) = mapa.compases.iter().find(|c| c.0 == m) else { continue };
                let Some(&(base, _)) = mapa.partes.get(*pi) else { continue };
                let x = tiempo_a_x(&c.3, c.1, c.2, compas, t - m as f64 * compas);
                let y = base - sp * 4.0 - alto * sp;
                let tam = sp * 1.5;
                let texto = if texto.is_empty() { "…".to_string() } else { texto.clone() };
                let ancho_t = texto.chars().count() as f32 * tam * 0.55;
                mapa.anotaciones.push((egui::Rect::from_min_max(pos2(x - sp * 0.3, y - tam), pos2(x + ancho_t + sp * 0.3, y + sp * 0.4)), k));
                el.push(Elem::Texto(pos2(x, y), texto, tam, true, 0));
            }
            sistemas.push(el);
            mapas.push(mapa);
            m0 = m1;
        }
        Diseno { sistemas, alto: top_alto(alto_sistema, sp), mapas }
    }

/// Llave de piano de `y0` a `y1` con la punta a la izquierda de `x` (dos curvas por mitad dan el grosor).
fn llave(el: &mut Vec<Elem>, x: f32, y0: f32, y1: f32, sp: f32) {
    let (h, ym, xp) = (y1 - y0, (y0 + y1) / 2.0, x - sp * 1.3);
    for (ancho, g) in [(0.0, sp * 0.14), (sp * 0.45, sp * 0.16), (sp * 0.22, sp * 0.26), (sp * 0.34, sp * 0.24)] {
        for s in [-1.0f32, 1.0] {
            let extremo = if s < 0.0 { y0 } else { y1 };
            let c1 = pos2(x - sp * 0.95 - ancho, extremo - s * h * 0.06);
            let c2 = pos2(x - sp * 0.15 - ancho, ym + s * h * 0.12);
            el.push(Elem::Curva([pos2(x, extremo), c1, c2, pos2(xp, ym)], g));
        }
    }
}

/// x de un tiempo dentro del compás, interpolando entre las figuras dibujadas.
pub(crate) fn tiempo_a_x(puntos: &[(f64, f32)], x0: f32, x1: f32, compas: f64, t: f64) -> f32 {
    let mut v: Vec<(f64, f32)> = vec![(0.0, x0)];
    v.extend(puntos.iter().copied().filter(|p| p.0 > 1e-9));
    v.push((compas, x1));
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    for w in v.windows(2) {
        if t <= w[1].0 + 1e-9 {
            let f = ((t - w[0].0) / (w[1].0 - w[0].0).max(1e-9)) as f32;
            return w[0].1 + (w[1].1 - w[0].1) * f;
        }
    }
    x1
}

/// Páginas A4 (HTML con la fuente musical incluida) de una partitura diseñada por `disenar`.
pub(crate) fn pagina_html(titulo: &str, compositor: &str, disenar: impl FnOnce(f32, f32) -> Diseno) -> anyhow::Result<String> {
    // A4 a 96 ppp: 794 × 1123 px con márgenes de 15 mm.
    let (ancho, alto_pag, margen) = (794.0f32, 1123.0f32, 57.0f32);
    let d = disenar(ancho - margen * 2.0, 6.0);
    anyhow::ensure!(!d.sistemas.is_empty(), "no hay notas para escribir");
    let (sistemas, alto) = (d.sistemas, d.alto);
    let por_pagina = (((alto_pag - margen * 2.0 - 60.0) / alto).floor() as usize).max(1);
    let mut html = format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><title>{t}</title><style>@font-face{{font-family:Bravura;src:url(data:font/otf;base64,{f})}}@page{{size:A4;margin:0}}body{{margin:0;background:#888}}.pagina{{width:{ancho}px;height:{alto_pag}px;background:#fff;margin:12px auto;page-break-after:always;position:relative}}@media print{{body{{background:#fff}}.pagina{{margin:0}}}}svg{{position:absolute;left:{margen}px}}h1{{font:600 22px sans-serif;text-align:center;margin:0;padding-top:{margen}px}}h2{{font:13px sans-serif;text-align:right;margin:4px {margen}px 0}}</style></head><body>",
        t = html_escapar(titulo),
        f = base64(FUENTE)
    );
    for (n, pagina) in sistemas.chunks(por_pagina).enumerate() {
        html += "<div class=\"pagina\">";
        if n == 0 {
            html += &format!("<h1>{}</h1><h2>{}</h2>", html_escapar(titulo), html_escapar(compositor));
        }
        let y0 = if n == 0 { margen + 60.0 } else { margen };
        for (k, s) in pagina.iter().enumerate() {
            html += &format!("<svg style=\"top:{}px\" width=\"{}\" height=\"{alto}\" viewBox=\"0 0 {} {alto}\">", y0 + k as f32 * alto, ancho - margen * 2.0, ancho - margen * 2.0);
            html += &svg(s);
            html += "</svg>";
        }
        html += "</div>";
    }
    html += "</body></html>";
    Ok(html)
}

/// Dibuja lo que suena en el tiempo `t` (en negras): una línea a lo largo del sistema y las notas que
/// están sonando resaltadas. Devuelve el rectángulo del cursor (para que la vista lo siga).
pub fn pintar_reproduccion(p: &egui::Painter, d: &Diseno, origen: Pos2, t: f64, compas: f64, sp: f32) -> Option<egui::Rect> {
    let color = Color32::from_rgb(0x1E, 0x88, 0xE5);
    let m = (t / compas).floor().max(0.0) as usize;
    let mut cursor = None;
    for (k, mapa) in d.mapas.iter().enumerate() {
        let dy = origen.y + k as f32 * d.alto;
        for (j, &(c, _, _)) in mapa.cabezas.iter().enumerate() {
            if let Some(&(a, dur)) = mapa.sonido.get(j)
                && t >= a - 1e-6
                && t < a + dur - 1e-6
            {
                let q = pos2(origen.x + c.x, dy + c.y);
                p.circle_filled(q, sp * 1.1, color.gamma_multiply(0.35));
                p.circle_stroke(q, sp * 0.9, egui::Stroke::new(sp * 0.22, color));
            }
        }
        let Some(c) = mapa.compases.iter().find(|c| c.0 == m) else { continue };
        let (Some(arriba), Some(abajo)) = (mapa.partes.first(), mapa.partes.last()) else { continue };
        let x = origen.x + tiempo_a_x(&c.3, c.1, c.2, compas, t - m as f64 * compas) - sp * 0.3;
        let (y0, y1) = (dy + arriba.0 - sp * 6.0, dy + abajo.0 + sp * 2.0);
        p.line_segment([pos2(x, y0), pos2(x, y1)], egui::Stroke::new(sp * 0.3, color.gamma_multiply(0.85)));
        cursor = Some(egui::Rect::from_min_max(pos2(x - sp * 4.0, y0), pos2(x + sp * 4.0, y1)));
    }
    cursor
}

/// Alto de un sistema con su margen arriba (acordes y números de compás) y abajo.
fn top_alto(alto_sistema: f32, sp: f32) -> f32 {
    alto_sistema + sp * 12.0
}

/// Pinta una lista de dibujo en egui desplazada en `d`.
pub fn pintar(p: &egui::Painter, el: &[Elem], d: egui::Vec2, color: Color32) {
    let musica = |t: f32| egui::FontId::new(t, egui::FontFamily::Name("musica".into()));
    for e in el {
        match e {
            Elem::Linea(a, b, w) => _ = p.line_segment([*a + d, *b + d], egui::Stroke::new(*w, color)),
            Elem::Curva(c, w) => _ = p.add(egui::epaint::CubicBezierShape::from_points_stroke(c.map(|q| q + d), false, Color32::TRANSPARENT, egui::Stroke::new(*w, color))),
            Elem::Glifo(q, c, t) => texto_base(p, *q + d, &c.to_string(), musica(*t), color, 0),
            Elem::Texto(q, s, t, negrita, alinear) => {
                // Alinear 9 = cifras de compás (fuente musical, centradas).
                let fuente = if *alinear == 9 { musica(*t) } else { egui::FontId::proportional(*t) };
                let color = if *negrita && *alinear != 9 { Color32::from_rgb(0x10, 0x30, 0x60) } else { color };
                texto_base(p, *q + d, s, fuente, color, if *alinear == 9 { 1 } else { *alinear });
            }
        }
    }
}

/// Texto con su línea base en `q` (0 izquierda, 1 centro, 2 derecha).
pub(crate) fn texto_base(p: &egui::Painter, q: Pos2, s: &str, fuente: egui::FontId, color: Color32, alinear: u8) {
    let g = p.layout_no_wrap(s.to_string(), fuente, color);
    let base = g.rows.first().map_or(0.0, |r| r.pos.y + r.row.glyphs.first().map_or(0.0, |gl| gl.pos.y));
    let dx = match alinear {
        1 => g.size().x / 2.0,
        2 => g.size().x,
        _ => 0.0,
    };
    p.galley(pos2(q.x - dx, q.y - base), g, color);
}

/// Una lista de dibujo como SVG (la fuente musical se llama Bravura en la página).
pub(crate) fn svg(el: &[Elem]) -> String {
    let mut s = String::new();
    for e in el {
        match e {
            Elem::Linea(a, b, w) => s += &format!("<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#000\" stroke-width=\"{w:.2}\"/>", a.x, a.y, b.x, b.y),
            Elem::Curva(c, w) => s += &format!("<path d=\"M{:.1} {:.1}C{:.1} {:.1} {:.1} {:.1} {:.1} {:.1}\" fill=\"none\" stroke=\"#000\" stroke-width=\"{w:.2}\"/>", c[0].x, c[0].y, c[1].x, c[1].y, c[2].x, c[2].y, c[3].x, c[3].y),
            Elem::Glifo(q, c, t) => s += &format!("<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"Bravura\" font-size=\"{t:.1}\">&#x{:X};</text>", q.x, q.y, *c as u32),
            Elem::Texto(q, txt, t, negrita, alinear) => {
                let (familia, anclaje) = match alinear {
                    9 => ("Bravura", "middle"),
                    1 => ("sans-serif", "middle"),
                    2 => ("sans-serif", "end"),
                    _ => ("sans-serif", "start"),
                };
                let contenido: String = if *alinear == 9 { txt.chars().map(|c| format!("&#x{:X};", c as u32)).collect() } else { html_escapar(txt) };
                s += &format!("<text x=\"{:.1}\" y=\"{:.1}\" font-family=\"{familia}\" font-size=\"{t:.1}\" text-anchor=\"{anclaje}\"{}>{contenido}</text>", q.x, q.y, if *negrita && *alinear != 9 { " font-weight=\"600\"" } else { "" });
            }
        }
    }
    s
}

pub(crate) fn html_escapar(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub(crate) fn base64(datos: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(datos.len().div_ceil(3) * 4);
    for c in datos.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= c.len() { T[(n >> (18 - 6 * i)) as usize & 63] as char } else { '=' });
        }
    }
    s
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn figuras_y_base64() {
        assert_eq!(figuras(4.0), vec![4.0]);
        assert_eq!(figuras(3.25), vec![3.0, 0.25]);
        assert_eq!(figuras(3.5), vec![3.5]);
        assert_eq!(figuras(1.25), vec![1.0, 0.25]);
        // Doble puntillo y fusas.
        assert_eq!(figuras(1.75), vec![1.75]);
        assert_eq!(figuras(0.3125), vec![0.25, 0.0625]);
        assert_eq!(base_y_puntos(0.1875), Some((0.125, 1)));
        assert_eq!(base_y_puntos(3.5), Some((2.0, 2)));
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
    }

    #[test]
    fn armadura() {
        let parte = |notas: &[u8]| Parte { nombre: String::new(), abrev: String::new(), clave: Clave::Sol, notas: notas.iter().map(|&k| (0.0, 1.0, k, false, 0)).collect(), llave: false };
        // Sol mayor (con Fa#): un sostenido. Fa mayor (con Sib): un bemol.
        assert_eq!(detectar_armadura(&[parte(&[67, 69, 71, 72, 74, 76, 78, 67])]), 1);
        assert_eq!(detectar_armadura(&[parte(&[65, 67, 69, 70, 72, 74, 76, 65])]), -1);
    }
}
