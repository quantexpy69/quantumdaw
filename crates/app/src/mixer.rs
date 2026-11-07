//! Consola de mezcla: canales angostos y planos (efectos arriba, ruteo, panorama, fader con sus
//! lecturas y el nombre abajo). Los canales se adaptan a la altura del panel.
use crate::controlador::{Destino, Pedido};
use crate::{
    library::LibItem,
    widgets::{self, rr},
    *,
};
use egui::{Rect, RichText, Sense, Stroke, StrokeKind, vec2};

const STRIP_W: f32 = 76.0;
const SLOTS: usize = 4;
/// Teclas del canal (M, S, grabar, I y polaridad): cinco caben a lo ancho.
const TECLA_MIX: f32 = 14.0;
/// Color del texto de salida (ruteo), como en las consolas: ámbar.
const SALIDA: Color32 = Color32::from_rgb(0xE8, 0xA2, 0x3A);
const FILA: f32 = 16.0;

/// Caja de una línea a todo el ancho del canal (entrada, salida, instrumento): clic para abrir.
fn caja(ui: &mut egui::Ui, texto: String, tinta: Color32, fondo: Color32) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(texto).size(10.5).color(tinta)).fill(fondo).stroke(Stroke::new(1.0, BORDER())).corner_radius(rr(2.0)).min_size(vec2(STRIP_W, 18.0)).truncate())
}

/// Encabezado «⏻ Inserciones» de la lista de efectos del canal.
fn cabecera_inserciones(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let (r, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
        widgets::draw_encendido(ui.painter(), r, TEXT_DIM());
        ui.label(RichText::new(tr("Inserciones")).size(10.0).color(TEXT_DIM()));
    });
}

/// Ranura de inserción: con efecto (punto de acento si está activo) o vacía.
fn ranura(ui: &mut egui::Ui, fx: Option<(String, bool)>) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(STRIP_W, FILA), Sense::click());
    let p = ui.painter();
    match fx {
        Some((nombre, on)) => {
            let borde = if on { ACCENT().gamma_multiply(0.55) } else { BORDER() };
            p.rect(r, rr(2.0), if resp.hovered() { ELEVATED() } else { chrome::FONDO_HONDO }, Stroke::new(1.0, borde), StrokeKind::Inside);
            p.circle_filled(r.left_center() + vec2(6.0, 0.0), 2.2, if on { ACCENT() } else { TEXT_DIM() });
            let g = p.layout_no_wrap(nombre, egui::FontId::proportional(10.0), if on { TEXT() } else { TEXT_DIM() });
            let recorte = Rect::from_min_max(r.min + vec2(11.0, 0.0), r.max - vec2(2.0, 0.0));
            p.with_clip_rect(recorte).galley(egui::pos2(recorte.left(), r.center().y - g.size().y / 2.0), g, TEXT());
        }
        None => _ = p.rect_filled(r, rr(2.0), chrome::FONDO_HONDO.gamma_multiply(0.6)),
    }
    resp
}

fn readout(ui: &mut egui::Ui, text: String, color: Color32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2((STRIP_W - 2.0) / 2.0, 16.0), Sense::click());
    ui.painter().rect_filled(r, rr(2.0), chrome::FONDO_HONDO);
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::monospace(10.0), color);
    resp
}

