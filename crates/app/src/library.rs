//! Biblioteca: secciones plegables y reordenables (archivos con árbol de discos y previsualización,
//! efectos, instrumentos y plugins), con arrastrar y soltar hacia el proyecto. El orden y lo que
//! está plegado se recuerdan entre sesiones.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Id, Painter, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, vec2};
use std::sync::mpsc;

/// Elemento que se arrastra desde la biblioteca hacia pistas o canales.
pub enum LibItem {
    Fx(FxKind),
    File(PathBuf),
    Instrument(&'static str),
}

/// Sección de la biblioteca que se arrastra para reordenar.
struct LibSection(usize);

const SECTIONS: [(&str, &str); 4] = [("files", "Archivos"), ("fx", "Efectos"), ("inst", "Instrumentos"), ("plugins", "Mis plugins")];
/// Índice de la sección de plugins (para abrirla desde «Agregar mis efectos»).
pub const PLUGINS_TAB: usize = 3;
const FX_CATEGORIES: [(&str, &[FxKind]); 5] = [
    ("Dinámica", &[FxKind::Compressor]),
    ("EQ y filtros", &[FxKind::Eq]),
    ("Tiempo y espacio", &[FxKind::Delay, FxKind::Reverb]),
    ("Color y saturación", &[FxKind::Drive]),
    ("Afinación", &[FxKind::Tune]),
];
const FS_TYPES: [&str; 9] = ["ext4", "btrfs", "xfs", "vfat", "exfat", "ntfs", "ntfs3", "fuseblk", "f2fs"];

fn describe(k: FxKind) -> &'static str {
    match k {
        FxKind::Eq => "Graves, medios y agudos",
        FxKind::Compressor => "Controla la dinámica",
        FxKind::Delay => "Ecos con feedback",
        FxKind::Reverb => "Espacio y ambiente",
        FxKind::Drive => "Calidez y distorsión",
        FxKind::Tune => "Afinación automática o manual de voces",
    }
}

/// Flecha de despliegue (▶ cerrada, ▼ abierta), dibujada para que se vea siempre nítida.
fn arrow(p: &Painter, c: Pos2, open: bool, color: Color32) {
    let s = 5.0;
    let pts = if open { vec![c + vec2(-s, -s * 0.5), c + vec2(s, -s * 0.5), c + vec2(0.0, s * 0.7)] } else { vec![c + vec2(-s * 0.5, -s), c + vec2(-s * 0.5, s), c + vec2(s * 0.7, 0.0)] };
    p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// Fila rellena con efecto al pasar el ratón; `paint` dibuja el contenido.
fn row(ui: &mut egui::Ui, h: f32, indent: f32, selected: bool, paint: impl FnOnce(&Painter, Rect)) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
    let rect = Rect::from_min_max(rect.min + vec2(indent, 0.0), rect.max);
    let hov = resp.hovered() || resp.dragged();
    let p = ui.painter_at(rect);
    p.rect_filled(
        rect,
        rr(8.0),
        if selected {
            ACCENT.gamma_multiply(0.35)
        } else if hov {
            BORDER
        } else {
            ELEVATED
        },
    );
    if hov {
        p.rect_stroke(rect, rr(8.0), Stroke::new(1.0, ACCENT.gamma_multiply(0.8)), StrokeKind::Inside);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    paint(&p, rect);
    resp
}

fn title(p: &Painter, pos: Pos2, text: &str, size: f32, color: Color32) {
    p.text(pos, Align2::LEFT_CENTER, tr(text), FontId::proportional(size), color);
}

/// Icono de carpeta o de archivo de audio/video.
fn file_icon(p: &Painter, c: Pos2, dir: bool, color: Color32) {
    if dir {
        p.rect_filled(Rect::from_center_size(c + vec2(0.0, 1.0), vec2(14.0, 10.0)), rr(2.0), color);
        p.rect_filled(Rect::from_min_size(c + vec2(-7.0, -6.0), vec2(6.0, 3.0)), rr(1.0), color);
    } else {
        p.circle_filled(c + vec2(-3.0, 4.0), 3.0, color);
        p.line_segment([c + vec2(0.0, 4.0), c + vec2(0.0, -6.0)], Stroke::new(1.6, color));
        p.line_segment([c + vec2(0.0, -6.0), c + vec2(5.0, -3.0)], Stroke::new(1.6, color));
    }
}

/// Lugares y discos montados (leídos de /proc/mounts).
fn places(project: &Path) -> Vec<(String, PathBuf)> {
    let h = home();
    let mut v: Vec<(String, PathBuf)> =
        [("Inicio", h.clone()), ("Música", h.join("Música")), ("Documentos", h.join("Documentos")), ("Escritorio", h.join("Escritorio")), ("Descargas", h.join("Descargas"))]
            .into_iter()
            .filter(|(_, p)| p.is_dir())
            .map(|(l, p)| (l.to_string(), p))
            .collect();
    v.push(("Proyecto actual".into(), project.to_path_buf()));
    for line in fs::read_to_string("/proc/mounts").unwrap_or_default().lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        let (Some(mnt), Some(fs_type)) = (f.get(1), f.get(2)) else {
            continue;
        };
        let mnt = mnt.replace("\\040", " ");
        let wanted = mnt == "/" || ["/run/media/", "/media/", "/mnt/"].iter().any(|p| mnt.starts_with(p));
        if wanted && FS_TYPES.contains(fs_type) && !v.iter().any(|x| x.1 == Path::new(&mnt)) {
            let label = if mnt == "/" { "Sistema (/)".to_string() } else { format!("Disco · {}", mnt.rsplit('/').next().unwrap_or(&mnt)) };
            v.push((label, PathBuf::from(mnt)));
        }
    }
    v
}

