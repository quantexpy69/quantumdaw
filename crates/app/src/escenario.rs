//! Escenario de clips (Clip Stage): una cuadrícula de pistas × escenas. Cada celda guarda una región;
//! al lanzarla suena en bucle en su pista desde el próximo compás (reemplazando al arreglo de esa
//! pista) y una escena lanza su columna entera. Las escenas se llenan con lo que suena en el cursor o
//! con la región seleccionada, y se pasan al arreglo cuando la idea está lista.
use crate::*;
use egui::{Align2, FontId, RichText, Sense, Stroke, pos2, vec2};

pub const ESCENAS: usize = 8;

impl App {
    /// Inicio del próximo compás (o el cursor si el transporte está parado, y arranca).
    fn proximo_compas(&mut self) -> u64 {
        let pos = self.pos();
        if !self.playing() {
            self.toggle_play();
            return pos;
        }
        let compas = self.bar_frames().max(1);
        pos.div_ceil(compas) * compas
    }

    /// Lanza la celda `k` de la pista `i` (o la detiene si ya suena).
    pub fn lanzar(&mut self, i: usize, k: usize) {
        if self.s.tracks[i].escenario.get(k).is_none_or(|c| c.is_none()) {
            return;
        }
        if self.s.tracks[i].lanzado.is_some_and(|l| l.0 == k) {
            self.s.tracks[i].lanzado = None;
        } else {
            let inicio = self.proximo_compas();
            self.s.tracks[i].lanzado = Some((k, inicio));
        }
        self.dirty = true;
    }

    /// Lanza la escena `k` entera: las pistas sin clip en ella se detienen.
    pub fn lanzar_escena(&mut self, k: usize) {
        let inicio = self.proximo_compas();
        for t in self.s.tracks.iter_mut() {
            t.lanzado = t.escenario.get(k).cloned().flatten().map(|_| (k, inicio));
        }
        self.dirty = true;
    }

    pub fn detener_escenario(&mut self) {
        self.s.tracks.iter_mut().for_each(|t| t.lanzado = None);
        self.dirty = true;
    }

    /// Llena la escena con lo que suena bajo el cursor en el arreglo (una región por pista).
    fn capturar_escena(&mut self, k: usize) {
        let pos = self.pos();
        self.edit();
        for t in self.s.tracks.iter_mut() {
            if let Some(c) = t.clips.iter().find(|c| c.start <= pos && pos < c.end()) {
                t.escenario[k] = Some(Clip { selected: false, ..c.clone() });
            }
        }
    }

    /// Pasa la escena al arreglo, en el cursor.
    fn escena_al_arreglo(&mut self, k: usize) {
        let pos = self.pos();
        self.edit();
        for t in self.s.tracks.iter_mut() {
            if let Some(c) = t.escenario.get(k).cloned().flatten() {
                t.clips.push(Clip { start: pos, selected: false, ..c });
            }
        }
    }

