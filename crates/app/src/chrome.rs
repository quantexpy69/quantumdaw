//! Menú superior, barra de control (LCD central), barra de estado y diálogos modales.
use crate::{widgets::rr, *};
use egui::{Id, RichText, Stroke, vec2};

const RATES: [u32; 6] = [0, 44_100, 48_000, 88_200, 96_000, 192_000];
const BUFFERS: [u32; 7] = [0, 64, 128, 256, 512, 1024, 2048];
const SIGNATURES: [(u32, u32); 8] = [(2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8), (9, 8), (12, 8)];
/// Altura interior de las cajas de la barra de control.
const BOX_H: f32 = 40.0;
/// Altura total de una fila de la barra de control (contenido, márgenes y borde).
pub const BAR_ROW_H: f32 = BOX_H + 12.0;
const LCD: Color32 = Color32::from_rgb(0x10, 0x10, 0x12);

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    ui.add(egui::Button::new(tr(label)).shortcut_text(shortcut)).clicked()
}

fn auto(v: u32, unit: &str) -> String {
    if v == 0 { "Sistema".into() } else { format!("{v} {unit}") }
}

/// Nombre legible de un dispositivo (nodo de PipeWire) con su tipo de conexión.
fn device_label(devices: &[DeviceInfo], id: &str) -> String {
    match id {
        "" => "Por defecto del sistema".into(),
        "-" => "Ninguna".into(),
        id => devices.iter().find(|d| d.id == id).map_or_else(|| id.into(), |d| format!("{} · {}", d.kind, d.name)),
    }
}

fn lcd_cell(ui: &mut egui::Ui, caption: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        add(ui);
        ui.label(RichText::new(tr(caption)).size(8.5).color(TEXT_DIM));
    });
}

