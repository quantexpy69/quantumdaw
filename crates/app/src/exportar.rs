//! Exportación: la mezcla, stems de pistas sueltas o stems por grupo, en el rango elegido
//! (proyecto, loop, selección, una sección del arreglo o compases), con cola para reverbs y delays.
use crate::{
    preferencias::{boton, boton_principal, dato, desplegable, fila, interruptor, nota, tarjeta},
    widgets::{self, rr},
    *,
};
use egui::{RichText, Sense, Stroke, StrokeKind, vec2};

/// Qué se exporta.
pub const MEZCLA: usize = 0;
pub const PISTAS: usize = 1;
pub const GRUPOS: usize = 2;

/// Rango de tiempo.
pub const PROYECTO: usize = 0;
pub const LOOP: usize = 1;
pub const SELECCION: usize = 2;
pub const SECCION: usize = 3;
pub const COMPASES: usize = 4;

/// Opciones del diálogo de exportación (se conservan entre una exportación y la siguiente).
#[derive(Clone)]
pub struct Exportacion {
    pub formato: Format,
    pub origen: usize,
    /// Pistas y grupos marcados (por índice).
    pub pistas: Vec<bool>,
    pub grupos: Vec<bool>,
    /// Modo grupos: las pistas sin grupo también salen, cada una como su propio stem.
    pub sueltas: bool,
    /// Modo stems: exportar además la mezcla completa.
    pub con_mezcla: bool,
    pub rango: usize,
    pub seccion: usize,
    /// Compases (desde 1) del rango personalizado, ambos incluidos.
    pub desde: u32,
    pub hasta: u32,
    /// Segundos extra al final del rango.
    pub cola: f32,
    /// Los stems pasan por los efectos y el volumen del master.
    pub master: bool,
    pub omitir_silencio: bool,
    pub numerar: bool,
}

/// Un archivo a escribir: nombre (sin extensión) y las pistas que suma (None = la mezcla).
struct Stem {
    nombre: String,
    pistas: Option<Vec<Arc<Params>>>,
}

impl App {
    /// Abre el diálogo con las últimas opciones, ajustadas a las pistas y grupos actuales.
    pub fn abrir_exportar(&mut self) {
        let n = self.s.tracks.len();
        let mut e = self.exportacion.take().unwrap_or_else(|| {
            let fin = self.compas_de(self.project_end()).max(1);
            Exportacion {
                formato: Format::Wav24,
                origen: MEZCLA,
                pistas: vec![],
                grupos: vec![],
                sueltas: true,
                con_mezcla: false,
                rango: PROYECTO,
                seccion: 0,
                desde: 1,
                hasta: fin,
                cola: 0.0,
                master: false,
                omitir_silencio: true,
                numerar: true,
            }
        });
        // Marcadas por defecto: las elegidas en el timeline o, si no hay, todas las que suenan.
        if e.pistas.len() != n {
            let elegidas = self.s.tracks.iter().any(|t| t.selected);
            e.pistas = self.s.tracks.iter().map(|t| exportable(t) && if elegidas { t.selected } else { !t.params.mute.load(Relaxed) }).collect();
        }
        if e.grupos.len() != self.s.groups.len() {
            e.grupos = vec![true; self.s.groups.len()];
        }
        e.seccion = e.seccion.min(self.s.sections.len().saturating_sub(1));
        if e.rango == SELECCION && self.time_sel.is_none() {
            e.rango = PROYECTO;
        }
        self.dialog = Some(Dialog::Export(Box::new(e)));
    }

    /// Compás (desde 1) en el que cae el frame `f`.
    fn compas_de(&self, f: u64) -> u32 {
        let por = self.engine.beats.load(Relaxed).max(1) as f64;
        (self.beats(f as f64) / por).ceil().max(1.0) as u32
    }

    fn inicio_compas(&self, k: u32) -> u64 {
        let por = self.engine.beats.load(Relaxed).max(1) as f64;
        self.frame_of(k.saturating_sub(1) as f64 * por).max(0.0) as u64
    }

    /// Rango elegido en frames, sin la cola.
    pub fn rango_exportacion(&self, e: &Exportacion) -> (u64, u64) {
        let en = &self.engine;
        match e.rango {
            LOOP => (en.loop_start.load(Relaxed), en.loop_end.load(Relaxed)),
            SELECCION => self.time_sel.unwrap_or_default(),
            SECCION => self.s.sections.get(e.seccion).map_or((0, 0), |s| (s.0, s.1)),
            COMPASES => (self.inicio_compas(e.desde), self.inicio_compas(e.hasta.max(e.desde) + 1)),
            _ => (0, self.project_end()),
        }
    }

