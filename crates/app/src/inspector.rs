//! Inspector (tecla I): panel a la izquierda con todo lo de la pista seleccionada: color y nombre,
//! volumen, panorama, botones R M S I, polaridad, efectos y ruteo, entrada y su ganancia, envíos,
//! instrumento y efectos. Las cabeceras de pista quedan compactas.
use crate::*;
use egui::{RichText, vec2};

/// Título de sección plegable del inspector (▾ PISTA).
fn seccion(ui: &mut egui::Ui, id: &str, texto: &str, add: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(RichText::new(tr(texto)).size(13.0).color(TEXT())).id_salt(("inspector", id)).default_open(true).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 8.0;
        add(ui);
    });
}

/// Columna de las etiquetas: todos los controles empiezan en el mismo lugar y llegan al mismo borde.
const ETIQUETA: f32 = 84.0;

/// Fila con etiqueta a la izquierda (columna fija) y el control ocupando el resto del ancho.
fn fila(ui: &mut egui::Ui, etiqueta: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(ETIQUETA, 26.0), egui::Sense::hover());
        ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, tr(etiqueta), egui::FontId::proportional(12.5), TEXT_DIM());
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 26.0), egui::Layout::left_to_right(egui::Align::Center), add);
    });
}

/// Valor a la derecha de un control, con ancho fijo para que todos los controles terminen igual.
fn valor(ui: &mut egui::Ui, texto: String) {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 20.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(RichText::new(texto).monospace().size(11.5).color(TEXT()));
    });
}

impl App {
    /// Pista que muestra el inspector: la primera seleccionada (o la primera pista).
    fn pista_inspector(&self) -> Option<usize> {
        self.s.tracks.iter().position(|t| t.selected).or((!self.s.tracks.is_empty()).then_some(0))
    }