    /// Vista del escenario (reemplaza al arreglo mientras está activa).
    pub fn vista_escenario(&mut self, ui: &mut egui::Ui) {
        let rr = widgets::rr;
        let (cab, ancho, alto) = (210.0, ((ui.available_width() - 210.0 - 20.0) / ESCENAS as f32).clamp(90.0, 160.0), 54.0);
        let pos = self.pos();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            // Cabecera de escenas.
            ui.horizontal(|ui| {
                ui.add_space(cab);
                for k in 0..ESCENAS {
                    let (r, resp) = ui.allocate_exact_size(vec2(ancho, 30.0), Sense::click());
                    ui.painter().rect_filled(r, rr(6.0), if resp.hovered() { BORDER() } else { ELEVATED() });
                    widgets::draw_play(ui.painter(), egui::Rect::from_center_size(r.left_center() + vec2(14.0, 0.0), vec2(8.0, 10.0)), ACCENT());
                    ui.painter().text(r.left_center() + vec2(26.0, 0.0), Align2::LEFT_CENTER, format!("{} {}", tr("Escena"), k + 1), FontId::proportional(12.5), TEXT());
                    if resp.clicked() {
                        self.lanzar_escena(k);
                    }
                    resp.on_hover_text(tr("Clic: lanzar la escena · clic derecho: más opciones")).context_menu(|ui| {
                        widgets::menu_style(ui);
                        if ui.button(tr("Capturar lo que suena en el cursor")).clicked() {
                            self.capturar_escena(k);
                        }
                        if ui.button(tr("Pasar la escena al arreglo (en el cursor)")).clicked() {
                            self.escena_al_arreglo(k);
                        }
                        if ui.button(tr("Vaciar la escena")).clicked() {
                            self.edit();
                            self.s.tracks.iter_mut().for_each(|t| t.escenario[k] = None);
                        }
                    });
                }
            });
            for i in 0..self.s.tracks.len() {
                ui.horizontal(|ui| {
                    // Cabecera de la pista: color, nombre y parar.
                    let (r, _) = ui.allocate_exact_size(vec2(cab - 6.0, alto), Sense::hover());
                    let t = &self.s.tracks[i];
                    ui.painter().rect_filled(r, rr(6.0), ELEVATED());
                    ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(5.0, r.height())), rr(3.0), t.color);
                    ui.painter().text(r.left_center() + vec2(16.0, -8.0), Align2::LEFT_CENTER, &t.name, FontId::proportional(13.5), TEXT());
                    let estado = if t.lanzado.is_some() { tr("sonando desde el escenario") } else { tr("arreglo") };
                    ui.painter().text(r.left_center() + vec2(16.0, 10.0), Align2::LEFT_CENTER, estado, FontId::proportional(10.5), if t.lanzado.is_some() { ACCENT() } else { TEXT_DIM() });
                    let parar = egui::Rect::from_center_size(r.right_center() - vec2(18.0, 0.0), vec2(22.0, 22.0));
                    let rp = ui.interact(parar, egui::Id::new(("parar-pista", i)), Sense::click());
                    ui.painter().rect_filled(parar, rr(4.0), if rp.hovered() { BORDER() } else { PANEL() });
                    ui.painter().rect_filled(egui::Rect::from_center_size(parar.center(), vec2(8.0, 8.0)), 1.0, TEXT_DIM());
                    if rp.on_hover_text(tr("Detener el escenario en esta pista (vuelve el arreglo)")).clicked() {
                        self.s.tracks[i].lanzado = None;
                        self.dirty = true;
                    }
                    for k in 0..ESCENAS {
                        let (r, resp) = ui.allocate_exact_size(vec2(ancho, alto), Sense::click());
                        let t = &self.s.tracks[i];
                        match &t.escenario[k] {
                            Some(c) => {
                                let sonando = t.lanzado.is_some_and(|l| l.0 == k);
                                ui.painter().rect_filled(r, rr(6.0), t.color.lerp_to_gamma(BG(), if sonando { 0.35 } else { 0.55 }));
                                ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(r.width(), 16.0)), egui::CornerRadius { nw: rr(6.0) as u8, ne: rr(6.0) as u8, sw: 0, se: 0 }, t.color);
                                let nombre = if c.notes().is_some() { format!("{} · MIDI", t.name) } else { t.name.clone() };
                                ui.painter().with_clip_rect(r).text(r.left_top() + vec2(6.0, 8.0), Align2::LEFT_CENTER, nombre, FontId::proportional(10.5), Color32::WHITE);
                                widgets::draw_play(ui.painter(), egui::Rect::from_center_size(r.left_center() + vec2(14.0, 8.0), vec2(9.0, 11.0)), if sonando { Color32::WHITE } else { Color32::from_white_alpha(170) });
                                let compases = c.len as f64 / self.bar_frames().max(1) as f64;
                                ui.painter().text(r.right_bottom() - vec2(6.0, 8.0), Align2::RIGHT_CENTER, format!("{compases:.0} {}", tr("comp.")), FontId::proportional(10.0), Color32::from_white_alpha(190));
                                if sonando && let Some((_, inicio)) = t.lanzado {
                                    // Avance dentro del bucle y borde del acento.
                                    let fr = ((pos.saturating_sub(inicio)) % c.len.max(1)) as f32 / c.len.max(1) as f32;
                                    ui.painter().rect_filled(egui::Rect::from_min_size(pos2(r.left(), r.bottom() - 3.0), vec2(r.width() * if pos >= inicio { fr } else { 0.0 }, 3.0)), 1.0, Color32::WHITE);
                                    ui.painter().rect_stroke(r, rr(6.0), Stroke::new(2.0, ACCENT()), egui::StrokeKind::Inside);
                                    ui.ctx().request_repaint();
                                }
                                if resp.clicked() {
                                    self.lanzar(i, k);
                                }
                            }
                            None => {
                                ui.painter().rect_stroke(r, rr(6.0), Stroke::new(1.0, if resp.hovered() { BORDER() } else { ELEVATED() }), egui::StrokeKind::Inside);
                                if resp.hovered() {
                                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "+", FontId::proportional(16.0), TEXT_DIM());
                                }
                            }
                        }
                        resp.context_menu(|ui| {
                            widgets::menu_style(ui);
                            let seleccionada = self.s.tracks[i].clips.iter().find(|c| c.selected).cloned();
                            if ui.add_enabled(seleccionada.is_some(), egui::Button::new(tr("Copiar aquí la región seleccionada de esta pista"))).clicked() {
                                self.edit();
                                self.s.tracks[i].escenario[k] = seleccionada.map(|c| Clip { selected: false, ..c });
                            }
                            if self.s.tracks[i].midi() && ui.button(tr("Nuevo clip MIDI vacío (1 compás)")).clicked() {
                                self.edit();
                                let compas = self.bar_frames();
                                self.s.tracks[i].escenario[k] = Some(Clip::new(Source::Midi(Arc::new(vec![])), 0, compas));
                            }
                            if self.s.tracks[i].escenario[k].is_some() {
                                if ui.button(tr("Pasar al arreglo (en el cursor)")).clicked() {
                                    let pos = self.pos();
                                    self.edit();
                                    if let Some(c) = self.s.tracks[i].escenario[k].clone() {
                                        self.s.tracks[i].clips.push(Clip { start: pos, ..c });
                                    }
                                }
                                if ui.button(RichText::new(tr("Vaciar la celda")).color(METER[2])).clicked() {
                                    self.edit();
                                    self.s.tracks[i].escenario[k] = None;
                                    if self.s.tracks[i].lanzado.is_some_and(|l| l.0 == k) {
                                        self.s.tracks[i].lanzado = None;
                                    }
                                }
                            }
                        });
                    }
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(cab);
                if ui.button(tr("Detener todo el escenario")).clicked() {
                    self.detener_escenario();
                }
                ui.label(RichText::new(tr("Clic derecho en una escena: capturar lo que suena en el cursor o pasarla al arreglo. Clic derecho en una celda: copiar la región seleccionada.")).size(11.0).color(TEXT_DIM()));
            });
        });
    }
}
