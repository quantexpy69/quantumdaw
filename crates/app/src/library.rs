//! Biblioteca: secciones plegables y reordenables (archivos con árbol de discos y previsualización,
//! efectos, instrumentos y plugins), con arrastrar y soltar hacia el proyecto. El orden y lo que
//! está plegado se recuerdan entre sesiones.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Id, Painter, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use std::sync::mpsc;

/// Elemento que se arrastra desde la biblioteca hacia pistas o canales.
pub enum LibItem {
    Fx(FxKind),
    File(PathBuf),
    Instrument(&'static str),
    /// QUANTUM Drummer: sobre una pista MIDI le pone el kit y abre el plugin (en vacío crea la pista).
    Drummer,
}

/// Sección de la biblioteca que se arrastra para reordenar.
struct LibSection(usize);

const SECTIONS: [(&str, &str); 4] = [("files", "Muestras"), ("fx", "Efectos"), ("inst", "Instrumentos"), ("plugins", "Plugins")];
/// Índice de la sección de plugins (para abrirla desde «Agregar mis efectos»).
pub const PLUGINS_TAB: usize = 3;
const FX_CATEGORIES: [(&str, &[FxKind]); 6] = [
    ("QUANTUM Pro", &[FxKind::VocalPro, FxKind::MasterPro, FxKind::TapePro, FxKind::SpacePro]),
    ("Dinámica", &[FxKind::Compressor, FxKind::Limiter, FxKind::Gate, FxKind::DeEsser]),
    ("EQ y filtros", &[FxKind::Curves, FxKind::Eq]),
    ("Tiempo y espacio", &[FxKind::Delay, FxKind::Reverb, FxKind::Chorus]),
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
        FxKind::Curves => "EQ con analizador y resonancias",
        FxKind::Limiter => "Techo y volumen para el master",
        FxKind::Gate => "Quita el ruido de fondo",
        FxKind::Chorus => "Ancho y movimiento",
        FxKind::DeEsser => "Controla las eses de la voz",
        FxKind::TapePro => "Cinta y válvula con VU analógicos",
        FxKind::SpacePro => "Reverb 3D, shimmer y eco",
        FxKind::VocalPro => "Toda la cadena de voz en una pantalla",
        FxKind::MasterPro => "Mastering con objetivo de LUFS",
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
            ACCENT().gamma_multiply(0.35)
        } else if hov {
            BORDER()
        } else {
            ELEVATED()
        },
    );
    if hov {
        p.rect_stroke(rect, rr(8.0), Stroke::new(1.0, ACCENT().gamma_multiply(0.8)), StrokeKind::Inside);
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
    pub fn library(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Biblioteca")).strong().size(15.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().button_padding = vec2(6.0, 2.0);
                if ui.small_button("×").on_hover_text(tr("Ocultar (Y)")).clicked() {
                    self.show_lib = false;
                }
                // Flotar dentro del programa, ventana independiente del sistema o acoplar al lateral.
                if ui.small_button(tr(if self.lib_float { "Acoplar" } else { "Flotar" })).clicked() {
                    if self.lib_float {
                        self.acoplar("lib-win");
                    } else {
                        self.lib_float = true;
                    }
                }
                if !self.lib_float && ui.small_button(tr("Separar")).on_hover_text(tr("Abre la biblioteca fuera del programa (por ejemplo en otro monitor)")).clicked() {
                    self.lib_float = true;
                    self.alternar_separada("lib-win");
                }
            });
        });
        // Buscador de toda la biblioteca.
        let buscando = self.buscador(ui);
        // Pestañas con icono y contador, en el orden guardado; se reordenan arrastrándolas.
        let mut order: Vec<usize> = self.config.lib_order.iter().filter_map(|id| SECTIONS.iter().position(|s| s.0 == id)).collect();
        let missing: Vec<usize> = (0..SECTIONS.len()).filter(|k| !order.contains(k)).collect();
        order.extend(missing);
        let cuentas = [0, FX_CATEGORIES.iter().flat_map(|c| c.1.iter()).filter(|k| visible(**k)).count(), instruments::CATALOG.len(), self.plugins.len()];
        let mut moved = None;
        let ancho = ((ui.available_width() - 6.0) / SECTIONS.len() as f32).max(60.0);
        egui::Frame::new().fill(BG()).corner_radius(rr(10.0)).inner_margin(3.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (pos, &k) in order.iter().enumerate() {
                    let on = self.lib_tab == k && !buscando;
                    let (r, resp) = ui.allocate_exact_size(vec2(ancho - 2.0, 46.0), Sense::click_and_drag());
                    let p = ui.painter();
                    p.rect_filled(r, rr(8.0), if on { ELEVATED() } else if resp.hovered() { PANEL() } else { Color32::TRANSPARENT });
                    if on {
                        p.rect_filled(Rect::from_min_size(r.left_bottom() + vec2(12.0, -3.0), vec2(r.width() - 24.0, 2.0)), 1.0, ACCENT());
                    }
                    let color = if on { ACCENT() } else { TEXT_DIM() };
                    icono_seccion(p, r.center_top() + vec2(0.0, 14.0), k, color);
                    // Nombre (abreviado si no entra) y, si sobra espacio, cuántos hay.
                    let nombre = tr(SECTIONS[k].1);
                    let cabe = |t: &str| p.layout_no_wrap(t.to_string(), FontId::proportional(11.0), TEXT()).size().x <= r.width() - 6.0;
                    let con_numero = format!("{nombre} {}", cuentas[k]);
                    let texto = if cuentas[k] > 0 && cabe(&con_numero) {
                        con_numero
                    } else if cabe(nombre) {
                        nombre.to_string()
                    } else {
                        format!("{}.", nombre.chars().take(6).collect::<String>())
                    };
                    p.text(r.center_bottom() - vec2(0.0, 13.0), Align2::CENTER_CENTER, texto, FontId::proportional(11.0), if on { TEXT() } else { TEXT_DIM() });
                    resp.dnd_set_drag_payload(LibSection(pos));
                    if resp.dnd_hover_payload::<LibSection>().is_some() {
                        p.vline(r.left() - 1.0, r.y_range(), Stroke::new(3.0, ACCENT()));
                    }
                    if let Some(from) = resp.dnd_release_payload::<LibSection>() {
                        moved = Some((from.0, pos));
                    }
                    if resp.clicked() {
                        if self.lib_tab != k {
                            self.lib_filtro = 0;
                        }
                        self.lib_tab = k;
                        self.lib_buscar.clear();
                    }
                    resp.on_hover_text(format!("{} · {}", tr(SECTIONS[k].1), tr("Clic: abrir · arrastra para cambiar el orden de las pestañas")));
                }
            });
        });
        if let Some((from, to)) = moved.filter(|(a, b)| a != b) {
            let k = order.remove(from);
            order.insert(to, k);
            self.config.lib_order = order.iter().map(|&k| SECTIONS[k].0.to_string()).collect();
            self.config.save();
        }
        let seccion = SECTIONS[self.lib_tab.min(SECTIONS.len() - 1)].0;
        if seccion == "files" && !buscando {
            self.preview_bar(ui);
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if buscando {
                self.resultados(ui);
                return;
            }
            match seccion {
                "files" => self.files_section(ui),
                "fx" => self.fx_section(ui),
                "inst" => self.instruments_section(ui),
                _ => self.plugins_section(ui),
            }
        });
    }

    /// Campo de búsqueda con lupa y botón para borrar; devuelve si hay algo escrito (en Muestras
    /// filtra los archivos del árbol; en las demás busca en toda la biblioteca).
    fn buscador(&mut self, ui: &mut egui::Ui) -> bool {
        let en_archivos = SECTIONS[self.lib_tab.min(SECTIONS.len() - 1)].0 == "files";
        egui::Frame::new().fill(ELEVATED()).stroke(Stroke::new(1.0, BORDER())).corner_radius(rr(9.0)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(16.0, 18.0), Sense::hover());
                ui.painter().circle_stroke(r.center() - vec2(1.5, 1.5), 5.0, Stroke::new(1.6, TEXT_DIM()));
                ui.painter().line_segment([r.center() + vec2(2.0, 2.0), r.center() + vec2(6.0, 6.0)], Stroke::new(1.8, TEXT_DIM()));
                let pista = if en_archivos { tr("Buscar en las carpetas abiertas…") } else { tr("Buscar en toda la biblioteca…") };
                let ancho = ui.available_width() - if self.lib_buscar.is_empty() { 0.0 } else { 24.0 };
                ui.add(egui::TextEdit::singleline(&mut self.lib_buscar).hint_text(pista).frame(egui::Frame::NONE).desired_width(ancho));
                if !self.lib_buscar.is_empty() && ui.add(egui::Button::new("×").frame(false)).on_hover_text(tr("Borrar la búsqueda")).clicked() {
                    self.lib_buscar.clear();
                }
            });
        });
        !self.lib_buscar.trim().is_empty() && !en_archivos
    }

    /// Resultados de la búsqueda en efectos, instrumentos y plugins.
    fn resultados(&mut self, ui: &mut egui::Ui) {
        let b = self.lib_buscar.trim().to_lowercase();
        let fx: Vec<FxKind> = FX_CATEGORIES.iter().flat_map(|c| c.1.iter().copied()).filter(|k| visible(*k)).filter(|k| tr(k.name()).to_lowercase().contains(&b) || tr(describe(*k)).to_lowercase().contains(&b) || tr(plugins::summary(*k)).to_lowercase().contains(&b)).collect();
        let inst: Vec<&'static instruments::Entry> = instruments::CATALOG.iter().filter(|e| e.name.to_lowercase().contains(&b) || e.family.to_lowercase().contains(&b) || tr(e.family).to_lowercase().contains(&b)).collect();
        let plug: Vec<_> = self.plugins.iter().filter(|p| p.0.to_lowercase().contains(&b) || p.1.to_lowercase().contains(&b)).cloned().collect();
        if fx.is_empty() && inst.is_empty() && plug.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| ui.label(RichText::new(tr("Nada coincide con la búsqueda.")).color(TEXT_DIM())));
            return;
        }
        if !fx.is_empty() {
            grupo(ui, "EFECTOS", fx.len());
            self.fx_grid(ui, &fx);
        }
        if !inst.is_empty() {
            grupo(ui, "INSTRUMENTOS", inst.len());
            self.inst_rows(ui, &inst);
        }
        if !plug.is_empty() {
            grupo(ui, "PLUGINS", plug.len());
            self.plugin_rows(ui, &plug);
        }
    }

    fn es_favorito(&self, clave: &str) -> bool {
        self.config.lib_favoritos.iter().any(|f| f == clave)
    }

    fn alternar_favorito(&mut self, clave: String) {
        match self.config.lib_favoritos.iter().position(|f| *f == clave) {
            Some(i) => _ = self.config.lib_favoritos.remove(i),
            None => self.config.lib_favoritos.push(clave),
        }
        self.config.save();
    }

    /// Fila de chips de filtro; devuelve el elegido.
    fn chips(&mut self, ui: &mut egui::Ui, nombres: &[String]) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (k, n) in nombres.iter().enumerate() {
                let on = self.lib_filtro == k;
                let g = ui.painter().layout_no_wrap(n.clone(), FontId::proportional(11.5), TEXT());
                let (r, resp) = ui.allocate_exact_size(vec2(g.size().x + 18.0, 24.0), Sense::click());
                ui.painter().rect(r, rr(12.0), if on { ACCENT() } else if resp.hovered() { BORDER() } else { ELEVATED() }, Stroke::new(1.0, if on { ACCENT() } else { BORDER() }), StrokeKind::Inside);
                ui.painter().galley_with_override_text_color(r.center() - g.size() / 2.0, g, if on { BG() } else { TEXT() });
                if resp.clicked() {
                    self.lib_filtro = k;
                }
            }
        });
        ui.add_space(2.0);
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
        ui.label(RichText::new(tr("Clic: escuchar · doble clic: importar · arrastrar: a una pista o al espacio vacío")).size(11.0).color(TEXT_DIM()));
    }

    /// Nodo del árbol de carpetas: se lee del disco solo al desplegarlo.
    fn tree_node(&mut self, ui: &mut egui::Ui, label: &str, path: &Path, depth: usize) {
        let indent = depth as f32 * 14.0;
        let is_dir = path.is_dir();
        let open = self.tree_open.contains(path);
        let selected = self.preview_file.as_deref() == Some(path);
        let resp = row(ui, 28.0, indent, selected, |p, r| {
            if is_dir {
                arrow(p, r.left_center() + vec2(12.0, 0.0), open, TEXT_DIM());
            }
            file_icon(p, r.left_center() + vec2(30.0, 0.0), is_dir, if is_dir { METER[1] } else { ACCENT() });
            title(p, r.left_center() + vec2(44.0, 0.0), label, 14.0, TEXT());
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
                if let Some(i) = self.s.tracks.iter().position(|t| t.selected && t.kind != TrackKind::Video)
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
                ui.label(RichText::new(tr("   (vacía)")).size(11.0).color(TEXT_DIM()));
            }
            let buscado = self.lib_buscar.trim().to_lowercase();
            for c in children {
                let name = c.file_name().unwrap_or_default().to_string_lossy().to_string();
                // La búsqueda filtra los archivos de las carpetas abiertas.
                if !buscado.is_empty() && !c.is_dir() && !name.to_lowercase().contains(&buscado) {
                    continue;
                }
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
        egui::Frame::new().fill(PANEL()).stroke(Stroke::new(1.0, BORDER())).corner_radius(rr(10.0)).inner_margin(8.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(file.file_name().unwrap_or_default().to_string_lossy()).strong().size(14.0));
            ui.horizontal(|ui| {
                let playing = self.engine.preview.load().is_some() && (self.engine.preview_loop.load(Relaxed) || (self.engine.preview_pos.load(Relaxed) as usize) < self.preview_len);
                let icon = |ui: &mut egui::Ui, play: bool| {
                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 28.0), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(r, rr(14.0), if resp.hovered() { BORDER() } else { ELEVATED() });
                    let c = r.center();
                    if play {
                        p.add(egui::Shape::convex_polygon(vec![c + vec2(-4.0, -7.0), c + vec2(-4.0, 7.0), c + vec2(7.0, 0.0)], METER[0], Stroke::NONE));
                    } else {
                        p.rect_filled(Rect::from_center_size(c, vec2(11.0, 11.0)), rr(2.0), TEXT());
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
            ui.painter().rect_filled(r, rr(3.0), BG());
            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * frac, r.height())), rr(3.0), ACCENT());
            if self.preview_rx.is_some() {
                ui.label(RichText::new(tr("Cargando…")).size(11.0).color(TEXT_DIM()));
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
        let mut nombres = vec![tr("Todos").to_string()];
        nombres.extend(FX_CATEGORIES.iter().map(|c| tr(c.0).to_string()));
        nombres.push(format!("★ {}", tr("Favoritos")));
        self.chips(ui, &nombres);
        let target = self.s.tracks.iter().position(|t| t.selected && t.kind != TrackKind::Video);
        let destino = target.map_or_else(|| tr("ninguna pista seleccionada").to_string(), |i| self.s.tracks[i].name.clone());
        ui.label(RichText::new(format!("{} {destino} · {}", tr("Doble clic añade a:"), tr("o arrastra a una pista"))).size(11.0).color(TEXT_DIM()));
        let favoritos = nombres.len() - 1;
        for (k, (cat, kinds)) in FX_CATEGORIES.iter().enumerate() {
            if self.lib_filtro != 0 && self.lib_filtro != k + 1 && self.lib_filtro != favoritos {
                continue;
            }
            let lista: Vec<FxKind> = kinds.iter().copied().filter(|x| visible(*x) && (self.lib_filtro != favoritos || self.es_favorito(&format!("fx:{x:?}")))).collect();
            if lista.is_empty() {
                if self.lib_filtro == k + 1 && kinds.iter().any(|x| x.pro().is_some()) {
                    ui.label(RichText::new(tr("Los plugins QUANTUM Pro vienen en la versión oficial de quantumdaw.com.")).size(11.5).color(TEXT_DIM()));
                }
                continue;
            }
            grupo(ui, cat, lista.len());
            self.fx_grid(ui, &lista);
            if kinds.iter().any(|x| x.pro().is_some()) && Self::drummer_disponible() && self.lib_filtro != favoritos {
                self.tarjeta_drummer(ui);
            }
        }
        if self.lib_filtro == favoritos && self.config.lib_favoritos.iter().all(|f| !f.starts_with("fx:")) {
            ui.label(RichText::new(tr("Marca efectos con la estrella para tenerlos aquí.")).size(11.5).color(TEXT_DIM()));
        }
    }

    /// Tarjeta de QUANTUM Drummer Pro (abre su ventana con la pista MIDI elegida).
    fn tarjeta_drummer(&mut self, ui: &mut egui::Ui) {
        let color = Color32::from_rgb(0xFF, 0x6B, 0x3D);
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
        let p = ui.painter();
        p.rect(r, rr(10.0), if resp.hovered() { BORDER() } else { ELEVATED() }, Stroke::new(1.0, if resp.hovered() { color } else { BORDER() }), StrokeKind::Inside);
        let tile = Rect::from_min_size(r.left_top() + vec2(8.0, 9.0), vec2(38.0, 38.0));
        p.rect_filled(tile, rr(8.0), color.gamma_multiply(0.22));
        // Bombo y platillo dibujados.
        p.circle_stroke(tile.center() + vec2(0.0, 4.0), 9.0, Stroke::new(1.8, color));
        p.line_segment([tile.center() + vec2(-12.0, -10.0), tile.center() + vec2(2.0, -12.0)], Stroke::new(1.8, color));
        p.text(pos2(tile.right() + 10.0, r.top() + 19.0), Align2::LEFT_CENTER, "QUANTUM Drummer", FontId::proportional(13.5), TEXT());
        p.text(pos2(tile.right() + 10.0, r.top() + 37.0), Align2::LEFT_CENTER, recortar(p, tr("Ritmos por estilo, variaciones y rellenos"), 10.5, r.right() - tile.right() - 20.0), FontId::proportional(10.5), TEXT_DIM());
        resp.dnd_set_drag_payload(LibItem::Drummer);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Clic: abrir con la pista MIDI elegida · arrastrar: sobre una pista (o al espacio vacío para crear una)")).clicked() {
            let pista = self.s.tracks.iter().position(|t| t.selected && t.midi());
            self.soltar_drummer(pista);
        }
    }

    /// Efectos en tarjetas (tantas columnas como entren).
    fn fx_grid(&mut self, ui: &mut egui::Ui, lista: &[FxKind]) {
        let target = self.s.tracks.iter().position(|t| t.selected && t.kind != TrackKind::Video);
        let cols = ((ui.available_width() + 6.0) / 150.0).floor().max(1.0) as usize;
        let w = (ui.available_width() - 6.0 * (cols as f32 - 1.0)) / cols as f32;
        for fila in lista.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for &k in fila {
                    let (r0, resp) = ui.allocate_exact_size(vec2(w, 62.0), Sense::click_and_drag());
                    let hov = resp.hovered() || resp.dragged();
                    let r = if hov { r0.translate(vec2(0.0, -1.0)) } else { r0 };
                    let p = ui.painter();
                    let color = plugins::kind_color(Some(k));
                    p.rect_filled(r.translate(vec2(0.0, 3.0)), rr(10.0), Color32::from_black_alpha(if hov { 60 } else { 30 }));
                    p.rect(r, rr(10.0), if hov { BORDER() } else { ELEVATED() }, Stroke::new(1.0, if hov { color.gamma_multiply(0.8) } else { BORDER() }), StrokeKind::Inside);
                    let tile = Rect::from_min_size(r.left_top() + vec2(8.0, 9.0), vec2(30.0, 30.0));
                    p.rect_filled(tile, rr(8.0), color.gamma_multiply(0.22));
                    icono_fx(p, tile.center(), k, color);
                    let nombre = tr(k.name()).trim_start_matches("QUANTUM ").to_string();
                    let ancho_txt = r.right() - tile.right() - 30.0;
                    p.text(pos2(tile.right() + 8.0, r.top() + 18.0), Align2::LEFT_CENTER, recortar(p, &nombre, 13.0, ancho_txt), FontId::proportional(13.0), TEXT());
                    let desc = recortar(p, tr(describe(k)), 10.5, r.right() - tile.right() - 14.0);
                    p.text(pos2(tile.right() + 8.0, r.top() + 36.0), Align2::LEFT_CENTER, desc, FontId::proportional(10.5), TEXT_DIM());
                    if let Some((_, _, precio)) = k.pro() {
                        // Insignia PRO y si tiene licencia o el precio.
                        let pro = Rect::from_min_size(pos2(tile.right() + 8.0, r.bottom() - 16.0), vec2(28.0, 12.0));
                        p.rect_filled(pro, rr(3.0), color);
                        p.text(pro.center(), Align2::CENTER_CENTER, "PRO", FontId::proportional(8.5), Color32::BLACK);
                        let estado = if engine::pro::licenciado(k) { tr("Con licencia").to_string() } else { format!("{} · {precio} US$", tr("Demo")) };
                        p.text(pos2(pro.right() + 6.0, pro.center().y), Align2::LEFT_CENTER, estado, FontId::proportional(9.0), TEXT_DIM());
                    } else if k.name().starts_with("QUANTUM") {
                        p.text(pos2(tile.right() + 8.0, r.bottom() - 10.0), Align2::LEFT_CENTER, "QUANTUM", FontId::proportional(8.5), color);
                    }
                    // Estrella de favorito.
                    let clave = format!("fx:{k:?}");
                    let fav = self.es_favorito(&clave);
                    let estrella = Rect::from_center_size(r.right_top() + vec2(-14.0, 14.0), vec2(18.0, 18.0));
                    let re = ui.interact(estrella, Id::new(("fav", &clave)), Sense::click());
                    if fav || hov || re.hovered() {
                        dibujar_estrella(ui.painter(), estrella.center(), 6.0, if fav { METER[1] } else { TEXT_DIM() }, fav);
                    }
                    if re.on_hover_text(tr(if fav { "Quitar de favoritos" } else { "Agregar a favoritos" })).clicked() {
                        self.alternar_favorito(clave);
                    }
                    if hov {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    resp.dnd_set_drag_payload(LibItem::Fx(k));
                    resp.context_menu(|ui| {
                        widgets::menu_style(ui);
                        let targets: Vec<usize> = (0..self.s.tracks.len()).filter(|&i| self.s.tracks[i].selected && self.admite_efectos(i)).collect();
                        if ui.add_enabled(!targets.is_empty(), egui::Button::new(tr("Añadir a las pistas seleccionadas"))).clicked() {
                            targets.into_iter().for_each(|i| self.add_fx(i, k));
                        }
                    });
                    if resp.double_clicked()
                        && let Some(i) = target
                    {
                        self.add_fx(i, k);
                    }
                    let sr = self.sr() as f32;
                    resp.on_hover_ui(|ui| {
                        plugins::preview(ui, k, sr);
                        ui.label(egui::RichText::new(tr("Arrastra a una pista o canal · doble clic: añadir a la pista seleccionada")).size(11.0).color(TEXT_DIM()));
                    });
                }
            });
        }
    }

    fn instruments_section(&mut self, ui: &mut egui::Ui) {
        let mut familias: Vec<&'static str> = vec![];
        for e in instruments::CATALOG.iter() {
            if !familias.contains(&e.family) {
                familias.push(e.family);
            }
        }
        let mut nombres = vec![tr("Todos").to_string()];
        nombres.extend(familias.iter().map(|f| tr(f).to_string()));
        nombres.push(tr("Instalados").to_string());
        nombres.push(format!("★ {}", tr("Favoritos")));
        self.chips(ui, &nombres);
        let (instalados, favoritos) = (nombres.len() - 2, nombres.len() - 1);
        let f = self.lib_filtro;
        let lista: Vec<&'static instruments::Entry> = instruments::CATALOG
            .iter()
            .filter(|e| match f {
                0 => true,
                x if x == instalados => instruments::installed(e.id),
                x if x == favoritos => self.es_favorito(&format!("inst:{}", e.id)),
                x => familias.get(x - 1) == Some(&e.family),
            })
            .collect();
        if lista.is_empty() {
            ui.label(RichText::new(tr(if f == favoritos { "Marca instrumentos con la estrella para tenerlos aquí." } else { "No hay instrumentos en este filtro." })).size(11.5).color(TEXT_DIM()));
        }
        // Kits de batería propios (de las muestras del usuario).
        if f == 0 || familias.get(f.wrapping_sub(1)).is_some_and(|x| x.contains("Bater")) {
            if Self::drummer_disponible() {
                grupo(ui, "BATERISTA", 1);
                self.tarjeta_drummer(ui);
            }
            grupo(ui, "MIS KITS", instruments::kits().len());
            for (id, nombre) in instruments::kits() {
                let r = row(ui, 40.0, 0.0, false, |p, r| {
                    p.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, &nombre, FontId::proportional(13.5), TEXT());
                    p.text(pos2(r.right() - 10.0, r.center().y), Align2::RIGHT_CENTER, tr("Kit propio"), FontId::proportional(10.5), METER[0]);
                });
                if let Some(sid) = instruments::id_estatico(&id) {
                    r.dnd_set_drag_payload(LibItem::Instrument(sid));
                    if r.on_hover_text(tr("Clic: usar en la pista MIDI seleccionada (o en una nueva) y abrir QUANTUM Drummer · arrastrar: sobre una pista MIDI")).clicked() {
                        let i = match self.s.tracks.iter().position(|t| t.selected && t.midi()) {
                            Some(i) => i,
                            None => {
                                self.add_track(nombre.clone(), TrackKind::Midi);
                                self.s.tracks.len() - 1
                            }
                        };
                        self.set_instrument(i, sid);
                        if Self::drummer_disponible() {
                            self.abrir_drummer(Some(i));
                        }
                    }
                }
            }
            if ui.button(tr("+ Crear kit de batería desde una carpeta…")).clicked() {
                self.abrir_kits();
            }
        }
        ui.label(RichText::new(tr("Clic: usar en la pista MIDI seleccionada (o descargar) · arrastrar: sobre una pista MIDI")).size(11.0).color(TEXT_DIM()));
        self.inst_rows(ui, &lista);
    }

    /// Instrumentos en filas con icono de familia, estado y favorito.
    fn inst_rows(&mut self, ui: &mut egui::Ui, lista: &[&'static instruments::Entry]) {
        let target = self.s.tracks.iter().position(|t| t.selected && t.midi());
        let mut assign = None;
        for e in lista {
            let st = self.downloads.get(e.id).and_then(|s| s.lock().ok().map(|s| s.clone())).unwrap_or_default();
            let installed = instruments::installed(e.id);
            let (estado, color) = match (installed, st.as_str()) {
                (true, _) => (tr("Instalado").to_string(), METER[0]),
                (false, "") | (false, "Listo") => (format!("{} MB", e.mb), ACCENT()),
                (false, s) => (s.to_string(), METER[1]),
            };
            let clave = format!("inst:{}", e.id);
            let fav = self.es_favorito(&clave);
            let mut estrella_r = Rect::NOTHING;
            let r = row(ui, 50.0, 0.0, target.is_some_and(|i| self.s.tracks[i].instrument == e.id), |p, r| {
                let c = color_familia(e.family);
                let tile = Rect::from_min_size(r.left_top() + vec2(8.0, 9.0), vec2(32.0, 32.0));
                p.rect_filled(tile, rr(8.0), c.gamma_multiply(0.22));
                p.text(tile.center(), Align2::CENTER_CENTER, e.family.chars().next().unwrap_or('·').to_string(), FontId::proportional(15.0), c);
                let libre = r.right() - tile.right() - 110.0;
                p.text(pos2(tile.right() + 10.0, r.top() + 17.0), Align2::LEFT_CENTER, recortar(p, e.name, 13.5, libre), FontId::proportional(13.5), TEXT());
                p.text(pos2(tile.right() + 10.0, r.top() + 35.0), Align2::LEFT_CENTER, recortar(p, &format!("{} · {}", tr(e.family), e.license), 10.5, libre), FontId::proportional(10.5), TEXT_DIM());
                // Estado como píldora y estrella.
                let g = p.layout_no_wrap(estado.clone(), FontId::proportional(10.5), color);
                let pill = Rect::from_min_size(pos2(r.right() - 36.0 - g.size().x - 14.0, r.center().y - 10.0), vec2(g.size().x + 14.0, 20.0));
                p.rect_filled(pill, rr(10.0), color.gamma_multiply(0.16));
                p.galley(pill.center() - g.size() / 2.0, g, color);
                estrella_r = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(18.0, 18.0));
                dibujar_estrella(p, estrella_r.center(), 6.0, if fav { METER[1] } else { BORDER() }, fav);
            });
            let re = ui.interact(estrella_r, Id::new(("fav", &clave)), Sense::click());
            if re.on_hover_text(tr(if fav { "Quitar de favoritos" } else { "Agregar a favoritos" })).clicked() {
                self.alternar_favorito(clave);
                continue;
            }
            r.dnd_set_drag_payload(LibItem::Instrument(e.id));
            r.context_menu(|ui| {
                widgets::menu_style(ui);
                if !installed && ui.button(format!("{} ({} MB)", tr("Descargar"), e.mb)).clicked() {
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
                ui.label(RichText::new(format!("{} · {} {}", e.credit, tr("licencia"), e.license)).size(11.0).color(TEXT_DIM()));
            });
            if r.clicked() {
                match (installed, target) {
                    (_, Some(i)) => assign = Some((i, e.id)),
                    (false, None) => {
                        let s = Arc::new(std::sync::Mutex::new(String::new()));
                        instruments::install(e, s.clone());
                        self.downloads.insert(e.id, s);
                    }
                    (true, None) => self.status = tr("Selecciona una pista MIDI (o arrastra el instrumento sobre ella)").into(),
                }
            }
            r.on_hover_text(format!("{} · {}", e.credit, tr("Clic: usar en la pista MIDI seleccionada (o descargar) · arrastrar: sobre una pista MIDI")));
        }
        if let Some((i, id)) = assign {
            self.set_instrument(i, id);
        }
    }

    fn plugins_section(&mut self, ui: &mut egui::Ui) {
        let formatos = ["VST3", "LV2", "CLAP"];
        let mut nombres = vec![format!("{} {}", tr("Todos"), self.plugins.len())];
        nombres.extend(formatos.iter().map(|f| format!("{f} {}", self.plugins.iter().filter(|p| p.1 == *f).count())));
        self.chips(ui, &nombres);
        ui.horizontal(|ui| {
            if ui.button(tr("Volver a buscar")).clicked() {
                self.plugins = scan_plugins();
            }
            ui.label(RichText::new(tr("Cargarlos en las pistas llega en la próxima versión.")).size(11.0).color(TEXT_DIM()));
        });
        if self.plugins.is_empty() {
            ui.label(RichText::new(tr("No se encontraron plugins VST3/LV2/CLAP. Instala por ejemplo LSP o x42 y pulsa «Volver a buscar».")).color(TEXT_DIM()));
        }
        let f = self.lib_filtro;
        let lista: Vec<_> = self.plugins.iter().filter(|p| f == 0 || formatos.get(f - 1) == Some(&p.1)).cloned().collect();
        self.plugin_rows(ui, &lista);
        if !self.config.hidden_plugins.is_empty() && ui.button(format!("{} ({})", tr("Mostrar plugins ocultos"), self.config.hidden_plugins.len())).clicked() {
            self.config.hidden_plugins.clear();
            self.config.save();
        }
    }

    /// Plugins de terceros en filas con su formato.
    fn plugin_rows(&mut self, ui: &mut egui::Ui, lista: &[(String, &'static str, PathBuf)]) {
        for (name, format, path) in lista.iter().cloned() {
            if self.config.hidden_plugins.contains(&path) {
                continue;
            }
            let r = row(ui, 44.0, 0.0, false, |p, r| {
                let c = match format {
                    "VST3" => Color32::from_rgb(0x5B, 0x9C, 0xFF),
                    "LV2" => Color32::from_rgb(0x3D, 0xDC, 0x84),
                    _ => Color32::from_rgb(0xFF, 0xB0, 0x20),
                };
                let pill = Rect::from_min_size(r.left_top() + vec2(8.0, 12.0), vec2(40.0, 20.0));
                p.rect_filled(pill, rr(6.0), c.gamma_multiply(0.2));
                p.text(pill.center(), Align2::CENTER_CENTER, format, FontId::proportional(10.0), c);
                p.text(pos2(pill.right() + 10.0, r.top() + 15.0), Align2::LEFT_CENTER, recortar(p, &name, 13.5, r.right() - pill.right() - 20.0), FontId::proportional(13.5), TEXT());
                p.text(pos2(pill.right() + 10.0, r.top() + 31.0), Align2::LEFT_CENTER, recortar(p, &path.display().to_string(), 10.0, r.right() - pill.right() - 20.0), FontId::proportional(10.0), TEXT_DIM());
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
                    ui.label(RichText::new(tr("Instalado en el sistema: se desinstala con el gestor de paquetes (dnf).")).size(11.0).color(TEXT_DIM()));
                }
            });
            r.on_hover_text(format!("{} · {}", path.display(), tr("clic derecho: desinstalar u ocultar")));
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
            ui.label(RichText::new(path.display().to_string()).size(11.5).color(TEXT_DIM()));
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
            // Los efectos van en cualquier pista: audio o instrumento (después del sonido del instrumento).
            LibItem::Fx(k) => match track.filter(|&i| self.admite_efectos(i)) {
                Some(i) => self.add_fx(i, *k),
                None => self.status = tr("Suelta el efecto sobre una pista o un canal").into(),
            },
            LibItem::Instrument(id) => match track.filter(|&i| self.s.tracks[i].midi()) {
                Some(i) => self.set_instrument(i, id),
                None => self.status = "Suelta el instrumento sobre una pista MIDI".into(),
            },
            LibItem::Drummer => self.soltar_drummer(track),
            LibItem::File(path) if video::is_video(path) => self.import_video(path.clone(), at),
            LibItem::File(path) => {
                let r = self.import_one(path, at, track);
                self.report(format!("Importado: {}", path.display()), r);
            }
        }
    }

    pub fn floating_windows(&mut self, ctx: &egui::Context) {
        // Mixer y biblioteca flotantes no dependen del panel inferior ni del lateral: abrir el piano roll
        // ya no muestra el mixer. Cerrar la ventana la quita (y deja de estar separada).
        if self.mixer_float && !self.ventana(ctx, "mixer-win", tr("Mixer").into(), [1100.0, 560.0], false, |app, ui| app.mixer(ui)) {
            self.mixer_float = false;
            self.config.separadas.retain(|s| s != "mixer-win");
            self.config.save();
        }
        if self.lib_float && !self.ventana(ctx, "lib-win", tr("Biblioteca").into(), [340.0, 640.0], false, |app, ui| app.library(ui)) {
            (self.lib_float, self.show_lib) = (false, false);
            self.config.separadas.retain(|s| s != "lib-win");
            self.config.save();
        }
    }
}

/// ¿Se muestra el efecto en esta compilación? (los Pro solo si su módulo está incluido)
fn visible(k: FxKind) -> bool {
    k.pro().is_none() || engine::pro::disponible()
}

/// Título de grupo con contador.
fn grupo(ui: &mut egui::Ui, texto: &str, n: usize) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr(texto).to_uppercase()).size(10.5).strong().color(TEXT_DIM()));
        ui.label(RichText::new(n.to_string()).size(10.0).color(ACCENT()));
    });
}

