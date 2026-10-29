//! Menú principal (al estilo de Studio One): canción nueva con sus ajustes y plantillas, datos de la
//! canción y derechos de autor, proyectos recientes, demo y créditos, con el logo y la versión.
use crate::{widgets::rr, *};
use egui::{Id, RichText, vec2};
use project::{Collection, SongMeta};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Logo sin el cuadro de fondo (ese queda para el icono de la app), teñido con el texto del tema.
pub fn logo(size: f32) -> egui::Image<'static> {
    egui::Image::new(egui::include_image!("../../../assets/quantumlogo-marca.svg")).tint(TEXT()).fit_to_exact_size(vec2(size, size))
}

/// Plantillas: (nombre, descripción, pistas (nombre, tipo, instrumento)).
type TemplateTrack = (&'static str, TrackKind, &'static str);
const TEMPLATES: [(&str, &str, &[TemplateTrack]); 5] = [
    ("Canción vacía", "Empieza desde cero", &[]),
    (
        "Banda",
        "Batería, bajo, guitarra y voz",
        &[("Batería", TrackKind::Midi, "bateria"), ("Bajo", TrackKind::Midi, "bajo-dedos"), ("Guitarra", TrackKind::AudioMono, ""), ("Voz", TrackKind::AudioMono, "")],
    ),
    (
        "Cuarteto de cuerdas",
        "Violín, viola, violonchelo y contrabajo",
        &[("Violín", TrackKind::Midi, "violin"), ("Viola", TrackKind::Midi, "viola"), ("Violonchelo", TrackKind::Midi, "violonchelo"), ("Contrabajo", TrackKind::Midi, "contrabajo")],
    ),
    (
        "Producción electrónica",
        "Batería, bajo y sintetizadores",
        &[
            ("Batería", TrackKind::Midi, "bateria"),
            ("Bajo synth", TrackKind::Midi, "synth:Bajo sintetizado"),
            ("Pad", TrackKind::Midi, "synth:Pad cálido"),
            ("Lead", TrackKind::Midi, "synth:Lead brillante"),
        ],
    ),
    ("Grabación de voz", "Voz principal, coros y clic", &[("Voz", TrackKind::AudioMono, ""), ("Coros", TrackKind::AudioStereo, ""), ("Clic", TrackKind::Click, "")]),
];

/// Acción elegida en el menú principal, que se ejecuta al cerrarlo.
type Action = Box<dyn FnOnce(&mut App)>;

pub struct Welcome {
    pub tab: u8,
    name: String,
    folder: PathBuf,
    rate: u32,
    bits: u16,
    bpm: f32,
    sig: (u32, u32),
    template: usize,
    meta: SongMeta,
    /// Proyecto (carpeta que agrupa canciones) donde se crea la canción, y quién la crea.
    collection: String,
    author: String,
    /// Proyectos y canciones encontrados (se leen al abrir la pestaña «Abrir»).
    listing: Option<Vec<Listed>>,
    /// Búsqueda en las preferencias, audio y MIDI en edición, atajo que se está capturando y carpetas
    /// de plugins revisadas (se leen al abrir su página).
    pub buscar: String,
    pub borrador: Option<crate::preferencias::Borrador>,
    pub captura: Option<keys::Capture>,
    pub carpetas: Option<Vec<crate::preferencias::Carpeta>>,
}

/// Espacios de trabajo: (nombre, para quién, biblioteca, panel inferior (None = oculto), automatización).
pub const ESPACIOS: [(&str, &str, bool, Option<u8>, bool); 7] = [
    ("Productor", "Arreglo con la biblioteca a mano y el mixer abajo", true, Some(0), false),
    ("Compositor", "Piano roll abajo, acordes y círculo de quintas", false, Some(1), false),
    ("Beatmaker", "Piano roll y biblioteca de instrumentos y efectos", true, Some(1), false),
    ("Ingeniero de mezcla", "Mixer grande y automatización visible", false, Some(0), true),
    ("Mastering", "Analizador y ventana de loudness del master", false, Some(2), false),
    ("Grabación", "Arreglo despejado para grabar tomas", false, None, false),
    ("Edición de audio", "Editor de audio abajo para cortar y procesar", false, Some(3), false),
];

/// Un proyecto con su ficha y sus canciones (carpeta y datos de cada una).
type Listed = (String, Option<Collection>, Vec<(PathBuf, Project)>);

/// Proyectos (carpetas con `proyecto.json` o con canciones dentro) y canciones sueltas de `root`.
fn scan(root: &Path) -> Vec<Listed> {
    let dirs = |d: &Path| -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        v.sort();
        v
    };
    let songs = |d: &Path| dirs(d).into_iter().filter_map(|s| Some((s.clone(), Project::load(&s).ok()?))).collect::<Vec<_>>();
    let mut out: Vec<Listed> = dirs(root)
        .into_iter()
        .filter(|d| !d.join(project::FILE).exists())
        .map(|d| (d.file_name().unwrap_or_default().to_string_lossy().to_string(), Collection::load(&d), songs(&d)))
        .filter(|l| l.1.is_some() || !l.2.is_empty())
        .collect();
    let loose = songs(root);
    if !loose.is_empty() {
        out.push((String::new(), None, loose));
    }
    out
}

/// Datos básicos de una canción en una línea: autor, creación, último guardado, formato, tempo y pistas.
fn song_info(p: &Project) -> String {
    let m = &p.meta;
    let mut parts = vec![];
    if !m.created_by.is_empty() {
        parts.push(format!("{} {}", tr("por"), m.created_by));
    }
    if !m.created_at.is_empty() {
        parts.push(format!("{} {}", tr("creada"), m.created_at));
    }
    if !m.saved_at.is_empty() {
        parts.push(format!("{} {}", tr("guardada"), m.saved_at));
    }
    let rate = if p.sample_rate == 0 { tr("Hz del sistema").to_string() } else { format!("{:.1} kHz", p.sample_rate as f32 / 1000.0) };
    parts.push(format!("{rate} / {} bits", p.rec_bits));
    parts.push(format!("{:.0} BPM {}/{}", p.bpm, p.signature.0, p.signature.1));
    parts.push(format!("{} {}", p.tracks.len(), tr("pistas")));
    parts.join(" · ")
}

impl Default for Welcome {
    fn default() -> Self {
        Self {
            tab: 0,
            name: "Mi canción".into(),
            folder: projects_dir(),
            rate: 48_000,
            bits: 24,
            bpm: 120.0,
            sig: (4, 4),
            template: 0,
            meta: SongMeta::default(),
            collection: String::new(),
            author: String::new(),
            listing: None,
            buscar: String::new(),
            borrador: None,
            captura: None,
            carpetas: None,
        }
    }
}

/// Campos de datos de la canción y derechos de autor.
/// Datos de una canción: una fila por campo, todos del mismo ancho.
fn meta_form(ui: &mut egui::Ui, m: &mut SongMeta) {
    use crate::preferencias::fila;
    for (label, field, hint) in [
        ("Título", &mut m.title, "Nombre de la canción"),
        ("Artista", &mut m.artist, "Intérprete o banda"),
        ("Álbum", &mut m.album, ""),
        ("Compositor/es", &mut m.composer, "Autores de la música y la letra"),
        ("Productor", &mut m.producer, ""),
        ("Año", &mut m.year, "2026"),
        ("Género", &mut m.genre, ""),
        ("Copyright ©", &mut m.copyright, "© 2026 Nombre del titular"),
        ("Editorial / sello", &mut m.publisher, ""),
        ("ISRC", &mut m.isrc, "CC-XXX-AA-NNNNN"),
    ] {
        fila(ui, label, |ui| campo(ui, field, tr(hint)));
    }
    fila(ui, "Notas", |ui| ui.add(egui::TextEdit::multiline(&mut m.notes).desired_width(ui.available_width()).desired_rows(2).margin(egui::Margin::symmetric(10, 4))));
}

/// Campo de texto de una fila: todo el ancho y la altura de las demás filas.
fn campo(ui: &mut egui::Ui, texto: &mut String, pista: &str) -> egui::Response {
    ui.add(egui::TextEdit::singleline(texto).hint_text(pista).desired_width(ui.available_width()).min_size(vec2(0.0, 28.0)).margin(egui::Margin::symmetric(10, 4)).vertical_align(egui::Align::Center).font(egui::FontId::proportional(13.5)))
}

