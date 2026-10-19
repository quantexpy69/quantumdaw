//! Menú principal (al estilo de Studio One): canción nueva con sus ajustes y plantillas, datos de la
//! canción y derechos de autor, proyectos recientes, demo y créditos, con el logo y la versión.
use crate::{widgets::rr, *};
use egui::{Id, RichText, vec2};
use project::{Collection, SongMeta};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn logo(size: f32) -> egui::Image<'static> {
    egui::Image::new(egui::include_image!("../../../assets/quantumlogo.svg")).fit_to_exact_size(vec2(size, size))
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
    /// Inicio rápido: pistas de instrumento y de audio de la canción nueva.
    inst_tracks: u32,
    audio_tracks: u32,
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
            inst_tracks: 1,
            audio_tracks: 1,
        }
    }
}

/// Campos de datos de la canción y derechos de autor.
fn meta_form(ui: &mut egui::Ui, m: &mut SongMeta, id: &str) {
    egui::Grid::new(id).num_columns(2).spacing([24.0, 12.0]).show(ui, |ui| {
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
            ui.label(RichText::new(tr(label)).size(14.0));
            ui.add(egui::TextEdit::singleline(field).hint_text(hint).desired_width(360.0).font(egui::FontId::proportional(14.0)));
            ui.end_row();
        }
        ui.label(RichText::new(tr("Notas")).size(14.0));
        ui.add(egui::TextEdit::multiline(&mut m.notes).desired_width(360.0).desired_rows(3));
        ui.end_row();
    });
}