/// Texto recortado con «…» para que entre en `ancho`.
fn recortar(p: &Painter, texto: &str, tam: f32, ancho: f32) -> String {
    let cabe = |s: &str| p.layout_no_wrap(s.to_string(), FontId::proportional(tam), TEXT()).size().x <= ancho;
    if cabe(texto) {
        return texto.to_string();
    }
    let mut s: String = texto.to_string();
    while !s.is_empty() && !cabe(&format!("{s}…")) {
        s.pop();
    }
    if s.is_empty() { String::new() } else { format!("{}…", s.trim_end()) }
}

/// Estrella de favoritos (rellena o solo el contorno).
fn dibujar_estrella(p: &Painter, c: Pos2, r: f32, color: Color32, llena: bool) {
    let pts: Vec<Pos2> = (0..10)
        .map(|k| {
            let a = std::f32::consts::PI * (k as f32 / 5.0) - std::f32::consts::FRAC_PI_2;
            let rad = if k % 2 == 0 { r } else { r * 0.45 };
            c + vec2(a.cos(), a.sin()) * rad
        })
        .collect();
    if llena {
        // Relleno con triángulos desde el centro (la estrella no es convexa).
        for k in 0..10 {
            p.add(egui::Shape::convex_polygon(vec![c, pts[k], pts[(k + 1) % 10]], color, Stroke::NONE));
        }
    } else {
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.2, color)));
    }
}