impl App {
    fn create_song(&mut self, w: &Welcome) -> anyhow::Result<()> {
        // Si se eligió un proyecto, la canción va dentro de su carpeta (que se crea con su ficha).
        let now = now_text();
        let root = match w.collection.trim() {
            "" => w.folder.clone(),
            c => {
                let dir = w.folder.join(safe(c).replace('_', " ").trim());
                if Collection::load(&dir).is_none() {
                    Collection { name: c.to_string(), author: w.author.trim().to_string(), created_at: now.clone(), description: String::new() }.save(&dir)?;
                }
                dir
            }
        };
        let base = root.join(safe(w.name.trim()).replace('_', " ").trim());
        let mut dir = base.clone();
        let mut n = 2;
        while dir.join(project::FILE).exists() {
            (dir, n) = (PathBuf::from(format!("{} {n}", base.display())), n + 1);
        }
        let mut meta = w.meta.clone();
        if meta.title.is_empty() {
            meta.title = w.name.clone();
        }
        (meta.created_by, meta.created_at, meta.saved_at) = (w.author.trim().to_string(), now.clone(), now);
        if !w.author.trim().is_empty() {
            self.config.author = w.author.trim().to_string();
            self.config.save();
        }
        let mut p = Project { sample_rate: w.rate, rec_bits: w.bits, bpm: w.bpm, signature: w.sig, meta, ..Default::default() };
        for (k, (name, kind, inst)) in TEMPLATES[w.template].2.iter().enumerate() {
            let mut t = TrackState::new(name.to_string(), *kind, PALETTE[k % PALETTE.len()]);
            t.instrument = inst.to_string();
            p.tracks.push(t);
        }
        fs::create_dir_all(&dir)?;
        p.save(&dir)?;
        self.reload(dir, p)
    }

    /// Abre el menú principal en la pestaña indicada.
    pub fn open_main_menu(&mut self, tab: u8) {
        let author = if self.config.author.is_empty() { user_name() } else { self.config.author.clone() };
        self.welcome = Some(Welcome { tab, author, ..Default::default() });
    }

    pub fn welcome_window(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::H)) && self.welcome.is_none() {
            self.open_main_menu(0);
        }
        let Some(mut w) = self.welcome.take() else {
            return;
        };
        let (mut close, mut action): (bool, Option<Action>) = (false, None);
        let pantalla = ctx.content_rect();
        let (ancho, alto) = ((pantalla.width() - 80.0).clamp(760.0, 1500.0), (pantalla.height() - 70.0).max(480.0));
        let frame = egui::Frame::popup(&ctx.global_style()).fill(PANEL()).inner_margin(0.0).corner_radius(rr(14.0)).stroke(egui::Stroke::new(1.0, BORDER()));
        let modal = egui::Modal::new(Id::new("welcome")).backdrop_color(Color32::from_black_alpha(200)).frame(frame);
        modal.show(ctx, |ui| {
            ui.set_width(ancho);
            ui.set_height(alto);
            ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // Barra lateral: logo, secciones, preferencias y acerca de.
                let (lat, _) = ui.allocate_exact_size(vec2(230.0, alto), egui::Sense::hover());
                ui.painter().rect_filled(lat, egui::CornerRadius { nw: rr(14.0) as u8, sw: rr(14.0) as u8, ne: 0, se: 0 }, BG());
                let mut lado = ui.new_child(egui::UiBuilder::new().max_rect(lat.shrink2(vec2(18.0, 22.0))).layout(egui::Layout::top_down(egui::Align::Min)));
                lado.horizontal(|ui| {
                    ui.add(logo(40.0));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("QUANTUM").size(18.0).strong().color(TEXT()));
                        ui.label(RichText::new(format!("v{VERSION}")).size(11.0).color(TEXT_DIM()));
                    });
                });
                lado.spacing_mut().item_spacing = vec2(8.0, 4.0);
                lado.add_space(16.0);
                let seccion = |ui: &mut egui::Ui, w: &mut Welcome, k: u8| {
                    let texto = crate::preferencias::seccion(k).0;
                    let (r, resp) = ui.allocate_exact_size(vec2(194.0, 34.0), egui::Sense::click());
                    let activa = w.tab == k;
                    // Sección activa: píldora del color de acento suave con su icono en acento.
                    if activa {
                        ui.painter().rect_filled(r, rr(10.0), ACCENT().gamma_multiply(0.16));
                        ui.painter().rect_filled(egui::Rect::from_min_size(r.min + vec2(0.0, 8.0), vec2(3.0, 18.0)), 1.5, ACCENT());
                    } else if resp.hovered() {
                        ui.painter().rect_filled(r, rr(10.0), PANEL());
                    }
                    let color = if activa { ACCENT() } else { TEXT_DIM() };
                    icono_seccion(ui.painter(), r.left_center() + vec2(22.0, 0.0), k, color);
                    ui.painter().text(r.left_center() + vec2(42.0, 0.0), egui::Align2::LEFT_CENTER, tr(texto), egui::FontId::proportional(14.0), if activa || resp.hovered() { TEXT() } else { TEXT_DIM() });
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        w.tab = k;
                        w.buscar.clear();
                    }
                };
                let grupo = |ui: &mut egui::Ui, texto: &str| {
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new(tr(texto)).size(10.5).strong().color(TEXT_DIM()));
                    });
                };
                for k in [0, 3, 2, 1] {
                    seccion(&mut lado, &mut w, k);
                }
                grupo(&mut lado, "PREFERENCIAS");
                for k in [10, 11, 12, 13, 14, 5, 15, 6, 4] {
                    seccion(&mut lado, &mut w, k);
                }
                grupo(&mut lado, "AMPLIAR");
                seccion(&mut lado, &mut w, 16);
                lado.add_space((lado.available_height() - 40.0).max(0.0));
                lado.label(RichText::new(tr("Lo que cambies aquí se guarda al instante.")).size(10.5).color(TEXT_DIM()));

                // Contenido.
                let (cont, _) = ui.allocate_exact_size(vec2(ancho - 230.0, alto), egui::Sense::hover());
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(cont.shrink2(vec2(30.0, 22.0))).layout(egui::Layout::top_down(egui::Align::Min)));
                let ui = &mut ui;
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                // × en la esquina superior derecha, fuera del flujo (no empuja la cabecera).
                let cerrar = egui::Rect::from_center_size(cont.right_top() + vec2(-26.0, 26.0), vec2(30.0, 30.0));
                if ui.put(cerrar, egui::Button::new(RichText::new("×").size(20.0).color(TEXT_DIM())).frame(false)).on_hover_text(tr("Cerrar")).clicked() {
                    close = true;
                }
                if w.tab != 0 {
                    crate::preferencias::cabecera(ui, w.tab, &mut w.buscar);
                }
                let cuerpo_h = ui.available_height() - 40.0;
                let buscando = w.tab != 0 && !w.buscar.trim().is_empty();
                egui::ScrollArea::vertical().id_salt(("pref", w.tab, buscando)).max_height(cuerpo_h).auto_shrink([false, false]).show(ui, |ui| match w.tab {
                    _ if buscando => {
                        if let Some(k) = crate::preferencias::resultados(ui, &w.buscar) {
                            (w.tab, w.buscar) = (k, String::new());
                        }
                    }
                    0 => self.panel_inicio(ui, &mut w, &mut action),
                    10 => self.pagina_audio(ui, &mut w),
                    11 => self.pagina_midi(ui, &mut w),
                    12 => self.pagina_comportamiento(ui),
                    13 => self.pagina_guardado(ui),
                    14 => self.pagina_atajos(ui, &mut w),
                    15 => self.pagina_plugins(ui, &mut w),
                    16 => self.pagina_tienda(ui),
                    4 => self.pagina_acerca(ui, &mut action),
                    1 => self.pagina_datos(ui),
                    2 => self.pagina_abrir(ui, &mut w, &mut action),
                    3 => self.pagina_nueva(ui, &mut w, &mut close),
                    5 => self.pagina_interfaz(ui),
                    6 => self.pagina_espacios(ui, &mut close),
                    _ => {}
                });
                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr("Abre este panel cuando quieras con Ctrl+Shift+H o en Archivo > Volver a Pantalla Inicial")).size(11.5).color(TEXT_DIM()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut show = !self.config.hide_welcome;
                        if ui.checkbox(&mut show, tr("Mostrar al iniciar")).changed() {
                            self.config.hide_welcome = !show;
                            self.config.save();
                        }
                    });
                });
            });
        });
        if let Some(a) = action {
            a(self);
            close = true;
        }
        if !close {
            self.welcome = Some(w);
        }
    }
}