/// Contenido de una carpeta: subcarpetas y archivos de audio/video (sin ocultos), ordenados.
fn list_dir(dir: &Path) -> Vec<PathBuf> {
    let media = |p: &Path| p.extension().is_some_and(|e| AUDIO_EXT.contains(&e.to_string_lossy().to_lowercase().as_str())) || video::is_video(p);
    let mut v: Vec<PathBuf> = fs::read_dir(dir).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    v.retain(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) && (p.is_dir() || media(p)));
    v.sort_by_key(|p| (!p.is_dir(), p.file_name().map(|n| n.to_string_lossy().to_lowercase())));
    v.truncate(500);
    v
}

/// Busca plugins VST3, LV2 y CLAP en las rutas estándar de Linux: (nombre, formato, ruta).
pub fn scan_plugins() -> Vec<(String, &'static str, PathBuf)> {
    let h = home();
    let roots: [(&'static str, &str, Vec<PathBuf>); 3] = [
        ("VST3", "vst3", vec![h.join(".vst3"), "/usr/lib64/vst3".into(), "/usr/lib/vst3".into(), "/usr/local/lib/vst3".into()]),
        ("LV2", "lv2", vec![h.join(".lv2"), "/usr/lib64/lv2".into(), "/usr/lib/lv2".into(), "/usr/local/lib/lv2".into()]),
        ("CLAP", "clap", vec![h.join(".clap"), "/usr/lib64/clap".into(), "/usr/lib/clap".into(), "/usr/local/lib/clap".into()]),
    ];
    let mut found: Vec<_> = roots
        .iter()
        .flat_map(|(format, ext, dirs)| dirs.iter().flat_map(|d| fs::read_dir(d).into_iter().flatten().flatten()).map(move |e| (format, ext, e.path())))
        .filter(|(_, ext, p)| p.extension().is_some_and(|e| e == **ext))
        .map(|(format, _, p)| (p.file_stem().unwrap_or_default().to_string_lossy().to_string(), *format, p))
        .collect();
    found.sort_by_key(|p| p.0.to_lowercase());
    found.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    found
}

impl App {
    fn is_open(&self, key: &str) -> bool {
        self.config.lib_open.iter().any(|k| k == key)
    }

    fn toggle(&mut self, key: &str) {
        match self.config.lib_open.iter().position(|k| k == key) {
            Some(i) => _ = self.config.lib_open.remove(i),
            None => self.config.lib_open.push(key.to_string()),
        }
        self.config.save();
    }

    /// Cabecera plegable (sección o categoría) con flecha y contador.
    fn fold_header(&mut self, ui: &mut egui::Ui, key: &str, text: &str, count: usize, big: bool) -> Response {
        let open = self.is_open(key);
        let (h, indent) = if big { (34.0, 0.0) } else { (28.0, 8.0) };
        let resp = row(ui, h, indent, false, |p, r| {
            if big {
                // Asa para reordenar la sección.
                for k in 0..3 {
                    for c in 0..2 {
                        p.circle_filled(r.left_center() + vec2(8.0 + c as f32 * 4.0, -4.0 + k as f32 * 4.0), 1.2, TEXT_DIM);
                    }
                }
            }
            arrow(p, r.left_center() + vec2(if big { 26.0 } else { 14.0 }, 0.0), open, if big { ACCENT } else { TEXT });
            title(p, r.left_center() + vec2(if big { 38.0 } else { 26.0 }, 0.0), text, if big { 16.0 } else { 14.0 }, TEXT);
            p.text(r.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(12.0), TEXT_DIM);
        });
        if resp.clicked() {
            self.toggle(key);
        }
        resp
    }

    pub fn library(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Biblioteca")).strong().size(17.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").on_hover_text(tr("Ocultar (Y)")).clicked() {
                    self.show_lib = false;
                }
                if ui.button(if self.lib_float { "Acoplar" } else { "Flotar" }).clicked() {
                    self.lib_float = !self.lib_float;
                }
            });
        });
        // Pestañas en el orden guardado; se reordenan arrastrándolas.
        let mut order: Vec<usize> = self.config.lib_order.iter().filter_map(|id| SECTIONS.iter().position(|s| s.0 == id)).collect();
        let missing: Vec<usize> = (0..SECTIONS.len()).filter(|k| !order.contains(k)).collect();
        order.extend(missing);
        let mut moved = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (pos, &k) in order.iter().enumerate() {
                let on = self.lib_tab == k;
                let r = ui.add(egui::Button::selectable(on, RichText::new(tr(SECTIONS[k].1)).size(14.0).strong()).min_size(vec2(0.0, 30.0)).sense(Sense::click_and_drag()));
                r.dnd_set_drag_payload(LibSection(pos));
                if r.dnd_hover_payload::<LibSection>().is_some() {
                    ui.painter().vline(r.rect.left() - 2.0, r.rect.y_range(), Stroke::new(3.0, ACCENT));
                }
                if let Some(from) = r.dnd_release_payload::<LibSection>() {
                    moved = Some((from.0, pos));
                }
                if r.clicked() {
                    self.lib_tab = k;
                }
                r.on_hover_text(tr("Clic: abrir · arrastra para cambiar el orden de las pestañas"));
            }
        });
        if let Some((from, to)) = moved.filter(|(a, b)| a != b) {
            let k = order.remove(from);
            order.insert(to, k);
            self.config.lib_order = order.iter().map(|&k| SECTIONS[k].0.to_string()).collect();
            self.config.save();
        }
        ui.separator();
        if self.lib_tab == 0 {
            self.preview_bar(ui);
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            match SECTIONS[self.lib_tab.min(SECTIONS.len() - 1)].0 {
                "files" => self.files_section(ui),
                "fx" => self.fx_section(ui),
                "inst" => self.instruments_section(ui),
                _ => self.plugins_section(ui),
            }
        });
    }

    fn files_section(&mut self, ui: &mut egui::Ui) {
        if self.places.is_empty() {
            self.places = places(&self.dir);
        }
        ui.horizontal(|ui| {
            if ui.button(tr("Actualizar discos")).clicked() {
                (self.places, self.tree_cache) = (places(&self.dir), HashMap::new());
            }
            ui.checkbox(&mut self.config.preview_manual, tr("Solo previsualizar con ▶")).on_hover_text(tr("Si está desactivado, al hacer clic en un audio se escucha al momento"));
        });
        for (label, path) in self.places.clone() {
            self.tree_node(ui, &label, &path, 0);
        }
        ui.label(RichText::new(tr("Clic: escuchar · doble clic: importar · arrastrar: a una pista o al espacio vacío")).size(11.0).color(TEXT_DIM));
    }

    /// Nodo del árbol de carpetas: se lee del disco solo al desplegarlo.
    fn tree_node(&mut self, ui: &mut egui::Ui, label: &str, path: &Path, depth: usize) {
        let indent = depth as f32 * 14.0;
        let is_dir = path.is_dir();
        let open = self.tree_open.contains(path);
        let selected = self.preview_file.as_deref() == Some(path);
        let resp = row(ui, 28.0, indent, selected, |p, r| {
            if is_dir {
                arrow(p, r.left_center() + vec2(12.0, 0.0), open, TEXT_DIM);
            }
            file_icon(p, r.left_center() + vec2(30.0, 0.0), is_dir, if is_dir { METER[1] } else { ACCENT });
            title(p, r.left_center() + vec2(44.0, 0.0), label, 14.0, TEXT);
        });
        if !is_dir {
            resp.dnd_set_drag_payload(LibItem::File(path.to_path_buf()));
            resp.context_menu(|ui| {
                widgets::menu_style(ui);
                if !video::is_video(path) && ui.button(tr("Escuchar")).clicked() {
                    self.preview_load(path.to_path_buf());
                }
                if ui.button(tr("Importar en una pista nueva (en el cursor)")).clicked() {
                    let at = self.pos();
                    if video::is_video(path) {
                        self.import_video(path.to_path_buf(), at);
                    } else {
                        let r = self.import_one(path, at, None);
                        self.report(format!("Importado: {}", path.display()), r);
                    }
                }
                if let Some(i) = self.s.tracks.iter().position(|t| t.selected && !t.midi())
                    && !video::is_video(path)
                    && ui.button(tr("Importar en la pista seleccionada")).clicked()
                {
                    let at = self.pos();
                    let r = self.import_one(path, at, Some(i));
                    self.report(format!("Importado: {}", path.display()), r);
                }
            });
        }
        if resp.double_clicked() && !is_dir {
            let at = self.pos();
            if video::is_video(path) {
                self.import_video(path.to_path_buf(), at);
            } else {
                self.import(vec![path.to_path_buf()]);
            }
        } else if resp.clicked() {
            if is_dir {
                if !self.tree_open.remove(path) {
                    self.tree_open.insert(path.to_path_buf());
                }
            } else if !video::is_video(path) {
                self.preview_load(path.to_path_buf());
            }
        }
        if is_dir && open {
            let children = self.tree_cache.entry(path.to_path_buf()).or_insert_with(|| list_dir(path)).clone();
            if children.is_empty() {
                ui.label(RichText::new(tr("   (vacía)")).size(11.0).color(TEXT_DIM));
            }
            for c in children {
                let name = c.file_name().unwrap_or_default().to_string_lossy().to_string();
                self.tree_node(ui, &name, &c, depth + 1);
            }
        }
    }

    fn preview_load(&mut self, path: PathBuf) {
        let (tx, rx) = mpsc::channel();
        let (p, rate) = (path.clone(), self.engine.sample_rate);
        std::thread::spawn(move || _ = tx.send(engine::decode(&p, rate).map_err(|e| e.to_string())));
        (self.preview_file, self.preview_rx, self.preview_len) = (Some(path), Some(rx), 0);
        self.engine.preview.store(None);
    }

    /// Reproductor de previsualización: nombre, ▶/■, loop, volumen y progreso.
    fn preview_bar(&mut self, ui: &mut egui::Ui) {
        if let Some(rx) = &self.preview_rx
            && let Ok(r) = rx.try_recv()
        {
            self.preview_rx = None;
            match r {
                Ok(frames) => {
                    self.preview_len = frames.len();
                    self.preview_data = Some(Arc::new(frames));
                    if !self.config.preview_manual {
                        self.preview_play();
                    }
                }
                Err(e) => self.status = format!("No se pudo leer el archivo: {e}"),
            }
        }
        let Some(file) = self.preview_file.clone() else {
            return;
        };
        egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BORDER)).corner_radius(rr(10.0)).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(file.file_name().unwrap_or_default().to_string_lossy()).strong().size(14.0));
            ui.horizontal(|ui| {
                let playing = self.engine.preview.load().is_some() && (self.engine.preview_loop.load(Relaxed) || (self.engine.preview_pos.load(Relaxed) as usize) < self.preview_len);
                let icon = |ui: &mut egui::Ui, play: bool| {
                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 28.0), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, rr(14.0), if resp.hovered() { BORDER } else { ELEVATED });
                    let c = r.center();
                    if play {
                        p.add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, -7.0), c + vec2(-4.0, 7.0), c + vec2(7.0, 0.0)], METER[0], Stroke::NONE));
                    } else {
                        p.rect_filled(Rect::from_center_size(c, vec2(11.0, 11.0)), rr(2.0), TEXT);
                    }
                    resp
                };
                if icon(ui, !playing).on_hover_text(if playing { "Detener" } else { "Reproducir" }).clicked() {
                    if playing {
                        self.engine.preview.store(None);
                    } else {
                        self.preview_play();
                    }
                }
                let mut looped = self.engine.preview_loop.load(Relaxed);
                if ui.toggle_value(&mut looped, tr("Loop")).changed() {
                    self.engine.preview_loop.store(looped, Relaxed);
                }
                let mut g = widgets::db(self.engine.preview_gain.get());
                if ui.add(egui::Slider::new(&mut g, -40.0..=6.0).show_value(false)).on_hover_text(tr("Volumen de previsualización")).changed() {
                    self.engine.preview_gain.set(10f32.powf(g / 20.0));
                }
            });
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
            let frac = if self.preview_len > 0 { (self.engine.preview_pos.load(Relaxed) as f32 / self.preview_len as f32).min(1.0) } else { 0.0 };
            ui.painter().rect_filled(r, rr(3.0), BG);
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * frac, r.height())), rr(3.0), ACCENT);
            if self.preview_rx.is_some() {
                ui.label(RichText::new(tr("Cargando…")).size(11.0).color(TEXT_DIM));
            }
        });
        if self.engine.preview.load().is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(50));
        }
    }

    fn preview_play(&mut self) {
        if let Some(d) = &self.preview_data {
            self.engine.preview_pos.store(0, Relaxed);
            self.engine.preview.store(Some(d.clone()));
        }
    }

    fn fx_section(&mut self, ui: &mut egui::Ui) {
        for (cat, kinds) in FX_CATEGORIES {
            let key = format!("fx:{cat}");
            self.fold_header(ui, &key, cat, kinds.len(), false);
            if !self.is_open(&key) {
                continue;
            }
            for &k in kinds {
                let r = row(ui, 44.0, 20.0, false, |p, r| {
                    p.rect_filled(Rect::from_min_size(r.left_top() + vec2(2.0, 8.0), vec2(4.0, r.height() - 16.0)), 2.0, plugins::kind_color(Some(k)));
                    title(p, r.left_center() + vec2(14.0, -8.0), k.name(), 14.5, TEXT);
                    title(p, r.left_center() + vec2(14.0, 10.0), describe(k), 12.0, TEXT_DIM);
                });
                r.dnd_set_drag_payload(LibItem::Fx(k));
                r.context_menu(|ui| {
                    widgets::menu_style(ui);
                    let targets: Vec<usize> = (0..self.s.tracks.len()).filter(|&i| self.s.tracks[i].selected && !self.s.tracks[i].midi()).collect();
                    if ui.add_enabled(!targets.is_empty(), egui::Button::new(tr("Añadir a las pistas seleccionadas"))).clicked() {
                        targets.into_iter().for_each(|i| self.add_fx(i, k));
                    }
                });
                let target = self.s.tracks.iter().position(|t| t.selected && !t.midi());
                if r.double_clicked()
                    && let Some(i) = target
                {
                    self.add_fx(i, k);
                }
                let sr = self.sr() as f32;
                r.on_hover_ui(|ui| {
                    plugins::preview(ui, k, sr);
                    ui.label(egui::RichText::new(tr("Arrastra a una pista o canal · doble clic: añadir a la pista seleccionada")).size(11.0).color(TEXT_DIM));
                });
            }
        }
    }

    fn instruments_section(&mut self, ui: &mut egui::Ui) {
        let target = self.s.tracks.iter().position(|t| t.selected && t.midi());
        let mut assign = None;
        let mut family = "";
        for e in instruments::CATALOG.iter() {
            if e.family != family {
                family = e.family;
                let n = instruments::CATALOG.iter().filter(|x| x.family == family).count();
                self.fold_header(ui, &format!("inst:{family}"), family, n, false);
            }
            if !self.is_open(&format!("inst:{family}")) {
                continue;
            }
            let st = self.downloads.get(e.id).and_then(|s| s.lock().ok().map(|s| s.clone())).unwrap_or_default();
            let installed = instruments::installed(e.id);
            let (right, color) = match (installed, st.as_str()) {
                (true, _) => ("Instalado".to_string(), METER[0]),
                (false, "") | (false, "Listo") => (format!("Descargar · {} MB", e.mb), ACCENT),
                (false, s) => (s.to_string(), METER[1]),
            };
            let r = row(ui, 46.0, 20.0, target.is_some_and(|i| self.s.tracks[i].instrument == e.id), |p, r| {
                title(p, r.left_center() + vec2(12.0, -9.0), e.name, 14.5, TEXT);
                title(p, r.left_center() + vec2(12.0, 10.0), &format!("{} · {}", e.credit, e.license), 11.5, TEXT_DIM);
                p.text(r.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, &right, FontId::proportional(12.0), color);
            });
            r.dnd_set_drag_payload(LibItem::Instrument(e.id));
            r.context_menu(|ui| {
                widgets::menu_style(ui);
                if !installed && ui.button(format!("Descargar ({} MB)", e.mb)).clicked() {
                    let s = Arc::new(std::sync::Mutex::new(String::new()));
                    instruments::install(e, s.clone());
                    self.downloads.insert(e.id, s);
                }
                if ui.add_enabled(target.is_some(), egui::Button::new(tr("Usar en la pista MIDI seleccionada"))).clicked() {
                    assign = target.map(|i| (i, e.id));
                }
                if ui.button(tr("Nueva pista MIDI con este instrumento")).clicked() {
                    self.add_track(e.name.to_string(), TrackKind::Midi);
                    assign = Some((self.s.tracks.len() - 1, e.id));
                }
                if installed {
                    ui.separator();
                    if ui.button(RichText::new(tr("Desinstalar…")).color(METER[2])).clicked() {
                        self.uninstall = Some((e.name.to_string(), instruments::dir(e.id), Some(e.id)));
                    }
                }
                ui.label(RichText::new(format!("{} · {} {}", e.credit, tr("licencia"), e.license)).size(11.0).color(TEXT_DIM));
            });
            if r.clicked() {
                match (installed, target) {
                    (_, Some(i)) => assign = Some((i, e.id)),
                    (false, None) => {
                        let s = Arc::new(std::sync::Mutex::new(String::new()));
                        instruments::install(e, s.clone());
                        self.downloads.insert(e.id, s);
                    }
                    (true, None) => self.status = "Selecciona una pista MIDI (o arrastra el instrumento sobre ella)".into(),
                }
            }
            r.on_hover_text(tr("Clic: usar en la pista MIDI seleccionada (o descargar) · arrastrar: sobre una pista MIDI"));
        }
        if let Some((i, id)) = assign {
            self.set_instrument(i, id);
        }
    }

    fn plugins_section(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button(tr("Volver a buscar")).clicked() {
                self.plugins = scan_plugins();
            }
            ui.label(RichText::new(tr("Cargarlos en las pistas llega en la próxima versión.")).size(11.0).color(TEXT_DIM));
        });
        if self.plugins.is_empty() {
            ui.label(RichText::new(tr("No se encontraron plugins VST3/LV2/CLAP. Instala por ejemplo LSP o x42 y pulsa «Volver a buscar».")).color(TEXT_DIM));
        }
        for (name, format, path) in self.plugins.clone() {
            if self.config.hidden_plugins.contains(&path) {
                continue;
            }
            let r = row(ui, 44.0, 0.0, false, |p, r| {
                p.text(r.left_center() + vec2(12.0, -8.0), Align2::LEFT_CENTER, format, FontId::proportional(11.0), ACCENT);
                title(p, r.left_center() + vec2(52.0, -8.0), &name, 14.5, TEXT);
                title(p, r.left_center() + vec2(12.0, 10.0), &path.display().to_string(), 11.0, TEXT_DIM);
            });
            r.context_menu(|ui| {
                widgets::menu_style(ui);
                if path.starts_with(home()) {
                    if ui.button(RichText::new(tr("Desinstalar (mover a la papelera)…")).color(METER[2])).clicked() {
                        self.uninstall = Some((name.clone(), path.clone(), None));
                    }
                } else {
                    if ui.button(tr("Ocultar de la lista")).clicked() {
                        self.config.hidden_plugins.push(path.clone());
                        self.config.save();
                    }
                    ui.label(RichText::new(tr("Instalado en el sistema: se desinstala con el gestor de paquetes (dnf).")).size(11.0).color(TEXT_DIM));
                }
            });
            r.on_hover_text(format!("{} · {}", path.display(), tr("clic derecho: desinstalar u ocultar")));
        }
        if !self.config.hidden_plugins.is_empty() && ui.button(format!("{} ({})", tr("Mostrar plugins ocultos"), self.config.hidden_plugins.len())).clicked() {
            self.config.hidden_plugins.clear();
            self.config.save();
        }
    }

    /// Confirma y ejecuta la desinstalación de un instrumento o plugin de terceros (a la papelera si se puede).
    pub fn uninstall_dialog(&mut self, ctx: &egui::Context) {
        let Some((name, path, inst)) = self.uninstall.clone() else { return };
        let mut close = false;
        let resp = egui::Modal::new(Id::new("uninstall")).show(ctx, |ui| {
            widgets::menu_style(ui);
            ui.set_width(440.0);
            ui.label(RichText::new(format!("{} «{name}»", tr("Desinstalar"))).size(18.0).strong());
            ui.label(RichText::new(path.display().to_string()).size(11.5).color(TEXT_DIM));
            ui.label(tr("Se moverá a la papelera. Las pistas que lo usen quedarán con el QUANTUM Synth."));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new(tr("Desinstalar")).color(METER[2]).strong()).clicked() {
                    let trashed = comando("gio").arg("trash").arg(&path).status().is_ok_and(|s| s.success());
                    let ok = trashed || if path.is_dir() { fs::remove_dir_all(&path) } else { fs::remove_file(&path) }.is_ok();
                    if let Some(id) = inst {
                        self.downloads.remove(id);
                        for t in self.s.tracks.iter_mut().filter(|t| t.instrument == id) {
                            (t.instrument, t.sampler) = (String::new(), None);
                        }
                        self.dirty = true;
                    }
                    self.plugins = scan_plugins();
                    self.status = if ok { format!("{} {name}", tr("Desinstalado:")) } else { format!("{} {name}", tr("No se pudo desinstalar")) };
                    close = true;
                }
                close |= ui.button(tr("Cancelar")).clicked();
            });
        });
        if close || resp.should_close() {
            self.uninstall = None;
        }
    }

    /// Suelta un elemento de la biblioteca: efecto o instrumento en la pista, o un archivo en `at`.
    pub fn drop_item(&mut self, item: &LibItem, track: Option<usize>, at: u64) {
        match item {
            LibItem::Fx(k) => match track.filter(|&i| !self.s.tracks[i].midi()) {
                Some(i) => self.add_fx(i, *k),
                None => self.status = "Suelta el efecto sobre una pista de audio".into(),
            },
            LibItem::Instrument(id) => match track.filter(|&i| self.s.tracks[i].midi()) {
                Some(i) => self.set_instrument(i, id),
                None => self.status = "Suelta el instrumento sobre una pista MIDI".into(),
            },
            LibItem::File(path) if video::is_video(path) => self.import_video(path.clone(), at),
            LibItem::File(path) => {
                let r = self.import_one(path, at, track);
                self.report(format!("Importado: {}", path.display()), r);
            }
        }
    }

    pub fn floating_windows(&mut self, ctx: &egui::Context) {
        if self.show_mixer && self.mixer_float {
            let mut open = true;
            egui::Window::new("Mixer").id(Id::new("mixer-win")).open(&mut open).default_size([1000.0, 500.0]).show(ctx, |ui| self.mixer(ui));
            self.show_mixer &= open;
        }
        if self.show_lib && self.lib_float {
            let mut open = true;
            egui::Window::new("Biblioteca").id(Id::new("lib-win")).open(&mut open).default_size([320.0, 600.0]).show(ctx, |ui| self.library(ui));
            self.show_lib &= open;
        }
    }
}