    /// Resumen del rango para el diálogo: compases y duración.
    pub fn resumen_exportacion(&self, e: &Exportacion) -> String {
        let (a, b) = self.rango_exportacion(e);
        if b <= a {
            return tr("El rango está vacío").to_string();
        }
        let seg = (b - a) as f64 / self.sr() + e.cola as f64;
        let (desde, hasta) = (self.compas_de(a + 1), self.compas_de(b));
        format!("{} {desde} – {hasta} · {}:{:04.1}", tr("Compases"), (seg / 60.0) as u64, seg % 60.0)
    }

    /// Archivos que saldrán con estas opciones.
    fn stems(&self, e: &Exportacion) -> Vec<Stem> {
        let pista = |t: &Track| Stem { nombre: t.name.clone(), pistas: Some(vec![t.params.clone()]) };
        let mut v: Vec<Stem> = match e.origen {
            PISTAS => self.s.tracks.iter().zip(&e.pistas).filter(|(t, on)| **on && exportable(t)).map(|(t, _)| pista(t)).collect(),
            GRUPOS => {
                let mut v: Vec<Stem> = self
                    .s
                    .groups
                    .iter()
                    .enumerate()
                    .filter(|(g, _)| e.grupos.get(*g).copied().unwrap_or(false))
                    .filter_map(|(g, grupo)| {
                        let pistas: Vec<_> = self.s.tracks.iter().filter(|t| t.group == Some(g) && exportable(t)).map(|t| t.params.clone()).collect();
                        (!pistas.is_empty()).then(|| Stem { nombre: grupo.name.clone(), pistas: Some(pistas) })
                    })
                    .collect();
                if e.sueltas {
                    v.extend(self.s.tracks.iter().filter(|t| t.group.is_none_or(|g| g >= self.s.groups.len()) && exportable(t)).map(pista));
                }
                v
            }
            _ => vec![],
        };
        if e.origen == MEZCLA || e.con_mezcla {
            v.insert(0, Stem { nombre: tr("Mezcla").to_string(), pistas: None });
        }
        v
    }

    /// Exporta según las opciones: un archivo (la mezcla) o una carpeta de stems.
    pub fn exportar(&mut self, e: Exportacion) -> anyhow::Result<String> {
        let (a, b) = self.rango_exportacion(&e);
        anyhow::ensure!(b > a, "el rango de exportación está vacío");
        let fin = b + (e.cola.max(0.0) as f64 * self.sr()) as u64;
        let stems = self.stems(&e);
        anyhow::ensure!(!stems.is_empty(), "no hay pistas ni grupos marcados para exportar");
        self.stop();
        let ext = e.formato.ext();
        if e.origen == MEZCLA {
            let nombre = safe(&self.dir.file_name().unwrap_or_default().to_string_lossy());
            let dialogo = rfd::FileDialog::new().set_directory(self.dir.join(RENDER_DIR)).set_file_name(format!("{nombre}.{ext}"));
            let ruta = dialogo.save_file().ok_or_else(|| anyhow::anyhow!("exportación cancelada"))?;
            self.escribir(&ruta, &e, (a, fin), None)?;
            return Ok(format!("{} {}", tr("Exportado:"), ruta.display()));
        }
        let _ = fs::create_dir_all(self.dir.join(RENDER_DIR));
        let dir = rfd::FileDialog::new().set_title(tr("Carpeta para los stems")).set_directory(self.dir.join(RENDER_DIR)).pick_folder();
        let dir = dir.ok_or_else(|| anyhow::anyhow!("exportación cancelada"))?;
        let (mut hechos, mut mudos, mut usados) = (0, 0, std::collections::HashSet::new());
        for (k, s) in stems.iter().enumerate() {
            let base = if e.numerar { format!("{:02}_{}", k + 1, safe(&s.nombre)) } else { safe(&s.nombre) };
            // Dos pistas con el mismo nombre no se pisan: la segunda lleva un número.
            let (mut nombre, mut n) = (base.clone(), 2);
            while !usados.insert(nombre.clone()) {
                (nombre, n) = (format!("{base}-{n}"), n + 1);
            }
            if self.escribir(&dir.join(format!("{nombre}.{ext}")), &e, (a, fin), s.pistas.as_deref())? {
                hechos += 1;
            } else {
                mudos += 1;
            }
        }
        let omitidos = if mudos > 0 { format!(" ({mudos} {})", tr("en silencio, omitidos")) } else { String::new() };
        Ok(format!("{hechos} {}{omitidos} {} {}", tr("stems exportados"), tr("en"), dir.display()))
    }