impl App {
    pub fn mixer(&mut self, ui: &mut egui::Ui) {
        let n = self.s.tracks.len();
        self.strip_rects.resize(n, Rect::NOTHING);
        // Distribución según la altura disponible: en paneles bajos se ocultan las ranuras de efectos y las
        // de envíos, grupo y automatización (están en la vista MIX), para que el fader conserve su alto.
        let h = ui.available_height();
        let compact = h < 480.0;
        // Las ranuras de efectos solo reservan lo que usa el canal con más efectos: el resto es del fader.
        let top_h = 16.0 + (FILA + 3.0) * if compact { 2.0 } else { SLOTS as f32 };
        // El fader ocupa todo el alto sobrante (se mide el resto del canal en el cuadro anterior).
        let fader_h = (h - self.strip_extra - 16.0).max(50.0);
        // El contenido se desplaza en vez de forzar la altura del panel: así se queda donde lo dejes.
        self.strip_measured = 0.0;
        // Fondo del mixer: menú contextual (se registra antes que los canales, que tienen prioridad).
        ui.interact(ui.available_rect_before_wrap(), egui::Id::new("mixer-bg"), Sense::click()).context_menu(|ui| {
            widgets::menu_style(ui);
            ui.label(RichText::new(tr("Mixer")).strong());
            if ui.button(tr("Nueva pista…")).clicked() {
                self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
            }
            if ui.button(tr(if self.mixer_float { "Acoplar mixer" } else { "Mixer flotante" })).clicked() {
                if self.mixer_float {
                    self.acoplar("mixer-win");
                } else {
                    self.mixer_float = true;
                }
            }
            if ui.button(tr("Reiniciar picos de todos los canales")).clicked() {
                self.peaks.iter_mut().for_each(|p| *p = 0.0);
            }
        });
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for i in 0..n {
                    self.strip(ui, Some(i), top_h, fader_h, compact);
                }
                if ui.add_sized([22.0, 22.0], egui::Button::new(RichText::new("+").size(14.0).color(TEXT_DIM())).fill(PANEL()).corner_radius(rr(2.0))).on_hover_text(tr("Nueva pista")).clicked() {
                    self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
                }
                ui.add_space(8.0);
                self.strip(ui, None, top_h, fader_h, compact);
            });
        });
        if self.strip_measured > 0.0 {
            self.strip_extra = self.strip_measured;
        }
        // Reordenar canales: destino según la posición horizontal del puntero.
        if let (Some(d), Some(p)) = (&mut self.strip_drag, ui.input(|i| i.pointer.interact_pos())) {
            d.1 = self.strip_rects.iter().filter(|r| r.center().x < p.x).count();
            let x = self.strip_rects.get(d.1).map_or_else(|| self.strip_rects.last().map_or(0.0, |r| r.right() + 3.0), |r| r.left() - 3.0);
            let y = self.strip_rects.first().map_or(0.0..=0.0, |r| r.top()..=r.bottom());
            ui.painter().vline(x, y, Stroke::new(3.0, ACCENT()));
        }
    }

    /// Canal (`None` = master), de arriba abajo: inserciones, entrada, salida, panorama, teclas, fader,
    /// lecturas, modo de automatización y el nombre sobre la raya (o el fondo) del color de la pista.
    fn strip(&mut self, ui: &mut egui::Ui, i: Option<usize>, top_h: f32, fader_h: f32, compact: bool) {
        let li = i.unwrap_or(self.s.tracks.len());
        let (level, peak) = (self.levels.get(li).copied().unwrap_or(0.0), self.peaks.get(li).copied().unwrap_or(0.0));
        let params = i.map(|i| self.s.tracks[i].params.clone());
        let selected = i.is_some_and(|i| self.s.tracks[i].selected);
        let color = i.map_or(ACCENT(), |i| self.s.tracks[i].color);
        let fader_h = if i.is_none() && !self.s.tracks.is_empty() { fader_h + (self.strip_extra - self.master_extra).max(0.0) } else { fader_h };
        if let Some(i) = i
            && let Some(prev) = self.strip_rects.get(i).copied().filter(|r| r.is_positive())
        {
            // Registrado antes que los controles: solo recibe los clics que caen en el fondo del canal.
            let fondo = ui.interact(prev, egui::Id::new(("strip-bg", i)), Sense::click());
            if fondo.clicked() {
                let m = ui.input(|i| i.modifiers);
                self.select_track(i, m);
            }
            fondo.context_menu(|ui| {
                widgets::menu_style(ui);
                self.track_menu(ui, i);
            });
        }
        let fondo = if selected { ELEVATED() } else { PANEL() };
        let frame = egui::Frame::new().fill(fondo).corner_radius(rr(2.0)).inner_margin(egui::Margin::symmetric(3, 3));
        let resp = frame.stroke(Stroke::new(1.0, if selected { ACCENT().gamma_multiply(0.5) } else { BORDER().gamma_multiply(0.6) })).show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            ui.vertical_centered(|ui| {
                let top = ui.scope(|ui| match i {
                    Some(i) if self.s.tracks[i].kind == TrackKind::Click => _ = caja(ui, tr("Metrónomo").to_string(), TEXT_DIM(), BG()),
                    Some(i) => self.fx_slots(ui, i, compact),
                    None => self.master_slots(ui, compact),
                });
                ui.add_space((top_h - top.response.rect.height()).max(0.0));
                // Fuente (entrada o instrumento) y salida (ruteo), como dos cajas iguales.
                match i {
                    Some(i) if self.s.tracks[i].midi() => {
                        let t = &self.s.tracks[i];
                        let name = match &t.plugin {
                            Some(p) => p.nombre().to_string(),
                            None if instruments::es_real(&t.instrument) => instruments::nombre(&t.instrument),
                            None => "QUANTUM Synth".to_string(),
                        };
                        if caja(ui, name, TEXT(), color.gamma_multiply(0.35)).on_hover_text(tr("Instrumento")).clicked() {
                            self.abrir_instrumento(i);
                        }
                    }
                    Some(i) if self.s.tracks[i].kind != TrackKind::Click => self.input_combo_ancho(ui, i, STRIP_W),
                    Some(_) => _ = caja(ui, "—".into(), TEXT_DIM(), BG()),
                    None => _ = caja(ui, tr("Salida").to_string(), TEXT_DIM(), BG()),
                }
                match i {
                    Some(i) => {
                        let t = &self.s.tracks[i];
                        let bus = t.sends.first().and_then(|(id, _)| self.s.tracks.iter().find(|b| b.id == *id)).map(|b| b.name.clone());
                        let salida = if t.to_master { tr("Master").to_string() } else { bus.unwrap_or_else(|| tr("Sin salida").to_string()) };
                        let n = t.sends.len();
                        let texto = if n > 0 && t.to_master { format!("{salida} +{n}") } else { salida };
                        if caja(ui, texto, SALIDA, BG()).on_hover_text(tr("Ruteo: salida, envíos y buses")).clicked() {
                            self.route_window = Some(i);
                        }
                    }
                    None => _ = caja(ui, tr("Altavoces").to_string(), SALIDA, BG()),
                }
                let mut touched = false;
                match &params {
                    Some(p) => {
                        let mut pan = p.pan.get();
                        let text = match (pan * 100.0).round() as i32 {
                            0 => "<C>".to_string(),
                            v if v < 0 => format!("L{}", -v),
                            v => format!("R{v}"),
                        };
                        let r = widgets::pan_h(ui, &mut pan, STRIP_W - 10.0).on_hover_text(tr("Panorama · arrastra · doble clic: centro"));
                        if let Some(i) = i {
                            crate::controlador::menu(&r, Pedido::Destino(Destino::Panorama(self.s.tracks[i].id)));
                        }
                        touched |= r.dragged();
                        p.pan.set(pan);
                        ui.label(RichText::new(text).size(10.0).color(if pan.abs() < 0.005 { TEXT_DIM() } else { ACCENT() }));
                    }
                    None => ui.add_space(37.0),
                }
                if let Some(i) = i {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let k = TECLA_MIX;
                        ui.add_space(((STRIP_W - 5.0 * k - 8.0) / 2.0).max(0.0));
                        self.mute_solo(ui, i);
                        self.polarity_key(ui, i);
                    });
                } else {
                    ui.add_space(TECLA_MIX);
                }
                let gain = params.as_ref().map_or(&self.engine.master.gain, |p| &p.gain);
                let r = widgets::fader(ui, gain, level, fader_h);
                crate::controlador::menu(&r, Pedido::Destino(i.map_or(Destino::VolumenMaster, |i| Destino::Volumen(self.s.tracks[i].id))));
                touched |= r.dragged();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    readout(ui, widgets::db_text(gain.get()), TEXT()).on_hover_text(tr("Volumen del fader"));
                    let c = if widgets::db(peak) > 0.0 { METER[2] } else { TEXT_DIM() };
                    if readout(ui, widgets::db_text(peak), c).on_hover_text(tr("Pico retenido · clic para reiniciar")).clicked() {
                        self.peaks[li] = 0.0;
                    }
                });
                match i {
                    Some(i) => {
                        self.s.tracks[i].touching |= touched;
                        self.modo_auto(ui, i);
                    }
                    None => _ = caja(ui, tr("Master").to_string(), TEXT_DIM(), BG()),
                }
                if let Some(g) = i.and_then(|i| self.s.tracks[i].group).and_then(|g| self.s.groups.get(g)) {
                    ui.label(RichText::new(&g.name).size(10.0).color(g.color));
                }
                // Abajo: número y nombre; el canal seleccionado se pinta entero del color de la pista.
                let (raya, _) = ui.allocate_exact_size(vec2(STRIP_W, 2.0), Sense::hover());
                ui.painter().rect_filled(raya, 0.0, color);
                let nombre = egui::Frame::new().fill(if selected { color.gamma_multiply(0.75) } else { Color32::TRANSPARENT }).inner_margin(egui::Margin::symmetric(3, 1));
                nombre.show(ui, |ui| {
                    ui.set_width(STRIP_W - 6.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        let tinta = if selected { Color32::WHITE } else { TEXT() };
                        match i {
                            Some(i) => {
                                ui.label(RichText::new(format!("{}", i + 1)).size(10.5).color(if selected { tinta } else { TEXT_DIM() }));
                                ui.add(egui::TextEdit::singleline(&mut self.s.tracks[i].name).frame(egui::Frame::new()).text_color(tinta).font(egui::FontId::proportional(11.5)).desired_width(STRIP_W - 22.0));
                            }
                            None => _ = ui.label(RichText::new(tr("Master")).size(11.5).color(TEXT())),
                        }
                    });
                });
                // Asa para seleccionar y reordenar: la franja del nombre.
                if let Some(i) = i {
                    let asa = Rect::from_min_size(raya.min - vec2(0.0, 1.0), vec2(STRIP_W, 5.0));
                    let g = ui.interact(asa, egui::Id::new(("strip-grip", i)), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Grab);
                    let g = g.on_hover_text(tr("Arrastra para reordenar · clic: seleccionar · clic derecho: opciones"));
                    if g.clicked() {
                        let m = ui.input(|i| i.modifiers);
                        self.select_track(i, m);
                    }
                    if g.drag_started() {
                        self.strip_drag = Some((i, i));
                    }
                    if g.drag_stopped()
                        && let Some((a, b)) = self.strip_drag.take()
                    {
                        self.move_track(a, b);
                    }
                }
            });
        });
        let extra = resp.response.rect.height() - fader_h;
        if i.is_some() || self.s.tracks.is_empty() {
            self.strip_measured = self.strip_measured.max(extra);
        }
        let Some(i) = i else {
            self.master_extra = extra;
            return;
        };
        let r = resp.response;
        self.strip_rects[i] = r.rect;
        if r.dnd_hover_payload::<LibItem>().is_some() {
            ui.painter().rect_stroke(r.rect, rr(4.0), Stroke::new(2.0, ACCENT()), StrokeKind::Inside);
        }
        if let Some(item) = r.dnd_release_payload::<LibItem>() {
            self.drop_item(&item, Some(i), self.pos());
        }
    }

    /// Tecla de polaridad (Ø) del tamaño de las de M, S, R e I.
    fn polarity_key(&mut self, ui: &mut egui::Ui, i: usize) {
        let inv = &self.s.tracks[i].params.invert;
        let on = inv.load(Relaxed);
        let resp = widgets::tecla(ui, vec2(TECLA_MIX, TECLA_MIX), on, METER[1], "Ø");
        if resp.on_hover_text(tr("Invertir polaridad (fase) del audio")).clicked() {
            inv.store(!on, Relaxed);
            self.dirty = true;
        }
    }

    /// Inserciones del master (su cadena de mastering): clic abre la ventana de mastering.
    fn master_slots(&mut self, ui: &mut egui::Ui, compact: bool) {
        cabecera_inserciones(ui);
        let filas = if compact { 2 } else { SLOTS };
        let mut abrir = false;
        for j in 0..filas {
            let fx = self.master_fx.get(j);
            abrir |= ranura(ui, fx.map(|f| (f.nombre().to_string(), !f.bypass.load(Relaxed)))).clicked();
        }
        if abrir || ui.interact(ui.min_rect(), egui::Id::new("master-ins"), Sense::click()).clicked() {
            self.mastering.abierta = true;
        }
    }

    /// Modo de automatización como en las consolas: «Auto: Lectura», con su menú.
    fn modo_auto(&mut self, ui: &mut egui::Ui, i: usize) {
        let modes: [(&str, &str, Color32); 5] = [
            ("Leer", "Lee la automatización", ACCENT()),
            ("Apagado", "Ignora la automatización", TEXT_DIM()),
            ("Toque", "Escribe mientras mueves el fader o el panorama", Color32::from_rgb(0xFF, 0xD2, 0x3F)),
            ("Retener", "Escribe desde que tocas un control hasta parar", METER[1]),
            ("Escribir", "Escribe siempre mientras se reproduce", METER[2]),
        ];
        let mode = self.s.tracks[i].auto_mode.min(4) as usize;
        let b = egui::Button::new(RichText::new(format!("Auto: {}", tr(modes[mode].0))).size(10.0).color(modes[mode].2))
            .fill(BG())
            .stroke(Stroke::new(1.0, modes[mode].2.gamma_multiply(0.55)))
            .corner_radius(rr(2.0))
            .min_size(vec2(STRIP_W, 18.0))
            .truncate();
        egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
            widgets::menu_style(ui);
            for (k, (name, tip, color)) in modes.iter().enumerate() {
                if ui.selectable_label(mode == k, RichText::new(tr(name)).color(*color)).on_hover_text(tr(tip)).clicked() {
                    self.s.tracks[i].auto_mode = k as u8;
                    ui.close();
                }
            }
            ui.separator();
            // Envíos y grupo también están aquí (antes ocupaban dos filas del canal).
            if ui.button(tr("Envíos y ruteo…")).clicked() {
                self.route_window = Some(i);
                ui.close();
            }
            ui.menu_button(tr("Grupo"), |ui| {
                widgets::menu_style(ui);
                for g in 0..self.s.groups.len() {
                    let label = RichText::new(&self.s.groups[g].name).color(self.s.groups[g].color);
                    if ui.selectable_label(self.s.tracks[i].group == Some(g), label).clicked() {
                        self.edit();
                        self.s.tracks[i].group = Some(g);
                    }
                }
                ui.separator();
                if ui.button(tr("Nuevo grupo con las seleccionadas")).clicked() {
                    self.s.tracks[i].selected = true;
                    self.group_selected();
                }
                if self.s.tracks[i].group.is_some() && ui.button(tr("Quitar del grupo")).clicked() {
                    self.edit();
                    self.s.tracks[i].group = None;
                }
            });
        });
    }

    /// Teclas M, S, R e I del mismo tamaño, cada una con su color.
    fn mute_solo(&mut self, ui: &mut egui::Ui, i: usize) {
        for (k, label, tip) in [(1, "M", "Mute (silenciar)"), (2, "S", "Solo"), (0, "", "Armar grabación"), (3, "I", "Monitoreo de entrada")] {
            let on = flag(&self.s.tracks[i].params, k).load(Relaxed);
            let lado = vec2(TECLA_MIX, TECLA_MIX);
            let tecla = if k == 0 {
                widgets::tecla_dibujo(ui, lado, on, widgets::color_bandera(k), |p, r, tinta| _ = p.circle_filled(r.center(), 3.5, if on { tinta } else { METER[2] }))
            } else {
                widgets::tecla(ui, lado, on, widgets::color_bandera(k), label)
            };
            if tecla.on_hover_text(tr(tip)).clicked() {
                for m in self.flag_targets(i, k) {
                    flag(&self.s.tracks[m].params, k).store(!on, Relaxed);
                }
            }
        }
    }

    /// Inserciones del canal: encabezado con «+», una ranura por efecto (clic abre, clic derecho bypass o
    /// quitar) y ranuras vacías hasta completar las filas que reserva el mixer.
    fn fx_slots(&mut self, ui: &mut egui::Ui, i: usize, compact: bool) {
        let (mut remove, mut add, mut add_lv2, mut abrir) = (None, None, None, false);
        ui.horizontal(|ui| {
            ui.set_width(STRIP_W);
            cabecera_inserciones(ui);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mas = ui.add(egui::Button::new(RichText::new("+").size(12.0).color(TEXT_DIM())).frame(false)).on_hover_text(tr("Agregar un efecto"));
                egui::Popup::menu(&mas).show(|ui| {
                    widgets::menu_style(ui);
                    if let Some(k) = crate::library::menu_efectos(ui) {
                        add = Some(k);
                    }
                    if let Some(info) = App::menu_efectos_lv2(ui) {
                        add_lv2 = Some(info);
                    }
                });
            });
        });
        let filas = if compact { 2 } else { SLOTS };
        let total = self.s.tracks[i].fx.len();
        for j in 0..filas {
            let fx = self.s.tracks[i].fx.get(j).cloned();
            let datos = fx.as_ref().map(|f| {
                let nombre = if j + 1 == filas && total > filas { format!("+{} más", total - j) } else { f.nombre().trim_start_matches("QUANTUM ").to_string() };
                (nombre, !f.bypass.load(Relaxed))
            });
            let b = ranura(ui, datos);
            if b.clicked() {
                abrir = true;
            }
            if let Some(fx) = fx {
                let on = !fx.bypass.load(Relaxed);
                b.on_hover_text(fx.nombre()).context_menu(|ui| {
                    widgets::menu_style(ui);
                    if ui.button(tr(if on { "Bypass" } else { "Activar" })).clicked() {
                        fx.bypass.store(on, Relaxed);
                    }
                    if ui.button(tr("Quitar")).clicked() {
                        remove = Some(j);
                    }
                });
            }
        }
        if let Some(info) = add_lv2 {
            self.add_fx_lv2(i, &info);
        }
        if let Some(j) = remove {
            self.edit();
            self.s.tracks[i].fx.remove(j);
        }
        if let Some(k) = add {
            self.add_fx(i, k);
            abrir = true;
        }
        if abrir {
            self.abrir_efectos(i);
        }
    }
}