    pub fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Inspector")).size(13.0).color(TEXT_DIM()));
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
            seccion(ui, "pista", "Pista", |ui| {
                ui.horizontal(|ui| {
                    let t = &mut self.s.tracks[i];
                    // Muestra de color del alto del nombre, y el nombre a lo ancho.
                    widgets::color_dot(ui, &mut t.color, 26.0);
                    ui.add(egui::TextEdit::singleline(&mut t.name).font(egui::FontId::proportional(14.0)).desired_width(f32::INFINITY).min_size(vec2(0.0, 26.0)).margin(egui::Margin::symmetric(8, 4)).vertical_align(egui::Align::Center));
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
                // Volumen con el medidor de la pista dentro del carril, y panorama a lo ancho.
                let nivel = self.levels.get(i).copied();
                let id = self.s.tracks[i].id;
                fila(ui, "Volumen", |ui| {
                    let f = widgets::fader_h(ui, &params.gain, ui.available_width() - 76.0, nivel);
                    crate::controlador::menu(&f, crate::controlador::Pedido::Destino(crate::controlador::Destino::Volumen(id)));
                    valor(ui, widgets::db_text(params.gain.get()));
                });
                fila(ui, "Panorama", |ui| {
                    let mut pan = params.pan.get();
                    let p = widgets::pan_h(ui, &mut pan, ui.available_width() - 76.0).on_hover_text(tr("Arrastra para mover · doble clic: centro"));
                    crate::controlador::menu(&p, crate::controlador::Pedido::Destino(crate::controlador::Destino::Panorama(id)));
                    if p.changed() {
                        params.pan.set(pan);
                    }
                    valor(ui, if pan.abs() < 0.01 { "C".to_string() } else if pan < 0.0 { format!("L {:.0}", -pan * 100.0) } else { format!("R {:.0}", pan * 100.0) });
                });
                fila(ui, "Pista", |ui| {
                    self.flag_buttons(ui, i);
                    ui.add_space(6.0);
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
                        valor(ui, format!("{d:+.1} dB"));
                    });
                    fila(ui, "Señal", |ui| {
                        let escucha = params.arm.load(Relaxed) || params.monitor.load(Relaxed);
                        widgets::meter_h(ui, if escucha { self.in_levels.get(i).copied().unwrap_or(0.0) } else { 0.0 }, ui.available_width());
                    });
                }
                fila(ui, "Monitoreo", |ui| {
                    let mut k = usize::from(params.monitor.load(Relaxed));
                    if widgets::segmentado(ui, &mut k, &["Apagado", "Encendido"]) {
                        params.monitor.store(k == 1, Relaxed);
                    }
                });
            });
            seccion(ui, "envios", "Envíos", |ui| {
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
                seccion(ui, "instrumento", "Instrumento", |ui| {
                    self.instrument_combo(ui, i);
                    let propio = !self.s.tracks[i].instrument.is_empty();
                    let r = ui.button(tr("Abrir instrumento")).on_hover_text(tr("Clic: abrir la ventana del instrumento · clic derecho: más opciones"));
                    if r.clicked() {
                        self.abrir_instrumento(i);
                    }
                    r.context_menu(|ui| {
                        widgets::menu_style(ui);
                        if ui.button(tr("Abrir instrumento")).clicked() {
                            self.abrir_instrumento(i);
                        }
                        if ui.add_enabled(propio, egui::Button::new(RichText::new(tr("Quitar instrumento (volver al QUANTUM Synth)")).color(METER[2]))).clicked() {
                            self.quitar_instrumento(i);
                        }
                    });
                });
            }
            seccion(ui, "efectos", "Inserciones", |ui| {
                let efectos: Vec<(String, Arc<Fx>)> = self.s.tracks[i].fx.iter().map(|f| (tr(f.nombre()).to_string(), f.clone())).collect();
                let mut quitar = None;
                // Ocho ranuras numeradas, como en una consola: encendido, nombre y, vacías, «Vacío».
                ui.spacing_mut().item_spacing.y = 2.0;
                for n in 0..efectos.len().max(8) {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::click());
                    let pt = ui.painter();
                    if resp.hovered() {
                        pt.rect_filled(r, widgets::rr(4.0), ELEVATED());
                    }
                    pt.text(r.left_center() + vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, (n + 1).to_string(), egui::FontId::proportional(11.5), TEXT_DIM());
                    let boton = egui::Rect::from_center_size(r.left_center() + vec2(28.0, 0.0), vec2(16.0, 16.0));
                    match efectos.get(n) {
                        Some((nombre, fx)) => {
                            let activo = !fx.bypass.load(Relaxed);
                            widgets::draw_encendido(pt, boton, if activo { ACCENT() } else { TEXT_DIM() });
                            pt.text(r.left_center() + vec2(44.0, 0.0), egui::Align2::LEFT_CENTER, nombre, egui::FontId::proportional(13.0), if activo { ACCENT() } else { TEXT_DIM() });
                            let en_boton = resp.interact_pointer_pos().is_some_and(|p| boton.expand(4.0).contains(p));
                            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Clic: abrir el efecto · botón: encender o apagar · clic derecho: más opciones"));
                            if resp.clicked() && en_boton {
                                fx.bypass.store(activo, Relaxed);
                            } else if resp.clicked() {
                                self.abrir_efectos(i);
                            }
                            resp.context_menu(|ui| {
                                widgets::menu_style(ui);
                                if ui.button(tr("Abrir efecto")).clicked() {
                                    self.abrir_efectos(i);
                                }
                                if ui.button(tr(if activo { "Desactivar (bypass)" } else { "Activar" })).clicked() {
                                    fx.bypass.store(activo, Relaxed);
                                }
                                ui.separator();
                                if ui.button(RichText::new(tr("Eliminar efecto")).color(METER[2])).clicked() {
                                    quitar = Some(n);
                                }
                            });
                        }
                        None => {
                            widgets::draw_encendido(pt, boton, BORDER());
                            let arrastrando = resp.dnd_hover_payload::<library::LibItem>().is_some();
                            let soltado = resp.dnd_release_payload::<library::LibItem>();
                            pt.text(r.left_center() + vec2(44.0, 0.0), egui::Align2::LEFT_CENTER, tr(if arrastrando { "Suelta aquí" } else { "Vacío" }), egui::FontId::proportional(12.5), if arrastrando { ACCENT() } else { TEXT_DIM() });
                            if resp.on_hover_text(tr("Clic: agregar un efecto · o arrastra uno desde la biblioteca")).clicked() {
                                self.abrir_efectos(i);
                            }
                            if let Some(item) = soltado {
                                let pos = self.pos();
                                self.drop_item(&item, Some(i), pos);
                            }
                        }
                    }
                }
                if let Some(j) = quitar {
                    self.edit();
                    self.s.tracks[i].fx.remove(j);
                    self.dirty = true;
                }
            });
        });
    }
}
