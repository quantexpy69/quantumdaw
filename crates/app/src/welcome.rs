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
const TABS: [&str; 5] = ["Nueva canción", "Datos de la canción", "Abrir", "Demo y recursos", "Acerca de"];

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
}

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
        }
    }
}

fn modified(dir: &Path) -> String {
    let Ok(t) = fs::metadata(dir.join(project::FILE)).and_then(|m| m.modified()) else {
        return String::new();
    };
    match t.elapsed().map_or(0, |e| e.as_secs() / 86_400) {
        0 => "hoy".into(),
        1 => "ayer".into(),
        d => format!("hace {d} días"),
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
        egui::Frame::new().fill(if on { ACCENT.gamma_multiply(0.35) } else { ELEVATED }).stroke(egui::Stroke::new(1.0, if on { ACCENT } else { BORDER })).corner_radius(rr(12.0)).inner_margin(14.0);
    let r = frame
        .show(ui, |ui| {
            ui.set_width(width);
            ui.vertical(add);
        })
        .response
        .interact(egui::Sense::click());
    if r.hovered() && !on {
        ui.painter().rect_stroke(r.rect, rr(12.0), egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.7)), egui::StrokeKind::Inside);
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
        let Some(mut w) = self.welcome.take() else {
            return;
        };
        let (mut close, mut action): (bool, Option<Action>) = (false, None);
        let max_h = (ctx.content_rect().height() - 260.0).max(240.0);
        let frame = egui::Frame::popup(&ctx.global_style()).fill(PANEL).inner_margin(28.0).corner_radius(rr(18.0)).stroke(egui::Stroke::new(1.0, BORDER));
        // Fondo muy oscurecido para aislar la ventana del proyecto que hay detrás.
        let modal = egui::Modal::new(Id::new("welcome")).backdrop_color(Color32::from_black_alpha(225)).frame(frame);
        modal.show(ctx, |ui| {
            ui.set_width(940.0);
            ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
            ui.horizontal_top(|ui| {
                // Columna izquierda: logo, versión y recientes.
                ui.vertical(|ui| {
                    ui.set_width(230.0);
                    ui.vertical_centered(|ui| {
                        ui.add_space(8.0);
                        ui.add(logo(136.0));
                        ui.add_space(6.0);
                        ui.label(RichText::new(tr("QUANTUM DAW")).size(24.0).strong().color(TEXT));
                        ui.label(RichText::new(format!("versión {VERSION}")).size(13.0).color(TEXT_DIM));
                    });
                    ui.add_space(18.0);
                    ui.label(RichText::new(tr("RECIENTES")).size(11.5).strong().color(TEXT_DIM));
                    ui.add_space(-4.0);
                    for dir in self.config.recent.iter().filter(|d| d.join(project::FILE).exists()).take(7).cloned().collect::<Vec<_>>() {
                        let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let parent = dir.parent().and_then(|p| p.file_name()).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                        // Fila completa: resalta al pasar el ratón y se separa con una línea sutil.
                        let (r, resp) = ui.allocate_exact_size(vec2(230.0, 44.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, rr(8.0), ELEVATED);
                            ui.painter().rect_filled(egui::Rect::from_min_size(r.min + vec2(0.0, 8.0), vec2(3.0, r.height() - 16.0)), 1.5, ACCENT);
                        }
                        ui.painter().text(r.left_top() + vec2(12.0, 7.0), egui::Align2::LEFT_TOP, &name, egui::FontId::proportional(14.5), if resp.hovered() { TEXT } else { TEXT.gamma_multiply(0.9) });
                        ui.painter().text(r.left_bottom() + vec2(12.0, -7.0), egui::Align2::LEFT_BOTTOM, format!("{parent} · {}", modified(&dir)), egui::FontId::proportional(11.0), TEXT_DIM);
                        ui.painter().hline(r.x_range(), r.bottom() + 0.5, egui::Stroke::new(1.0, BORDER.gamma_multiply(0.6)));
                        if resp.on_hover_text(dir.display().to_string()).clicked() {
                            action = Some(Box::new(move |app: &mut Self| {
                                let r = app.open_dir(dir);
                                app.report("Proyecto abierto", r);
                            }));
                        }
                    }
                });
                ui.add_space(28.0);
                // Columna derecha: pestañas (con desplazamiento si la ventana es baja).
                ui.vertical(|ui| {
                    ui.set_width(640.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (k, label) in TABS.iter().enumerate() {
                            let b = egui::Button::selectable(w.tab == k as u8, RichText::new(tr(label)).size(14.5).strong()).min_size(vec2(0.0, 32.0));
                            if ui.add(b).clicked() {
                                w.tab = k as u8;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| match w.tab {
                        0 => {
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
                                    ui.add(egui::Label::new(RichText::new(w.folder.display().to_string()).color(TEXT_DIM)).truncate()).on_hover_text(w.folder.display().to_string());
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
                            ui.label(RichText::new(tr("PLANTILLA")).size(11.5).strong().color(TEXT_DIM));
                            // Tarjetas de plantilla del mismo ancho y alto, tres por fila.
                            let size = vec2(200.0, 74.0);
                            for row in TEMPLATES.iter().enumerate().collect::<Vec<_>>().chunks(3) {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    for &(k, (name, desc, _)) in row {
                                        let on = w.template == k;
                                        let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
                                        let fill = if on { ACCENT.gamma_multiply(0.35) } else if r.hovered() { BORDER } else { ELEVATED };
                                        ui.painter().rect(rect, rr(12.0), fill, egui::Stroke::new(1.0, if on || r.hovered() { ACCENT } else { BORDER }), egui::StrokeKind::Inside);
                                        let inner = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 12.0))).layout(egui::Layout::top_down(egui::Align::Min)));
                                        inner.spacing_mut().item_spacing.y = 3.0;
                                        inner.label(RichText::new(tr(name)).size(14.5).strong());
                                        inner.add(egui::Label::new(RichText::new(tr(desc)).size(12.0).color(TEXT_DIM)).wrap());
                                        if r.clicked() {
                                            w.template = k;
                                        }
                                    }
                                });
                            }
                        }
                        1 => {
                            ui.label(RichText::new(format!("Proyecto actual: {}", self.dir.display())).color(TEXT_DIM));
                            ui.add_space(4.0);
                            meta_form(ui, &mut self.meta, "song-meta");
                            ui.add_space(10.0);
                            ui.vertical_centered(|ui| {
                                if ui.add(egui::Button::new(RichText::new(tr("Guardar datos")).size(16.0).strong()).fill(ACCENT).min_size(vec2(220.0, 40.0)).corner_radius(rr(20.0))).clicked() {
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
                                        ui.label(RichText::new(format!("{by}{} {} · {} {}", tr("creado"), c.created_at, songs.len(), tr("canciones"))).size(12.0).color(TEXT_DIM));
                                    }
                                });
                                for (dir, p) in songs {
                                    let name = if p.meta.title.is_empty() { dir.file_name().unwrap_or_default().to_string_lossy().to_string() } else { p.meta.title.clone() };
                                    let r = card(ui, false, 600.0, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&name).size(15.0).strong());
                                            if !p.meta.artist.is_empty() {
                                                ui.label(RichText::new(&p.meta.artist).color(TEXT_DIM));
                                            }
                                        });
                                        ui.label(RichText::new(song_info(&p)).size(11.5).color(TEXT_DIM));
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
                            ui.label(RichText::new(tr("Proyecto de demostración con batería, bajo y pad para probar el mixer y los efectos.")).size(14.0).color(TEXT_DIM));
                            if ui.button(RichText::new(tr("Abrir la demo")).size(14.5)).clicked() {
                                action = Some(Box::new(|app: &mut Self| {
                                    let dir = projects_dir().join("Demo");
                                    let r = Project::open_or_demo(&dir).and_then(|p| app.reload(dir, p));
                                    app.report("Demo abierta", r);
                                }));
                            }
                            ui.add_space(8.0);
                            ui.label(RichText::new(tr("Instrumentos reales gratuitos: en la Biblioteca (Y) → Instrumentos, o desde el piano roll.")).color(TEXT_DIM));
                            ui.label(RichText::new(tr("Atajos: Herramientas → Atajos de teclado y ratón.")).color(TEXT_DIM));
                        }
                        _ => {
                            ui.label(RichText::new(format!("Quantum DAW {VERSION}")).size(20.0).strong());
                            ui.label(RichText::new(tr("Estación de audio digital escrita en Rust.")).size(14.0));
                            ui.label(RichText::new(tr("Desarrollo: Ivan Cheaib — QUANTUM DAW")).size(14.0).strong().color(ACCENT));
                            ui.add_space(8.0);
                            for site in ["www.quantumdaw.com", "www.quantex.com.py"] {
                                ui.hyperlink_to(RichText::new(site).size(15.0).color(ACCENT), format!("https://{site}"));
                            }
                        }
                    });
                });
            });
            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                let mut show = !self.config.hide_welcome;
                if ui.checkbox(&mut show, tr("Mostrar al iniciar")).changed() {
                    self.config.hide_welcome = !show;
                    self.config.save();
                }
                // «Crear canción» (en su pestaña) y «Continuar» centrados juntos en la ventana.
                let size = vec2(240.0, 42.0);
                let buttons = if w.tab == 0 { size.x * 2.0 + 12.0 } else { size.x };
                let left = ui.min_rect().left();
                ui.add_space(((940.0 - buttons) / 2.0 - (ui.cursor().min.x - left)).max(0.0));
                if w.tab == 0 && ui.add(egui::Button::new(RichText::new(tr("Crear canción")).size(16.0).strong()).fill(ACCENT).min_size(size).corner_radius(rr(21.0))).clicked() {
                    let r = self.create_song(&w);
                    self.report(format!("Canción «{}» creada", w.name), r);
                    close = true;
                }
                close |= ui.add(egui::Button::new(RichText::new(tr("Continuar con el proyecto")).size(15.0)).min_size(size).corner_radius(rr(21.0))).clicked();
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