    /// Renderiza y escribe un archivo; devuelve false si se omitió por estar en silencio.
    fn escribir(&self, ruta: &Path, e: &Exportacion, (a, b): (u64, u64), pistas: Option<&[Arc<Params>]>) -> anyhow::Result<bool> {
        let mut muestras = match pistas {
            Some(p) => self.engine.bounce_stem(a, b, p, e.master),
            None => self.engine.bounce(a, b, None),
        };
        if pistas.is_some() && e.omitir_silencio && muestras.iter().all(|s| s.abs() < 1e-5) {
            return Ok(false);
        }
        let mut sr = self.engine.sample_rate;
        if e.formato == Format::Mp3 && sr > 48_000 {
            let frames = muestras.chunks(2).map(|c| [c[0], c[1]]).collect();
            (muestras, sr) = (engine::resample(frames, sr, 48_000).into_iter().flatten().collect(), 48_000);
        }
        project::export(ruta, e.formato, sr, &muestras)?;
        Ok(true)
    }
}

/// Las pistas de video no tienen audio que exportar.
fn exportable(t: &Track) -> bool {
    t.kind != TrackKind::Video
}

/// Diálogo de exportación. Devuelve Some(true) al exportar y Some(false) al cancelar.
pub fn dialogo(ui: &mut egui::Ui, e: &mut Exportacion, tracks: &[Track], groups: &[Group], sections: &[(u64, u64, String)], hay_seleccion: bool, resumen: &str) -> Option<bool> {
    let mut salida = None;
    ui.set_width(720.0);
    ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
    ui.label(RichText::new(tr("Exportar")).size(21.0).strong());
    ui.label(RichText::new(tr("La mezcla completa o stems de pistas y grupos, en el rango que elijas")).size(12.5).color(TEXT_DIM()));
    ui.add_space(6.0);
    egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() * 0.72).auto_shrink([false, true]).show(ui, |ui| {
        tarjeta(ui, "QUÉ EXPORTAR", |ui| {
            widgets::segmentado(ui, &mut e.origen, &["Mezcla", "Stems por pista", "Stems por grupo"]);
            match e.origen {
                PISTAS => lista_pistas(ui, e, tracks),
                GRUPOS => lista_grupos(ui, e, tracks, groups),
                _ => nota_ancha(ui, "Un solo archivo estéreo con todo lo que suena, después del master."),
            }
            if e.origen != MEZCLA {
                fila(ui, "Incluir la mezcla", |ui| interruptor(ui, &mut e.con_mezcla));
            }
        });
        tarjeta(ui, "RANGO", |ui| {
            // Sin selección de tiempo o sin secciones, esas opciones no tienen rango que exportar.
            let activas = [true, true, hay_seleccion, !sections.is_empty(), true];
            widgets::segmentado_con(ui, &mut e.rango, &["Proyecto", "Loop", "Selección", "Sección", "Compases"], &activas);
            match e.rango {
                SECCION => {
                    fila(ui, "Sección", |ui| {
                        let nombre = sections.get(e.seccion).map_or("—", |s| s.2.as_str());
                        desplegable(ui, "exp-seccion", nombre, |ui| {
                            for (k, s) in sections.iter().enumerate() {
                                ui.selectable_value(&mut e.seccion, k, &s.2);
                            }
                        });
                    });
                }
                COMPASES => {
                    fila(ui, "Compases", |ui| {
                        ui.label(RichText::new(tr("del")).color(TEXT_DIM()));
                        ui.add(egui::DragValue::new(&mut e.desde).range(1..=9999).speed(0.2));
                        ui.label(RichText::new(tr("al")).color(TEXT_DIM()));
                        ui.add(egui::DragValue::new(&mut e.hasta).range(e.desde..=9999).speed(0.2));
                    });
                }
                _ => {}
            }
            if !hay_seleccion || sections.is_empty() {
                let falta = match (hay_seleccion, sections.is_empty()) {
                    (false, true) => "Para Selección marca un tramo en la regla; para Sección crea secciones en el arreglo.",
                    (false, false) => "Para Selección marca un tramo en la regla del timeline.",
                    _ => "Para Sección crea secciones del arreglo en el timeline.",
                };
                nota(ui, falta);
            }
            fila(ui, "Cola", |ui| {
                let texto = format!("{:.1} s", e.cola);
                preferencias::barra(ui, &mut e.cola, 0.0..=10.0, texto);
            });
            nota(ui, "Tiempo extra al final para que las reverbs y los delays terminen de sonar.");
            fila(ui, "Duración", |ui| dato(ui, "", resumen));
        });
        tarjeta(ui, "FORMATO Y OPCIONES", |ui| {
            fila(ui, "Formato", |ui| {
                desplegable(ui, "exp-formato", e.formato.label(), |ui| {
                    Format::ALL.iter().for_each(|&f| _ = ui.selectable_value(&mut e.formato, f, f.label()));
                });
            });
            if e.origen != MEZCLA {
                fila(ui, "Pasar por el master", |ui| interruptor(ui, &mut e.master));
                nota(ui, "Con «No» los stems salen sin los efectos ni el volumen del master, listos para mezclar en otro lado.");
                fila(ui, "Omitir stems en silencio", |ui| interruptor(ui, &mut e.omitir_silencio));
                fila(ui, "Numerar los archivos", |ui| interruptor(ui, &mut e.numerar));
            }
        });
    });
    let archivos = cuenta(e, tracks, groups);
    preferencias::a_la_derecha(ui, |ui| {
        let texto = match (e.origen, archivos) {
            (MEZCLA, _) => tr("Exportar…").to_string(),
            (_, 1) => tr("Exportar 1 archivo…").to_string(),
            (_, n) => format!("{} {n} {}", tr("Exportar"), tr("archivos…")),
        };
        if ui.add_enabled_ui(archivos > 0 && !resumen.starts_with(tr("El rango")), |ui| boton_principal(ui, &texto)).inner.clicked() {
            salida = Some(true);
        }
        if boton(ui, "Cancelar").clicked() {
            salida = Some(false);
        }
    });
    salida
}