/// Color de cada familia de instrumentos.
fn color_familia(f: &str) -> Color32 {
    let h = f.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    const C: [Color32; 6] = [
        Color32::from_rgb(0x5B, 0x9C, 0xFF),
        Color32::from_rgb(0xFF, 0xB0, 0x20),
        Color32::from_rgb(0x3D, 0xDC, 0x84),
        Color32::from_rgb(0xB0, 0x8C, 0xF2),
        Color32::from_rgb(0xFF, 0x6B, 0x9A),
        Color32::from_rgb(0x2E, 0xD8, 0xC3),
    ];
    C[h as usize % C.len()]
}

/// Icono de cada pestaña: muestras (nota), efectos (perilla), instrumentos (teclado), plugins (enchufe).
fn icono_seccion(p: &Painter, c: Pos2, k: usize, color: Color32) {
    let s = Stroke::new(1.6, color);
    match k {
        0 => {
            p.circle_filled(c + vec2(-3.0, 4.0), 3.2, color);
            p.line_segment([c + vec2(0.0, 4.0), c + vec2(0.0, -7.0)], s);
            p.line_segment([c + vec2(0.0, -7.0), c + vec2(6.0, -4.0)], s);
        }
        1 => {
            p.circle_stroke(c, 7.0, s);
            p.line_segment([c, c + vec2(4.0, -5.0)], s);
        }
        2 => {
            p.rect_stroke(Rect::from_center_size(c, vec2(16.0, 12.0)), 2.0, s, StrokeKind::Middle);
            for x in [-4.0, 0.0, 4.0] {
                p.line_segment([c + vec2(x, -6.0), c + vec2(x, 6.0)], Stroke::new(1.0, color));
            }
        }
        _ => {
            p.rect_stroke(Rect::from_center_size(c + vec2(0.0, 1.0), vec2(12.0, 9.0)), 3.0, s, StrokeKind::Middle);
            p.line_segment([c + vec2(-3.0, -3.5), c + vec2(-3.0, -7.0)], s);
            p.line_segment([c + vec2(3.0, -3.5), c + vec2(3.0, -7.0)], s);
            p.line_segment([c + vec2(0.0, 5.5), c + vec2(0.0, 8.0)], s);
        }
    }
}