/// Hora del día para el saludo del panel de inicio.
fn saludo() -> &'static str {
    use chrono::Timelike;
    match chrono::Local::now().hour() {
        5..=11 => "Buenos días",
        12..=19 => "Buenas tardes",
        _ => "Buenas noches",
    }
}

/// Tiempo transcurrido desde que se guardó un proyecto («hace 2 h»).
fn hace(dir: &Path) -> String {
    let Some(s) = fs::metadata(dir.join(project::FILE)).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).map(|e| e.as_secs()) else {
        return String::new();
    };
    match s {
        0..=59 => tr("ahora").into(),
        60..=3599 => format!("{} {} min", tr("hace"), s / 60),
        3600..=86_399 => format!("{} {} h", tr("hace"), s / 3600),
        _ => format!("{} {} d", tr("hace"), s / 86_400),
    }
}

/// Título de sección en mayúsculas pequeñas.
fn titulo(ui: &mut egui::Ui, texto: &str, nota: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(4.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(r, 2.0, ACCENT());
        ui.label(RichText::new(tr(texto)).size(11.0).strong().color(TEXT()));
        if !nota.is_empty() {
            ui.label(RichText::new(tr(nota)).size(11.0).color(TEXT_DIM().gamma_multiply(0.8)));
        }
    });
}

/// Iconos de la barra lateral (líneas simples, sin depender de la fuente).
fn icono_seccion(p: &egui::Painter, c: egui::Pos2, k: u8, color: Color32) {
    let s = egui::Stroke::new(1.6, color);
    let r = |w: f32, h: f32, d: egui::Vec2| egui::Rect::from_center_size(c + d, vec2(w, h));
    match k {
        // Inicio: casa.
        0 => {
            p.line_segment([c + vec2(-7.0, -1.0), c + vec2(0.0, -7.0)], s);
            p.line_segment([c + vec2(0.0, -7.0), c + vec2(7.0, -1.0)], s);
            p.rect_stroke(r(10.0, 8.0, vec2(0.0, 3.0)), 1.0, s, egui::StrokeKind::Middle);
        }
        // Nueva canción: hoja con +.
        3 => {
            p.rect_stroke(r(12.0, 15.0, vec2(0.0, 0.0)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-3.5, 0.0), c + vec2(3.5, 0.0)], s);
            p.line_segment([c + vec2(0.0, -3.5), c + vec2(0.0, 3.5)], s);
        }
        // Abrir: carpeta.
        2 => {
            p.rect_stroke(r(15.0, 10.0, vec2(0.0, 2.0)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-7.5, -3.0), c + vec2(-2.0, -3.0)], s);
        }
        // Datos: líneas de texto.
        1 => (0..3).for_each(|i| _ = p.line_segment([c + vec2(-7.0, -5.0 + i as f32 * 5.0), c + vec2(if i == 2 { 2.0 } else { 7.0 }, -5.0 + i as f32 * 5.0)], s)),
        // Interfaz: paleta (círculo con puntos).
        5 => {
            p.circle_stroke(c, 7.5, s);
            (0..3).for_each(|i| _ = p.circle_filled(c + egui::Vec2::angled(-2.2 + i as f32 * 1.1) * 4.0, 1.4, color));
        }
        // Espacios de trabajo: cuadrícula.
        6 => (0..4).for_each(|i| _ = p.rect_stroke(r(5.5, 5.5, vec2(if i % 2 == 0 { -3.5 } else { 3.5 }, if i < 2 { -3.5 } else { 3.5 })), 1.0, s, egui::StrokeKind::Middle)),
        // Audio: onda.
        10 => (0..5).for_each(|i| {
            let h = [3.0, 7.0, 5.0, 8.0, 3.0][i];
            p.line_segment([c + vec2(-8.0 + i as f32 * 4.0, -h), c + vec2(-8.0 + i as f32 * 4.0, h)], s);
        }),
        // MIDI: conector DIN (círculo con cinco pines).
        11 => {
            p.circle_stroke(c, 7.5, s);
            (0..5).for_each(|i| _ = p.circle_filled(c + egui::Vec2::angled(std::f32::consts::PI * (1.0 + i as f32 / 4.0)) * 4.0, 1.2, color));
        }
        // Comportamiento: tres deslizadores.
        12 => (0..3).for_each(|i| {
            let x = -5.0 + i as f32 * 5.0;
            p.line_segment([c + vec2(x, -7.0), c + vec2(x, 7.0)], s);
            p.rect_filled(r(5.0, 3.0, vec2(x, [-3.0, 3.0, -1.0][i])), 1.0, color);
        }),
        // Guardado: reloj con flecha (historial).
        13 => {
            p.circle_stroke(c, 7.0, s);
            p.line_segment([c, c + vec2(0.0, -4.5)], s);
            p.line_segment([c, c + vec2(3.5, 2.0)], s);
        }
        // Atajos: tecla.
        14 => {
            p.rect_stroke(r(16.0, 11.0, vec2(0.0, 0.0)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-4.0, 2.0), c + vec2(4.0, 2.0)], s);
        }
        // Plugins: enchufe.
        15 => {
            p.rect_stroke(r(10.0, 7.0, vec2(0.0, -1.0)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-2.5, -4.5), c + vec2(-2.5, -8.0)], s);
            p.line_segment([c + vec2(2.5, -4.5), c + vec2(2.5, -8.0)], s);
            p.line_segment([c + vec2(0.0, 2.5), c + vec2(0.0, 7.5)], s);
        }
        // Tienda: bolsa.
        16 => {
            p.rect_stroke(r(13.0, 10.0, vec2(0.0, 3.0)), 2.0, s, egui::StrokeKind::Middle);
            p.circle_stroke(c + vec2(0.0, -2.0), 3.5, s);
        }
        // Acerca de: «i» en un círculo.
        _ => {
            p.circle_stroke(c, 7.5, s);
            p.line_segment([c + vec2(0.0, -1.0), c + vec2(0.0, 4.0)], s);
            p.circle_filled(c + vec2(0.0, -4.0), 1.2, color);
        }
    }
}

/// Icono de cada plantilla.
fn icono_plantilla(p: &egui::Painter, c: egui::Pos2, k: usize, color: Color32) {
    let s = egui::Stroke::new(1.8, color);
    match k {
        // Vacía: hoja.
        0 => _ = p.rect_stroke(egui::Rect::from_center_size(c, vec2(16.0, 20.0)), 3.0, s, egui::StrokeKind::Middle),
        // Banda: tres barras de mezclador.
        1 => (0..3).for_each(|i| {
            let x = -7.0 + i as f32 * 7.0;
            p.line_segment([c + vec2(x, -9.0), c + vec2(x, 9.0)], s);
            p.rect_filled(egui::Rect::from_center_size(c + vec2(x, [-3.0, 4.0, -1.0][i]), vec2(6.0, 3.0)), 1.0, color);
        }),
        // Cuerdas: clave de sol simplificada (curva y línea).
        2 => {
            p.line_segment([c + vec2(2.0, -10.0), c + vec2(-1.0, 10.0)], s);
            p.circle_stroke(c + vec2(0.0, 3.0), 5.0, s);
        }
        // Electrónica: forma de onda cuadrada.
        3 => {
            let pts = [(-10.0, 4.0), (-5.0, 4.0), (-5.0, -6.0), (0.0, -6.0), (0.0, 4.0), (5.0, 4.0), (5.0, -6.0), (10.0, -6.0)];
            p.add(egui::Shape::line(pts.iter().map(|&(x, y)| c + vec2(x, y)).collect(), s));
        }
        // Voz: micrófono.
        _ => {
            p.rect_stroke(egui::Rect::from_center_size(c + vec2(0.0, -4.0), vec2(8.0, 12.0)), 4.0, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(0.0, 5.0), c + vec2(0.0, 10.0)], s);
            p.line_segment([c + vec2(-5.0, 10.0), c + vec2(5.0, 10.0)], s);
        }
    }
}