/// Cantidad de archivos que saldrán (sin descontar los que resulten en silencio).
fn cuenta(e: &Exportacion, tracks: &[Track], groups: &[Group]) -> usize {
    let n = match e.origen {
        PISTAS => tracks.iter().zip(&e.pistas).filter(|(t, on)| **on && exportable(t)).count(),
        GRUPOS => {
            let grupos = (0..groups.len()).filter(|&g| e.grupos.get(g).copied().unwrap_or(false) && tracks.iter().any(|t| t.group == Some(g) && exportable(t))).count();
            grupos + if e.sueltas { tracks.iter().filter(|t| t.group.is_none_or(|g| g >= groups.len()) && exportable(t)).count() } else { 0 }
        }
        _ => 1,
    };
    n + usize::from(e.origen != MEZCLA && e.con_mezcla)
}

/// Botón chico de la cabecera de la lista (Todas, Ninguna…).
fn pequeno(ui: &mut egui::Ui, texto: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(tr(texto)).size(12.0)).min_size(vec2(0.0, 24.0)).corner_radius(rr(6.0)))
}

fn nota_ancha(ui: &mut egui::Ui, texto: &str) {
    ui.add(egui::Label::new(RichText::new(tr(texto)).size(12.0).color(TEXT_DIM())).wrap());
}

/// Fila marcable de la lista: casilla, color, nombre y un dato a la derecha.
fn renglon(ui: &mut egui::Ui, on: &mut bool, activa: bool, color: Color32, nombre: &str, detalle: &str) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), if activa { Sense::click() } else { Sense::hover() });
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    if resp.hovered() && activa {
        p.rect_filled(r, rr(6.0), BORDER().gamma_multiply(0.6));
    }
    let caja = egui::Rect::from_center_size(r.left_center() + vec2(18.0, 0.0), vec2(16.0, 16.0));
    let marca = *on && activa;
    p.rect(caja, rr(4.0), if marca { ACCENT() } else { BG() }, Stroke::new(1.0, if marca { ACCENT() } else { BORDER() }), StrokeKind::Inside);
    if marca {
        let c = caja.center();
        p.line_segment([c + vec2(-4.0, 0.0), c + vec2(-1.0, 3.5)], Stroke::new(2.0, BG()));
        p.line_segment([c + vec2(-1.0, 3.5), c + vec2(4.5, -3.5)], Stroke::new(2.0, BG()));
    }
    p.rect_filled(egui::Rect::from_center_size(r.left_center() + vec2(42.0, 0.0), vec2(10.0, 18.0)), rr(3.0), if activa { color } else { color.gamma_multiply(0.35) });
    let texto = if activa { TEXT() } else { TEXT_DIM() };
    let nombre_r = egui::Rect::from_min_max(r.min + vec2(56.0, 0.0), egui::pos2(r.right() - 190.0, r.bottom()));
    p.with_clip_rect(nombre_r).text(nombre_r.left_center(), egui::Align2::LEFT_CENTER, nombre, egui::FontId::proportional(13.5), texto);
    p.text(r.right_center() - vec2(10.0, 0.0), egui::Align2::RIGHT_CENTER, detalle, egui::FontId::proportional(12.0), TEXT_DIM());
}