/// Icono de cada efecto (lo que hace, dibujado).
pub fn icono_fx(p: &Painter, c: Pos2, k: FxKind, color: Color32) {
    let s = Stroke::new(1.7, color);
    let curva = |f: &dyn Fn(f32) -> f32| -> Vec<Pos2> { (0..=16).map(|i| i as f32 / 16.0).map(|t| c + vec2(-9.0 + 18.0 * t, f(t))).collect() };
    match k {
        FxKind::Eq | FxKind::Curves => {
            let f = |t: f32| -5.0 * (-(t - 0.3) * (t - 0.3) * 40.0).exp() + 4.0 * (-(t - 0.75) * (t - 0.75) * 50.0).exp();
            p.add(egui::Shape::line(curva(&f), s));
        }
        FxKind::Compressor | FxKind::Limiter => {
            p.line_segment([c + vec2(-8.0, 8.0), c + vec2(0.0, 0.0)], s);
            let y = if k == FxKind::Limiter { 0.0 } else { -3.0 };
            p.line_segment([c, c + vec2(8.0, y)], s);
            if k == FxKind::Limiter {
                p.line_segment([c + vec2(-9.0, -2.0), c + vec2(9.0, -2.0)], Stroke::new(1.0, color.gamma_multiply(0.6)));
            }
        }
        FxKind::Delay => (0..3).for_each(|i| _ = p.circle_filled(c + vec2(-6.0 + i as f32 * 6.0, 0.0), 3.0 - i as f32 * 0.8, color)),
        FxKind::Reverb => (0..3).for_each(|i| _ = p.add(egui::Shape::line((0..=8).map(|j| j as f32 / 8.0 * std::f32::consts::PI).map(|a| c + vec2(-6.0, 6.0) + vec2(a.cos(), -a.sin()) * (4.0 + i as f32 * 3.5) + vec2(6.0, 0.0)).collect(), Stroke::new(1.4, color.gamma_multiply(1.0 - i as f32 * 0.25))))),
        FxKind::Drive => {
            let f = |t: f32| (-(t * std::f32::consts::TAU * 1.5).sin() * 9.0).clamp(-5.0, 5.0);
            p.add(egui::Shape::line(curva(&f), s));
        }
        FxKind::Tune => {
            p.circle_filled(c + vec2(-2.0, 5.0), 3.2, color);
            p.line_segment([c + vec2(1.0, 5.0), c + vec2(1.0, -7.0)], s);
            p.line_segment([c + vec2(1.0, -7.0), c + vec2(7.0, -4.0)], s);
        }
        FxKind::Gate => _ = p.add(egui::Shape::line(vec![c + vec2(-9.0, 5.0), c + vec2(-4.0, 5.0), c + vec2(-4.0, -5.0), c + vec2(4.0, -5.0), c + vec2(4.0, 5.0), c + vec2(9.0, 5.0)], s)),
        FxKind::Chorus => {
            for (d, a) in [(0.0, 1.0), (2.5, 0.5)] {
                let f = move |t: f32| (t * std::f32::consts::TAU * 1.2 + d).sin() * 4.5;
                p.add(egui::Shape::line(curva(&f), Stroke::new(1.5, color.gamma_multiply(a))));
            }
        }
        FxKind::DeEsser => _ = p.text(c, Align2::CENTER_CENTER, "S", FontId::proportional(16.0), color),
        FxKind::TapePro => {
            for x in [-5.0, 5.0] {
                p.circle_stroke(c + vec2(x, -1.0), 4.0, s);
            }
            p.line_segment([c + vec2(-8.0, 5.0), c + vec2(8.0, 5.0)], s);
        }
        FxKind::SpacePro => {
            for (d, r) in [(vec2(-4.0, -3.0), 2.4), (vec2(4.0, 2.0), 1.8), (vec2(0.0, 6.0), 1.2), (vec2(6.0, -6.0), 1.4)] {
                p.circle_filled(c + d, r, color);
            }
        }
        FxKind::VocalPro => {
            p.rect_stroke(Rect::from_center_size(c + vec2(0.0, -3.0), vec2(7.0, 11.0)), 3.5, s, StrokeKind::Middle);
            p.line_segment([c + vec2(0.0, 4.0), c + vec2(0.0, 8.0)], s);
            p.line_segment([c + vec2(-4.0, 8.0), c + vec2(4.0, 8.0)], s);
        }
        FxKind::MasterPro => {
            for (k, h) in [5.0f32, 9.0, 13.0, 8.0].iter().enumerate() {
                p.rect_filled(Rect::from_min_max(c + vec2(-8.0 + k as f32 * 4.5, 7.0 - h), c + vec2(-5.0 + k as f32 * 4.5, 7.0)), 1.0, color);
            }
        }
    }
}