/// Color estable de un nombre (avatares de los recientes).
fn color_de(nombre: &str) -> Color32 {
    const C: [Color32; 6] = [
        Color32::from_rgb(0x4F, 0xA3, 0xF7),
        Color32::from_rgb(0xB0, 0x7C, 0xF2),
        Color32::from_rgb(0xF2, 0x7C, 0xB0),
        Color32::from_rgb(0x4A, 0xD6, 0xA0),
        Color32::from_rgb(0xF2, 0xA6, 0x4A),
        Color32::from_rgb(0x5C, 0xD0, 0xE0),
    ];
    C[nombre.bytes().fold(7usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)) % C.len()]
}

impl App {
    /// Aplica un espacio de trabajo (paneles visibles según la forma de trabajar).
    pub fn aplicar_espacio(&mut self, k: usize) {
        let (_, _, lib, abajo, auto) = ESPACIOS[k];
        self.show_lib = lib;
        self.show_auto = auto;
        match abajo {
            Some(t) => (self.show_mixer, self.bottom_tab) = (true, t),
            None => self.show_mixer = false,
        }
        if k == 4 {
            self.mastering.abierta = true;
        }
        if k == 1 {
            self.roll.circulo = self.roll.target.is_some();
        }
        self.status = format!("{}: {}", tr("Espacio de trabajo"), tr(ESPACIOS[k].0));
    }

    /// Datos de la canción abierta, en filas, con el botón de guardar.
    fn pagina_datos(&mut self, ui: &mut egui::Ui) {
        use crate::preferencias::{fila, ruta, tarjeta};
        tarjeta(ui, "CANCIÓN ABIERTA", |ui| {
            let dir = self.dir.display().to_string();
            fila(ui, "Carpeta", |ui| ruta(ui, ui.available_width(), &dir));
        });
        tarjeta(ui, "DATOS Y DERECHOS", |ui| {
            meta_form(ui, &mut self.meta);
            ui.add_space(4.0);
            crate::preferencias::a_la_derecha(ui, |ui| {
                if crate::preferencias::boton_principal(ui, "Guardar datos").clicked() {
                    self.save();
                }
            });
        });
    }