fn card(ui: &mut egui::Ui, on: bool, width: f32, add: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let frame =
        egui::Frame::new().fill(if on { ACCENT().gamma_multiply(0.35) } else { ELEVATED() }).stroke(egui::Stroke::new(1.0, if on { ACCENT() } else { BORDER() })).corner_radius(rr(12.0)).inner_margin(14.0);
    let r = frame
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(add);
        })
        .response
        .interact(egui::Sense::click());
    if r.hovered() && !on {
        ui.painter().rect_stroke(r.rect, rr(12.0), egui::Stroke::new(1.0, ACCENT().gamma_multiply(0.7)), egui::StrokeKind::Inside);
    }
    r
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
                        ui.label(RichText::new(tr("QUANTUM DAW")).size(18.0).strong().color(TEXT()));
                        ui.label(RichText::new(format!("v{VERSION}")).size(11.0).color(TEXT_DIM()));
                    });
                });
                lado.spacing_mut().item_spacing = vec2(8.0, 4.0);
                lado.add_space(16.0);
                let seccion = |ui: &mut egui::Ui, w: &mut Welcome, k: u8, texto: &str| {
                    let (r, resp) = ui.allocate_exact_size(vec2(194.0, 34.0), egui::Sense::click());
                    let activa = w.tab == k;
                    if activa || resp.hovered() {
                        ui.painter().rect_filled(r, rr(7.0), if activa { ELEVATED() } else { PANEL() });
                    }
                    if activa {
                        ui.painter().rect_filled(egui::Rect::from_min_size(r.min + vec2(0.0, 7.0), vec2(3.0, 20.0)), 1.5, ACCENT());
                    }
                    ui.painter().text(r.left_center() + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, tr(texto), egui::FontId::proportional(14.0), if activa { TEXT() } else { TEXT_DIM() });
                    if resp.clicked() {
                        w.tab = k;
                    }
                };
                seccion(&mut lado, &mut w, 0, "Inicio");
                seccion(&mut lado, &mut w, 3, "Nueva canción");
                seccion(&mut lado, &mut w, 2, "Abrir");
                seccion(&mut lado, &mut w, 1, "Datos de la canción");
                lado.add_space(10.0);
                lado.label(RichText::new(tr("PREFERENCIAS")).size(10.5).strong().color(TEXT_DIM()));
                seccion(&mut lado, &mut w, 5, "Interfaz");
                seccion(&mut lado, &mut w, 6, "Espacios de trabajo");
                for (texto, accion) in [("Audio y MIDI", 0u8), ("Atajos", 1)] {
                    let (r, resp) = lado.allocate_exact_size(vec2(194.0, 34.0), egui::Sense::click());
                    if resp.hovered() {
                        lado.painter().rect_filled(r, rr(7.0), PANEL());
                    }
                    lado.painter().text(r.left_center() + vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, tr(texto), egui::FontId::proportional(14.0), TEXT_DIM());
                    if resp.clicked() {
                        action = Some(Box::new(move |app: &mut Self| {
                            app.dialog = Some(if accion == 0 {
                                Dialog::Settings(app.audio.clone(), app.rec_bits, engine::audio_devices(), engine::midi_ports(), app.config.midi_port.clone())
                            } else {
                                Dialog::Keys(None)
                            });
                        }));
                    }
                }
                seccion(&mut lado, &mut w, 4, "Acerca de");
                lado.add_space((lado.available_height() - 40.0).max(0.0));
                lado.label(RichText::new(tr("Lo que cambies aquí se guarda al instante.")).size(10.5).color(TEXT_DIM()));

                // Contenido.
                let (cont, _) = ui.allocate_exact_size(vec2(ancho - 230.0, alto), egui::Sense::hover());
                let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(cont.shrink2(vec2(30.0, 22.0))).layout(egui::Layout::top_down(egui::Align::Min)));
                let ui = &mut ui;
                ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                    if ui.add(egui::Button::new(RichText::new("×").size(18.0)).frame(false)).on_hover_text(tr("Cerrar")).clicked() {
                        close = true;
                    }
                });
                ui.add_space(-14.0);
                let cuerpo_h = ui.available_height() - 40.0;
                egui::ScrollArea::vertical().max_height(cuerpo_h).auto_shrink([false, false]).show(ui, |ui| match w.tab {
                    0 => self.panel_inicio(ui, &mut w, &mut action),
                    1 => {
                            ui.label(RichText::new(format!("Proyecto actual: {}", self.dir.display())).color(TEXT_DIM()));
                            ui.add_space(4.0);
                            meta_form(ui, &mut self.meta, "song-meta");
                            ui.add_space(10.0);
                            ui.vertical_centered(|ui| {
                                if ui.add(egui::Button::new(RichText::new(tr("Guardar datos")).size(16.0).strong()).fill(ACCENT()).min_size(vec2(220.0, 40.0)).corner_radius(rr(20.0))).clicked() {
                                    self.save();
                                }
                            });
                        }
                    2 => {
                            let listing = w.listing.get_or_insert_with(|| scan(&projects_dir())).clone();
                            for (coll, info, songs) in listing {
                                // Cabecera del proyecto con su ficha.
                                let title = if coll.is_empty() { tr("Canciones sin proyecto").to_string() } else { coll.clone() };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(title).size(16.0).strong().color(METER[1]));
                                    if let Some(c) = &info {
                                        let by = if c.author.is_empty() { String::new() } else { format!("{} {} · ", tr("por"), c.author) };
                                        ui.label(RichText::new(format!("{by}{} {} · {} {}", tr("creado"), c.created_at, songs.len(), tr("canciones"))).size(12.0).color(TEXT_DIM()));
                                    }
                                });
                                for (dir, p) in songs {
                                    let name = if p.meta.title.is_empty() { dir.file_name().unwrap_or_default().to_string_lossy().to_string() } else { p.meta.title.clone() };
                                    let r = card(ui, false, 600.0, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&name).size(15.0).strong());
                                            if !p.meta.artist.is_empty() {
                                                ui.label(RichText::new(&p.meta.artist).color(TEXT_DIM()));
                                            }
                                        });
                                        ui.label(RichText::new(song_info(&p)).size(11.5).color(TEXT_DIM()));
                                    });
                                    if r.on_hover_text(dir.display().to_string()).clicked() {
                                        action = Some(Box::new(move |app: &mut Self| {
                                            let r = app.open_dir(dir);
                                            app.report("Proyecto abierto", r);
                                        }));
                                    }
                                }
                                ui.add_space(6.0);
                            }
                            if ui.button(RichText::new(tr("Abrir otra carpeta de proyecto…")).size(14.5)).clicked() {
                                action = Some(Box::new(|app: &mut Self| {
                                    let r = app.open_dialog(false);
                                    app.report("Proyecto abierto", r);
                                }));
                            }
                        }
                    3 => {
                        ui.label(RichText::new(tr("Nueva canción")).size(22.0).strong());

                            egui::Grid::new("new-song").num_columns(2).spacing([24.0, 14.0]).show(ui, |ui| {
                                ui.label(RichText::new(tr("Nombre de la canción")).size(14.0));
                                ui.add(egui::TextEdit::singleline(&mut w.name).desired_width(360.0).font(egui::FontId::proportional(15.0)));
                                ui.end_row();
                                ui.label(RichText::new(tr("Artista")).size(14.0));
                                ui.add(egui::TextEdit::singleline(&mut w.meta.artist).desired_width(360.0));
                                ui.end_row();
                                ui.label(RichText::new(tr("Copyright ©")).size(14.0));
                                ui.add(egui::TextEdit::singleline(&mut w.meta.copyright).hint_text(tr("© 2026 Nombre del titular")).desired_width(360.0));
                                ui.end_row();
                                ui.label(RichText::new(tr("Proyecto")).size(14.0));
                                ui.horizontal(|ui| {
                                    ui.add(egui::TextEdit::singleline(&mut w.collection).hint_text(tr("Opcional: agrupa varias canciones")).desired_width(290.0));
                                    let existing: Vec<String> = scan(&w.folder).into_iter().map(|l| l.0).filter(|n| !n.is_empty()).collect();
                                    egui::ComboBox::from_id_salt("w-coll").selected_text(tr("Elegir")).width(70.0).show_ui(ui, |ui| {
                                        ui.selectable_value(&mut w.collection, String::new(), tr("(sin proyecto)"));
                                        for n in existing {
                                            ui.selectable_value(&mut w.collection, n.clone(), n);
                                        }
                                    });
                                });
                                ui.end_row();
                                ui.label(RichText::new(tr("Creado por")).size(14.0));
                                ui.add(egui::TextEdit::singleline(&mut w.author).desired_width(360.0));
                                ui.end_row();
                                ui.label(RichText::new(tr("Ubicación")).size(14.0));
                                ui.horizontal(|ui| {
                                    ui.set_max_width(420.0);
                                    if ui.button(tr("Cambiar…")).clicked()
                                        && let Some(d) = rfd::FileDialog::new().set_directory(&w.folder).pick_folder()
                                    {
                                        w.folder = d;
                                    }
                                    ui.add(egui::Label::new(RichText::new(w.folder.display().to_string()).color(TEXT_DIM())).truncate()).on_hover_text(w.folder.display().to_string());
                                });
                                ui.end_row();
                                ui.label(RichText::new(tr("Frecuencia de muestreo")).size(14.0));
                                egui::ComboBox::from_id_salt("w-rate").selected_text(format!("{} Hz", w.rate)).show_ui(ui, |ui| {
                                    for r in [44_100, 48_000, 88_200, 96_000, 192_000] {
                                        ui.selectable_value(&mut w.rate, r, format!("{r} Hz"));
                                    }
                                });
                                ui.end_row();
                                ui.label(RichText::new(tr("Resolución de grabación")).size(14.0));
                                egui::ComboBox::from_id_salt("w-bits").selected_text(format!("{} bits", w.bits)).show_ui(ui, |ui| {
                                    for b in [16, 24, 32] {
                                        ui.selectable_value(&mut w.bits, b, format!("{b} bits"));
                                    }
                                });
                                ui.end_row();
                                ui.label(RichText::new(tr("Tempo y métrica")).size(14.0));
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(&mut w.bpm).range(20.0..=300.0).speed(0.5).suffix(" BPM"));
                                    egui::ComboBox::from_id_salt("w-sig").width(64.0).selected_text(format!("{}/{}", w.sig.0, w.sig.1)).show_ui(ui, |ui| {
                                        for s in [(2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8), (12, 8)] {
                                            ui.selectable_value(&mut w.sig, s, format!("{}/{}", s.0, s.1));
                                        }
                                    });
                                });
                                ui.end_row();
                            });
                            ui.add_space(10.0);
                            ui.label(RichText::new(tr("PLANTILLA")).size(11.5).strong().color(TEXT_DIM()));
                            // Tarjetas de plantilla del mismo ancho y alto, tres por fila.
                            let size = vec2(200.0, 74.0);
                            for row in TEMPLATES.iter().enumerate().collect::<Vec<_>>().chunks(3) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    for &(k, (name, desc, _)) in row {
                                        let on = w.template == k;
                                        let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
                                        let fill = if on { ACCENT().gamma_multiply(0.35) } else if r.hovered() { BORDER() } else { ELEVATED() };
                                        ui.painter().rect(rect, rr(12.0), fill, egui::Stroke::new(1.0, if on || r.hovered() { ACCENT() } else { BORDER() }), egui::StrokeKind::Inside);
                                        let inner = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 12.0))).layout(egui::Layout::top_down(egui::Align::Min)));
                                        inner.spacing_mut().item_spacing.y = 3.0;
                                        inner.label(RichText::new(tr(name)).size(14.5).strong());
                                        inner.add(egui::Label::new(RichText::new(tr(desc)).size(12.0).color(TEXT_DIM())).wrap());
                                        if r.clicked() {
                                            w.template = k;
                                        }
                                    }
                                });
                            }
                        ui.add_space(8.0);
                        if ui.add(egui::Button::new(RichText::new(tr("Crear canción")).size(15.0).strong().color(BG())).fill(ACCENT()).min_size(vec2(240.0, 40.0)).corner_radius(rr(8.0))).clicked() {
                            let r = self.create_song(&w);
                            self.report(format!("Canción «{}» creada", w.name), r);
                            close = true;
                        }
                    }
                    5 => self.panel_interfaz(ui),
                    6 => self.panel_espacios(ui, &mut close),
                    _ => {
                            ui.label(RichText::new(format!("Quantum DAW {VERSION}")).size(20.0).strong());
                            ui.label(RichText::new(tr("Estación de audio digital escrita en Rust.")).size(14.0));
                            ui.label(RichText::new(tr("Desarrollado por QUANTEX")).size(14.0).strong().color(ACCENT()));
                            ui.add_space(8.0);
                            for site in ["www.quantumdaw.com", "www.quantex.com.py"] {
                                ui.hyperlink_to(RichText::new(site).size(15.0).color(ACCENT()), format!("https://{site}"));
                            }
                            ui.add_space(8.0);

                            ui.label(RichText::new(tr("Proyecto de demostración con batería, bajo y pad para probar el mixer y los efectos.")).size(14.0).color(TEXT_DIM()));
                            if ui.button(RichText::new(tr("Abrir la demo")).size(14.5)).clicked() {
                                action = Some(Box::new(|app: &mut Self| {
                                    let dir = projects_dir().join("Demo");
                                    let r = Project::open_or_demo(&dir).and_then(|p| app.reload(dir, p));
                                    app.report("Demo abierta", r);
                                }));
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new(tr("Instrumentos reales gratuitos: en la Biblioteca (Y) → Instrumentos, o desde el piano roll.")).color(TEXT_DIM()));
                            ui.label(RichText::new(tr("Atajos: Herramientas → Atajos de teclado y ratón.")).color(TEXT_DIM()));
                    }
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
        ui.label(RichText::new(tr(texto)).size(11.0).strong().color(TEXT_DIM()));
        if !nota.is_empty() {
            ui.label(RichText::new(tr(nota)).size(11.0).color(TEXT_DIM().gamma_multiply(0.8)));
        }
    });
}

