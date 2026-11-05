//! Consola de mezcla: canales angostos y planos (efectos arriba, ruteo, panorama, fader con sus
//! lecturas y el nombre abajo). Los canales se adaptan a la altura del panel.
use crate::controlador::{Destino, Pedido};
use crate::{
    library::LibItem,
    widgets::{self, rr},
    *,
};
use egui::{Rect, RichText, Sense, Stroke, StrokeKind, vec2};

const STRIP_W: f32 = 100.0;
const SLOTS: usize = 4;

/// Línea separadora fina entre secciones del canal.
fn sep(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(STRIP_W, 7.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(4.0), r.center().y, Stroke::new(1.0, BORDER()));
}

fn readout(ui: &mut egui::Ui, text: String, color: Color32) -> egui::Response {
    let frame = egui::Frame::new().fill(BG()).corner_radius(rr(5.0)).inner_margin(egui::Margin::symmetric(4, 2));
    frame
        .show(ui, |ui| {
            ui.set_width(42.0);
            ui.label(RichText::new(text).monospace().size(11.0).color(color));
        })
        .response
        .interact(Sense::click())
}

impl App {
    pub fn mixer(&mut self, ui: &mut egui::Ui) {
        let n = self.s.tracks.len();
        self.strip_rects.resize(n, Rect::NOTHING);
        // Distribución según la altura disponible: en paneles bajos se ocultan las ranuras de efectos y las
        // de envíos, grupo y automatización (están en la vista MIX), para que el fader conserve su alto.
        let h = ui.available_height();
        let compact = h < 640.0;
        // Las ranuras de efectos solo reservan lo que usa el canal con más efectos: el resto es del fader.
        let ranuras = self.s.tracks.iter().map(|t| t.fx.len().min(SLOTS) + usize::from(t.fx.len() > SLOTS)).max().unwrap_or(0);
        let top_h = if compact { 30.0 } else { 74.0 + 24.0 * ranuras as f32 };
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
                for i in 0..n {
                    self.strip(ui, Some(i), top_h, fader_h, compact);
                }
                if ui.add_sized([40.0, fader_h + 120.0], egui::Button::new(RichText::new("+").size(22.0).color(TEXT_DIM()))).on_hover_text(tr("Nueva pista")).clicked() {
                    self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
                }
                ui.add_space(12.0);
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

    /// Canal (`None` = master): asa, entrada/instrumento, efectos, panorama, lecturas, fader, botones y nombre.
    fn strip(&mut self, ui: &mut egui::Ui, i: Option<usize>, top_h: f32, fader_h: f32, compact: bool) {
        let li = i.unwrap_or(self.s.tracks.len());
        let (level, peak) = (self.levels.get(li).copied().unwrap_or(0.0), self.peaks.get(li).copied().unwrap_or(0.0));
        let params = i.map(|i| self.s.tracks[i].params.clone());
        let selected = i.is_some_and(|i| self.s.tracks[i].selected);
        let color = i.map_or(ACCENT(), |i| self.s.tracks[i].color);
        // El master no tiene las filas de botones de las pistas: su fader crece para igualar la altura.
        let fader_h = if i.is_none() && !self.s.tracks.is_empty() { fader_h + (self.strip_extra - self.master_extra).max(0.0) } else { fader_h };
        // Clic derecho en cualquier parte del canal (con el área del cuadro anterior, debajo de sus controles).
        if let Some(i) = i
            && let Some(prev) = self.strip_rects.get(i).copied().filter(|r| r.is_positive())
        {
            ui.interact(prev, egui::Id::new(("strip-bg", i)), Sense::click()).context_menu(|ui| {
                widgets::menu_style(ui);
                self.track_menu(ui, i);
            });
        }
        let frame = egui::Frame::new().fill(if selected { BORDER() } else { PANEL() }).corner_radius(rr(3.0)).inner_margin(egui::Margin::symmetric(4, 4));
        let resp = frame.stroke(Stroke::new(1.0, if selected { ACCENT().gamma_multiply(0.6) } else { BG() })).show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical_centered(|ui| {
                let sense = if i.is_some() { Sense::click_and_drag() } else { Sense::hover() };
                // Asa: una línea fina del color de la pista a todo el ancho.
                let (grip, g) = ui.allocate_exact_size(vec2(STRIP_W, 8.0), sense);
                ui.painter().rect_filled(Rect::from_min_size(grip.min, vec2(grip.width(), 3.0)), rr(1.0), color);
                if let Some(i) = i {
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
                    g.context_menu(|ui| {
                        widgets::menu_style(ui);
                        self.track_menu(ui, i);
                    });
                    icons::picker(ui, &mut self.s.tracks[i], 20.0);
                } else {
                    ui.add(welcome::logo(24.0));
                }
                let top = ui.scope(|ui| match i {
                    Some(i) if compact => {
                        if matches!(self.s.tracks[i].kind, TrackKind::AudioMono | TrackKind::AudioStereo) {
                            self.input_combo(ui, i);
                        }
                        self.fx_button(ui, i);
                    }
                    Some(i) if self.s.tracks[i].kind == TrackKind::Click => _ = ui.label(RichText::new(tr("Clic (metrónomo)")).size(11.0).color(TEXT_DIM())),
                    Some(i) if self.s.tracks[i].midi() => {
                        let t = &self.s.tracks[i];
                        let name = match &t.plugin {
                            Some(p) => p.nombre().to_string(),
                            None if instruments::es_real(&t.instrument) => instruments::nombre(&t.instrument),
                            None => "QUANTUM Synth".to_string(),
                        };
                        let b = egui::Button::new(RichText::new(name).size(11.0)).fill(color.gamma_multiply(0.5)).min_size(vec2(STRIP_W - 8.0, 20.0)).truncate();
                        if ui.add(b).on_hover_text(tr("Instrumento")).clicked() {
                            self.abrir_instrumento(i);
                        }
                        self.fx_slots(ui, i);
                    }
                    Some(i) => {
                        self.input_combo(ui, i);
                        self.fx_slots(ui, i);
                    }
                    None => _ = ui.label(RichText::new(tr("Salida")).size(11.0).color(TEXT_DIM())),
                });
                ui.add_space((top_h - top.response.rect.height()).max(0.0));
                sep(ui);
                // Envíos, grupo y modo de automatización, sobre el panorama.
                if let Some(i) = i.filter(|_| !compact) {
                    self.strip_slots(ui, i);
                    sep(ui);
                }
                let mut touched = false;
                match &params {
                    Some(p) => {
                        let mut pan = p.pan.get();
                        let text = match (pan * 100.0).round() as i32 {
                            0 => "C".to_string(),
                            v if v < 0 => format!("L{}", -v),
                            v => format!("R{v}"),
                        };
                        let r = widgets::pan_h(ui, &mut pan, STRIP_W - 12.0).on_hover_text(tr("Panorama · arrastra · doble clic: centro"));
                        if let Some(i) = i {
                            crate::controlador::menu(&r, Pedido::Destino(Destino::Panorama(self.s.tracks[i].id)));
                        }
                        touched |= r.dragged();
                        p.pan.set(pan);
                        ui.label(RichText::new(text).size(11.0).strong().color(if pan.abs() < 0.005 { TEXT_DIM() } else { ACCENT() }));
                    }
                    None => ui.add_space(40.0),
                }
                sep(ui);
                let gain = params.as_ref().map_or(&self.engine.master.gain, |p| &p.gain);
                let r = widgets::fader(ui, gain, level, fader_h);
                crate::controlador::menu(&r, Pedido::Destino(i.map_or(Destino::VolumenMaster, |i| Destino::Volumen(self.s.tracks[i].id))));
                touched |= r.dragged();
                // Volumen y pico retenido debajo del fader.
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    readout(ui, widgets::db_text(gain.get()), TEXT()).on_hover_text(tr("Volumen del fader"));
                    let c = if widgets::db(peak) > 0.0 { METER[2] } else { TEXT_DIM() };
                    if readout(ui, widgets::db_text(peak), c).on_hover_text(tr("Pico retenido · clic para reiniciar")).clicked() {
                        self.peaks[li] = 0.0;
                    }
                });
                sep(ui);
                if let Some(i) = i {
                    self.s.tracks[i].touching |= touched;
                    // M, S, R e I: cuatro teclas iguales que llenan el ancho del canal.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add_space((STRIP_W - 4.0 * widgets::TECLA - 12.0) / 2.0);
                        self.mute_solo(ui, i);
                    });
                    if let Some(g) = self.s.tracks[i].group.and_then(|g| self.s.groups.get(g)) {
                        ui.label(RichText::new(&g.name).size(11.0).color(g.color));
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add_space((STRIP_W - 4.0 * widgets::TECLA - 12.0) / 2.0);
                        self.polarity_button(ui, i);
                        self.route_button(ui, i);
                    });
                    sep(ui);
                }
                // Abajo: número y nombre, con una raya del color de la pista (como en las consolas).
                let (raya, _) = ui.allocate_exact_size(vec2(STRIP_W, 3.0), Sense::hover());
                ui.painter().rect_filled(raya, rr(1.0), color);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    match i {
                        Some(i) => {
                            ui.label(RichText::new(format!("{}", i + 1)).size(12.0).color(TEXT_DIM()));
                            ui.add(egui::TextEdit::singleline(&mut self.s.tracks[i].name).frame(egui::Frame::new()).text_color(TEXT()).font(egui::FontId::proportional(12.5)).desired_width(STRIP_W - 22.0));
                        }
                        None => _ = ui.label(RichText::new(tr("Master")).size(12.5).color(TEXT())),
                    }
                });
            });
        });
        // Se mide el canal de pista más alto (sin fader) y el master por separado.
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
            ui.painter().rect_stroke(r.rect, rr(10.0), Stroke::new(2.0, ACCENT()), StrokeKind::Inside);
        }
        if let Some(item) = r.dnd_release_payload::<LibItem>() {
            self.drop_item(&item, Some(i), self.pos());
        }
    }

    /// Teclas M, S, R e I del mismo tamaño, cada una con su color.
    fn mute_solo(&mut self, ui: &mut egui::Ui, i: usize) {
        for (k, label, tip) in [(1, "M", "Mute (silenciar)"), (2, "S", "Solo"), (0, "R", "Armar grabación"), (3, "I", "Monitoreo de entrada")] {
            let on = flag(&self.s.tracks[i].params, k).load(Relaxed);
            if widgets::tecla(ui, vec2(widgets::TECLA, widgets::TECLA), on, widgets::color_bandera(k), label).on_hover_text(tr(tip)).clicked() {
                for m in self.flag_targets(i, k) {
                    flag(&self.s.tracks[m].params, k).store(!on, Relaxed);
                }
            }
        }
    }

    /// Ranuras sobre el panorama: envíos (abre el ruteo), grupo y modo de automatización.
    fn strip_slots(&mut self, ui: &mut egui::Ui, i: usize) {
        let w = STRIP_W - 8.0;
        let slot = |text: String, color: Color32| egui::Button::new(RichText::new(text).size(11.0).color(color)).fill(BG()).min_size(vec2(w, 20.0)).truncate();
        // Envíos.
        let n = self.s.tracks[i].sends.len();
        let text = if n == 0 { tr("Envíos").to_string() } else { format!("{} · {n}", tr("Envíos")) };
        if ui.add(slot(text, if n > 0 { ACCENT() } else { TEXT_DIM() })).on_hover_text(tr("Envíos a buses: clic para abrir el ruteo")).clicked() {
            self.route_window = Some(i);
        }
        // Grupo.
        let group = self.s.tracks[i].group.and_then(|g| self.s.groups.get(g)).map(|g| (g.name.clone(), g.color));
        let (text, color) = group.clone().unwrap_or_else(|| (tr("Grupo").to_string(), TEXT_DIM()));
        egui::containers::menu::MenuButton::from_button(slot(text, color)).ui(ui, |ui| {
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
            if group.is_some() && ui.button(tr("Quitar del grupo")).clicked() {
                self.edit();
                self.s.tracks[i].group = None;
            }
        });
        // Modo de automatización (Lectura, Apagado, Toque, Retención, Escritura).
        let modes: [(&str, &str, Color32); 5] = [
            ("Lectura", "Lee la automatización", METER[0]),
            ("Apagado", "Ignora la automatización", TEXT_DIM()),
            ("Toque", "Escribe mientras mueves el fader o el panorama", Color32::from_rgb(0xFF, 0xD2, 0x3F)),
            ("Retención", "Escribe desde que tocas un control hasta parar", METER[1]),
            ("Escritura", "Escribe siempre mientras se reproduce", METER[2]),
        ];
        let mode = self.s.tracks[i].auto_mode.min(4) as usize;
        egui::containers::menu::MenuButton::from_button(slot(tr(modes[mode].0).to_string(), modes[mode].2)).ui(ui, |ui| {
            widgets::menu_style(ui);
            for (k, (name, tip, color)) in modes.iter().enumerate() {
                if ui.selectable_label(mode == k, RichText::new(tr(name)).color(*color)).on_hover_text(tr(tip)).clicked() {
                    self.s.tracks[i].auto_mode = k as u8;
                    ui.close();
                }
            }
        });
    }

    /// Ranuras de efectos: clic abre el plugin, clic derecho bypass/quitar, "+" añade.
    fn fx_slots(&mut self, ui: &mut egui::Ui, i: usize) {
        let (mut remove, mut add, mut add_lv2, mut abrir) = (None, None, None, false);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Inserciones")).size(11.0).color(TEXT_DIM()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button(RichText::new("+").size(13.0).color(TEXT_DIM()), |ui| {
                    widgets::menu_style(ui);
                    for k in FxKind::disponibles() {
                        if ui.button(k.name()).clicked() {
                            add = Some(k);
                        }
                    }
                    if let Some(info) = App::menu_efectos_lv2(ui) {
                        add_lv2 = Some(info);
                    }
                })
                .response
                .on_hover_text(tr("Agregar un efecto"));
            });
        });
        for (j, fx) in self.s.tracks[i].fx.iter().enumerate().take(SLOTS) {
            let on = !fx.bypass.load(Relaxed);
            let text = RichText::new(format!("• {}", fx.nombre())).size(10.5).color(if on { TEXT() } else { TEXT_DIM() });
            let b = ui.add(egui::Button::new(text).fill(chrome::FONDO_HONDO).stroke(Stroke::new(1.0, if on { ACCENT().gamma_multiply(0.5) } else { BORDER() })).corner_radius(rr(2.0)).min_size(vec2(STRIP_W - 8.0, 18.0)).truncate());
            if b.clicked() {
                abrir = true;
            }
            b.context_menu(|ui| {
                widgets::menu_style(ui);
                if ui.button(if on { "Bypass" } else { "Activar" }).clicked() {
                    fx.bypass.store(on, Relaxed);
                }
                if ui.button(tr("Quitar")).clicked() {
                    remove = Some(j);
                }
            });
        }
        let extra = self.s.tracks[i].fx.len().saturating_sub(SLOTS);
        if extra > 0 {
            ui.label(RichText::new(format!("+{extra} más")).size(10.0).color(TEXT_DIM()));
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
        // Tocar un efecto (o añadir uno) lo muestra abajo, en Dispositivos.
        if abrir {
            self.abrir_efectos(i);
        }
    }
}