    /// Proyectos (con su ficha) y sus canciones, cada uno en su tarjeta.
    fn pagina_abrir(&mut self, ui: &mut egui::Ui, w: &mut Welcome, action: &mut Option<Action>) {
        use crate::preferencias::{boton, parrafo, tarjeta};
        let listing = w.listing.get_or_insert_with(|| scan(&projects_dir())).clone();
        if listing.is_empty() {
            tarjeta(ui, "PROYECTOS", |ui| parrafo(ui, "Todavía no hay canciones en la carpeta de proyectos."));
        }
        for (coll, info, songs) in listing {
            let titulo = if coll.is_empty() { tr("Canciones sin proyecto").to_uppercase() } else { coll.to_uppercase() };
            tarjeta(ui, &titulo, |ui| {
                if let Some(c) = &info {
                    let by = if c.author.is_empty() { String::new() } else { format!("{} {} · ", tr("por"), c.author) };
                    parrafo(ui, &format!("{by}{} {} · {} {}", tr("creado"), c.created_at, songs.len(), tr("canciones")));
                }
                for (dir, p) in songs {
                    let name = if p.meta.title.is_empty() { dir.file_name().unwrap_or_default().to_string_lossy().to_string() } else { p.meta.title.clone() };
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), egui::Sense::click());
                    let pt = ui.painter();
                    pt.rect(r, rr(9.0), if resp.hovered() { BORDER() } else { BG() }, egui::Stroke::new(1.0, if resp.hovered() { ACCENT().gamma_multiply(0.6) } else { BORDER() }), egui::StrokeKind::Inside);
                    let insignia = egui::Rect::from_min_size(r.left_top() + vec2(10.0, 10.0), vec2(30.0, 30.0));
                    let c = color_de(&name);
                    pt.rect_filled(insignia, rr(8.0), c.gamma_multiply(0.22));
                    pt.text(insignia.center(), egui::Align2::CENTER_CENTER, name.chars().take(2).collect::<String>().to_uppercase(), egui::FontId::proportional(12.0), c);
                    let titulo = if p.meta.artist.is_empty() { name.clone() } else { format!("{name}  ·  {}", p.meta.artist) };
                    let pc = pt.with_clip_rect(r.shrink(2.0));
                    pc.text(r.left_center() + vec2(52.0, -9.0), egui::Align2::LEFT_CENTER, titulo, egui::FontId::proportional(14.0), TEXT());
                    pc.text(r.left_center() + vec2(52.0, 10.0), egui::Align2::LEFT_CENTER, song_info(&p), egui::FontId::proportional(11.5), TEXT_DIM());
                    if resp.on_hover_text(dir.display().to_string()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *action = Some(Box::new(move |app: &mut Self| {
                            let r = app.open_dir(dir);
                            app.report("Proyecto abierto", r);
                        }));
                    }
                }
            });
        }
        tarjeta(ui, "OTRA UBICACIÓN", |ui| {
            ui.horizontal(|ui| {
                if boton(ui, "Abrir otra carpeta de proyecto…").clicked() {
                    *action = Some(Box::new(|app: &mut Self| {
                        let r = app.open_dialog(false);
                        app.report("Proyecto abierto", r);
                    }));
                }
            });
        });
    }

    /// Canción nueva: datos, formato y plantilla en tarjetas, con el botón de crear al final.
    fn pagina_nueva(&mut self, ui: &mut egui::Ui, w: &mut Welcome, close: &mut bool) {
        use crate::preferencias::{desplegable, fila, ruta, tarjeta};
        tarjeta(ui, "CANCIÓN", |ui| {
            fila(ui, "Nombre de la canción", |ui| campo(ui, &mut w.name, ""));
            fila(ui, "Artista", |ui| campo(ui, &mut w.meta.artist, ""));
            fila(ui, "Copyright ©", |ui| campo(ui, &mut w.meta.copyright, tr("© 2026 Nombre del titular")));
            fila(ui, "Proyecto", |ui| {
                let existentes: Vec<String> = scan(&w.folder).into_iter().map(|l| l.0).filter(|n| !n.is_empty()).collect();
                ui.add(egui::TextEdit::singleline(&mut w.collection).hint_text(tr("Opcional: agrupa varias canciones")).desired_width(ui.available_width() - 130.0).min_size(vec2(0.0, 28.0)).margin(egui::Margin::symmetric(10, 4)).vertical_align(egui::Align::Center));
                egui::ComboBox::from_id_salt("w-coll").selected_text(tr("Elegir")).width(120.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut w.collection, String::new(), tr("(sin proyecto)"));
                    for n in existentes {
                        ui.selectable_value(&mut w.collection, n.clone(), n);
                    }
                });
            });
            fila(ui, "Creado por", |ui| campo(ui, &mut w.author, ""));
            fila(ui, "Ubicación", |ui| {
                ruta(ui, ui.available_width() - 130.0, &w.folder.display().to_string());
                if ui.add(egui::Button::new(tr("Cambiar…")).min_size(vec2(120.0, 30.0)).corner_radius(rr(8.0))).clicked()
                    && let Some(d) = rfd::FileDialog::new().set_directory(&w.folder).pick_folder()
                {
                    w.folder = d;
                }
            });
        });
        tarjeta(ui, "FORMATO Y TEMPO", |ui| {
            fila(ui, "Frecuencia de muestreo", |ui| {
                desplegable(ui, "w-rate", format!("{} Hz", w.rate), |ui| {
                    for r in [44_100, 48_000, 88_200, 96_000, 192_000] {
                        ui.selectable_value(&mut w.rate, r, format!("{r} Hz"));
                    }
                });
            });
            fila(ui, "Resolución de grabación", |ui| {
                desplegable(ui, "w-bits", format!("{} bits", w.bits), |ui| {
                    for b in [16, 24, 32] {
                        ui.selectable_value(&mut w.bits, b, format!("{b} bits"));
                    }
                });
            });
            fila(ui, "Tempo", |ui| ui.add_sized([120.0, 28.0], egui::DragValue::new(&mut w.bpm).range(20.0..=300.0).speed(0.5).suffix(" BPM")));
            fila(ui, "Métrica", |ui| {
                desplegable(ui, "w-sig", format!("{}/{}", w.sig.0, w.sig.1), |ui| {
                    for s in [(2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8), (12, 8)] {
                        ui.selectable_value(&mut w.sig, s, format!("{}/{}", s.0, s.1));
                    }
                });
            });
        });
        tarjeta(ui, "PLANTILLA", |ui| {
            // Tarjetas del mismo ancho y alto, tres por fila.
            let espacio = 12.0;
            let tam = vec2(((ui.available_width() - espacio * 2.0) / 3.0).floor(), 70.0);
            for fila_p in TEMPLATES.iter().enumerate().collect::<Vec<_>>().chunks(3) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = espacio;
                    for &(k, (name, desc, _)) in fila_p {
                        let on = w.template == k;
                        let (rect, r) = ui.allocate_exact_size(tam, egui::Sense::click());
                        let fill = if on { ACCENT().gamma_multiply(0.16) } else if r.hovered() { BORDER() } else { BG() };
                        ui.painter().rect(rect, rr(10.0), fill, egui::Stroke::new(if on { 1.5 } else { 1.0 }, if on { ACCENT() } else { BORDER() }), egui::StrokeKind::Inside);
                        let icono = egui::Rect::from_center_size(rect.left_center() + vec2(30.0, 0.0), vec2(34.0, 34.0));
                        ui.painter().rect_filled(icono, rr(9.0), ACCENT().gamma_multiply(if on { 0.3 } else { 0.12 }));
                        icono_plantilla(ui.painter(), icono.center(), k, if on { Color32::WHITE } else { ACCENT() });
                        let pc = ui.painter().with_clip_rect(rect.shrink(4.0));
                        pc.text(rect.left_center() + vec2(58.0, -9.0), egui::Align2::LEFT_CENTER, tr(name), egui::FontId::proportional(14.0), TEXT());
                        pc.text(rect.left_center() + vec2(58.0, 10.0), egui::Align2::LEFT_CENTER, tr(desc), egui::FontId::proportional(11.5), TEXT_DIM());
                        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            w.template = k;
                        }
                    }
                });
            }
        });
        ui.allocate_ui_with_layout(vec2(ui.available_width().min(crate::preferencias::ANCHO), 40.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let b = egui::Button::new(RichText::new(tr("Crear canción")).size(14.5).strong().color(BG())).fill(ACCENT()).min_size(vec2(220.0, 40.0)).corner_radius(rr(10.0));
            if ui.add(b).clicked() {
                let r = self.create_song(w);
                self.report(format!("Canción «{}» creada", w.name), r);
                *close = true;
            }
        });
    }

    /// Tienda de quantumdaw.com: productos en tarjetas (o el aviso sin conexión).
    fn pagina_tienda(&mut self, ui: &mut egui::Ui) {
        use crate::preferencias::{boton, parrafo, tarjeta};
        let web = web::contenido();
        let base = web::base();
        let productos = web.as_ref().map(|c| c.productos.clone()).unwrap_or_default();
        if productos.is_empty() {
            tarjeta(ui, "TIENDA QUANTUM", |ui| {
                parrafo(ui, match &web {
                    None => "Cargando la tienda…",
                    Some(c) if !c.en_linea => "Sin conexión con quantumdaw.com por ahora. Los instrumentos gratuitos siguen disponibles en la Biblioteca.",
                    Some(_) => "Todavía no hay productos publicados.",
                });
                ui.horizontal(|ui| {
                    if boton(ui, "Abrir la tienda en el navegador").clicked() {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(format!("{base}/tienda.html")));
                    }
                });
            });
            return;
        }
        let ancho = ui.available_width().min(crate::preferencias::ANCHO + 400.0);
        let n = ((ancho + 16.0) / 260.0).floor().clamp(1.0, 4.0) as usize;
        let tam = vec2(((ancho - 16.0 * (n as f32 - 1.0)) / n as f32).floor(), 260.0);
        for grupo in productos.chunks(n) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for pr in grupo {
                    let precio = web::precio(pr);
                    let img = web::imagen(ui.ctx(), pr.imagen.as_deref());
                    let tipo = pr.tipo.to_uppercase();
                    let t = Tarjeta { imagen: img, portada: Portada::Producto(&pr.tipo), arriba: &tipo, titulo: &pr.nombre, texto: &pr.resumen, pie: (&precio, if pr.gratis { VERDE_PLAY } else { ACCENT() }), etiqueta: pr.etiqueta.as_deref() };
                    if tarjeta_imagen(ui, tam, t) {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(pr.enlace.clone().unwrap_or_else(|| format!("{base}/tienda.html#{}", pr.slug))));
                    }
                }
            });
            ui.add_space(16.0);
        }
    }

    /// Panel «Inicio»: saludo, demos, proyecto nuevo, recientes, inicio rápido e instrumentos.
    fn panel_inicio(&mut self, ui: &mut egui::Ui, w: &mut Welcome, action: &mut Option<Action>) {
        let ancho = ui.available_width();
        // Saludo y accesos rápidos: seguir con el último proyecto o abrir otro.
        ui.add_space(8.0);
        let nombre = self.config.author.split_whitespace().next().map(str::to_string).unwrap_or_else(|| user_name().split_whitespace().next().unwrap_or("").to_string());
        let nombre: String = nombre.chars().take(1).flat_map(char::to_uppercase).chain(nombre.chars().skip(1)).collect();
        let saludo_txt = if nombre.is_empty() { tr(saludo()).to_string() } else { format!("{}, {nombre}", tr(saludo())) };
        let ultimo = self.config.recent.iter().find(|d| d.join(project::FILE).exists()).cloned();
        // Banner: degradado con el logo de fondo, el saludo y los accesos rápidos.
        let (banner, _) = ui.allocate_exact_size(vec2(ancho, 120.0), egui::Sense::hover());
        // Colores opacos: los extremos y la malla se superponen y con transparencia se notaría la unión.
        degradado(ui.painter(), banner, rr(16.0), PANEL().lerp_to_gamma(ACCENT(), 0.6), Color32::from_rgb(0x4B, 0x2E, 0x8F));
        let marca = egui::Rect::from_center_size(banner.center() + vec2(banner.width() * 0.08, 0.0), vec2(150.0, 150.0)).intersect(banner);
        egui::Image::new(egui::include_image!("../../../assets/quantumlogo-marca.svg")).tint(Color32::from_white_alpha(28)).paint_at(ui, marca);
        ui.scope_builder(egui::UiBuilder::new().max_rect(banner.shrink2(vec2(28.0, 22.0))), |ui| {
            ui.horizontal_centered(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(saludo_txt).size(28.0).strong().color(Color32::WHITE));
                    ui.label(RichText::new(tr("Sigue donde lo dejaste o empieza algo nuevo.")).size(14.0).color(Color32::from_white_alpha(190)));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let abrir = egui::Button::new(RichText::new(tr("Abrir…")).size(14.0).color(Color32::WHITE)).fill(Color32::from_black_alpha(70)).min_size(vec2(100.0, 42.0)).corner_radius(rr(10.0));
                    if ui.add(abrir).clicked() {
                        *action = Some(Box::new(|app: &mut Self| {
                            let r = app.open_dialog(false);
                            app.report("Proyecto abierto", r);
                        }));
                    }
                    // La demo se abre y empieza a sonar: la forma más rápida de oír el programa.
                    let demo = egui::Button::new(RichText::new(format!("         {}", tr("Escuchar la demo"))).size(14.0).strong().color(BG())).fill(VERDE_PLAY).min_size(vec2(0.0, 42.0)).corner_radius(rr(10.0));
                    let demo = ui.add(demo);
                    crate::widgets::draw_play(ui.painter(), egui::Rect::from_center_size(demo.rect.left_center() + vec2(18.0, 0.0), vec2(14.0, 16.0)), BG());
                    if demo.clicked() {
                        *action = Some(Box::new(|app: &mut Self| {
                            let dir = projects_dir().join("Demo");
                            let r = Project::open_or_demo(&dir).and_then(|p| app.reload(dir, p));
                            let ok = r.is_ok();
                            app.report("Demo abierta", r);
                            if ok && !app.playing() {
                                app.toggle_play();
                            }
                        }));
                    }
                    if let Some(dir) = ultimo {
                        let nombre = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let b = egui::Button::new(RichText::new(format!("{}  ·  {nombre}", tr("Continuar"))).size(14.0).strong().color(BG())).fill(ACCENT()).min_size(vec2(0.0, 42.0)).corner_radius(rr(10.0));
                        if ui.add(b).clicked() {
                            *action = Some(Box::new(move |app: &mut Self| {
                                if app.dir != dir {
                                    let r = app.open_dir(dir);
                                    app.report("Proyecto abierto", r);
                                }
                            }));
                        }
                    }
                });
            });
        });
        ui.add_space(28.0);
        // Nuevo proyecto (plantillas) y recientes, lado a lado.
        let col = (ancho - 32.0) * 0.6;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(col);
                titulo(ui, "NUEVO PROYECTO", "");
                ui.add_space(6.0);
                let n = TEMPLATES.len() as f32;
                let tarjeta = vec2((col - 10.0 * (n - 1.0)) / n, 104.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    for (k, (nombre, desc, _)) in TEMPLATES.iter().enumerate() {
                        let on = w.template == k;
                        let (r, resp) = ui.allocate_exact_size(tarjeta, egui::Sense::click());
                        let p = ui.painter();
                        let fondo = if on { ACCENT().gamma_multiply(0.14) } else if resp.hovered() { ELEVATED() } else { PANEL() };
                        p.rect(r, rr(12.0), fondo, egui::Stroke::new(if on { 1.5 } else { 1.0 }, if on { ACCENT() } else { BORDER() }), egui::StrokeKind::Inside);
                        let icono = egui::Rect::from_center_size(r.center_top() + vec2(0.0, 36.0), vec2(36.0, 36.0));
                        p.rect_filled(icono, rr(10.0), ACCENT().gamma_multiply(if on { 0.3 } else { 0.12 }));
                        icono_plantilla(p, icono.center(), k, if on { Color32::WHITE } else { ACCENT() });
                        // Nombre centrado; los largos ocupan dos líneas.
                        let mut job = egui::text::LayoutJob::simple(tr(nombre).to_string(), egui::FontId::proportional(12.5), TEXT(), r.width() - 12.0);
                        job.halign = egui::Align::Center;
                        job.wrap.max_rows = 2;
                        let g = ui.painter().layout_job(job);
                        let y = r.bottom() - 12.0 - g.size().y;
                        p.galley(egui::pos2(r.center().x, y), g, TEXT());
                        if resp.on_hover_text(tr(desc)).clicked() {
                            w.template = k;
                        }
                    }
                });
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    paso_a_paso(ui, "TEMPO (BPM)", &mut w.bpm, 20.0..=300.0, 1.0, "");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                        let b = egui::Button::new(RichText::new(format!("+  {}", tr("Crear proyecto"))).size(14.0).strong().color(BG())).fill(ACCENT()).min_size(vec2(220.0, 40.0)).corner_radius(rr(10.0));
                        if ui.add(b).clicked() {
                            let mut nueva = Welcome { name: tr("Mi canción").into(), author: w.author.clone(), bpm: w.bpm, template: w.template, ..Default::default() };
                            nueva.folder = w.folder.clone();
                            *action = Some(Box::new(move |app: &mut Self| {
                                let r = app.create_song(&nueva);
                                app.report(tr("Proyecto creado"), r);
                            }));
                        }
                    });
                });
            });
            ui.add_space(32.0);
            ui.vertical(|ui| {
                // Lo que queda tras la columna izquierda, el hueco de 32 y el espaciado entre ambas.
                ui.set_width(ancho - col - 32.0 - 2.0 * ui.spacing().item_spacing.x);
                ui.horizontal(|ui| {
                    titulo(ui, "RECIENTES", "");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new(tr("Ver todos…")).size(12.0).color(TEXT_DIM())).frame(false)).clicked() {
                            w.tab = 2;
                        }
                    });
                });
                ui.add_space(6.0);
                let recientes: Vec<PathBuf> = self.config.recent.iter().filter(|d| d.join(project::FILE).exists()).take(4).cloned().collect();
                egui::Frame::new().fill(PANEL()).stroke(egui::Stroke::new(1.0, BORDER())).corner_radius(rr(12.0)).inner_margin(8.0).show(ui, |ui| {
                ui.set_min_height(200.0);
                ui.set_width(ui.available_width());
                if recientes.is_empty() {
                    ui.add_space(50.0);
                    ui.vertical_centered(|ui| {
                        icono_seccion(ui.painter(), ui.cursor().center_top() + vec2(0.0, 14.0), 2, TEXT_DIM());
                        ui.add_space(36.0);
                        ui.label(RichText::new(tr("Todavía no hay proyectos recientes.")).strong().color(TEXT()));
                        ui.label(RichText::new(tr("Las canciones que guardes o abras aparecen aquí.")).size(12.0).color(TEXT_DIM()));
                    });
                }
                for dir in recientes {
                    let nombre = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), egui::Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, rr(8.0), ELEVATED());
                    }
                    let insignia = egui::Rect::from_min_size(r.left_top() + vec2(8.0, 8.0), vec2(30.0, 30.0));
                    let c = color_de(&nombre);
                    ui.painter().rect_filled(insignia, rr(8.0), c.gamma_multiply(0.22));
                    let iniciales: String = nombre.chars().take(2).collect::<String>().to_uppercase();
                    ui.painter().text(insignia.center(), egui::Align2::CENTER_CENTER, iniciales, egui::FontId::proportional(12.0), c);
                    ui.painter().with_clip_rect(egui::Rect::from_min_max(r.min, r.max - vec2(70.0, 0.0))).text(r.left_center() + vec2(48.0, 0.0), egui::Align2::LEFT_CENTER, &nombre, egui::FontId::proportional(14.0), TEXT());
                    ui.painter().text(r.right_center() - vec2(10.0, 0.0), egui::Align2::RIGHT_CENTER, hace(&dir), egui::FontId::proportional(11.0), TEXT_DIM());
                    if resp.on_hover_text(dir.display().to_string()).clicked() {
                        *action = Some(Box::new(move |app: &mut Self| {
                            let r = app.open_dir(dir);
                            app.report("Proyecto abierto", r);
                        }));
                    }
                }
                });
            });
        });
        ui.add_space(32.0);
        // Novedades del blog y tienda, en una sola fila de tarjetas.
        let web = web::contenido();
        let base = web::base();
        let abrir = |ui: &egui::Ui, url: String| ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        ui.horizontal(|ui| {
            titulo(ui, "NOVEDADES", "");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(tr("Tienda")).size(12.0).color(TEXT_DIM())).frame(false)).clicked() {
                    abrir(ui, format!("{base}/tienda.html"));
                }
                if ui.add(egui::Button::new(RichText::new(tr("Blog")).size(12.0).color(TEXT_DIM())).frame(false)).clicked() {
                    abrir(ui, format!("{base}/noticias.html"));
                }
            });
        });
        ui.add_space(6.0);
        let (noticias, productos) = web.as_ref().map(|c| (c.noticias.clone(), c.productos.clone())).unwrap_or_default();
        if noticias.is_empty() && productos.is_empty() {
            let texto = match &web {
                None => tr("Cargando novedades…"),
                Some(c) if !c.en_linea => tr("Sin conexión con quantumdaw.com por ahora."),
                Some(_) => tr("Todavía no hay novedades publicadas."),
            };
            ui.label(RichText::new(texto).size(12.0).color(TEXT_DIM()));
            ui.add_space(10.0);
            // Sin novedades: tarjetas para empezar con lo que ya trae el programa.
            let locales = [
                (Portada::Noticia, "DEMO", "Escuchar la demo", "Batería, bajo y pad listos para probar el mixer y los efectos.", "Abrir y reproducir", ACCENT(), 0u8),
                (Portada::Producto("instrumento"), "GRATIS", "Instrumentos reales", "Guitarras, bajos, batería, cuerdas y piano libres, con su licencia.", "Gratis", VERDE_PLAY, 1),
                (Portada::Producto("plugin"), "EFECTOS", "Plugins QUANTUM", "Curves, compresor, reverb, delay, saturación, Tune y más.", "Incluidos", ACCENT(), 2),
            ];
            let n = locales.len() as f32;
            let tam = vec2(((ancho - 16.0 * (n - 1.0)) / n).min(340.0), 250.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                for (portada, arriba, titulo, texto, pie, color, k) in locales {
                    let t = Tarjeta { imagen: None, portada, arriba: tr(arriba), titulo: tr(titulo), texto: tr(texto), pie: (tr(pie), color), etiqueta: None };
                    if tarjeta_imagen(ui, tam, t) {
                        *action = Some(Box::new(move |app: &mut Self| match k {
                            0 => {
                                let dir = projects_dir().join("Demo");
                                let r = Project::open_or_demo(&dir).and_then(|p| app.reload(dir, p));
                                let ok = r.is_ok();
                                app.report("Demo abierta", r);
                                if ok && !app.playing() {
                                    app.toggle_play();
                                }
                            }
                            1 => (app.show_lib, app.lib_tab) = (true, library::INSTRUMENTS_TAB),
                            _ => (app.show_lib, app.lib_tab) = (true, library::FX_TAB),
                        }));
                    }
                }
            });
            return;
        }
        // Hasta cuatro: las noticias más recientes y luego la tienda.
        let n = ((ancho + 16.0) / 250.0).floor().clamp(1.0, 4.0) as usize;
        let tam = vec2((ancho - 16.0 * (n as f32 - 1.0)) / n as f32, 250.0);
        let n_noticias = noticias.len().min(n.div_ceil(2)).max(n.saturating_sub(productos.len())).min(noticias.len());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            for nt in noticias.iter().take(n_noticias) {
                let img = web::imagen(ui.ctx(), nt.imagen.as_deref());
                let t = Tarjeta { imagen: img, portada: Portada::Noticia, arriba: &nt.fecha, titulo: &nt.titulo, texto: &nt.resumen, pie: (tr("Leer la noticia"), ACCENT()), etiqueta: None };
                if tarjeta_imagen(ui, tam, t) {
                    abrir(ui, format!("{base}/noticia.php?n={}", nt.slug));
                }
            }
            for pr in productos.iter().take(n - n_noticias) {
                let precio = web::precio(pr);
                let img = web::imagen(ui.ctx(), pr.imagen.as_deref());
                let tipo = pr.tipo.to_uppercase();
                let t = Tarjeta { imagen: img, portada: Portada::Producto(&pr.tipo), arriba: &tipo, titulo: &pr.nombre, texto: &pr.resumen, pie: (&precio, if pr.gratis { VERDE_PLAY } else { ACCENT() }), etiqueta: pr.etiqueta.as_deref() };
                if tarjeta_imagen(ui, tam, t) {
                    abrir(ui, pr.enlace.clone().unwrap_or_else(|| format!("{base}/tienda.html#{}", pr.slug)));
                }
            }
        });
    }

}