/// Contador con botones − y + (pistas, tempo).
fn contador(ui: &mut egui::Ui, etiqueta: &str, valor: &mut f32, min: f32, max: f32) {
    ui.vertical(|ui| {
        ui.label(RichText::new(tr(etiqueta)).size(10.5).strong().color(TEXT_DIM()));
        egui::Frame::new().fill(ELEVATED()).corner_radius(rr(8.0)).stroke(egui::Stroke::new(1.0, BORDER())).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new("−").size(16.0)).frame(false).min_size(vec2(26.0, 26.0))).clicked() {
                    *valor = (*valor - 1.0).max(min);
                }
                ui.add_sized([46.0, 26.0], egui::Label::new(RichText::new(format!("{valor:.0}")).size(15.0).strong()));
                if ui.add(egui::Button::new(RichText::new("+").size(16.0)).frame(false).min_size(vec2(26.0, 26.0))).clicked() {
                    *valor = (*valor + 1.0).min(max);
                }
            });
        });
    });
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

    /// Panel «Inicio»: saludo, demos, proyecto nuevo, recientes, inicio rápido e instrumentos.
    fn panel_inicio(&mut self, ui: &mut egui::Ui, w: &mut Welcome, action: &mut Option<Action>) {
        // Banner con degradado azul → violeta → rosa.
        let ancho = ui.available_width();
        let (banner, _) = ui.allocate_exact_size(vec2(ancho, 128.0), egui::Sense::hover());
        let mut mesh = egui::Mesh::default();
        let colores = [Color32::from_rgb(0x1E, 0x5F, 0xC8), Color32::from_rgb(0x6A, 0x3F, 0xC9), Color32::from_rgb(0xD0, 0x3A, 0x8C)];
        for k in 0..2 {
            let (a, b) = (banner.left() + banner.width() * k as f32 / 2.0, banner.left() + banner.width() * (k + 1) as f32 / 2.0);
            let base = mesh.vertices.len() as u32;
            for (x, y, c) in [(a, banner.top(), colores[k]), (b, banner.top(), colores[k + 1]), (b, banner.bottom(), colores[k + 1]), (a, banner.bottom(), colores[k])] {
                mesh.colored_vertex(egui::pos2(x, y), c);
            }
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        ui.painter().add(mesh);
        ui.painter().rect_stroke(banner, rr(12.0), egui::Stroke::new(6.0, PANEL()), egui::StrokeKind::Outside);
        let p = ui.painter();
        p.text(banner.left_top() + vec2(30.0, 38.0), egui::Align2::LEFT_CENTER, tr(saludo()), egui::FontId::proportional(28.0), Color32::WHITE);
        p.text(banner.left_top() + vec2(30.0, 74.0), egui::Align2::LEFT_CENTER, tr("Sigue donde lo dejaste o empieza algo nuevo."), egui::FontId::proportional(14.0), Color32::from_white_alpha(220));
        let ultimo = self.config.recent.iter().find(|d| d.join(project::FILE).exists()).cloned();
        let mut botones = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_max(banner.right_top() + vec2(-420.0, 38.0), banner.right_bottom() - vec2(24.0, 38.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if botones.add(egui::Button::new(RichText::new(tr("Abrir…")).size(14.0)).min_size(vec2(110.0, 46.0)).corner_radius(rr(8.0))).clicked() {
            *action = Some(Box::new(|app: &mut Self| {
                let r = app.open_dialog(false);
                app.report("Proyecto abierto", r);
            }));
        }
        if let Some(dir) = ultimo {
            let nombre = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
            let b = egui::Button::new(RichText::new(format!("{}\n{nombre}  ▶", tr("CONTINUAR"))).size(13.5).strong().color(BG())).fill(ACCENT()).min_size(vec2(220.0, 46.0)).corner_radius(rr(8.0));
            if botones.add(b).clicked() {
                *action = Some(Box::new(move |app: &mut Self| {
                    if app.dir != dir {
                        let r = app.open_dir(dir);
                        app.report("Proyecto abierto", r);
                    }
                }));
            }
        }
        ui.add_space(14.0);
        // Canciones demo.
        titulo(ui, "CANCIONES DEMO", "Ábrela para escuchar y explorar.");
        ui.horizontal(|ui| {
            let (r, resp) = ui.allocate_exact_size(vec2(230.0, 52.0), egui::Sense::click());
            ui.painter().rect(r, rr(8.0), if resp.hovered() { BORDER() } else { ELEVATED() }, egui::Stroke::new(1.0, BORDER()), egui::StrokeKind::Inside);
            ui.painter().circle_filled(r.left_center() + vec2(26.0, 0.0), 14.0, VERDE_PLAY);
            widgets::draw_play(ui.painter(), egui::Rect::from_center_size(r.left_center() + vec2(27.0, 0.0), vec2(10.0, 12.0)), BG());
            ui.painter().text(r.left_center() + vec2(50.0, -8.0), egui::Align2::LEFT_CENTER, "Demo", egui::FontId::proportional(14.0), TEXT());
            ui.painter().text(r.left_center() + vec2(50.0, 10.0), egui::Align2::LEFT_CENTER, tr("Batería, bajo y pad"), egui::FontId::proportional(11.0), TEXT_DIM());
            if resp.clicked() {
                *action = Some(Box::new(|app: &mut Self| {
                    let dir = projects_dir().join("Demo");
                    let r = Project::open_or_demo(&dir).and_then(|p| app.reload(dir, p));
                    app.report("Demo abierta", r);
                }));
            }
        });
        ui.add_space(10.0);
        // Proyecto nuevo (plantillas) y recientes en dos columnas.
        let col = (ancho - 20.0) * 0.6;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(col);
                titulo(ui, "EMPEZAR UN PROYECTO NUEVO", "");
                let tarjeta = vec2((col - 20.0) / 3.0, 92.0);
                for fila in TEMPLATES.iter().enumerate().collect::<Vec<_>>().chunks(3) {
                    ui.horizontal(|ui| {
                        for &(k, (nombre, desc, _)) in fila {
                            let on = w.template == k;
                            let (r, resp) = ui.allocate_exact_size(tarjeta, egui::Sense::click());
                            ui.painter().rect(r, rr(10.0), if on { ACCENT().gamma_multiply(0.18) } else if resp.hovered() { BORDER() } else { ELEVATED() }, egui::Stroke::new(if on { 2.0 } else { 1.0 }, if on { ACCENT() } else { BORDER() }), egui::StrokeKind::Inside);
                            let icono = egui::Rect::from_min_size(r.left_top() + vec2(14.0, 14.0), vec2(30.0, 22.0));
                            ui.painter().rect_filled(icono, rr(4.0), ACCENT().gamma_multiply(if on { 1.0 } else { 0.5 }));
                            ui.painter().text(r.left_bottom() + vec2(14.0, -32.0), egui::Align2::LEFT_CENTER, tr(nombre), egui::FontId::proportional(13.5), TEXT());
                            ui.painter().with_clip_rect(r.shrink(4.0)).text(r.left_bottom() + vec2(14.0, -14.0), egui::Align2::LEFT_CENTER, tr(desc), egui::FontId::proportional(10.5), TEXT_DIM());
                            if resp.clicked() {
                                w.template = k;
                            }
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let (mut ni, mut na, mut bpm) = (w.inst_tracks as f32, w.audio_tracks as f32, w.bpm);
                    contador(ui, "PISTAS DE INSTRUMENTO", &mut ni, 0.0, 16.0);
                    contador(ui, "PISTAS DE AUDIO", &mut na, 0.0, 16.0);
                    contador(ui, "TEMPO (BPM)", &mut bpm, 20.0, 300.0);
                    (w.inst_tracks, w.audio_tracks, w.bpm) = (ni as u32, na as u32, bpm);
                    ui.vertical(|ui| {
                        ui.add_space(16.0);
                        let b = egui::Button::new(RichText::new(format!("+   {}", tr("Crear proyecto"))).size(15.0).strong().color(BG())).fill(ACCENT()).min_size(vec2((ui.available_width()).max(160.0), 38.0)).corner_radius(rr(8.0));
                        if ui.add(b).clicked() {
                            let (ni, na, plantilla) = (w.inst_tracks, w.audio_tracks, w.template);
                            let mut nueva = Welcome { name: tr("Mi canción").into(), author: w.author.clone(), bpm: w.bpm, template: plantilla, ..Default::default() };
                            nueva.folder = w.folder.clone();
                            *action = Some(Box::new(move |app: &mut Self| {
                                let r = app.create_song(&nueva);
                                if r.is_ok() {
                                    // Pistas del inicio rápido además de las de la plantilla.
                                    for k in 0..ni {
                                        app.add_track(format!("{} {}", tr("Instrumento"), k + 1), TrackKind::Midi);
                                    }
                                    for k in 0..na {
                                        app.add_track(format!("Audio {}", k + 1), TrackKind::AudioStereo);
                                    }
                                }
                                app.report(tr("Proyecto creado"), r);
                            }));
                        }
                    });
                });
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.set_width(ancho - col - 20.0);
                let recientes: Vec<PathBuf> = self.config.recent.iter().filter(|d| d.join(project::FILE).exists()).take(6).cloned().collect();
                ui.horizontal(|ui| {
                    titulo(ui, "PROYECTOS RECIENTES", "");
                    ui.label(RichText::new(recientes.len().to_string()).size(10.5).color(ACCENT()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(RichText::new(tr("Ver todos…")).size(12.0)).frame(false)).clicked() {
                            w.tab = 2;
                        }
                    });
                });
                egui::Frame::new().fill(ELEVATED()).corner_radius(rr(10.0)).stroke(egui::Stroke::new(1.0, BORDER())).inner_margin(6.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if recientes.is_empty() {
                        ui.label(RichText::new(tr("Todavía no hay proyectos recientes.")).color(TEXT_DIM()));
                    }
                    for dir in recientes {
                        let nombre = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, rr(6.0), BORDER());
                        }
                        let insignia = egui::Rect::from_min_size(r.left_top() + vec2(8.0, 10.0), vec2(30.0, 30.0));
                        ui.painter().rect_filled(insignia, rr(6.0), ACCENT().gamma_multiply(0.3));
                        let iniciales: String = nombre.chars().take(2).collect();
                        ui.painter().text(insignia.center(), egui::Align2::CENTER_CENTER, iniciales, egui::FontId::proportional(12.0), ACCENT());
                        ui.painter().text(r.left_top() + vec2(48.0, 16.0), egui::Align2::LEFT_CENTER, &nombre, egui::FontId::proportional(13.5), TEXT());
                        ui.painter().with_clip_rect(egui::Rect::from_min_max(r.min, r.max - vec2(70.0, 0.0))).text(r.left_top() + vec2(48.0, 34.0), egui::Align2::LEFT_CENTER, dir.parent().map(|p| p.display().to_string()).unwrap_or_default(), egui::FontId::proportional(10.5), TEXT_DIM());
                        ui.painter().text(r.right_center() - vec2(8.0, 0.0), egui::Align2::RIGHT_CENTER, hace(&dir), egui::FontId::proportional(10.5), TEXT_DIM());
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
        ui.add_space(12.0);
        // Espacios de trabajo e instrumentos.
        titulo(ui, "¿CÓMO TRABAJAS HOY?", "Elige un espacio de trabajo; puedes cambiarlo en Ver.");
        ui.horizontal_wrapped(|ui| {
            for (k, (nombre, desc, ..)) in ESPACIOS.iter().enumerate() {
                if ui.add(egui::Button::new(RichText::new(tr(nombre)).size(13.0)).min_size(vec2(130.0, 32.0)).corner_radius(rr(16.0))).on_hover_text(tr(desc)).clicked() {
                    *action = Some(Box::new(move |app: &mut Self| app.aplicar_espacio(k)));
                }
            }
        });
        ui.add_space(10.0);
        titulo(ui, "AMPLÍA TU ESTUDIO", "Instrumentos reales gratuitos y libres para descargar.");
        // Tarjetas del mismo tamaño, tantas como entren en una fila.
        let n = ((ancho + 10.0) / 230.0).floor().max(1.0) as usize;
        let tarjeta = vec2((ancho - 10.0 * (n as f32 - 1.0)) / n as f32, 86.0);
        ui.horizontal(|ui| {
            for e in instruments::CATALOG.iter().take(n) {
                let instalado = instruments::installed(e.id);
                let (r, resp) = ui.allocate_exact_size(tarjeta, egui::Sense::click());
                // Franja de color arriba (como la portada de un paquete de sonidos).
                ui.painter().rect(r, rr(10.0), if resp.hovered() { BORDER() } else { ELEVATED() }, egui::Stroke::new(1.0, BORDER()), egui::StrokeKind::Inside);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(r.width(), 6.0)), egui::CornerRadius { nw: rr(10.0) as u8, ne: rr(10.0) as u8, sw: 0, se: 0 }, ACCENT().gamma_multiply(0.7));
                let pc = ui.painter().with_clip_rect(r.shrink(6.0));
                pc.text(r.left_top() + vec2(12.0, 24.0), egui::Align2::LEFT_CENTER, e.name, egui::FontId::proportional(13.5), TEXT());
                pc.text(r.left_top() + vec2(12.0, 44.0), egui::Align2::LEFT_CENTER, format!("{} · {} MB · {}", e.family, e.mb, e.license), egui::FontId::proportional(10.5), TEXT_DIM());
                let (texto, color) = if instalado { (tr("Instalado"), VERDE_PLAY) } else { (tr("Descargar en la biblioteca"), ACCENT()) };
                pc.text(r.left_bottom() + vec2(12.0, -14.0), egui::Align2::LEFT_CENTER, texto, egui::FontId::proportional(11.5), color);
                if resp.clicked() && !instalado {
                    *action = Some(Box::new(|app: &mut Self| app.show_lib = true));
                }
            }
        });
    }

    /// Preferencias de interfaz: tema y tamaño.
    fn panel_interfaz(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new(tr("Interfaz")).size(22.0).strong());
        titulo(ui, "TEMA", "");
        ui.horizontal_wrapped(|ui| {
            for (k, (nombre, c)) in TEMAS.iter().enumerate() {
                let on = self.config.tema == k;
                let (r, resp) = ui.allocate_exact_size(vec2(150.0, 70.0), egui::Sense::click());
                ui.painter().rect(r, rr(10.0), c[1], egui::Stroke::new(if on { 2.0 } else { 1.0 }, if on { c[6] } else { c[3] }), egui::StrokeKind::Inside);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.left_top() + vec2(10.0, 10.0), vec2(40.0, 8.0)), 3.0, c[6]);
                ui.painter().rect_filled(egui::Rect::from_min_size(r.left_top() + vec2(10.0, 24.0), vec2(90.0, 6.0)), 3.0, c[3]);
                ui.painter().text(r.left_bottom() + vec2(10.0, -14.0), egui::Align2::LEFT_CENTER, tr(nombre), egui::FontId::proportional(13.0), c[4]);
                if resp.clicked() {
                    self.config.tema = k;
                    TEMA.store(k, Relaxed);
                    theme(ui.ctx());
                    self.config.save();
                }
            }
        });
        ui.add_space(10.0);
        titulo(ui, "TAMAÑO DE LA INTERFAZ", "");
        ui.horizontal(|ui| {
            let actual = ui.ctx().zoom_factor();
            for (texto, z) in [("90 %", 0.9), ("100 %", 1.0), ("115 %", 1.15), ("130 %", 1.3), ("150 %", 1.5)] {
                if ui.add(egui::Button::selectable((actual - z).abs() < 0.01, texto)).clicked() {
                    ui.ctx().set_zoom_factor(z);
                    self.config.ui_scale = z;
                    self.config.save();
                }
            }
        });
    }

    /// Espacios de trabajo con su descripción.
    fn panel_espacios(&mut self, ui: &mut egui::Ui, cerrar: &mut bool) {
        ui.label(RichText::new(tr("Espacios de trabajo")).size(22.0).strong());
        ui.label(RichText::new(tr("Cada forma de trabajar con los paneles que necesita a mano.")).color(TEXT_DIM()));
        for (k, (nombre, desc, ..)) in ESPACIOS.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width().min(640.0), 54.0), egui::Sense::click());
            ui.painter().rect(r, rr(10.0), if resp.hovered() { BORDER() } else { ELEVATED() }, egui::Stroke::new(1.0, BORDER()), egui::StrokeKind::Inside);
            ui.painter().text(r.left_center() + vec2(16.0, -9.0), egui::Align2::LEFT_CENTER, tr(nombre), egui::FontId::proportional(14.5), TEXT());
            ui.painter().text(r.left_center() + vec2(16.0, 10.0), egui::Align2::LEFT_CENTER, tr(desc), egui::FontId::proportional(11.5), TEXT_DIM());
            if resp.clicked() {
                self.aplicar_espacio(k);
                *cerrar = true;
            }
        }
    }
}