/// Marco de la lista, con alto máximo y desplazamiento.
fn marco_lista(ui: &mut egui::Ui, id: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new().fill(BG()).stroke(Stroke::new(1.0, BORDER())).corner_radius(rr(8.0)).inner_margin(4).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt(id).max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            add(ui);
        });
    });
}

fn tipo(t: &Track) -> &'static str {
    match t.kind {
        TrackKind::AudioMono => "Audio mono",
        TrackKind::AudioStereo => "Audio estéreo",
        TrackKind::Midi => "Instrumento",
        TrackKind::Click => "Clic",
        TrackKind::Video => "Video",
    }
}

fn lista_pistas(ui: &mut egui::Ui, e: &mut Exportacion, tracks: &[Track]) {
    ui.horizontal(|ui| {
        let marcadas = e.pistas.iter().zip(tracks).filter(|(on, t)| **on && exportable(t)).count();
        ui.label(RichText::new(format!("{marcadas} {} {}", tr("de"), tracks.iter().filter(|t| exportable(t)).count())).size(12.5).color(TEXT_DIM()));
        preferencias::a_la_derecha(ui, |ui| {
            if pequeno(ui, "Ninguna").clicked() {
                e.pistas.iter_mut().for_each(|on| *on = false);
            }
            if pequeno(ui, "Las que suenan").clicked() {
                e.pistas.iter_mut().zip(tracks).for_each(|(on, t)| *on = !t.params.mute.load(Relaxed));
            }
            if tracks.iter().any(|t| t.selected) && pequeno(ui, "Seleccionadas").clicked() {
                e.pistas.iter_mut().zip(tracks).for_each(|(on, t)| *on = t.selected);
            }
            if pequeno(ui, "Todas").clicked() {
                e.pistas.iter_mut().for_each(|on| *on = true);
            }
        });
    });
    marco_lista(ui, "exp-pistas", |ui| {
        for (t, on) in tracks.iter().zip(e.pistas.iter_mut()) {
            let mut detalle = tr(tipo(t)).to_string();
            if t.params.mute.load(Relaxed) {
                detalle = format!("{} · {detalle}", tr("Silenciada"));
            }
            renglon(ui, on, exportable(t), t.color, &t.name, &detalle);
        }
    });
}

fn lista_grupos(ui: &mut egui::Ui, e: &mut Exportacion, tracks: &[Track], groups: &[Group]) {
    if groups.is_empty() {
        nota_ancha(ui, "El proyecto no tiene grupos. Selecciona pistas en el timeline y agrúpalas (Pista → Agrupar seleccionadas) para exportarlas juntas en un stem.");
    } else {
        nota_ancha(ui, "Cada grupo sale como un stem con la suma de sus pistas.");
        marco_lista(ui, "exp-grupos", |ui| {
            for (g, (grupo, on)) in groups.iter().zip(e.grupos.iter_mut()).enumerate() {
                let miembros: Vec<&str> = tracks.iter().filter(|t| t.group == Some(g) && exportable(t)).map(|t| t.name.as_str()).collect();
                let detalle = match miembros.len() {
                    0 => tr("Sin pistas").to_string(),
                    1 => format!("1 {}", tr("pista")),
                    n => format!("{n} {}", tr("pistas")),
                };
                renglon(ui, on, !miembros.is_empty(), grupo.color, &grupo.name, &detalle);
                if !miembros.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(56.0);
                        ui.add(egui::Label::new(RichText::new(miembros.join(", ")).size(11.5).color(TEXT_DIM())).truncate());
                    });
                    ui.add_space(4.0);
                }
            }
        });
    }
    let sueltas = tracks.iter().filter(|t| t.group.is_none_or(|g| g >= groups.len()) && exportable(t)).count();
    if sueltas > 0 {
        fila(ui, "Pistas sin grupo", |ui| {
            interruptor(ui, &mut e.sueltas);
            ui.label(RichText::new(format!("{sueltas} {}", tr("como stems sueltos"))).size(12.0).color(TEXT_DIM()));
        });
    }
}
