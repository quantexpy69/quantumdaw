//! Inspector (tecla I): panel a la izquierda con todo lo de la pista seleccionada: color y nombre,
//! volumen, panorama, botones R M S I, polaridad, efectos y ruteo, entrada y su ganancia, envíos,
//! instrumento y efectos. Las cabeceras de pista quedan compactas.
use crate::*;
use egui::{RichText, vec2};

/// Título de sección plegable del inspector (▾ PISTA).
fn seccion(ui: &mut egui::Ui, id: &str, texto: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(tr(texto)).size(11.5).strong().color(TEXT_DIM())).id_salt(("inspector", id)).default_open(true).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        add(ui);
    });
}

/// Fila con etiqueta a la izquierda (columna fija) y el control a la derecha.
fn fila(ui: &mut egui::Ui, etiqueta: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized([70.0, 20.0], egui::Label::new(RichText::new(tr(etiqueta)).size(12.5).color(TEXT_DIM())));
        add(ui);
    });
}

impl App {
    /// Pista que muestra el inspector: la primera seleccionada.
    fn pista_inspector(&self) -> Option<usize> {
        self.s.tracks.iter().position(|t| t.selected)
    }

    pub fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Inspector")).size(15.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("×").on_hover_text(tr("Ocultar (I)")).clicked() {
                    self.show_inspector = false;
                }
            });
        });
        let Some(i) = self.pista_inspector() else {
            ui.add_space(20.0);
            ui.label(RichText::new(tr("Selecciona una pista para ver y cambiar sus ajustes.")).color(TEXT_DIM()));
            return;
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            seccion(ui, "pista", "PISTA", |ui| {
                ui.horizontal(|ui| {
                    let t = &mut self.s.tracks[i];
                    widgets::color_dot(ui, &mut t.color);
                    ui.add(egui::TextEdit::singleline(&mut t.name).font(egui::FontId::proportional(14.5)).desired_width(f32::INFINITY));
                });
                let tipo = match self.s.tracks[i].kind {
                    TrackKind::AudioMono => tr("Pista de audio mono"),
                    TrackKind::AudioStereo => tr("Pista de audio estéreo"),
                    TrackKind::Midi => tr("Pista MIDI"),
                    TrackKind::Click => tr("Pista de clic"),
                    TrackKind::Video => tr("Pista de video"),
                };
                ui.label(RichText::new(tipo).size(12.0).color(TEXT_DIM()));
                let params = self.s.tracks[i].params.clone();
                fila(ui, "Volumen", |ui| {
                    widgets::fader_h(ui, &params.gain, 120.0);
                    ui.label(RichText::new(widgets::db_text(params.gain.get())).monospace().size(11.5));
                });
                fila(ui, "Panorama", |ui| {
                    let mut pan = params.pan.get();
                    if widgets::pan_knob(ui, &mut pan).changed() {
                        params.pan.set(pan);
                    }
                    let lado = if pan.abs() < 0.01 { "C".to_string() } else if pan < 0.0 { format!("L {:.0}", -pan * 100.0) } else { format!("R {:.0}", pan * 100.0) };
                    ui.label(RichText::new(lado).monospace().size(11.5));
                });
                ui.horizontal(|ui| {
                    ui.add_space(74.0);
                    self.flag_buttons(ui, i);
                });
                ui.horizontal(|ui| {
                    ui.add_space(74.0);
                    self.polarity_button(ui, i);
                    self.fx_button(ui, i);
                    self.route_button(ui, i);
                });
                if !self.s.tracks[i].midi() && matches!(self.s.tracks[i].kind, TrackKind::AudioMono | TrackKind::AudioStereo) {
                    fila(ui, "Entrada", |ui| self.input_combo(ui, i));
                    fila(ui, "Ganancia", |ui| {
                        let mut d = widgets::db(params.in_gain.get());
                        if widgets::mini_knob(ui, &mut d, -24.0, 24.0, 0.0, METER[0]).changed() {
                            params.in_gain.set(10f32.powf(d / 20.0));
                        }
                        ui.label(RichText::new(format!("{d:+.1} dB")).monospace().size(11.5));
                    });
                    fila(ui, "Señal", |ui| {
                        let escucha = params.arm.load(Relaxed) || params.monitor.load(Relaxed);
                        widgets::meter_h(ui, if escucha { self.in_levels.get(i).copied().unwrap_or(0.0) } else { 0.0 }, 150.0);
                    });
                }
                fila(ui, "Monitoreo", |ui| {
                    let mut on = params.monitor.load(Relaxed);
                    ui.selectable_value(&mut on, false, tr("Apagado"));
                    ui.selectable_value(&mut on, true, tr("Encendido"));
                    params.monitor.store(on, Relaxed);
                });
            });
            seccion(ui, "envios", "ENVÍOS", |ui| {
                let envios = self.s.tracks[i].sends.clone();
                if envios.is_empty() {
                    ui.label(RichText::new(tr("Sin envíos.")).size(12.0).color(TEXT_DIM()));
                }
                for (k, (destino, gan)) in envios.iter().enumerate() {
                    let nombre = self.s.tracks.iter().find(|t| t.id == *destino).map_or("?".to_string(), |t| t.name.clone());
                    let mut d = widgets::db(*gan);
                    fila(ui, &nombre, |ui| {
                        if ui.add(egui::Slider::new(&mut d, -60.0..=6.0).suffix(" dB").show_value(true)).changed() {
                            self.s.tracks[i].sends[k].1 = 10f32.powf(d / 20.0);
                            self.dirty = true;
                        }
                    });
                }
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), egui::Sense::click());
                ui.painter().rect_stroke(r, widgets::rr(6.0), egui::Stroke::new(1.0, if resp.hovered() { ACCENT() } else { BORDER() }), egui::StrokeKind::Inside);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, format!("+ {}", tr("Envío")), egui::FontId::proportional(13.0), TEXT());
                if resp.clicked() {
                    self.route_window = Some(i);
                }
            });
            if self.s.tracks[i].midi() {
                seccion(ui, "instrumento", "INSTRUMENTO", |ui| {
                    self.instrument_combo(ui, i);
                    if ui.button(tr("Ver instrumento")).clicked() {
                        self.roll.instrumento = true;
                    }
                });
            }
            seccion(ui, "efectos", "EFECTOS", |ui| {
                let efectos: Vec<(String, Arc<Fx>)> = self.s.tracks[i].fx.iter().map(|f| (tr(f.kind.name()).to_string(), f.clone())).collect();
                if efectos.is_empty() {
                    ui.label(RichText::new(tr("Sin efectos. Arrastra uno desde la biblioteca.")).size(12.0).color(TEXT_DIM()));
                }
                for (nombre, fx) in efectos {
                    ui.horizontal(|ui| {
                        let mut activo = !fx.bypass.load(Relaxed);
                        if ui.checkbox(&mut activo, "").changed() {
                            fx.bypass.store(!activo, Relaxed);
                        }
                        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.0, plugins::kind_color(Some(fx.kind)));
                        ui.label(nombre);
                    });
                }
                if ui.button(tr("Abrir efectos de la pista…")).clicked() {
                    self.fx_window = Some(i);
                }
            });
        });
    }
}