/// Portada dibujada cuando la noticia o el producto no tiene imagen.
enum Portada<'a> {
    Noticia,
    Producto(&'a str),
}

/// Contenido de una tarjeta con imagen.
struct Tarjeta<'a> {
    imagen: Option<egui::TextureHandle>,
    portada: Portada<'a>,
    arriba: &'a str,
    titulo: &'a str,
    texto: &'a str,
    pie: (&'a str, Color32),
    etiqueta: Option<&'a str>,
}

/// Tarjeta de novedad o producto: imagen arriba (recortada para llenar), fecha o tipo, título,
/// resumen y pie (precio o «leer»). Se eleva al pasar el ratón; devuelve si se hizo clic.
fn tarjeta_imagen(ui: &mut egui::Ui, tam: egui::Vec2, t: Tarjeta) -> bool {
    let (r0, resp) = ui.allocate_exact_size(tam, egui::Sense::click());
    let hov = resp.hovered();
    let r = if hov { r0.translate(vec2(0.0, -3.0)) } else { r0 };
    let p = ui.painter();
    let radio = rr(14.0);
    p.rect_filled(r.translate(vec2(0.0, if hov { 8.0 } else { 5.0 })), radio, Color32::from_black_alpha(if hov { 90 } else { 50 }));
    p.rect(r, radio, ELEVATED(), egui::Stroke::new(1.0, if hov { ACCENT().gamma_multiply(0.7) } else { BORDER() }), egui::StrokeKind::Inside);
    // Imagen (o portada dibujada) con las esquinas de arriba redondeadas.
    let img_r = egui::Rect::from_min_size(r.min, vec2(r.width(), (r.height() * 0.5).round()));
    let esquinas = egui::CornerRadius { nw: radio as u8, ne: radio as u8, sw: 0, se: 0 };
    match &t.imagen {
        Some(tex) => {
            // Recorte tipo «cover»: se llena el recuadro sin deformar.
            let (iw, ih) = (tex.size()[0] as f32, tex.size()[1] as f32);
            let (a_img, a_r) = (iw / ih.max(1.0), img_r.width() / img_r.height());
            let uv = if a_img > a_r {
                let f = a_r / a_img;
                egui::Rect::from_min_max(egui::pos2((1.0 - f) / 2.0, 0.0), egui::pos2((1.0 + f) / 2.0, 1.0))
            } else {
                let f = a_img / a_r;
                egui::Rect::from_min_max(egui::pos2(0.0, (1.0 - f) / 2.0), egui::pos2(1.0, (1.0 + f) / 2.0))
            };
            let zoom = if hov { img_r.expand(0.0) } else { img_r };
            p.add(egui::epaint::RectShape::filled(zoom, esquinas, Color32::WHITE).with_texture(tex.id(), uv));
        }
        None => portada(ui, img_r, esquinas, t.titulo, &t.portada),
    }
    // Degradado inferior sobre la imagen para que la etiqueta se lea.
    if let Some(e) = t.etiqueta.filter(|e| !e.is_empty()) {
        let g = p.layout_no_wrap(e.to_uppercase(), egui::FontId::proportional(10.5), Color32::WHITE);
        let er = egui::Rect::from_min_size(img_r.left_top() + vec2(12.0, 12.0), g.size() + vec2(14.0, 8.0));
        p.rect_filled(er, rr(6.0), ACCENT());
        p.galley(er.min + vec2(7.0, 4.0), g, BG());
    }
    let pc = p.with_clip_rect(r.shrink(2.0));
    let x = r.left() + 16.0;
    let mut y = img_r.bottom() + 20.0;
    pc.text(egui::pos2(x, y), egui::Align2::LEFT_CENTER, t.arriba, egui::FontId::proportional(10.5), ACCENT());
    y += 12.0;
    // Título (hasta dos líneas) y resumen (lo que entre).
    let ancho = r.width() - 32.0;
    let mut job = egui::text::LayoutJob::simple(t.titulo.to_string(), egui::FontId::proportional(16.0), TEXT(), ancho);
    job.wrap.max_rows = 2;
    let gt = ui.painter().layout_job(job);
    let alto_t = gt.size().y;
    pc.galley(egui::pos2(x, y), gt, TEXT());
    y += alto_t + 6.0;
    let filas = (((r.bottom() - 34.0 - y) / 15.0).floor().max(0.0)) as usize;
    if filas > 0 {
        let mut job = egui::text::LayoutJob::simple(t.texto.to_string(), egui::FontId::proportional(12.0), TEXT_DIM(), ancho);
        job.wrap.max_rows = filas;
        let gr = ui.painter().layout_job(job);
        pc.galley(egui::pos2(x, y), gr, TEXT_DIM());
    }
    // Pie: precio en píldora (tienda) o enlace (noticias).
    let pie_y = r.bottom() - 20.0;
    if matches!(t.portada, Portada::Producto(_)) {
        let g = p.layout_no_wrap(t.pie.0.to_string(), egui::FontId::proportional(13.0), t.pie.1);
        let pr = egui::Rect::from_min_size(egui::pos2(x - 2.0, pie_y - 12.0), g.size() + vec2(18.0, 8.0));
        p.rect_filled(pr, rr(12.0), t.pie.1.gamma_multiply(0.16));
        p.galley(pr.min + vec2(9.0, 4.0), g, t.pie.1);
        let color = if hov { TEXT() } else { TEXT_DIM() };
        let tr_ = p.text(egui::pos2(r.right() - 30.0, pie_y), egui::Align2::RIGHT_CENTER, tr("Ver"), egui::FontId::proportional(12.0), color);
        flecha(p, egui::pos2(tr_.right() + 9.0, pie_y), color, hov);
    } else {
        let tr_ = p.text(egui::pos2(x, pie_y), egui::Align2::LEFT_CENTER, t.pie.0, egui::FontId::proportional(12.5), t.pie.1);
        flecha(p, egui::pos2(tr_.right() + 10.0, pie_y), t.pie.1, hov);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Portada dibujada: degradado con el color del título y un motivo según el tipo.
fn portada(ui: &egui::Ui, r: egui::Rect, esquinas: egui::CornerRadius, titulo: &str, tipo: &Portada) {
    let p = ui.painter();
    let c = color_de(titulo);
    let c2 = Color32::from_rgb(c.b() / 2 + 40, c.r() / 3 + 20, c.g() / 2 + 60);
    p.rect_filled(r, esquinas, c2);
    let mut mesh = egui::Mesh::default();
    let (a, b) = (c.gamma_multiply(0.95), Color32::from_black_alpha(0));
    for (pt, col) in [(r.left_top(), a), (r.right_top(), b), (r.right_bottom(), b), (r.left_bottom(), a)] {
        mesh.colored_vertex(pt, col);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.with_clip_rect(r.shrink2(vec2(0.0, 0.0))).add(mesh);
    let pc = p.with_clip_rect(r);
    // Luces suaves.
    for (fx, fy, rad, al) in [(0.85, 0.1, 90.0, 26), (0.15, 0.95, 70.0, 18)] {
        pc.circle_filled(egui::pos2(r.left() + r.width() * fx, r.top() + r.height() * fy), rad, Color32::from_white_alpha(al));
    }
    let centro = r.center();
    let blanco = |a: u8| Color32::from_white_alpha(a);
    match tipo {
        Portada::Noticia => {
            egui::Image::new(egui::include_image!("../../../assets/quantumlogo-marca.svg")).tint(blanco(70)).paint_at(ui, egui::Rect::from_center_size(centro, vec2(r.height() * 0.62, r.height() * 0.62)));
        }
        Portada::Producto(t) if t.to_lowercase().contains("instrument") => {
            let teclas = egui::Rect::from_center_size(centro, vec2(r.width() * 0.55, r.height() * 0.42));
            pc.rect_filled(teclas, rr(6.0), blanco(200));
            for k in 1..10 {
                let xk = teclas.left() + teclas.width() * k as f32 / 10.0;
                pc.line_segment([egui::pos2(xk, teclas.top()), egui::pos2(xk, teclas.bottom())], egui::Stroke::new(1.0, c2));
                if ![3, 7].contains(&k) {
                    pc.rect_filled(egui::Rect::from_center_size(egui::pos2(xk, teclas.top() + teclas.height() * 0.3), vec2(teclas.width() / 18.0, teclas.height() * 0.6)), 1.0, c2);
                }
            }
        }
        Portada::Producto(t) if t.to_lowercase().contains("sonido") || t.to_lowercase().contains("sample") => {
            let pts: Vec<egui::Pos2> = (0..80).map(|i| i as f32 / 79.0).map(|t| egui::pos2(r.left() + 20.0 + (r.width() - 40.0) * t, centro.y + (t * 37.0).sin() * (t * 3.1).sin().abs() * r.height() * 0.3)).collect();
            pc.add(egui::Shape::line(pts, egui::Stroke::new(2.5, blanco(210))));
        }
        Portada::Producto(_) => {
            // Plugin: una perilla grande con su arco de valor.
            let rad = r.height() * 0.28;
            pc.circle_filled(centro, rad + 10.0, Color32::from_black_alpha(60));
            let arco = |desde: f32, hasta: f32, color: Color32, w: f32| {
                let pts: Vec<egui::Pos2> = (0..=40).map(|i| desde + (hasta - desde) * i as f32 / 40.0).map(|a| centro + vec2(a.cos(), a.sin()) * (rad + 4.0)).collect();
                pc.add(egui::Shape::line(pts, egui::Stroke::new(w, color)));
            };
            let (a0, a1) = (std::f32::consts::PI * 0.75, std::f32::consts::PI * 2.25);
            arco(a0, a1, blanco(50), 4.0);
            arco(a0, a0 + (a1 - a0) * 0.7, blanco(230), 4.0);
            pc.circle_filled(centro, rad - 4.0, Color32::from_rgb(0x1A, 0x1D, 0x26));
            pc.circle_stroke(centro, rad - 4.0, egui::Stroke::new(1.5, blanco(60)));
            let ang = a0 + (a1 - a0) * 0.7;
            pc.line_segment([centro + vec2(ang.cos(), ang.sin()) * (rad * 0.25), centro + vec2(ang.cos(), ang.sin()) * (rad - 10.0)], egui::Stroke::new(3.0, blanco(240)));
        }
    }
}

/// Degradado horizontal de `a` a `b` con las esquinas redondeadas: los extremos son rectángulos
/// redondeados de color sólido y el tramo central, una malla con el degradado.
fn degradado(p: &egui::Painter, r: egui::Rect, radio: f32, a: Color32, b: Color32) {
    let ancho_extremo = (radio * 2.0).min(r.width() / 2.0);
    p.rect_filled(egui::Rect::from_min_size(r.min, vec2(ancho_extremo, r.height())), radio, a);
    p.rect_filled(egui::Rect::from_min_max(egui::pos2(r.right() - ancho_extremo, r.top()), r.max), radio, b);
    let medio = r.shrink2(vec2(radio, 0.0));
    let mut mesh = egui::Mesh::default();
    for (pt, col) in [(medio.left_top(), a), (medio.right_top(), b), (medio.right_bottom(), b), (medio.left_bottom(), a)] {
        mesh.colored_vertex(pt, col);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(mesh);
}

/// Campo numérico con botones − y + a los lados (pasos de `paso`), con su título encima.
fn paso_a_paso(ui: &mut egui::Ui, titulo: &str, valor: &mut f32, rango: std::ops::RangeInclusive<f32>, paso: f32, sufijo: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(tr(titulo)).size(10.5).strong().color(TEXT_DIM()));
        egui::Frame::new().fill(PANEL()).stroke(egui::Stroke::new(1.0, BORDER())).corner_radius(rr(10.0)).inner_margin(egui::Margin::symmetric(4, 3)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let boton = |t: &str| egui::Button::new(RichText::new(t).size(16.0).color(TEXT())).frame(false).min_size(vec2(30.0, 30.0));
                if ui.add(boton("−")).clicked() {
                    *valor = (*valor - paso).max(*rango.start());
                }
                ui.add_sized([74.0, 30.0], egui::DragValue::new(valor).range(rango.clone()).speed(0.5).suffix(sufijo));
                if ui.add(boton("+")).clicked() {
                    *valor = (*valor + paso).min(*rango.end());
                }
            });
        });
    });
}

/// Flecha «→» dibujada (se adelanta un poco al pasar el ratón).
fn flecha(p: &egui::Painter, c: egui::Pos2, color: Color32, hov: bool) {
    let c = c + vec2(if hov { 3.0 } else { 0.0 }, 0.0);
    let s = egui::Stroke::new(1.5, color);
    p.line_segment([c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], s);
    p.line_segment([c + vec2(5.0, 0.0), c + vec2(1.5, -3.5)], s);
    p.line_segment([c + vec2(5.0, 0.0), c + vec2(1.5, 3.5)], s);
}
