//! Consola de mezcla al estilo Logic. Los canales se adaptan a la altura del panel.
use crate::{
    library::LibItem,
    widgets::{self, rr},
    *,
};
use egui::{Rect, RichText, Sense, Stroke, StrokeKind, vec2};

const STRIP_W: f32 = 112.0;
const SLOTS: usize = 4;

/// Línea separadora fina entre secciones del canal.
fn sep(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(STRIP_W, 7.0), Sense::hover());
    ui.painter().hline(r.x_range().shrink(4.0), r.center().y, Stroke::new(1.0, BORDER));
}

fn readout(ui: &mut egui::Ui, text: String, color: Color32) -> egui::Response {
    let frame = egui::Frame::new().fill(BG).corner_radius(rr(5.0)).inner_margin(egui::Margin::symmetric(4, 2));
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
        // Distribución según la altura disponible: en paneles bajos se ocultan las ranuras de efectos.
        let h = ui.available_height();
        let compact = h < 430.0;
        let top_h = if compact { 30.0 } else { 150.0 };
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
            if ui.button(if self.mixer_float { "Acoplar mixer" } else { "Mixer flotante" }).clicked() {
                self.mixer_float = !self.mixer_float;
            }
            if ui.button(tr("Reiniciar picos de todos los canales")).clicked() {
                self.peaks.iter_mut().for_each(|p| *p = 0.0);
            }
        });
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                for i in 0..n {
                    if self.s.tracks[i].kind != TrackKind::Video {
                        self.strip(ui, Some(i), top_h, fader_h, compact);
                    }
                }
                if ui.add_sized([40.0, fader_h + 120.0], egui::Button::new(RichText::new("+").size(22.0).color(TEXT_DIM))).on_hover_text(tr("Nueva pista")).clicked() {
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
            ui.painter().vline(x, y, Stroke::new(3.0, ACCENT));
        }
    }

    /// Canal (`None` = master): asa, entrada/instrumento, efectos, panorama, lecturas, fader, botones y nombre.
    fn strip(&mut self, ui: &mut egui::Ui, i: Option<usize>, top_h: f32, fader_h: f32, compact: bool) {
        let li = i.unwrap_or(self.s.tracks.len());
        let (level, peak) = (self.levels.get(li).copied().unwrap_or(0.0), self.peaks.get(li).copied().unwrap_or(0.0));
        let params = i.map(|i| self.s.tracks[i].params.clone());
        let selected = i.is_some_and(|i| self.s.tracks[i].selected);
        let color = i.map_or(ACCENT, |i| self.s.tracks[i].color);
        // Clic derecho en cualquier parte del canal (con el área del cuadro anterior, debajo de sus controles).
        if let Some(i) = i
            && let Some(prev) = self.strip_rects.get(i).copied().filter(|r| r.is_positive())
        {
            ui.interact(prev, egui::Id::new(("strip-bg", i)), Sense::click()).context_menu(|ui| {
                widgets::menu_style(ui);
                self.track_menu(ui, i);
            });
        }
        let frame = egui::Frame::new().fill(if selected { BORDER } else { ELEVATED }).corner_radius(rr(10.0)).inner_margin(6.0);
        let resp = frame.stroke(Stroke::new(1.0, if selected { ACCENT } else { BORDER })).show(ui, |ui| {
            ui.set_width(STRIP_W);
            ui.vertical_centered(|ui| {
                let sense = if i.is_some() { Sense::click_and_drag() } else { Sense::hover() };
                let (grip, g) = ui.allocate_exact_size(vec2(STRIP_W, 10.0), sense);
                ui.painter().rect_filled(grip.shrink2(vec2(34.0, 2.0)), rr(3.0), color);
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
                    icons::picker(ui, &mut self.s.tracks[i], 26.0);
                } else {
                    ui.add(welcome::logo(26.0));
                }
                let top = ui.scope(|ui| match i {
                    Some(i) if compact => self.fx_button(ui, i),
                    Some(i) if self.s.tracks[i].kind == TrackKind::Click => _ = ui.label(RichText::new(tr("Clic (metrónomo)")).size(11.0).color(TEXT_DIM)),
                    Some(i) if self.s.tracks[i].midi() => {
                        let t = &self.s.tracks[i];
                        let name = instruments::find(&t.instrument).map_or("QUANTUM Synth", |e| e.name);
                        let b = egui::Button::new(RichText::new(name).size(11.0)).fill(color.gamma_multiply(0.5)).min_size(vec2(STRIP_W - 8.0, 20.0)).truncate();
                        if ui.add(b).on_hover_text(tr("Instrumento")).clicked() {
                            self.fx_window = Some(i);
                        }
                        self.fx_slots(ui, i);
                    }
                    Some(i) => {
                        self.input_combo(ui, i);
                        self.fx_slots(ui, i);
                    }
                    None => _ = ui.label(RichText::new(tr("Salida")).size(11.0).color(TEXT_DIM)),
                });
                ui.add_space((top_h - top.response.rect.height()).max(0.0));
                sep(ui);
                match &params {
                    Some(p) => {
                        let mut pan = p.pan.get();
                        let text = match (pan * 100.0).round() as i32 {
                            0 => "C".to_string(),
                            v if v < 0 => format!("L{}", -v),
                            v => format!("R{v}"),
                        };
                        widgets::pan_knob(ui, &mut pan).on_hover_text(tr("Panorama · arrastra arriba/abajo · doble clic: centro"));
                        p.pan.set(pan);
                        ui.label(RichText::new(text).size(11.0).strong().color(if pan.abs() < 0.005 { TEXT_DIM } else { ACCENT }));
                    }
                    None => ui.add_space(68.0),
                }
                sep(ui);
                let gain = params.as_ref().map_or(&self.engine.master.gain, |p| &p.gain);
                ui.horizontal(|ui| {
                    ui.add_space(6.0);
                    readout(ui, widgets::db_text(gain.get()), TEXT).on_hover_text(tr("Volumen del fader"));
                    let c = if widgets::db(peak) > 0.0 { METER[2] } else { METER[0] };
                    if readout(ui, widgets::db_text(peak), c).on_hover_text(tr("Pico retenido · clic para reiniciar")).clicked() {
                        self.peaks[li] = 0.0;
                    }
                });
                widgets::fader(ui, gain, level, fader_h);
                sep(ui);
                if let Some(i) = i {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        self.flag_buttons(ui, i);
                    });
                    let t = &self.s.tracks[i];
                    let group = t.group.and_then(|g| self.s.groups.get(g));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}", i + 1)).monospace().strong().color(TEXT_DIM));
                        ui.label(group.map_or(RichText::new(" "), |g| RichText::new(&g.name).color(g.color)).size(11.0));
                    });
                    ui.horizontal(|ui| {
                        self.polarity_button(ui, i);
                        self.route_button(ui, i);
                    });
                    sep(ui);
                }
                let name = egui::Frame::new().fill(color.gamma_multiply(0.85)).corner_radius(rr(8.0)).inner_margin(egui::Margin::symmetric(4, 2));
                name.show(ui, |ui| match i {
                    Some(i) => {
                        _ = ui.add(egui::TextEdit::singleline(&mut self.s.tracks[i].name).frame(egui::Frame::new()).text_color(BG).horizontal_align(egui::Align::Center).desired_width(STRIP_W - 10.0))
                    }
                    None => {
                        ui.set_width(STRIP_W - 10.0);
                        ui.label(RichText::new(tr("Stereo Out")).strong().color(BG));
                    }
                });
            });
        });
        // Se mide el canal más alto (los de pista tienen más controles que el master).
        self.strip_measured = self.strip_measured.max(resp.response.rect.height() - fader_h);
        let Some(i) = i else { return };
        let r = resp.response;
        self.strip_rects[i] = r.rect;
        if r.dnd_hover_payload::<LibItem>().is_some() {
            ui.painter().rect_stroke(r.rect, rr(10.0), Stroke::new(2.0, ACCENT), StrokeKind::Inside);
        }
        if let Some(item) = r.dnd_release_payload::<LibItem>() {
            self.drop_item(&item, Some(i), self.pos());
        }
    }

    /// Ranuras de efectos: clic abre el plugin, clic derecho bypass/quitar, "+" añade.
    fn fx_slots(&mut self, ui: &mut egui::Ui, i: usize) {
        ui.label(RichText::new(tr("AUDIO FX")).size(9.0).color(TEXT_DIM));
        let (mut remove, mut add) = (None, None);
        for (j, fx) in self.s.tracks[i].fx.iter().enumerate().take(SLOTS) {
            let on = !fx.bypass.load(Relaxed);
            let text = RichText::new(fx.kind.name()).size(11.0).color(if on { TEXT } else { TEXT_DIM });
            let b = ui.add(egui::Button::new(text).fill(if on { ACCENT.gamma_multiply(0.4) } else { BG }).min_size(vec2(STRIP_W - 8.0, 20.0)));
            if b.clicked() {
                self.fx_window = Some(i);
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
            ui.label(RichText::new(format!("+{extra} más")).size(10.0).color(TEXT_DIM));
        }
        ui.menu_button(RichText::new(tr("+ efecto")).size(11.0).color(TEXT_DIM), |ui| {
            for k in FxKind::ALL {
                if ui.button(k.name()).clicked() {
                    add = Some(k);
                }
            }
            ui.separator();
            if ui.button(tr("Agregar mis efectos…")).on_hover_text(tr("Plugins VST3 / LV2 / CLAP instalados")).clicked() {
                (self.show_lib, self.lib_tab) = (true, library::PLUGINS_TAB);
            }
        });
        if let Some(j) = remove {
            self.edit();
            self.s.tracks[i].fx.remove(j);
        }
        if let Some(k) = add {
            self.add_fx(i, k);
        }
    }
}