impl App {
    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            ui.menu_button(RichText::new(tr("Archivo")).size(16.0), |ui| {
                widgets::menu_style(ui);
                if item(ui, tr("Nuevo proyecto…"), "") {
                    let r = self.open_dialog(true);
                    self.report("Proyecto creado", r);
                }
                ui.menu_button(tr("Proyectos recientes"), |ui| {
                    widgets::menu_style(ui);
                    for dir in self.config.recent.clone() {
                        let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
                        if ui.button(name).on_hover_text(dir.display().to_string()).clicked() {
                            let r = self.open_dir(dir);
                            self.report("Proyecto abierto", r);
                        }
                    }
                });
                if item(ui, tr("Abrir proyecto…"), "") {
                    let r = self.open_dialog(false);
                    self.report("Proyecto abierto", r);
                }
                ui.separator();
                if item(ui, tr("Guardar"), tr("Ctrl+S")) {
                    self.save();
                }
                if item(ui, tr("Guardar como…"), tr("Ctrl+Shift+S")) {
                    let r = self.save_as();
                    self.report("Proyecto guardado", r);
                }
                ui.separator();
                if item(ui, tr("Importar video…"), "")
                    && let Some(f) = rfd::FileDialog::new().add_filter("Video", &video::VIDEO_EXT).pick_file()
                {
                    let at = self.pos();
                    self.import_video(f, at);
                }
                if item(ui, tr("Ir al menú principal…"), "") {
                    self.open_main_menu(0);
                }
                if item(ui, tr("Datos de la canción y copyright…"), "") {
                    self.open_main_menu(1);
                }
                if item(ui, tr("Importar audio…"), tr("Ctrl+I")) {
                    let files = rfd::FileDialog::new().add_filter("Audio", &AUDIO_EXT).pick_files();
                    self.import(files.unwrap_or_default());
                }
                if item(ui, tr("Exportar…"), tr("Ctrl+E")) {
                    self.dialog = Some(Dialog::Export(Format::Wav24, 0, 0, 0));
                }
                ui.separator();
                if item(ui, tr("Salir"), "") {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button(RichText::new(tr("Editar")).size(16.0), |ui| {
                widgets::menu_style(ui);
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(tr("Deshacer")).shortcut_text("Ctrl+Z")).clicked() {
                    self.undo();
                }
                if ui.add_enabled(!self.redo.is_empty(), egui::Button::new(tr("Rehacer")).shortcut_text("Ctrl+Shift+Z")).clicked() {
                    self.redo();
                }
                ui.separator();
                if item(ui, tr("Cortar"), tr("Ctrl+X")) {
                    self.copy(true);
                }
                if item(ui, tr("Copiar"), tr("Ctrl+C")) {
                    self.copy(false);
                }
                if item(ui, tr("Pegar en el cursor"), tr("Ctrl+V")) {
                    self.paste();
                }
                if item(ui, tr("Eliminar"), tr("Supr")) {
                    self.delete();
                }
                if item(ui, tr("Dividir en el cursor"), tr("S")) {
                    self.split(self.pos());
                }
                if item(ui, tr("Seleccionar todo"), tr("Ctrl+A")) {
                    self.select_all();
                }
                ui.separator();
                if ui.add_enabled(self.time_sel.is_some(), egui::Button::new(tr("Loop = selección de tiempo"))).clicked() {
                    let (a, b) = self.time_sel.unwrap_or_default();
                    self.set_loop(a, b);
                }
                if item(ui, tr("Loop de todo el proyecto"), "") {
                    self.loop_all();
                }
            });
            ui.menu_button(RichText::new(tr("Ver")).size(16.0), |ui| {
                widgets::menu_style(ui);
                ui.checkbox(&mut self.show_lib, tr("Biblioteca (Y)"));
                ui.checkbox(&mut self.show_mixer, tr("Panel inferior (X)"));
                for (k, label) in [(0, "Mixer"), (1, "Piano roll"), (2, "Analizador")] {
                    if ui.radio(self.show_mixer && self.bottom_tab == k, tr(label)).clicked() {
                        (self.show_mixer, self.bottom_tab) = (true, k);
                    }
                }
                ui.checkbox(&mut self.show_auto, tr("Automatización (A)"));
                ui.checkbox(&mut self.show_video, tr("Visor de video"));
                ui.checkbox(&mut self.show_arrange, tr("Pista de arreglo (Intro, Estrofa, Coro…)"));
                ui.checkbox(&mut self.show_chords, tr("Pista de acordes"));
                ui.separator();
                ui.checkbox(&mut self.lib_float, tr("Biblioteca flotante"));
                ui.checkbox(&mut self.mixer_float, tr("Mixer flotante"));
                let mut round = widgets::ROUND.load(Relaxed);
                if ui.checkbox(&mut round, tr("Interfaz redondeada")).changed() {
                    widgets::ROUND.store(round, Relaxed);
                    self.config.square = !round;
                    self.config.save();
                    theme(ui.ctx());
                }
                ui.menu_button(tr("Tamaño de la interfaz"), |ui| {
                    widgets::menu_style(ui);
                    let current = ui.ctx().zoom_factor();
                    for (label, z) in [("90 %", 0.9), ("100 %", 1.0), ("115 %", 1.15), ("130 %", 1.3), ("150 %", 1.5), ("175 %", 1.75)] {
                        if ui.radio((current - z).abs() < 0.01, tr(label)).clicked() {
                            ui.ctx().set_zoom_factor(z);
                            self.config.ui_scale = z;
                            self.config.save();
                        }
                    }
                });
                ui.separator();
                if item(ui, tr("Acercar"), "+") {
                    self.zoom(1.25);
                }
                if item(ui, tr("Alejar"), "-") {
                    self.zoom(0.8);
                }
                ui.menu_button(tr("Altura de pistas"), |ui| {
                    widgets::menu_style(ui);
                    ui.label(RichText::new(tr("Afecta a las pistas seleccionadas (o a todas). También puedes arrastrar el borde inferior de cada cabecera.")).size(11.0).color(TEXT_DIM));
                    for (label, h) in [("Compacta", 60.0), ("Normal", TRACK_H), ("Grande", 160.0), ("Muy grande", 240.0)] {
                        if ui.button(tr(label)).clicked() {
                            let any = self.s.tracks.iter().any(|t| t.selected);
                            self.s.tracks.iter_mut().filter(|t| !any || t.selected).for_each(|t| t.height = h);
                        }
                    }
                });
                if item(ui, tr("Ajustar proyecto a la ventana"), "") {
                    let secs = (self.project_end() as f32 / self.sr() as f32).max(1.0);
                    (self.pps, self.sx) = ((ui.ctx().content_rect().width() - self.header_w() - 40.0) / secs, 0.0);
                }
                ui.separator();
                if item(ui, tr("Ir al inicio"), tr("Inicio")) {
                    self.go(0);
                }
                if item(ui, tr("Ir al final"), tr("Fin")) {
                    self.go(self.project_end());
                }
            });
            ui.menu_button(RichText::new(tr("Pista")).size(16.0), |ui| {
                widgets::menu_style(ui);
                if item(ui, tr("Nueva pista…"), tr("Ctrl+T")) {
                    self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
                }
                if item(ui, tr("Agrupar seleccionadas"), "") {
                    self.group_selected();
                }
                if item(ui, tr("Grupos…"), "") {
                    self.dialog = Some(Dialog::Groups);
                }
                ui.separator();
                if item(ui, tr("Eliminar pistas seleccionadas"), "") {
                    self.delete_tracks(|_, t| t.selected);
                }
            });
            ui.menu_button(RichText::new(tr("Herramientas")).size(16.0), |ui| {
                widgets::menu_style(ui);
                for (tool, label) in [(Tool::Select, "Selección (V)"), (Tool::Pencil, "Lápiz de automatización (P)"), (Tool::Blade, "Cuchilla (B)")] {
                    if ui.radio(self.tool == tool, tr(label)).clicked() {
                        self.set_tool(tool);
                    }
                }
                ui.checkbox(&mut self.snap, tr("Ajustar a la rejilla (N)"));
                let mut metro = self.engine.metronome.load(Relaxed);
                if ui.checkbox(&mut metro, tr("Metrónomo (K)")).changed() {
                    self.engine.metronome.store(metro, Relaxed);
                }
                ui.separator();
                if item(ui, tr("Atajos de teclado y ratón…"), "") {
                    self.dialog = Some(Dialog::Keys(None));
                }
                ui.menu_button(tr("Idioma"), |ui| {
                    widgets::menu_style(ui);
                    for (k, name) in i18n::LANGS.iter().enumerate() {
                        if ui.radio(self.config.lang == k as u8, *name).clicked() {
                            self.config.lang = k as u8;
                            i18n::set(k as u8);
                            self.config.save();
                        }
                    }
                });
                if item(ui, tr("Configuración de audio…"), "") {
                    self.dialog = Some(Dialog::Settings(self.audio.clone(), self.rec_bits, engine::audio_devices(), engine::midi_ports(), self.config.midi_port.clone()));
                }
            });
        });
    }

    /// Grupos de la barra de control en filas: todo en una línea si cabe; si no, herramientas abajo.
    pub fn bar_rows(&self) -> Vec<&'static [&'static str]> {
        const ALL: [&str; 4] = ["views", "transport", "lcd", "tools"];
        if self.bar_two_rows { vec![&ALL[..3], &ALL[3..]] } else { vec![&ALL] }
    }

    /// Barra de control fija: vistas | transporte | tiempo, metrónomo y tempo | herramientas, en una
    /// caja de altura uniforme con líneas separadoras, centrada en la ventana.
    pub fn control_bar(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let rows = self.bar_rows();
        self.bar_widths.resize(rows.len(), 0.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        for (ri, row) in rows.iter().enumerate() {
            let lead = ((width - self.bar_widths[ri]) / 2.0).max(4.0);
            let r = ui.horizontal(|ui| {
                ui.add_space(lead);
                let frame = egui::Frame::new().fill(LCD).stroke(Stroke::new(1.0, BORDER)).corner_radius(rr(12.0)).inner_margin(egui::Margin::symmetric(10, 5));
                frame.show(ui, |ui| {
                    ui.allocate_ui_with_layout(vec2(ui.available_width(), BOX_H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.set_min_height(BOX_H);
                        ui.spacing_mut().interact_size.y = 30.0;
                        for (k, id) in row.iter().enumerate() {
                            if k > 0 {
                                ui.add_space(6.0);
                                let (line, _) = ui.allocate_exact_size(vec2(1.0, BOX_H - 4.0), egui::Sense::hover());
                                ui.painter().rect_filled(line, 0.0, BORDER);
                                ui.add_space(6.0);
                            }
                            self.bar_group(ui, id);
                        }
                    });
                });
            });
            // En el primer cuadro aún no se conoce el ancho de la fila: se repite el cuadro centrado.
            let w = r.response.rect.width() - lead;
            if (self.bar_widths[ri] - w).abs() > 1.0 {
                ui.ctx().request_discard("centrar la barra de control");
            }
            self.bar_widths[ri] = w;
        }
        let single = self.bar_widths.iter().sum::<f32>() + 12.0 * (self.bar_widths.len() as f32 - 1.0);
        let two = single > width - 8.0;
        if two != self.bar_two_rows {
            self.bar_two_rows = two;
            self.bar_widths.clear();
            ui.ctx().request_discard("filas de la barra de control");
        }
    }

    /// Contenido de un grupo de la barra de control.
    fn bar_group(&mut self, ui: &mut egui::Ui, id: &str) {
        match id {
            "views" => {
                ui.toggle_value(&mut self.show_lib, RichText::new(tr("Biblioteca")).size(13.5)).on_hover_text("Y");
                ui.toggle_value(&mut self.show_mixer, RichText::new(tr("Mixer")).size(13.5)).on_hover_text("X");
                ui.toggle_value(&mut self.show_auto, RichText::new(tr("Automatización")).size(13.5)).on_hover_text("A");
            }
            "transport" => self.transport_box(ui),
            "lcd" => self.lcd_box(ui),
            _ => {
                type Draw = fn(&egui::Painter, egui::Rect, Color32);
                let tools: [(Tool, Draw, &str); 3] = [
                    (Tool::Select, widgets::draw_pointer, "Selección (V)"),
                    (Tool::Pencil, widgets::draw_pencil, "Lápiz: automatización y muestras (P)"),
                    (Tool::Blade, widgets::draw_scissors, "Cuchilla: clic en una región para cortarla (B)"),
                ];
                for (tool, draw, tip) in tools {
                    if widgets::icon_button(ui, self.tool == tool, draw).on_hover_text(tr(tip)).clicked() {
                        self.set_tool(tool);
                    }
                }
                ui.toggle_value(&mut self.snap, RichText::new(tr("Snap")).size(13.0)).on_hover_text(tr("Ajustar a la rejilla de tiempos (N)"));
            }
        }
    }

    /// Transporte con iconos dibujados: inicio, stop, reproducir/pausa, grabar, final y loop.
    fn transport_box(&mut self, ui: &mut egui::Ui) {
        if widgets::icon_button(ui, false, widgets::draw_prev).on_hover_text(tr("Ir al inicio (Inicio)")).clicked() {
            self.go(0);
        }
        if widgets::icon_button(ui, false, widgets::draw_stop).on_hover_text(tr("Detener")).clicked() {
            self.stop();
            self.go(0);
        }
        let playing = self.playing();
        if widgets::icon_button(ui, playing, if playing { widgets::draw_pause } else { widgets::draw_play }).on_hover_text(tr("Reproducir / pausa (Espacio)")).clicked() {
            self.toggle_play();
        }
        if widgets::record_button(ui, self.recording()).on_hover_text(tr("Grabar en pistas armadas (Ctrl+R)")).clicked() {
            self.toggle_record();
        }
        if widgets::icon_button(ui, false, widgets::draw_next).on_hover_text(tr("Ir al final (Fin)")).clicked() {
            self.go(self.project_end());
        }
        if widgets::icon_button(ui, self.engine.looping.load(Relaxed), widgets::draw_loop).on_hover_text(tr("Loop (L) · arrastra en la regla para definir el rango")).clicked() {
            self.toggle_loop();
        }
    }

    /// LCD: posición, tiempo, tempo y métrica, con el metrónomo al lado. Verde al reproducir, rojo al grabar.
    fn lcd_box(&mut self, ui: &mut egui::Ui) {
        let pos = self.pos() as f64;
        let beats = self.engine.beats.load(Relaxed).max(1) as u64;
        let b = self.beats(pos);
        let s = pos / self.sr();
        let (fill, stroke, ink) = match (self.recording(), self.playing()) {
            (true, _) => (Color32::from_rgb(0x3A, 0x10, 0x14), METER[2], Color32::from_rgb(0xFF, 0xC4, 0xC6)),
            (false, true) => (Color32::from_rgb(0x0E, 0x2A, 0x1A), METER[0], Color32::from_rgb(0xC8, 0xFF, 0xDD)),
            _ => (Color32::from_rgb(0x0C, 0x0C, 0x0E), BORDER, TEXT),
        };
        let lcd = egui::Frame::new().fill(fill).corner_radius(rr(9.0)).stroke(Stroke::new(1.0, stroke)).inner_margin(egui::Margin::symmetric(14, 2));
        lcd.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 20.0;
                let big = |t: String| RichText::new(t).monospace().size(19.0).color(ink);
                lcd_cell(ui, "COMPÁS", |ui| _ = ui.label(big(format!("{:>3} {} {:02}", b as u64 / beats + 1, b as u64 % beats + 1, (b.fract() * 100.0) as u32))));
                lcd_cell(ui, "TIEMPO", |ui| _ = ui.label(big(format!("{:02}:{:06.3}", (s / 60.0) as u32, s % 60.0))));
                lcd_cell(ui, "TEMPO", |ui| {
                    let mut bpm = engine::bpm_at(&self.s.tempo, pos);
                    let drag = egui::DragValue::new(&mut bpm).range(20.0..=300.0).speed(0.5).fixed_decimals(1);
                    if ui.add(drag).on_hover_text(tr("Tempo en la posición del cursor · arrastra o doble clic para escribir")).changed() {
                        self.set_tempo_here(bpm);
                    }
                });
                lcd_cell(ui, "MÉTRICA", |ui| {
                    let sig = (self.engine.beats.load(Relaxed), self.engine.unit.load(Relaxed));
                    egui::ComboBox::from_id_salt("sig").width(50.0).selected_text(format!("{}/{}", sig.0, sig.1)).show_ui(ui, |ui| {
                        for (n, d) in SIGNATURES {
                            if ui.selectable_label(sig == (n, d), format!("{n}/{d}")).clicked() {
                                self.engine.beats.store(n, Relaxed);
                                self.engine.unit.store(d, Relaxed);
                            }
                        }
                    });
                });
            });
        });
        let metro = self.engine.metronome.load(Relaxed);
        if widgets::icon_button(ui, metro, widgets::draw_metronome).on_hover_text(tr("Metrónomo (K)")).clicked() {
            self.engine.metronome.store(!metro, Relaxed);
        }
        self.tempo_tool(ui);
    }

    /// Contador de tempo inteligente: marcar el pulso (TAP) o detectar el BPM del audio seleccionado.
    fn tempo_tool(&mut self, ui: &mut egui::Ui) {
        let resp = widgets::icon_button(ui, false, widgets::draw_tap).on_hover_text(tr("Contador de tempo: marca el pulso o detecta el BPM del audio"));
        egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            widgets::menu_style(ui);
            ui.set_width(300.0);
            ui.label(RichText::new(tr("Contador de tempo")).size(16.0).strong());
            let now = ui.input(|i| i.time);
            let tap = egui::Button::new(RichText::new("TAP").size(22.0).strong().color(BG)).fill(METER[1]).min_size(vec2(300.0, 64.0)).corner_radius(rr(12.0));
            let tapped = ui.add(tap).on_hover_text(tr("Haz clic (o pulsa T con el ratón encima) al ritmo de la canción")).clicked() || (ui.ui_contains_pointer() && ui.input(|i| i.key_pressed(Key::T)));
            if tapped {
                if self.taps.last().is_some_and(|&t| now - t > 2.0) {
                    self.taps.clear();
                }
                self.taps.push(now);
                let n = self.taps.len().min(9);
                if n >= 3 {
                    let last = &self.taps[self.taps.len() - n..];
                    self.tempo_guess = Some((60.0 * (n - 1) as f64 / (last[n - 1] - last[0])) as f32);
                }
            }
            let sel = self.s.tracks.iter().flat_map(|t| &t.clips).find(|c| c.selected).and_then(|c| Some((c.buf()?.clone(), c.offset as usize, c.len as usize)));
            if ui.add_enabled(sel.is_some(), egui::Button::new(tr("Detectar BPM de la región seleccionada"))).clicked()
                && let Some((b, off, len)) = sel
            {
                self.tempo_guess = engine::detect_bpm(&b.frames[off..(off + len).min(b.frames.len())], self.sr() as f32);
                if self.tempo_guess.is_none() {
                    self.status = tr("La región es demasiado corta para detectar el tempo").into();
                }
            }
            ui.horizontal(|ui| {
                let text = self.tempo_guess.map_or("— BPM".to_string(), |b| format!("{b:.1} BPM"));
                ui.label(RichText::new(text).monospace().size(26.0).color(METER[0]));
                if let Some(b) = &mut self.tempo_guess {
                    if ui.button("½").on_hover_text(tr("Mitad")).clicked() {
                        *b /= 2.0;
                    }
                    if ui.button("×2").on_hover_text(tr("Doble")).clicked() {
                        *b *= 2.0;
                    }
                }
            });
            if let Some(bpm) = self.tempo_guess
                && ui.button(RichText::new(tr("Aplicar al tempo del proyecto")).strong()).clicked()
            {
                self.edit();
                self.set_tempo_here(bpm.clamp(20.0, 300.0).round());
                self.status = format!("{} {:.0} BPM", tr("Tempo del proyecto:"), bpm.round());
            }
            ui.label(RichText::new(tr("Se aplica al tramo de tempo donde está el cursor.")).size(11.5).color(TEXT_DIM));
        });
    }

    /// Franja superior con el logo, el nombre, la versión y la canción abierta.
    pub fn title_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            ui.add(welcome::logo(24.0));
            ui.label(RichText::new(tr("QUANTUM DAW")).size(15.0).strong().color(TEXT));
            ui.label(RichText::new(format!("v{}", welcome::VERSION)).size(11.0).color(TEXT_DIM));
            let song = if self.meta.title.is_empty() { self.dir.file_name().unwrap_or_default().to_string_lossy().to_string() } else { self.meta.title.clone() };
            ui.separator();
            ui.label(RichText::new(song).size(13.0).color(TEXT));
            if !self.meta.artist.is_empty() {
                ui.label(RichText::new(format!("· {}", self.meta.artist)).size(12.5).color(TEXT_DIM));
            }
        });
    }

    pub fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new(&self.status).size(12.0).color(TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let e = &self.engine;
                let buf = match self.audio.buffer {
                    0 => String::new(),
                    b => format!(" · {b} smp ({:.1} ms)", b as f64 / self.sr() * 1000.0),
                };
                let input = match e.in_channels {
                    0 => "sin entrada".to_string(),
                    n => format!("entrada: {} ({n} canales)", device_label(&self.devices, &self.audio.input)),
                };
                ui.label(RichText::new(format!("{} Hz{buf} · {input}", e.sample_rate)).size(12.0).color(TEXT_DIM));
            });
        });
    }

    pub fn dialogs(&mut self, ctx: &egui::Context) {
        enum Act {
            NewTrack(String, TrackKind, u32, Option<Color32>),
            Settings(AudioConfig, u16, String),
            Region(RegionOp),
            Tempo(u64, u64, f32, bool),
            Export(Format, u8, u8, usize),
            DissolveGroup(usize),
            Name(NameKind, usize, String),
        }
        // Editor de atajos: necesita `self` completo, así que se maneja aparte del resto.
        if let Some(Dialog::Keys(mut capture)) = self.dialog {
            let mut close = false;
            let resp = egui::Modal::new(Id::new("keys")).show(ctx, |ui| {
                ui.set_width(560.0);
                self.keys_editor(ui, &mut capture);
                close |= ui.button(tr("Cerrar")).clicked();
            });
            self.dialog = if close || (resp.should_close() && capture.is_none()) { None } else { Some(Dialog::Keys(capture)) };
            return;
        }
        // Compases del tramo de tempo (calculados antes de prestar el diálogo).
        let tempo_range = match &self.dialog {
            Some(Dialog::Tempo(a, b, ..)) => {
                let bar = |f: u64| {
                    let beats = self.beats(f as f64);
                    let per = self.engine.beats.load(Relaxed).max(1) as f64;
                    format!("{}.{}", (beats / per) as u64 + 1, (beats % per) as u64 + 1)
                };
                format!("Del compás {} al {}", bar(*a), bar(*b))
            }
            _ => String::new(),
        };
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let (mut act, mut close) = (None, false);
        let has_sel = self.time_sel.is_some();
        let (tracks, groups) = (&self.s.tracks, &mut self.s.groups);
        let in_channels = self.engine.in_channels;
        let input_error = self.engine.input_error.clone();
        let mut show_hdmi = self.config.show_hdmi;
        let resp = egui::Modal::new(Id::new("dialog")).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            match dialog {
                Dialog::NewTrack(name, kind, count, color) => {
                    ui.set_width(640.0);
                    ui.spacing_mut().item_spacing = vec2(10.0, 12.0);
                    ui.vertical_centered(|ui| ui.heading(RichText::new(tr("Nuevas pistas")).size(21.0)));
                    // Tarjetas de tipo de pista, cada una con su icono y color de acento.
                    let kinds = [
                        (TrackKind::AudioMono, "Audio mono", "Voz, guitarra o bajo por una entrada", "microfono", Color32::from_rgb(0x6D, 0xB4, 0xF2)),
                        (TrackKind::AudioStereo, "Audio estéreo", "Sintetizadores, loops y pares de micrófonos", "audio", Color32::from_rgb(0x6D, 0xD3, 0x9C)),
                        (TrackKind::Midi, "MIDI / instrumento", "Piano roll con instrumentos reales o el QUANTUM Synth", "teclado", Color32::from_rgb(0xB0, 0x8C, 0xF2)),
                        (TrackKind::Click, "Pista de clic", "El metrónomo como audio, exportable como stem", "metronomo", Color32::from_rgb(0xF2, 0xB8, 0x5C)),
                    ];
                    // Dos columnas de tarjetas del mismo tamaño, centradas en el diálogo.
                    let card = vec2(314.0, 82.0);
                    for pair in kinds.chunks(2) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            ui.add_space(((ui.available_width() - 2.0 * card.x - 12.0) / 2.0).max(0.0));
                            for &(k, label, hint, icon, accent) in pair {
                                let on = *kind == k;
                                let (rect, r) = ui.allocate_exact_size(card, egui::Sense::click());
                                let fill = if on { accent.gamma_multiply(0.28) } else if r.hovered() { BORDER } else { ELEVATED };
                                ui.painter().rect(rect, rr(12.0), fill, egui::Stroke::new(if on { 2.0 } else { 1.0 }, if on { accent } else { BORDER }), egui::StrokeKind::Inside);
                                {
                                    let ui = &mut ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)).layout(egui::Layout::left_to_right(egui::Align::Center)));
                                    ui.add(icons::image(icon).tint(accent).fit_to_exact_size(vec2(36.0, 36.0)));
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 2.0;
                                        ui.label(RichText::new(tr(label)).size(15.0).strong());
                                        ui.add(egui::Label::new(RichText::new(tr(hint)).size(12.0).color(TEXT_DIM)).wrap());
                                    });
                                }
                                if r.clicked() {
                                    *kind = k;
                                }
                            }
                        });
                    }
                    egui::Grid::new("new-track-opts").num_columns(2).spacing([18.0, 12.0]).show(ui, |ui| {
                        ui.label(RichText::new(tr("Cantidad")).size(14.0));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(count).range(1..=64).speed(0.2));
                            for n in [1, 2, 4, 8, 16] {
                                if ui.selectable_label(*count == n, RichText::new(n.to_string()).size(14.0)).clicked() {
                                    *count = n;
                                }
                            }
                        });
                        ui.end_row();
                        ui.label(RichText::new(tr("Nombre")).size(14.0));
                        ui.add(egui::TextEdit::singleline(name).hint_text(tr("Opcional; se numeran si son varias")).desired_width(360.0));
                        ui.end_row();
                        ui.label(RichText::new(tr("Color")).size(14.0));
                        ui.horizontal(|ui| {
                            if ui.selectable_label(color.is_none(), tr("Automático")).clicked() {
                                *color = None;
                            }
                            for c in PALETTE.map(rgb) {
                                let (r, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), egui::Sense::click());
                                ui.painter().circle_filled(r.center(), 9.0, c);
                                if *color == Some(c) || resp.hovered() {
                                    ui.painter().circle_stroke(r.center(), 11.0, egui::Stroke::new(2.0, TEXT));
                                }
                                if resp.clicked() {
                                    *color = Some(c);
                                }
                            }
                        });
                        ui.end_row();
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(((ui.available_width() - 302.0) / 2.0).max(0.0));
                        let label = if *count == 1 { "Crear pista".to_string() } else { format!("Crear {count} pistas") };
                        let b = egui::Button::new(RichText::new(&label).size(15.5).strong()).fill(ACCENT).min_size(vec2(180.0, 38.0)).corner_radius(rr(19.0));
                        if ui.add(b).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                            act = Some(Act::NewTrack(name.clone(), *kind, *count, *color));
                        }
                        close |= ui.add(egui::Button::new(RichText::new(tr("Cancelar")).size(14.5)).min_size(vec2(110.0, 38.0))).clicked();
                    });
                }
                Dialog::Settings(cfg, bits, devices, ports, midi) => {
                    ui.heading(tr("Configuración de audio y MIDI"));
                    let hdmi = show_hdmi;
                    let visible = |input: bool| devices.iter().filter(move |d| d.input == input && (hdmi || d.kind != "HDMI"));
                    egui::Grid::new("settings").num_columns(2).spacing([16.0, 8.0]).show(ui, |ui| {
                        ui.label(tr("Salida"));
                        egui::ComboBox::from_id_salt("out").width(280.0).selected_text(device_label(devices, &cfg.output)).show_ui(ui, |ui| {
                            ui.selectable_value(&mut cfg.output, String::new(), tr("Por defecto del sistema"));
                            visible(false).for_each(|d| _ = ui.selectable_value(&mut cfg.output, d.id.clone(), format!("{} · {}", d.kind, d.name)));
                        });
                        ui.end_row();
                        ui.label(tr("Entrada"));
                        egui::ComboBox::from_id_salt("in").width(280.0).selected_text(device_label(devices, &cfg.input)).show_ui(ui, |ui| {
                            ui.selectable_value(&mut cfg.input, String::new(), tr("Por defecto del sistema"));
                            ui.selectable_value(&mut cfg.input, "-".into(), tr("Ninguna"));
                            visible(true).for_each(|d| _ = ui.selectable_value(&mut cfg.input, d.id.clone(), format!("{} · {}", d.kind, d.name)));
                        });
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut show_hdmi, tr("Mostrar salidas HDMI / DisplayPort"));
                        ui.end_row();
                        ui.label(tr("Entrada MIDI"));
                        let midi_label = match midi.as_str() {
                            "" => "Todos los dispositivos".to_string(),
                            "-" => "Ninguna".to_string(),
                            m => m.to_string(),
                        };
                        egui::ComboBox::from_id_salt("midi").width(280.0).selected_text(midi_label).show_ui(ui, |ui| {
                            ui.selectable_value(midi, String::new(), tr("Todos los dispositivos"));
                            ui.selectable_value(midi, "-".into(), tr("Ninguna"));
                            ports.iter().for_each(|p| _ = ui.selectable_value(midi, p.clone(), p));
                        });
                        ui.end_row();
                        ui.label(tr("Frecuencia de muestreo"));
                        egui::ComboBox::from_id_salt("rate").selected_text(auto(cfg.rate, "Hz")).show_ui(ui, |ui| {
                            RATES.iter().for_each(|&r| _ = ui.selectable_value(&mut cfg.rate, r, auto(r, "Hz")));
                        });
                        ui.end_row();
                        ui.label(tr("Tamaño de buffer"));
                        egui::ComboBox::from_id_salt("buffer").selected_text(auto(cfg.buffer, "samples")).show_ui(ui, |ui| {
                            BUFFERS.iter().for_each(|&b| _ = ui.selectable_value(&mut cfg.buffer, b, auto(b, "samples")));
                        });
                        ui.end_row();
                        ui.label(tr("Resolución de grabación"));
                        egui::ComboBox::from_id_salt("bits").selected_text(format!("{bits} bits")).show_ui(ui, |ui| {
                            for b in [16, 24, 32] {
                                ui.selectable_value(bits, b, if b == 32 { "32 bits float".into() } else { format!("{b} bits") });
                            }
                        });
                        ui.end_row();
                    });
                    let state = match (&input_error, in_channels) {
                        (Some(e), _) => format!("La entrada no se pudo abrir: {e}"),
                        (None, 0) => "Sin entrada activa.".into(),
                        (None, n) => format!("Entrada activa con {n} canales."),
                    };
                    ui.label(RichText::new(state).size(11.0).color(TEXT_DIM));
                    ui.label(RichText::new(tr("Buffers pequeños = menos latencia y más CPU. Al aplicar se reinicia el motor de audio.")).size(11.0).color(TEXT_DIM));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Aplicar")).clicked() {
                            act = Some(Act::Settings(cfg.clone(), *bits, midi.clone()));
                        }
                        close |= ui.button(tr("Cancelar")).clicked();
                    });
                }
                Dialog::Export(format, range, source, track) => {
                    ui.heading(tr("Exportar"));
                    egui::ComboBox::from_label(tr("Formato")).selected_text(format.label()).show_ui(ui, |ui| {
                        Format::ALL.iter().for_each(|&f| _ = ui.selectable_value(format, f, f.label()));
                    });
                    ui.label(RichText::new(tr("Qué exportar")).strong());
                    ui.radio_value(source, 0, tr("Mezcla completa del proyecto"));
                    ui.horizontal(|ui| {
                        ui.radio_value(source, 1, tr("Solo una pista:"));
                        let name = tracks.get(*track).map_or("—", |t| t.name.as_str());
                        egui::ComboBox::from_id_salt("exp-track").selected_text(name).show_ui(ui, |ui| {
                            for (i, t) in tracks.iter().enumerate().filter(|(_, t)| t.kind != TrackKind::Midi) {
                                if ui.selectable_value(track, i, &t.name).clicked() {
                                    *source = 1;
                                }
                            }
                        });
                    });
                    ui.radio_value(source, 2, tr("Cada pista por separado (stems)"));
                    ui.label(RichText::new(tr("Rango")).strong());
                    ui.radio_value(range, 0, tr("Todo el proyecto"));
                    ui.radio_value(range, 1, tr("Rango de loop"));
                    ui.add_enabled_ui(has_sel, |ui| ui.radio_value(range, 2, tr("Selección de tiempo")));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Exportar…")).clicked() {
                            act = Some(Act::Export(*format, *range, *source, *track));
                        }
                        close |= ui.button(tr("Cancelar")).clicked();
                    });
                }
                Dialog::Stretch(pct) => {
                    ui.heading(tr("Estirar tiempo"));
                    ui.label(RichText::new(tr("Cambia la duración de las regiones seleccionadas sin cambiar el tono.")).color(TEXT_DIM));
                    ui.add(egui::Slider::new(pct, 25.0..=400.0).suffix(" %").text(tr("Duración")));
                    ui.horizontal(|ui| {
                        for v in [50.0, 75.0, 90.0, 110.0, 150.0, 200.0] {
                            if ui.button(format!("{v} %")).clicked() {
                                *pct = v;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button(tr("Aplicar")).clicked() {
                            act = Some(Act::Region(RegionOp::Stretch(*pct as f64 / 100.0)));
                        }
                        close |= ui.button(tr("Cancelar")).clicked();
                    });
                }
                Dialog::Transpose(st) => {
                    ui.heading(tr("Transponer"));
                    ui.label(RichText::new(tr("Cambia el tono sin cambiar la duración (en MIDI mueve las notas).")).color(TEXT_DIM));
                    ui.add(egui::Slider::new(st, -12.0..=12.0).step_by(1.0).suffix(" semitonos"));
                    ui.horizontal(|ui| {
                        if ui.button(tr("Aplicar")).clicked() {
                            act = Some(Act::Region(RegionOp::Transpose(*st)));
                        }
                        close |= ui.button(tr("Cancelar")).clicked();
                    });
                }
                Dialog::Keys(_) => {}
                Dialog::Tempo(a, b, bpm, adapt) => {
                    ui.heading(tr("Tempo del tramo"));
                    ui.label(RichText::new(&tempo_range).color(TEXT_DIM));
                    ui.add(egui::DragValue::new(bpm).range(20.0..=300.0).speed(0.5).fixed_decimals(1).suffix(" BPM"));
                    ui.checkbox(adapt, tr("Adaptar el audio y el MIDI de todas las pistas a este tempo"));
                    ui.label(
                        RichText::new(if *adapt {
                            "El material del tramo se estira (time-stretch) y lo que viene después se desplaza."
                        } else {
                            "Solo cambian la rejilla y el metrónomo en este tramo; el audio no se toca."
                        })
                        .size(11.0)
                        .color(TEXT_DIM),
                    );
                    ui.horizontal(|ui| {
                        if ui.button(tr("Aplicar")).clicked() {
                            act = Some(Act::Tempo(*a, *b, *bpm, *adapt));
                        }
                        close |= ui.button(tr("Cancelar")).clicked();
                    });
                }
                Dialog::Name(kind, k, name) => {
                    ui.set_width(460.0);
                    let title = match kind {
                        NameKind::Marker => "Nombre de la marca",
                        NameKind::Chord => "Acorde",
                        NameKind::Section => "Sección del arreglo",
                        NameKind::Group => "Nombre del grupo",
                    };
                    ui.heading(tr(title));
                    let r = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY).font(egui::FontId::proportional(18.0)));
                    if ui.memory(|m| m.focused().is_none()) {
                        r.request_focus();
                    }
                    let chip = |ui: &mut egui::Ui, text: &str, on: bool| ui.add(egui::Button::selectable(on, RichText::new(text).size(14.0)).min_size(vec2(38.0, 28.0))).clicked();
                    match kind {
                        NameKind::Chord => {
                            // Acorde = raíz + tipo; los botones cambian una parte y conservan la otra.
                            let split = name.char_indices().nth(1).filter(|(_, c)| *c == '#' || *c == 'b').map_or(name.chars().next().map_or(0, |c| c.len_utf8()), |(i, c)| i + c.len_utf8());
                            let (root, quality) = (name[..split].to_string(), name[split..].to_string());
                            ui.label(RichText::new(tr("Raíz")).color(TEXT_DIM));
                            ui.horizontal_wrapped(|ui| {
                                for r in ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"] {
                                    if chip(ui, r, root == r) {
                                        *name = format!("{r}{quality}");
                                    }
                                }
                            });
                            ui.label(RichText::new(tr("Tipo")).color(TEXT_DIM));
                            ui.horizontal_wrapped(|ui| {
                                for q in ["", "m", "7", "maj7", "m7", "sus2", "sus4", "dim", "aug", "add9", "6", "9"] {
                                    if chip(ui, if q.is_empty() { tr("Mayor") } else { q }, quality == q) {
                                        *name = format!("{root}{q}");
                                    }
                                }
                            });
                        }
                        NameKind::Section => {
                            ui.horizontal_wrapped(|ui| {
                                for s in ["Intro", "Estrofa", "Pre coro", "Coro", "Puente", "Solo", "Instrumental", "Outro"] {
                                    if chip(ui, tr(s), name == tr(s)) {
                                        *name = tr(s).to_string();
                                    }
                                }
                            });
                        }
                        _ => {}
                    }
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(RichText::new(tr("Aceptar")).strong()).fill(ACCENT)).clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                            act = Some(Act::Name(*kind, *k, name.clone()));
                        }
                        close |= ui.button(tr("Cancelar")).clicked();
                    });
                }
                Dialog::Groups => {
                    ui.heading(tr("Grupos"));
                    if groups.is_empty() {
                        ui.label(RichText::new(tr("No hay grupos. Selecciona pistas y usa Pista → Agrupar seleccionadas.")).color(TEXT_DIM));
                    }
                    for (g, group) in groups.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgba(&mut group.color);
                            ui.text_edit_singleline(&mut group.name);
                            ui.checkbox(&mut group.edit, tr("Edición vinculada")).on_hover_text(tr("Seleccionar una región selecciona también las del resto del grupo en ese tiempo"));
                            if ui.button(tr("Disolver")).clicked() {
                                act = Some(Act::DissolveGroup(g));
                            }
                        });
                    }
                    close |= ui.button(tr("Cerrar")).clicked();
                }
            }
        });
        if let Some(Dialog::Settings(_, _, devices, ..)) = &self.dialog {
            self.devices = devices.clone();
        }
        if resp.should_close() || close {
            self.dialog = None;
        }
        self.config.show_hdmi = show_hdmi;
        match act {
            Some(Act::NewTrack(name, kind, count, color)) => {
                let prefix = match kind {
                    TrackKind::Midi => "MIDI",
                    TrackKind::Click => "Clic",
                    _ => "Audio",
                };
                let first = self.s.tracks.len();
                for k in 0..count as usize {
                    let n = self.s.tracks.len() + 1;
                    let name = match (name.trim().is_empty(), count) {
                        (true, _) => format!("{prefix} {n}"),
                        (false, 1) => name.clone(),
                        (false, _) => format!("{} {}", name.trim(), k + 1),
                    };
                    self.add_track(name, kind);
                }
                // Quedan seleccionadas todas las pistas nuevas (con el color elegido, si lo hay).
                for (k, t) in self.s.tracks.iter_mut().enumerate() {
                    t.selected = k >= first;
                    if let Some(c) = color.filter(|_| k >= first) {
                        t.color = c;
                    }
                }
                self.status = format!("{count} pista(s) creada(s)");
                self.dialog = None;
            }
            Some(Act::Tempo(a, b, bpm, adapt)) => {
                let r = self.apply_tempo(a, b, bpm, adapt);
                self.report(format!("Tempo {bpm:.1} BPM aplicado al tramo"), r);
                self.dialog = None;
            }
            Some(Act::Region(op)) => {
                let r = self.region_op(op);
                self.report("Regiones procesadas", r);
                self.dialog = None;
            }
            Some(Act::Settings(cfg, bits, midi)) => {
                self.config.midi_port = midi;
                let mut p = self.to_project();
                (p.output_device, p.input_device, p.sample_rate, p.buffer, p.rec_bits) = (cfg.output, cfg.input, cfg.rate, cfg.buffer, bits);
                let r = self.reload(self.dir.clone(), p);
                if r.is_err() || self.engine.input_error.is_none() {
                    self.report("Motor de audio reiniciado", r);
                }
                self.dialog = None;
            }
            Some(Act::Export(format, range, source, track)) => {
                let r = self.export(format, range, source, track);
                self.status = r.unwrap_or_else(|e| format!("Error: {e}"));
                self.dialog = None;
            }
            Some(Act::DissolveGroup(g)) => self.remove_group(g),
            Some(Act::Name(kind, k, name)) => {
                let s = &mut self.s;
                let slot = match kind {
                    NameKind::Marker => s.markers.get_mut(k).map(|m| &mut m.1),
                    NameKind::Chord => s.chords.get_mut(k).map(|m| &mut m.2),
                    NameKind::Section => s.sections.get_mut(k).map(|m| &mut m.2),
                    NameKind::Group => s.groups.get_mut(k).map(|g| &mut g.name),
                };
                if let Some(slot) = slot
                    && !name.trim().is_empty()
                {
                    *slot = name.trim().to_string();
                }
                self.dialog = None;
            }
            None => {}
        }
    }
}
