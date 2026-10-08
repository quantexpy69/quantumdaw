//! Timeline: regla, cabeceras, regiones (audio/MIDI), fades, tomas (comping), razor, automatización
//! y edición de muestras con zoom profundo.
use crate::{keys::Wheel, library::LibItem, widgets::rr, *};
use egui::{Align2, CornerRadius, CursorIcon, FontId, Id, Mesh, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};
use engine::{PEAK_BLOCK, fader_pos};

pub const HEADER_W: f32 = 300.0;
const RULER_H: f32 = 30.0;
const TITLE_H: f32 = 16.0;
const TEMPO_H: f32 = 22.0;
const MARK_H: f32 = 22.0;
const MARK: Color32 = Color32::from_rgb(0xFF, 0xC8, 0x57);
/// Columna de grupos (barra vertical con el nombre, como en Ardour).
const GROUP_W: f32 = 20.0;
const TAKE_H: f32 = 38.0;
const MIN_H: f32 = 44.0;
const MAX_H: f32 = 320.0;

pub enum Drag {
    Loop(u64),
    /// Selección de tiempo (razor): inicio y pista donde empezó.
    Range(u64, usize),
    /// Clips movidos: (pista, clip, inicio original), desplazamiento acumulado y clip de referencia.
    Clips(Vec<(usize, usize, u64)>, f64, u64),
    /// Tirador de fade: pista, clip y si es el de salida.
    Fade(usize, usize, bool),
    /// Borde de región: pista, clip, borde izquierdo, estirar (Alt) y clip original.
    Edge(usize, usize, bool, bool, Clip),
    /// Swipe comping: pista, toma, inicio y fin del barrido.
    Swipe(usize, usize, u64, u64),
    /// Mover el contenido del razor: desplazamiento acumulado en frames.
    RazorMove(f64),
    /// Tramo elegido en el carril de tempo (inicio, fin).
    Tempo(u64, u64),
}

/// Zona de una región bajo el puntero.
#[derive(Clone, Copy, PartialEq)]
enum Zone {
    Body,
    FadeIn,
    FadeOut,
    Left,
    Right,
    /// Esquina inferior derecha: estirar en el tiempo (time-stretch manual).
    Stretch,
}

/// Forma de onda resumida (picos por bloque) en un único mesh.
fn peak_mesh(c: &Clip, b: &AudioBuf, r: Rect, xr: (f32, f32), fpp: f64, color: Color32) -> Mesh {
    let (mid, half) = (r.center().y, r.height() / 2.0);
    let mut mesh = Mesh::default();
    for x in (r.left().max(xr.0) as i32)..(r.right().min(xr.1) as i32) {
        let f0 = c.offset as f64 + (x as f32 - r.left()) as f64 * fpp;
        let (b0, b1) = (f0 as usize / PEAK_BLOCK, (f0 + fpp) as usize / PEAK_BLOCK + 1);
        let peak = b.peaks.get(b0..b1.min(b.peaks.len())).map_or(0.0, |s| s.iter().fold(0f32, |a, &b| a.max(b))) * c.gain;
        let h = (peak * half).clamp(0.5, half);
        mesh.add_colored_rect(Rect::from_x_y_ranges(x as f32..=x as f32 + 1.0, mid - h..=mid + h), color);
    }
    mesh
}

impl App {
    pub fn timeline(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let (sr, n) = (self.sr(), self.s.tracks.len());
        let x0 = full.left() + HEADER_W;
        let lanes_top = full.top() + MARK_H + RULER_H + TEMPO_H;
        // Carril de marcas arriba de todo, con los números de compás.
        let mark_lane = Rect::from_min_max(pos2(x0, full.top()), pos2(full.right(), full.top() + MARK_H));
        let body = Rect::from_min_max(pos2(x0, lanes_top), full.max);
        let ruler = Rect::from_min_max(pos2(x0, mark_lane.bottom()), pos2(full.right(), mark_lane.bottom() + RULER_H));
        // Carril de tempo, siempre visible sobre la pista 1.
        let tempo_lane = Rect::from_min_max(pos2(x0, ruler.bottom()), pos2(full.right(), lanes_top));
        let heads = Rect::from_min_max(pos2(full.left(), lanes_top), pos2(x0, full.bottom()));

        // Rueda del ratón según los atajos configurados (por defecto: vertical, Shift horizontal,
        // Ctrl zoom, Ctrl+Shift altura de pistas). El pellizco del touchpad también hace zoom.
        if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| full.contains(*p)) {
            let anchor = (p.x.max(x0) - x0) as f64;
            let zoom_at = |app: &mut Self, f: f32| {
                let t = (anchor + app.sx) / app.pps as f64;
                app.zoom(f);
                app.sx = t * app.pps as f64 - anchor;
            };
            for (act, d) in self.wheel_events(ui) {
                match act {
                    Wheel::ScrollV => (self.scroll.y, self.sx) = (self.scroll.y - d.y, self.sx - d.x as f64),
                    Wheel::ScrollH => self.sx -= (d.y + d.x) as f64,
                    Wheel::ZoomH => zoom_at(self, 1.0015f32.powf(d.y + d.x)),
                    Wheel::ZoomV => {
                        let (f, any) = (1.0015f32.powf(d.y), self.s.tracks.iter().any(|t| t.selected));
                        self.s.tracks.iter_mut().filter(|t| !any || t.selected).for_each(|t| t.height = (t.height * f).clamp(MIN_H, MAX_H));
                    }
                }
            }
            for e in ui.input(|i| i.events.clone()) {
                if let Event::Zoom(f) = e {
                    zoom_at(self, f);
                }
            }
        }

        // Geometría: altura principal de cada pista + carriles de tomas visibles.
        let main_h: Vec<f32> = self.s.tracks.iter().map(|t| t.height).collect();
        let takes_n: Vec<usize> = self.s.tracks.iter().map(|t| if t.show_takes && t.takes.len() > 1 { t.takes.len() } else { 0 }).collect();
        let heights: Vec<f32> = main_h.iter().zip(&takes_n).map(|(h, k)| h + *k as f32 * TAKE_H).collect();
        let tops: Vec<f32> = std::iter::once(0.0)
            .chain(heights.iter().scan(0.0, |acc, h| {
                *acc += h;
                Some(*acc)
            }))
            .collect();
        let total = tops[n];
        let max_y = (total + 80.0 - body.height()).max(0.0);
        (self.scroll.y, self.sx) = (self.scroll.y.clamp(0.0, max_y), self.sx.max(0.0));

        let (pps, sx, sy) = (self.pps as f64, self.sx, self.scroll.y);
        // Coordenadas en f64: con zoom de muestra los píxeles superan la precisión de f32.
        let x_of = |f: u64| x0 + (f as f64 / sr * pps - sx) as f32;
        let frame_at = |x: f32| (((x - x0) as f64 + sx) / pps * sr).max(0.0);
        // Las pistas añadidas durante este cuadro (p. ej. al importar) usan la altura por defecto.
        let lane_h = |i: usize| heights.get(i).copied().unwrap_or(TRACK_H);
        let main = |i: usize| main_h.get(i).copied().unwrap_or(TRACK_H);
        let lane_y = |i: usize| lanes_top - sy + tops.get(i).copied().unwrap_or_else(|| total + (i - n) as f32 * TRACK_H);
        let lane_at = |y: f32| {
            let y = y - lanes_top + sy;
            (y >= 0.0 && y < total).then(|| tops.partition_point(|&t| t <= y) - 1)
        };
        let take_at = |p: Pos2| {
            let i = lane_at(p.y)?;
            let rel = p.y - lane_y(i) - main(i);
            let k = (rel / TAKE_H) as usize;
            (rel >= 0.0 && k < takes_n[i]).then_some((i, k))
        };
        let clip_rect = |i: usize, c: &Clip| Rect::from_x_y_ranges(x_of(c.start)..=x_of(c.end()), lane_y(i) + 4.0..=lane_y(i) + main(i) - 4.0);
        // Valor de automatización según la altura dentro del carril (0 abajo, 1 arriba).
        let value_at = |i: usize, y: f32| (1.0 - (y - lane_y(i) - 10.0) / (main(i) - 20.0)).clamp(0.0, 1.0);
        // Ajuste a la rejilla siguiendo el mapa de tempo.
        let (snap_on, beat, unit) = (self.snap, self.engine.beat_frames(), self.engine.unit.load(Relaxed));
        let tmap = self.s.tempo.clone();
        let snap = move |f: f64| (if snap_on { engine::frame_at_beat(&tmap, sr, unit, engine::beats_at(&tmap, sr, unit, f).round()) } else { f }).max(0.0) as u64;
        let sample_zoom = pps / sr >= 1.0;

        let ruler_r = ui.interact(ruler, Id::new("ruler"), Sense::click_and_drag());
        let body_r = ui.interact(body, Id::new("body"), Sense::click_and_drag());
        let (mods, press, ptr, pressed, down) = ui.input(|i| (i.modifiers, i.pointer.press_origin(), i.pointer.interact_pos(), i.pointer.primary_pressed(), i.pointer.primary_down()));
        // Región y zona bajo un punto (los tiradores de fade están en la barra de título).
        let zone_at = |s: &Session, p: Pos2| -> Option<(usize, usize, Zone)> {
            let i = lane_at(p.y).filter(|&i| p.y < lane_y(i) + main(i))?;
            let j = s.tracks[i].clips.iter().rposition(|c| clip_rect(i, c).expand2(vec2(4.0, 0.0)).contains(p))?;
            let (c, r) = (&s.tracks[i].clips[j], clip_rect(i, &s.tracks[i].clips[j]));
            let (fi, fo) = (x_of(c.start + c.fade_in), x_of(c.end() - c.fade_out));
            let zone = match () {
                _ if p.y < r.top() + TITLE_H && (p.x - fi).abs() < 7.0 => Zone::FadeIn,
                _ if p.y < r.top() + TITLE_H && (p.x - fo).abs() < 7.0 => Zone::FadeOut,
                _ if p.y > r.bottom() - 14.0 && (p.x - r.right()).abs() < 14.0 => Zone::Stretch,
                _ if (p.x - r.left()).abs() < 6.0 => Zone::Left,
                _ if (p.x - r.right()).abs() < 6.0 => Zone::Right,
                _ => Zone::Body,
            };
            Some((i, j, zone))
        };
        let in_razor = |sel: Option<(u64, u64)>, lanes: Option<(usize, usize)>, p: Pos2| {
            let (Some((a, b)), Some(i)) = (sel, lane_at(p.y)) else {
                return false;
            };
            let f = frame_at(p.x) as u64;
            f >= a && f < b && lanes.is_none_or(|(l0, l1)| (l0..=l1).contains(&i))
        };

        // Carril de tempo: arrastrar = elegir un tramo y su tempo; clic = editar el tramo;
        // clic derecho sobre una marca = quitarla.
        let tempo_r = ui
            .interact(tempo_lane, Id::new("tempo-lane"), Sense::click_and_drag())
            .on_hover_text(tr("Tempo: arrastra para elegir un tramo y cambiar su tempo · clic: editar el tramo · clic derecho en una marca: quitarla"));
        if tempo_r.drag_started() {
            let a = snap(frame_at(press.unwrap_or_default().x));
            self.drag = Some(Drag::Tempo(a, a));
        } else if tempo_r.clicked() {
            let f = frame_at(ptr.unwrap_or_default().x) as u64;
            let k = self.s.tempo.iter().rposition(|s| s.0 <= f).unwrap_or(0);
            let a = self.s.tempo[k].0;
            let b = self.s.tempo.get(k + 1).map_or_else(|| self.project_end().max(a + self.bar_frames() * 4), |n| n.0);
            self.dialog = Some(Dialog::Tempo(a, b, self.s.tempo[k].1, false));
        } else if tempo_r.secondary_clicked() {
            let x = ptr.unwrap_or_default().x;
            self.tempo_ctx = Some((frame_at(x) as u64, (1..self.s.tempo.len()).find(|&k| (x_of(self.s.tempo[k].0) - x).abs() < 8.0)));
        }
        tempo_r.context_menu(|ui| {
            widgets::menu_style(ui);
            let Some((f, marker)) = self.tempo_ctx else {
                return;
            };
            ui.label(RichText::new(tr("Tempo")).strong());
            if ui.button(tr("Editar este tramo…")).clicked() {
                let k = self.s.tempo.iter().rposition(|s| s.0 <= f).unwrap_or(0);
                let a = self.s.tempo[k].0;
                let b = self.s.tempo.get(k + 1).map_or_else(|| self.project_end().max(a + self.bar_frames() * 4), |n| n.0);
                self.dialog = Some(Dialog::Tempo(a, b, self.s.tempo[k].1, false));
            }
            if let Some(k) = marker
                && ui.button(tr("Quitar esta marca de tempo")).clicked()
            {
                self.edit();
                self.s.tempo.remove(k);
            }
            if ui.add_enabled(self.s.tempo.len() > 1, egui::Button::new(tr("Tempo único (quitar todos los cambios)"))).clicked() {
                self.edit();
                self.s.tempo.truncate(1);
            }
        });
        // Carril de marcas: doble clic (o M) añade, arrastrar mueve, clic va a la marca, clic derecho: menú.
        let mark_r = ui
            .interact(mark_lane, Id::new("marks"), Sense::click_and_drag())
            .on_hover_text(tr("Marcas: doble clic o M para añadir · arrastra para mover · clic derecho: renombrar o quitar"));
        let mark_hit = |s: &Session, x: f32| s.markers.iter().rposition(|m| (x_of(m.0) - 5.0..x_of(m.0) + 70.0).contains(&x));
        if mark_r.drag_started() {
            self.mark_drag = press.and_then(|p| mark_hit(&self.s, p.x));
            if self.mark_drag.is_some() {
                self.edit();
            }
        }
        if mark_r.dragged()
            && let (Some(k), Some(p)) = (self.mark_drag, ptr)
        {
            self.s.markers[k].0 = snap(frame_at(p.x));
        }
        if mark_r.drag_stopped() {
            self.mark_drag = None;
            self.s.markers.sort_by_key(|m| m.0);
        }
        let x = ptr.unwrap_or_default().x;
        if mark_r.double_clicked() && mark_hit(&self.s, x).is_none() {
            self.add_marker(snap(frame_at(x)));
        } else if mark_r.clicked() {
            self.go(mark_hit(&self.s, x).map_or(frame_at(x) as u64, |k| self.s.markers[k].0));
        }
        if mark_r.secondary_clicked() {
            self.mark_ctx = Some((snap(frame_at(x)), mark_hit(&self.s, x)));
        }
        mark_r.context_menu(|ui| {
            widgets::menu_style(ui);
            let Some((f, hit)) = self.mark_ctx else { return };
            ui.label(RichText::new(tr("Marcas")).strong());
            match hit.filter(|&k| k < self.s.markers.len()) {
                Some(k) => {
                    ui.text_edit_singleline(&mut self.s.markers[k].1);
                    if ui.button(tr("Ir a la marca")).clicked() {
                        self.go(self.s.markers[k].0);
                    }
                    if ui.button(tr("Quitar marca")).clicked() {
                        self.edit();
                        self.s.markers.remove(k);
                        ui.close();
                    }
                }
                None => {
                    if ui.button(tr("Añadir marca aquí")).clicked() {
                        self.add_marker(f);
                    }
                }
            }
            if ui.add_enabled(!self.s.markers.is_empty(), egui::Button::new(tr("Quitar todas las marcas"))).clicked() {
                self.edit();
                self.s.markers.clear();
            }
        });
        ruler_r.context_menu(|ui| {
            widgets::menu_style(ui);
            ui.label(RichText::new(tr("Regla")).strong());
            if ui.button(tr("Loop de todo el proyecto")).clicked() {
                self.loop_all();
            }
            if ui.add_enabled(self.time_sel.is_some(), egui::Button::new(tr("Loop = selección de tiempo"))).clicked() {
                let (a, b) = self.time_sel.unwrap_or_default();
                self.set_loop(a, b);
            }
            let looping = self.engine.looping.load(Relaxed);
            if ui.button(if looping { "Desactivar loop" } else { "Activar loop" }).clicked() {
                self.toggle_loop();
            }
            ui.separator();
            if ui.button(tr("Ir al inicio")).clicked() {
                self.go(0);
            }
            if ui.button(tr("Ir al final")).clicked() {
                self.go(self.project_end());
            }
        });
        // Regla: clic = mover cursor, arrastrar = definir rango de loop.
        if ruler_r.drag_started() {
            self.drag = Some(Drag::Loop(snap(frame_at(press.unwrap_or_default().x))));
        } else if ruler_r.clicked() {
            self.go(frame_at(ptr.unwrap_or_default().x) as u64);
        }

        if let Some(item) = body_r.dnd_release_payload::<LibItem>() {
            let p = ptr.unwrap_or_default();
            self.drop_item(&item, lane_at(p.y), snap(frame_at(p.x)));
        }

        if self.tool == Tool::Pencil {
            // Con zoom de muestra, el lápiz dibuja la forma de onda; si no, la automatización.
            if body_r.hovered() && pressed {
                self.edit();
                self.pencil = None;
                if let Some((i, j, _)) = ptr.filter(|_| sample_zoom).and_then(|p| zone_at(&self.s, p))
                    && let Some(b) = self.s.tracks[i].clips[j].buf()
                {
                    self.sample_edit = Some((i, j, b.frames.clone(), None));
                }
            }
            match (&mut self.sample_edit, ptr.filter(|_| down)) {
                (Some((i, j, frames, last)), Some(p)) => {
                    let (i, j) = (*i, *j);
                    let c = &self.s.tracks[i].clips[j];
                    let r = clip_rect(i, c);
                    let (mid, half) = ((r.top() + TITLE_H + r.bottom()) / 2.0, (r.bottom() - r.top() - TITLE_H) / 2.0 - 3.0);
                    let idx = (c.offset as f64 + (frame_at(p.x) - c.start as f64).round()).clamp(c.offset as f64, (c.offset + c.len - 1) as f64) as usize;
                    let v = ((mid - p.y) / half / c.gain.max(1e-3)).clamp(-1.0, 1.0);
                    let from = last.unwrap_or(idx);
                    frames[from.min(idx)..=from.max(idx)].fill([v; 2]);
                    *last = Some(idx);
                    let file = c.buf().map(|b| b.file.clone()).unwrap_or_default();
                    self.s.tracks[i].clips[j].src = Source::Audio(AudioBuf::new(file, frames.clone()));
                    self.dirty = true;
                }
                (Some(_), None) => {
                    // Al soltar, la edición se guarda como archivo nuevo (el original no se toca).
                    if let Some((i, j, frames, _)) = self.sample_edit.take() {
                        let base = self.s.tracks[i].clips[j].buf().map(|b| b.file.clone()).unwrap_or_default();
                        match self.write_audio(&base, "editado", frames) {
                            Ok(buf) => self.s.tracks[i].clips[j].src = Source::Audio(buf),
                            Err(e) => self.status = format!("Error: {e}"),
                        }
                    }
                }
                (None, _) => match ptr.filter(|_| body_r.is_pointer_button_down_on()).and_then(|p| Some((p, lane_at(p.y)?))) {
                    Some((p, i)) => {
                        let v = value_at(i, p.y);
                        let v = if self.s.tracks[i].auto_param == 1 { v * 2.0 - 1.0 } else { v };
                        self.draw_point(i, frame_at(p.x) as u64, v);
                    }
                    None => self.pencil = None,
                },
            }
            // Clic derecho: borra el punto de automatización más cercano.
            if let Some((p, i)) = ptr.filter(|_| body_r.secondary_clicked()).and_then(|p| Some((p, lane_at(p.y)?))) {
                let (f, tol) = (frame_at(p.x), 8.0 / pps * sr);
                if let Some(k) = self.s.tracks[i].auto_mut().iter().position(|q| (q.0 as f64 - f).abs() < tol) {
                    self.edit();
                    self.s.tracks[i].auto_mut().remove(k);
                }
            }
        } else if body_r.drag_started() || body_r.clicked() || body_r.secondary_clicked() {
            // Al soltar un clic egui ya no conserva el origen de la pulsación: se usa la posición actual.
            let p = if body_r.drag_started() { press } else { body_r.interact_pointer_pos() }.unwrap_or_default();
            let at = snap(frame_at(p.x));
            if let Some((i, k)) = take_at(p) {
                // Carriles de tomas: barrer elige ese tramo; un clic compara A/B usando toda la toma.
                if body_r.drag_started() {
                    self.drag = Some(Drag::Swipe(i, k, at, at));
                } else if body_r.clicked() {
                    self.edit();
                    self.comp_take(i, 0, u64::MAX, k);
                    self.status = format!("Toma {} activa en «{}»", k + 1, self.s.tracks[i].name);
                }
            } else if body_r.drag_started() && self.tool == Tool::Select && in_razor(self.time_sel, self.razor_lanes, p) {
                self.drag = Some(Drag::RazorMove(0.0));
            } else {
                match zone_at(&self.s, p) {
                    Some((i, j, _)) if body_r.secondary_clicked() => {
                        if !self.s.tracks[i].clips[j].selected {
                            self.deselect_clips();
                            self.s.tracks[i].clips[j].selected = true;
                        }
                        (self.ctx_region, self.ctx_lane) = (Some((i, j)), Some((i, at)));
                    }
                    None if body_r.secondary_clicked() => (self.ctx_region, self.ctx_lane) = (None, lane_at(p.y).map(|i| (i, at))),
                    Some((i, j, _)) if self.tool == Tool::Blade && body_r.clicked() => {
                        self.edit();
                        let c = self.s.tracks[i].clips.remove(j);
                        self.s.tracks[i].clips.extend([c.trim(0, at), c.trim(at, u64::MAX)].into_iter().flatten());
                        self.status = "Región dividida".into();
                    }
                    Some((i, j, _)) if body_r.double_clicked() && self.s.tracks[i].midi() => self.open_roll(i, j),
                    Some((i, j, _)) if body_r.double_clicked() && self.s.tracks[i].clips[j].buf().is_some() => self.open_audio_editor(i, j),
                    Some((i, j, zone)) if body_r.drag_started() && self.tool == Tool::Select && zone != Zone::Body => {
                        self.edit();
                        let c = self.s.tracks[i].clips[j].clone();
                        self.drag = Some(match zone {
                            Zone::FadeIn | Zone::FadeOut => Drag::Fade(i, j, zone == Zone::FadeOut),
                            _ => Drag::Edge(i, j, zone == Zone::Left, zone == Zone::Stretch || (zone == Zone::Right && mods.alt), c),
                        });
                    }
                    Some((i, j, _)) => {
                        let selected = self.s.tracks[i].clips[j].selected;
                        if body_r.clicked() && (mods.command || mods.shift) {
                            self.s.tracks[i].clips[j].selected = !selected;
                        } else if !selected {
                            if !mods.command && !mods.shift {
                                self.deselect_clips();
                            }
                            self.s.tracks[i].clips[j].selected = true;
                            self.select_linked(i, j);
                        }
                        (self.time_sel, self.razor_lanes) = (None, None);
                        if body_r.drag_started() && self.tool == Tool::Select {
                            self.edit();
                            let orig = self.s.tracks.iter().enumerate().flat_map(|(ti, t)| t.clips.iter().enumerate().filter(|c| c.1.selected).map(move |(ci, c)| (ti, ci, c.start)));
                            self.drag = Some(Drag::Clips(orig.collect(), 0.0, self.s.tracks[i].clips[j].start));
                        }
                    }
                    None if body_r.drag_started() => self.drag = Some(Drag::Range(at, lane_at(p.y).unwrap_or(n.saturating_sub(1)))),
                    None => {
                        self.deselect_clips();
                        (self.time_sel, self.razor_lanes) = (None, None);
                        self.go(frame_at(p.x) as u64);
                        match lane_at(p.y) {
                            Some(i) if body_r.double_clicked() && self.s.tracks[i].midi() => self.new_midi_region(i, at),
                            None if body_r.double_clicked() && p.y > lane_y(n) => self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None)),
                            _ => {}
                        }
                    }
                }
            }
        }
        body_r.context_menu(|ui| {
            widgets::menu_style(ui);
            self.region_menu(ui);
        });
        if body_r.dragged() || ruler_r.dragged() || tempo_r.dragged() {
            let p = ptr.unwrap_or_default();
            let at = frame_at(p.x);
            match &mut self.drag {
                Some(Drag::Clips(orig, acc, anchor)) => {
                    *acc += body_r.drag_delta().x as f64 / pps * sr;
                    let min = orig.iter().map(|o| o.2).min().unwrap_or(0) as i64;
                    let delta = (snap(*anchor as f64 + *acc) as i64 - *anchor as i64).max(-min);
                    for &(i, j, s) in orig.iter() {
                        self.s.tracks[i].clips[j].start = (s as i64 + delta) as u64;
                    }
                }
                Some(Drag::Fade(i, j, out)) => {
                    let c = &mut self.s.tracks[*i].clips[*j];
                    if *out {
                        c.fade_out = c.end().saturating_sub(at as u64).min(c.len - c.fade_in);
                    } else {
                        c.fade_in = (at as u64).saturating_sub(c.start).min(c.len - c.fade_out);
                    }
                }
                Some(Drag::Edge(i, j, left, stretch, orig)) => {
                    let max_len = orig.buf().map_or(u64::MAX, |b| b.frames.len() as u64 - orig.offset);
                    let c = &mut self.s.tracks[*i].clips[*j];
                    if *left {
                        let edge = (snap(at) as i64).clamp(orig.start as i64 - orig.offset as i64, orig.end() as i64 - 64) as u64;
                        let d = edge as i64 - orig.start as i64;
                        (c.start, c.offset, c.len) = (edge, (orig.offset as i64 + d) as u64, (orig.len as i64 - d) as u64);
                    } else {
                        let len = snap(at).saturating_sub(orig.start).max(64);
                        c.len = if *stretch { len } else { len.min(max_len) };
                    }
                    c.fade_in = c.fade_in.min(c.len);
                    c.fade_out = c.fade_out.min(c.len - c.fade_in);
                }
                Some(Drag::Range(a, l0)) => {
                    let l1 = lane_at(p.y).unwrap_or(if p.y < lanes_top { 0 } else { n.saturating_sub(1) });
                    self.time_sel = Some(((*a).min(snap(at)), (*a).max(snap(at))));
                    self.razor_lanes = Some(((*l0).min(l1), (*l0).max(l1)));
                }
                Some(Drag::Swipe(_, _, _, b)) => *b = snap(at),
                Some(Drag::RazorMove(acc)) => *acc += body_r.drag_delta().x as f64 / pps * sr,
                Some(Drag::Tempo(_, b)) => *b = snap(at),
                Some(Drag::Loop(a)) => {
                    let (a, b) = ((*a).min(snap(at)), (*a).max(snap(at)));
                    self.set_loop(a, b);
                }
                None => {}
            }
            self.dirty |= matches!(self.drag, Some(Drag::Clips(..) | Drag::Fade(..) | Drag::Edge(..)));
        } else if let Some(drag) = self.drag.take() {
            match drag {
                // Alt + arrastrar el borde derecho: time-stretch al soltar.
                Drag::Edge(i, j, false, true, orig) => {
                    let factor = self.s.tracks[i].clips[j].len as f64 / orig.len as f64;
                    self.s.tracks[i].clips[j] = orig;
                    let scope = ["la región", "toda la pista", "todo el grupo"][self.stretch_scope as usize % 3];
                    let r = self.stretch_scoped(i, j, factor);
                    self.report(format!("Estirado al {:.0} % ({scope})", factor * 100.0), r);
                }
                Drag::Swipe(i, k, a, b) if a != b => {
                    self.edit();
                    self.comp_take(i, a.min(b), a.max(b), k);
                }
                Drag::Tempo(a, b) if a != b => {
                    let (a, b) = (a.min(b), a.max(b));
                    self.dialog = Some(Dialog::Tempo(a, b, engine::bpm_at(&self.s.tempo, a as f64), true));
                }
                Drag::RazorMove(acc) => {
                    let a = self.time_sel.map_or(0, |s| s.0);
                    self.move_razor(snap(a as f64 + acc) as i64 - a as i64);
                }
                _ => {}
            }
        }
        if let (Some(d), Some(p)) = (&mut self.track_drag, ptr) {
            d.1 = (0..n).filter(|&k| lane_y(k) + lane_h(k) / 2.0 < p.y).count();
        }

        // Cursor según la zona bajo el puntero.
        if let Some(p) = body_r.hover_pos() {
            let icon = match (self.tool, zone_at(&self.s, p).map(|z| z.2)) {
                (Tool::Pencil, _) => CursorIcon::Cell,
                (Tool::Blade, _) => CursorIcon::Crosshair,
                _ if take_at(p).is_some() => CursorIcon::Text,
                _ if in_razor(self.time_sel, self.razor_lanes, p) => CursorIcon::Grab,
                (_, Some(Zone::Stretch)) => CursorIcon::ResizeNwSe,
                (_, Some(Zone::FadeIn | Zone::FadeOut | Zone::Left | Zone::Right)) => CursorIcon::ResizeHorizontal,
                _ => CursorIcon::Default,
            };
            ui.ctx().set_cursor_icon(icon);
        }

        // Fotogramas de las pistas de video (texturas en caché; se piden antes de dibujar).
        let mut thumbs = vec![];
        for i in 0..n {
            let Some(v) = self.s.tracks[i].video.clone() else {
                continue;
            };
            let (top, h) = (lane_y(i) + 4.0, main(i) - 8.0);
            let tw = (h * v.w as f32 / v.h as f32).max(8.0);
            let start = self.s.tracks[i].video_start;
            let end = start + (v.duration() * sr) as u64;
            let (xs, xe) = (x_of(start).max(x0), x_of(end).min(full.right()));
            let mut x = x_of(start) + ((xs - x_of(start)) / tw).floor() * tw;
            while x < xe {
                if let Some((tex, _)) = self.video_texture(i, frame_at(x + tw / 2.0) as u64) {
                    thumbs.push((Rect::from_min_size(pos2(x, top), vec2(tw, h)), tex));
                }
                x += tw;
            }
        }

        // ---------- Dibujo ----------
        let painter = ui.painter_at(full);
        painter.rect_filled(full, 0.0, BG);
        painter.rect_filled(ruler, 0.0, PANEL);
        let lp = painter.with_clip_rect(body);
        let drop_lane = body_r.dnd_hover_payload::<LibItem>().and(ptr).and_then(|p| lane_at(p.y));
        for (i, t) in self.s.tracks.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x0, lane_y(i)), vec2(body.width(), lane_h(i)));
            if t.selected {
                lp.rect_filled(r, 0.0, PANEL);
            }
            lp.hline(r.x_range(), r.bottom(), Stroke::new(1.0, BORDER));
            if drop_lane == Some(i) {
                lp.rect_stroke(r.shrink(2.0), rr(6.0), Stroke::new(2.0, ACCENT), StrokeKind::Inside);
            }
        }

        // Rejilla de compases y tiempos con densidad adaptativa al zoom.
        // (siguen el mapa de tempo: los compases se ensanchan o estrechan con cada cambio).
        let beats = self.engine.beats.load(Relaxed).max(1) as f64;
        let bar_at = |k: f64| self.frame_of(k * beats);
        let first = (self.beats(frame_at(x0)) / beats).floor().max(0.0);
        let bar_px = (bar_at(first + 1.0) - bar_at(first)) / sr * pps;
        let mut step = 1u64;
        while bar_px * (step as f64) < 56.0 {
            step *= 2;
        }
        let grid = painter.with_clip_rect(Rect::from_min_max(ruler.min, full.max));
        let marks = painter.with_clip_rect(mark_lane);
        marks.rect_filled(mark_lane, 0.0, Color32::from_rgb(0x18, 0x18, 0x1C));
        let mut k = first as u64 / step * step;
        while x_of(bar_at(k as f64) as u64) < full.right() {
            let x = x_of(bar_at(k as f64) as u64);
            grid.vline(x, ruler.center().y..=full.bottom(), Stroke::new(1.0, BORDER));
            grid.text(pos2(x + 4.0, ruler.top() + 3.0), Align2::LEFT_TOP, (k + 1).to_string(), FontId::monospace(11.0), TEXT_DIM);
            marks.vline(x, mark_lane.bottom() - 6.0..=mark_lane.bottom(), Stroke::new(1.0, BORDER));
            marks.text(pos2(x + 3.0, mark_lane.bottom() - 1.0), Align2::LEFT_BOTTOM, (k + 1).to_string(), FontId::monospace(9.0), TEXT_DIM.gamma_multiply(0.7));
            if step == 1 && beat / sr * pps > 14.0 {
                for b in 1..beats as u64 {
                    grid.vline(x_of(self.frame_of(k as f64 * beats + b as f64) as u64), lanes_top..=full.bottom(), Stroke::new(1.0, PANEL));
                }
            }
            k += step;
        }
        // Carril de tempo: un tramo por cambio de tempo, con su BPM.
        let tp = painter.with_clip_rect(tempo_lane);
        tp.rect_filled(tempo_lane, 0.0, Color32::from_rgb(0x1F, 0x1F, 0x24));
        for (k, &(s, bpm)) in self.s.tempo.iter().enumerate() {
            let (xa, xb) = (x_of(s), self.s.tempo.get(k + 1).map_or(full.right(), |n| x_of(n.0)));
            tp.rect_filled(Rect::from_x_y_ranges(xa..=xb, tempo_lane.y_range()), 0.0, ACCENT.gamma_multiply(if k % 2 == 0 { 0.12 } else { 0.22 }));
            tp.vline(xa, tempo_lane.y_range(), Stroke::new(2.0, ACCENT));
            tp.text(pos2(xa + 5.0, tempo_lane.center().y), Align2::LEFT_CENTER, format!("{bpm:.1} BPM"), FontId::proportional(11.5), TEXT);
        }
        if let Some(Drag::Tempo(a, b)) = &self.drag {
            tp.rect_filled(Rect::from_x_y_ranges(x_of((*a).min(*b))..=x_of((*a).max(*b)), tempo_lane.y_range()), 0.0, METER[1].gamma_multiply(0.45));
        }
        if sample_zoom {
            let t = frame_at(x0) / sr;
            grid.text(pos2(x0 + 4.0, ruler.bottom() - 2.0), Align2::LEFT_BOTTOM, format!("{:.4} s · {:.0} px por muestra", t, pps / sr), FontId::monospace(10.0), METER[1]);
        }

        // Rango de loop (regla) y selección de tiempo / razor.
        let (ls, le) = (self.engine.loop_start.load(Relaxed), self.engine.loop_end.load(Relaxed));
        if le > ls {
            let on = self.engine.looping.load(Relaxed);
            let r = Rect::from_x_y_ranges(x_of(ls)..=x_of(le), ruler.bottom() - 9.0..=ruler.bottom() - 1.0);
            grid.rect_filled(r, rr(4.0), if on { METER[1] } else { BORDER });
            if on {
                lp.rect_filled(Rect::from_x_y_ranges(r.x_range(), body.y_range()), 0.0, METER[1].gamma_multiply(0.04));
            }
        }
        if let Some((a, b)) = self.time_sel {
            let ys = self.razor_lanes.map_or(body.y_range(), |(l0, l1)| (lane_y(l0)..=lane_y(l1) + lane_h(l1)).into());
            let r = Rect::from_x_y_ranges(x_of(a)..=x_of(b), ys);
            lp.rect_filled(r, rr(4.0), Color32::from_white_alpha(16));
            lp.rect_stroke(r, rr(4.0), Stroke::new(1.0, TEXT_DIM), StrokeKind::Inside);
            if let Some(Drag::RazorMove(acc)) = &self.drag {
                let dx = (*acc / sr * pps) as f32;
                lp.rect_stroke(r.translate(vec2(dx, 0.0)), rr(4.0), Stroke::new(2.0, ACCENT), StrokeKind::Inside);
            }
        }

        // Regiones al estilo Logic: barra de título de color, contenido y fades.
        let dim = if self.show_auto { 0.55 } else { 1.0 };
        let k6 = rr(6.0) as u8;
        let hover = body_r.hover_pos().and_then(|p| zone_at(&self.s, p)).map(|z| (z.0, z.1));
        let fpp = sr / pps;
        let xr = (x0, full.right());
        for (i, t) in self.s.tracks.iter().enumerate() {
            let (top, color, lh) = (lane_y(i), t.color, main(i));
            if t.midi() && t.clips.is_empty() {
                lp.text(pos2(x0 + 12.0, top + lh / 2.0), Align2::LEFT_CENTER, tr("Doble clic para crear una región MIDI"), FontId::proportional(12.0), TEXT_DIM);
            }
            if t.kind == TrackKind::Video && t.video.is_none() {
                lp.text(pos2(x0 + 12.0, top + lh / 2.0), Align2::LEFT_CENTER, tr("Video no disponible"), FontId::proportional(12.0), TEXT_DIM);
            }
            if t.kind == TrackKind::Click {
                lp.text(pos2(x0 + 12.0, top + lh / 2.0), Align2::LEFT_CENTER, tr("Clic del metrónomo · sigue el tempo y la métrica · se exporta como stem"), FontId::proportional(12.0), TEXT_DIM);
            }
            for (j, c) in t.clips.iter().enumerate() {
                let r = clip_rect(i, c);
                if r.right() < x0 || r.left() > full.right() || r.bottom() < lanes_top || r.top() > full.bottom() {
                    continue;
                }
                lp.rect_filled(r, rr(6.0), color.gamma_multiply((if c.selected { 0.42 } else { 0.22 }) * dim));
                let title = Rect::from_min_max(r.min, pos2(r.right(), r.top() + TITLE_H));
                lp.rect_filled(title, CornerRadius { nw: k6, ne: k6, sw: 0, se: 0 }, color.gamma_multiply(0.85 * dim));
                let label = format!("{} · {:.0} BPM", t.name, engine::bpm_at(&self.s.tempo, c.start as f64));
                lp.with_clip_rect(title.intersect(body)).text(title.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.0), BG);
                if c.selected {
                    lp.rect_stroke(r, rr(6.0), Stroke::new(1.5, TEXT), StrokeKind::Inside);
                }
                let inner = Rect::from_min_max(pos2(r.left(), title.bottom() + 3.0), pos2(r.right(), r.bottom() - 3.0));
                let wave = color.gamma_multiply(dim);
                match &c.src {
                    Source::Audio(b) if fpp >= 48.0 => _ = lp.add(peak_mesh(c, b, inner, xr, fpp, wave)),
                    // Zoom cercano: forma de onda real con signo, desde las muestras.
                    Source::Audio(b) => {
                        let (mid, half) = (inner.center().y, inner.height() / 2.0);
                        let mono = |k: usize| b.frames.get(k).map_or(0.0, |s| (s[0] + s[1]) * 0.5 * c.gain);
                        lp.hline(inner.x_range(), mid, Stroke::new(1.0, wave.gamma_multiply(0.4)));
                        if fpp >= 1.0 {
                            let mut mesh = Mesh::default();
                            for x in (r.left().max(x0) as i32)..(r.right().min(full.right()) as i32) {
                                let f0 = c.offset as f64 + (x as f32 - r.left()) as f64 * fpp;
                                let (lo, hi) = (f0 as usize..(f0 + fpp).ceil() as usize).fold((0f32, 0f32), |(lo, hi), k| (lo.min(mono(k)), hi.max(mono(k))));
                                mesh.add_colored_rect(Rect::from_x_y_ranges(x as f32..=x as f32 + 1.0, mid - hi.min(1.0) * half - 0.5..=mid - lo.max(-1.0) * half + 0.5), wave);
                            }
                            lp.add(mesh);
                        } else {
                            let first = (c.offset as f64 + frame_at(x0.max(r.left())) - c.start as f64).floor().max(c.offset as f64) as usize;
                            let last = ((c.offset as f64 + frame_at(full.right().min(r.right())) - c.start as f64).ceil() as usize + 1).min((c.offset + c.len) as usize);
                            let pts: Vec<Pos2> = (first..last).map(|k| pos2(x_of(c.start + (k as u64 - c.offset)), mid - mono(k).clamp(-1.0, 1.0) * half)).collect();
                            if pps / sr >= 6.0 {
                                pts.iter().for_each(|p| _ = lp.circle_filled(*p, 2.5, TEXT));
                            }
                            lp.add(egui::Shape::line(pts, Stroke::new(1.5, wave)));
                        }
                    }
                    Source::Midi(notes) => {
                        let (lo, hi) = notes.iter().fold((127u8, 0u8), |(lo, hi), n| (lo.min(n.key), hi.max(n.key)));
                        let span = (hi.saturating_sub(lo) as f32 + 1.0).max(12.0);
                        let origin = c.origin();
                        let p = lp.with_clip_rect(r.intersect(body));
                        for nt in notes.iter() {
                            let s = (origin + nt.start as i64).max(0) as u64;
                            let y = inner.bottom() - (nt.key.saturating_sub(lo) as f32 + 0.5) / span * inner.height();
                            p.hline(x_of(s)..=x_of(s + nt.len).max(x_of(s) + 2.0), y, Stroke::new(3.0, wave));
                        }
                    }
                }
                // Fades: curva; tiradores visibles al pasar el ratón o con la región seleccionada.
                for (len, out) in [(c.fade_in, false), (c.fade_out, true)] {
                    if len > 0 {
                        let (a, b) = if out { (x_of(c.end() - len), r.right()) } else { (r.left(), x_of(c.start + len)) };
                        let pts: Vec<Pos2> = (0..=16)
                            .map(|k| {
                                let t = k as f32 / 16.0;
                                let g = (if out { 1.0 - t } else { t } * std::f32::consts::FRAC_PI_2).sin();
                                pos2(a + (b - a) * t, r.bottom() - g * (r.bottom() - title.bottom()))
                            })
                            .collect();
                        lp.add(egui::Shape::line(pts, Stroke::new(1.5, TEXT)));
                    }
                    if (c.selected || hover == Some((i, j))) && !out {
                        // Asa de time-stretch en la esquina inferior derecha.
                        let k = r.right_bottom() - vec2(4.0, 4.0);
                        for d in [4.0, 8.0] {
                            lp.line_segment([k - vec2(d, 0.0), k - vec2(0.0, d)], Stroke::new(1.5, TEXT));
                        }
                    }
                    if c.selected || hover == Some((i, j)) {
                        let x = if out { x_of(c.end() - c.fade_out) } else { x_of(c.start + c.fade_in) };
                        lp.circle_filled(pos2(x.clamp(r.left() + 5.0, r.right() - 5.0), title.center().y), 4.5, TEXT);
                    }
                }
            }
            self.draw_takes(&lp, i, lane_y(i) + lh, xr, &x_of, fpp);
            if self.recording() && t.params.arm.load(Relaxed) {
                self.draw_live_take(&lp, i, Rect::from_x_y_ranges(x0..=full.right(), top + 4.0..=top + lh - 4.0), &x_of, &frame_at);
            }
            // Curva de automatización (volumen = ámbar, panorama = azul).
            if self.show_auto {
                let (pts, c, cur) = match t.auto_param {
                    0 => (&t.vol_auto, METER[1], fader_pos(t.params.gain.get())),
                    _ => (&t.pan_auto, ACCENT, (t.params.pan.get() + 1.0) / 2.0),
                };
                let norm = |v: f32| {
                    if t.auto_param == 0 { v } else { (v + 1.0) / 2.0 }
                };
                let y = |v: f32| top + lh - 10.0 - v * (lh - 20.0);
                let line: Vec<Pos2> = match (pts.first(), pts.last()) {
                    (Some(a), Some(b)) => {
                        std::iter::once(pos2(x0, y(norm(a.1)))).chain(pts.iter().map(|&(f, v)| pos2(x_of(f), y(norm(v))))).chain(std::iter::once(pos2(full.right(), y(norm(b.1))))).collect()
                    }
                    _ => vec![pos2(x0, y(cur)), pos2(full.right(), y(cur))],
                };
                lp.add(egui::Shape::line(line, Stroke::new(if pts.is_empty() { 1.0 } else { 2.0 }, c.gamma_multiply(if pts.is_empty() { 0.5 } else { 1.0 }))));
                for &(f, v) in pts.iter().filter(|p| (x0..full.right()).contains(&x_of(p.0))) {
                    lp.circle_filled(pos2(x_of(f), y(norm(v))), 3.0, c);
                }
                let label = if t.auto_param == 0 { "Volumen" } else { "Panorama" };
                lp.text(pos2(x0 + 8.0, top + lh - 8.0), Align2::LEFT_BOTTOM, tr(label), FontId::proportional(11.0), c);
            }
        }
        for (r, tex) in &thumbs {
            lp.image(tex.id(), *r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            lp.rect_stroke(*r, 0.0, Stroke::new(1.0, BG), StrokeKind::Inside);
        }
        if let Some(Drag::Swipe(i, k, a, b)) = &self.drag {
            let y = lane_y(*i) + main(*i) + *k as f32 * TAKE_H;
            lp.rect_stroke(Rect::from_x_y_ranges(x_of((*a).min(*b))..=x_of((*a).max(*b)), y + 2.0..=y + TAKE_H - 2.0), rr(4.0), Stroke::new(2.0, ACCENT), StrokeKind::Inside);
        }

        // Marcas: línea vertical sobre todas las pistas y etiqueta con el nombre.
        for (k, (f, name)) in self.s.markers.iter().enumerate() {
            let x = x_of(*f);
            if x < x0 - 80.0 || x > full.right() {
                continue;
            }
            grid.vline(x, ruler.top()..=full.bottom(), Stroke::new(1.0, MARK.gamma_multiply(0.45)));
            let font = FontId::proportional(11.5);
            let w = ui.fonts_mut(|fo| fo.layout_no_wrap(name.clone(), font.clone(), BG).size().x);
            let pill = Rect::from_min_size(pos2(x, mark_lane.top() + 2.0), vec2(w.min(110.0) + 12.0, MARK_H - 8.0));
            let on = self.mark_drag == Some(k);
            marks.rect_filled(pill, CornerRadius { nw: 0, ne: 6, sw: 0, se: 6 }, if on { TEXT } else { MARK });
            marks.with_clip_rect(pill.intersect(mark_lane)).text(pill.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, name, font, BG);
        }

        // Cursor de reproducción.
        let px = x_of(self.pos());
        if px >= x0 {
            painter.vline(px, ruler.top()..=full.bottom(), Stroke::new(1.5, TEXT));
        }

        // Cabeceras de pista (widgets). El fondo de la columna tiene su propio menú contextual.
        painter.rect_filled(Rect::from_min_max(full.min, pos2(x0, full.bottom())), 0.0, PANEL);
        ui.interact(heads, Id::new("heads-bg"), Sense::click()).context_menu(|ui| {
            widgets::menu_style(ui);
            ui.label(RichText::new(tr("Pistas")).strong());
            if ui.button(tr("Nueva pista…")).clicked() {
                self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
            }
            if ui.button(tr("Seleccionar todas las pistas")).clicked() {
                self.s.tracks.iter_mut().for_each(|t| t.selected = true);
            }
            if ui.button(tr("Agrupar seleccionadas")).clicked() {
                self.group_selected();
            }
            ui.menu_button(tr("Altura de las pistas"), |ui| {
                for (label, h) in [("Compacta", 60.0), ("Normal", TRACK_H), ("Grande", 160.0)] {
                    if ui.button(tr(label)).clicked() {
                        self.s.tracks.iter_mut().for_each(|t| t.height = h);
                    }
                }
            });
            ui.separator();
            self.delete_selected_item(ui);
        });
        for (i, &takes) in takes_n.iter().enumerate() {
            let r = Rect::from_min_size(pos2(full.left() + GROUP_W, lane_y(i)), vec2(HEADER_W - GROUP_W, main(i))).shrink2(vec2(4.0, 3.0));
            if r.bottom() > heads.top() && r.top() < heads.bottom() {
                self.track_header(ui, i, r, heads);
            }
            // Botones A/B de las tomas, uno por carril.
            for k in 0..takes {
                let take_r = Rect::from_min_size(pos2(full.left() + 24.0, lane_y(i) + main(i) + k as f32 * TAKE_H + 6.0), vec2(HEADER_W - 34.0, TAKE_H - 12.0));
                if take_r.bottom() > heads.top() && take_r.top() < heads.bottom() {
                    let active = self.s.tracks[i].comp.iter().any(|s| s.2 == k);
                    let b =
                        egui::Button::new(RichText::new(format!("Toma {}", k + 1)).size(11.0).color(if active { BG } else { TEXT_DIM })).fill(if active { self.s.tracks[i].color } else { ELEVATED });
                    if ui.put(take_r, b).on_hover_text(tr("Clic: escuchar esta toma completa (A/B) · barre en su carril para elegir tramos")).clicked() {
                        self.edit();
                        self.comp_take(i, 0, u64::MAX, k);
                    }
                }
            }
        }
        // Barras de grupo: una por cada tramo de pistas seguidas del mismo grupo, con su nombre en vertical.
        let mut k = 0;
        while k < n {
            let Some(g) = self.s.tracks[k].group else {
                k += 1;
                continue;
            };
            let first = k;
            while k + 1 < n && self.s.tracks[k + 1].group == Some(g) {
                k += 1;
            }
            let bar = Rect::from_x_y_ranges(full.left() + 3.0..=full.left() + GROUP_W - 1.0, lane_y(first) + 3.0..=lane_y(k) + lane_h(k) - 3.0);
            if let Some(grp) = self.s.groups.get(g).filter(|_| bar.bottom() > heads.top() && bar.top() < heads.bottom()) {
                let p = ui.painter().with_clip_rect(heads);
                p.rect_filled(bar, rr(6.0), grp.color);
                let light = grp.color.r() as u32 * 3 + grp.color.g() as u32 * 6 + grp.color.b() as u32 > 1300;
                let galley = p.layout_no_wrap(grp.name.clone(), FontId::proportional(12.0), if light { BG } else { TEXT });
                let (gw, gh) = (galley.size().x.min(bar.height() - 6.0), galley.size().y);
                let pos = pos2(bar.center().x - gh / 2.0, bar.center().y + gw / 2.0);
                p.with_clip_rect(bar.intersect(heads)).add(egui::epaint::TextShape::new(pos, galley, TEXT).with_angle(-std::f32::consts::FRAC_PI_2));
                let r = ui
                    .interact(bar, Id::new(("group-bar", first)), Sense::click())
                    .on_hover_text(format!("Grupo «{}» · clic: seleccionar sus pistas · clic derecho: nombre, color y opciones", grp.name));
                if r.clicked() {
                    self.s.tracks.iter_mut().for_each(|t| t.selected = t.group == Some(g));
                }
                r.context_menu(|ui| {
                    widgets::menu_style(ui);
                    self.group_menu(ui, g);
                });
            }
            k += 1;
        }
        if let Some((_, to)) = self.track_drag {
            painter.hline(full.x_range(), lane_y(to), Stroke::new(2.0, ACCENT));
        }
        let add = Rect::from_min_size(pos2(full.left() + 5.0, lane_y(n) + 8.0), vec2(HEADER_W - 10.0, 30.0));
        if add.top() < full.bottom() && ui.put(add, egui::Button::new(RichText::new(tr("+  Nueva pista")).color(TEXT_DIM))).clicked() {
            self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
        }
        painter.rect_filled(Rect::from_min_max(full.min, pos2(x0, lanes_top)), 0.0, PANEL);
        painter.text(pos2(full.left() + 12.0, tempo_lane.center().y), Align2::LEFT_CENTER, tr("TEMPO"), FontId::proportional(11.0), ACCENT);
        painter.text(pos2(full.left() + 12.0, mark_lane.center().y), Align2::LEFT_CENTER, tr("MARCAS"), FontId::proportional(11.0), MARK);
        painter.text(pos2(x0 - 8.0, mark_lane.center().y), Align2::RIGHT_CENTER, tr("doble clic o M: añadir marca"), FontId::proportional(10.0), TEXT_DIM);
        painter.text(pos2(x0 - 8.0, tempo_lane.center().y), Align2::RIGHT_CENTER, tr("arrastra para cambiar el tempo de un tramo"), FontId::proportional(10.0), TEXT_DIM);
    }

    /// Menú de un grupo: nombre, color, edición vinculada, selección y disolver.
    pub fn group_menu(&mut self, ui: &mut egui::Ui, g: usize) {
        let Some(grp) = self.s.groups.get_mut(g) else {
            return;
        };
        ui.label(RichText::new(tr("Grupo")).strong());
        ui.add(egui::TextEdit::singleline(&mut grp.name).hint_text(tr("Nombre del grupo")).desired_width(200.0));
        egui::widgets::color_picker::color_picker_color32(ui, &mut grp.color, egui::widgets::color_picker::Alpha::Opaque);
        ui.checkbox(&mut grp.edit, tr("Edición vinculada (seleccionar y mover juntas)"));
        ui.separator();
        if ui.button(tr("Seleccionar sus pistas")).clicked() {
            self.s.tracks.iter_mut().for_each(|t| t.selected = t.group == Some(g));
        }
        if ui.button(RichText::new(tr("Disolver grupo")).color(METER[2])).clicked() {
            self.remove_group(g);
            ui.close();
        }
    }

    /// Carriles de tomas: el material de cada toma y, resaltados, los tramos que usa el comp.
    fn draw_takes(&self, p: &Painter, i: usize, y0: f32, xr: (f32, f32), x_of: &dyn Fn(u64) -> f32, fpp: f64) {
        let t = &self.s.tracks[i];
        let count = if t.show_takes && t.takes.len() > 1 { t.takes.len() } else { 0 };
        for k in 0..count {
            let row = Rect::from_x_y_ranges(xr.0..=xr.1, y0 + k as f32 * TAKE_H..=y0 + (k + 1) as f32 * TAKE_H);
            p.rect_filled(row, 0.0, if k % 2 == 0 { BG } else { PANEL });
            for &(a, b, _) in t.comp.iter().filter(|s| s.2 == k) {
                p.rect_filled(Rect::from_x_y_ranges(x_of(a).max(xr.0)..=x_of(b.min(u64::MAX / 2)).min(xr.1), row.y_range()), 0.0, t.color.gamma_multiply(0.18));
            }
            for c in &t.takes[k] {
                let r = Rect::from_x_y_ranges(x_of(c.start)..=x_of(c.end()), row.top() + 3.0..=row.bottom() - 3.0);
                p.rect_stroke(r, rr(4.0), Stroke::new(1.0, t.color.gamma_multiply(0.6)), StrokeKind::Inside);
                if let Some(b) = c.buf() {
                    p.add(peak_mesh(c, b, r, xr, fpp.max(1.0), t.color.gamma_multiply(0.7)));
                }
            }
            p.hline(row.x_range(), row.bottom(), Stroke::new(1.0, BORDER));
        }
    }

    /// Onda (o notas MIDI) de la toma mientras se graba, dibujada en vivo.
    fn draw_live_take(&self, p: &Painter, i: usize, lane: Rect, x_of: &dyn Fn(u64) -> f32, frame_at: &dyn Fn(f32) -> f64) {
        let t = &self.s.tracks[i];
        let start = self.engine.rec_start.load(Relaxed);
        let r = Rect::from_x_y_ranges(x_of(start)..=x_of(self.pos()), lane.y_range());
        p.rect_filled(r, rr(6.0), METER[2].gamma_multiply(0.25));
        p.rect_stroke(r, rr(6.0), Stroke::new(1.0, METER[2]), StrokeKind::Inside);
        // Barra de título de la toma en curso: nombre de la pista y tempo.
        let title = Rect::from_min_max(r.min, pos2(r.right(), r.top() + TITLE_H));
        let k6 = rr(6.0) as u8;
        p.rect_filled(title, CornerRadius { nw: k6, ne: k6, sw: 0, se: 0 }, METER[2]);
        let label = format!("● {} · {:.0} BPM", t.name, engine::bpm_at(&self.s.tempo, start as f64));
        p.with_clip_rect(title.intersect(lane)).text(title.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.0), TEXT);
        let lane = Rect::from_min_max(pos2(lane.left(), title.bottom()), lane.max);
        if t.midi() {
            let mut held: HashMap<u8, u64> = HashMap::new();
            let note_y = |key: u8| lane.bottom() - (key.saturating_sub(36) as f32 / 60.0).min(1.0) * lane.height();
            for &(at, e) in &self.rec_midi {
                match (e[0] & 0xF0, e[2]) {
                    (0x90, v) if v > 0 => _ = held.insert(e[1], at),
                    (0x80 | 0x90, _) => {
                        if let Some(s) = held.remove(&e[1]) {
                            p.hline(x_of(s)..=x_of(at).max(x_of(s) + 2.0), note_y(e[1]), Stroke::new(3.0, TEXT));
                        }
                    }
                    _ => {}
                }
            }
            held.into_iter().for_each(|(key, s)| _ = p.hline(x_of(s)..=x_of(self.pos()), note_y(key), Stroke::new(3.0, TEXT)));
            return;
        }
        let ich = self.engine.in_channels.max(1);
        let (l, g) = ((t.input as usize).min(ich - 1), t.params.in_gain.get());
        let frames = self.rec_data.len() / ich;
        let (mid, half) = (lane.center().y, lane.height() / 2.0 - 4.0);
        let mut mesh = Mesh::default();
        for x in (r.left().max(lane.left()) as i32)..(r.right().min(lane.right()) as i32) {
            let f0 = (frame_at(x as f32) as u64).saturating_sub(start) as usize;
            let f1 = ((frame_at(x as f32 + 1.0) as u64).saturating_sub(start) as usize).min(frames);
            let stride = ((f1.saturating_sub(f0)) / 64).max(1);
            let peak = (f0..f1).step_by(stride).map(|k| self.rec_data[k * ich + l].abs() * g).fold(0f32, f32::max);
            let h = (peak * half).clamp(0.5, half);
            mesh.add_colored_rect(Rect::from_x_y_ranges(x as f32..=x as f32 + 1.0, mid - h..=mid + h), TEXT);
        }
        p.add(mesh);
    }

    /// Menú contextual del timeline: herramientas de región o acciones en el espacio vacío.
    fn region_menu(&mut self, ui: &mut egui::Ui) {
        let run = |app: &mut Self, op: RegionOp| {
            let r = app.region_op(op);
            app.report("Listo", r);
        };
        if self.time_sel.is_some() {
            ui.label(RichText::new(tr("Selección (razor)")).strong());
            if ui.button(tr("Cortar selección")).clicked() {
                self.copy(true);
            }
            if ui.button(tr("Copiar selección")).clicked() {
                self.copy(false);
            }
            if ui.button(tr("Eliminar selección")).clicked() {
                self.delete();
            }
            if ui.button(tr("Loop = selección")).clicked() {
                let (a, b) = self.time_sel.unwrap_or_default();
                self.set_loop(a, b);
            }
            ui.separator();
        }
        match self.ctx_region.filter(|&(i, j)| self.s.tracks.get(i).is_some_and(|t| j < t.clips.len())) {
            Some((i, j)) => {
                let midi = self.s.tracks[i].midi();
                ui.label(RichText::new(tr("Región")).strong());
                if midi && ui.button(tr("Abrir en piano roll")).clicked() {
                    self.open_roll(i, j);
                }
                if ui.button(tr("Dividir en el cursor")).clicked() {
                    self.split(self.pos());
                }
                if ui.button(tr("Duplicar")).clicked() {
                    run(self, RegionOp::Duplicate);
                }
                if ui.button(tr("Copiar")).clicked() {
                    self.copy(false);
                }
                if ui.button(tr("Eliminar")).clicked() {
                    self.delete();
                }
                ui.separator();
                ui.label(RichText::new(tr("Transformar")).strong());
                if ui.button(tr("Estirar tiempo (time-stretch)…")).on_hover_text(tr("También: arrastra la esquina inferior derecha de la región")).clicked() {
                    self.dialog = Some(Dialog::Stretch(100.0));
                }
                ui.horizontal(|ui| {
                    ui.label(tr("Al estirar con el ratón:"));
                    for (k, label) in ["Región", "Pista", "Grupo"].iter().enumerate() {
                        ui.radio_value(&mut self.stretch_scope, k as u8, *label);
                    }
                });
                if ui.button(tr("Transponer…")).clicked() {
                    self.dialog = Some(Dialog::Transpose(0.0));
                }
                if midi {
                    if ui.button(tr("Cuantizar a 1/16")).clicked() {
                        run(self, RegionOp::Quantize);
                    }
                } else {
                    if ui.button(tr("Normalizar")).clicked() {
                        run(self, RegionOp::Normalize);
                    }
                    if ui.button(tr("Invertir (reverse)")).clicked() {
                        run(self, RegionOp::Reverse);
                    }
                }
                if ui.button(tr("Quitar fades")).on_hover_text(tr("Los fades se ajustan arrastrando los puntos de la barra de título")).clicked() {
                    run(self, RegionOp::ClearFades);
                }
            }
            None => {
                if ui.add_enabled(!self.clipboard.is_empty(), egui::Button::new(tr("Pegar en el cursor"))).clicked() {
                    self.paste();
                }
                if let Some((i, at)) = self.ctx_lane.filter(|&(i, _)| self.s.tracks.get(i).is_some_and(|t| t.midi()))
                    && ui.button(tr("Nueva región MIDI aquí")).clicked()
                {
                    self.new_midi_region(i, at);
                }
                if ui.button(tr("Nueva pista…")).clicked() {
                    self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
                }
            }
        }
    }

    /// Cabecera: número, color, nombre, R/M/S/I, FX, ROUTE, automatización, entrada y medidor de salida.
    fn track_header(&mut self, ui: &mut egui::Ui, i: usize, r: Rect, clip: Rect) {
        let resp = ui.interact(r, Id::new(("head", i)), Sense::click_and_drag()).on_hover_text(tr("Clic: seleccionar (Ctrl: varias) · arrastra para reordenar · clic derecho: opciones"));
        if resp.clicked() {
            let m = ui.input(|i| i.modifiers);
            self.select_track(i, m);
        }
        if resp.drag_started() {
            self.track_drag = Some((i, i));
        }
        if resp.drag_stopped()
            && let Some((a, b)) = self.track_drag.take()
        {
            self.move_track(a, b);
        }
        if let Some(item) = resp.dnd_release_payload::<LibItem>() {
            self.drop_item(&item, Some(i), self.pos());
        }
        resp.context_menu(|ui| {
            widgets::menu_style(ui);
            self.track_menu(ui, i);
        });
        // Borde inferior: arrastrar cambia la altura de la pista; doble clic vuelve a la normal.
        let edge = ui.interact(Rect::from_x_y_ranges(r.x_range(), r.bottom() - 2.0..=r.bottom() + 5.0), Id::new(("head-h", i)), Sense::click_and_drag());
        if edge.hovered() || edge.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
        }
        let t = &mut self.s.tracks[i];
        if edge.dragged() {
            t.height = (t.height + edge.drag_delta().y).clamp(MIN_H, MAX_H);
        }
        if edge.double_clicked() {
            t.height = TRACK_H;
        }
        let t = &self.s.tracks[i];
        let p = ui.painter().with_clip_rect(clip);
        p.rect_filled(r, rr(8.0), if t.selected { BORDER } else { ELEVATED });
        if t.selected || resp.dnd_hover_payload::<LibItem>().is_some() {
            p.rect_stroke(r, rr(8.0), Stroke::new(1.0, ACCENT), StrokeKind::Inside);
        }
        // Franja de color con el número de pista.
        let k8 = rr(8.0) as u8;
        p.rect_filled(Rect::from_min_size(r.min, vec2(26.0, r.height())), CornerRadius { nw: k8, sw: k8, ne: 0, se: 0 }, t.color);
        let light = t.color.r() as u32 * 3 + t.color.g() as u32 * 6 + t.color.b() as u32 > 1300;
        p.text(pos2(r.left() + 13.0, r.top() + 15.0), Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(17.0), if light { BG } else { TEXT });

        // Medidor de salida de la pista (vertical, en el borde derecho).
        let meter = Rect::from_x_y_ranges(r.right() - 9.0..=r.right() - 4.0, r.top() + 6.0..=r.bottom() - 6.0);
        let level = self.levels.get(i).copied().unwrap_or(0.0);
        let db = widgets::db(level);
        p.rect_filled(meter, rr(2.0), BG);
        let fill = ((db + 60.0) / 66.0).clamp(0.0, 1.0);
        p.rect_filled(Rect::from_min_max(pos2(meter.left(), meter.bottom() - fill * meter.height()), meter.max), rr(2.0), METER[(db > -12.0) as usize + (db > -3.0) as usize]);
        ui.scope_builder(UiBuilder::new().max_rect(Rect::from_min_max(r.min + vec2(34.0, 7.0), r.max - vec2(12.0, 7.0))), |ui| {
            ui.set_clip_rect(clip.intersect(r));
            ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
            let t = &mut self.s.tracks[i];
            ui.horizontal(|ui| {
                icons::picker(ui, t, 22.0);
                widgets::color_dot(ui, &mut t.color);
                ui.add(egui::TextEdit::singleline(&mut t.name).desired_width(124.0).font(FontId::proportional(14.0)));
                if t.takes.len() > 1 {
                    let label = RichText::new(format!("Tomas {}", t.takes.len())).size(10.0).color(if t.show_takes { ACCENT } else { TEXT_DIM });
                    if ui.add(egui::Button::new(label).small()).on_hover_text(tr("Mostrar u ocultar los carriles de tomas (comping)")).clicked() {
                        t.show_takes = !t.show_takes;
                    }
                } else {
                    let kind = match t.kind {
                        TrackKind::AudioMono => "MONO",
                        TrackKind::AudioStereo => "STEREO",
                        TrackKind::Midi => "MIDI",
                        TrackKind::Click => "CLIC",
                        TrackKind::Video => "VIDEO",
                    };
                    ui.label(RichText::new(kind).size(9.0).color(TEXT_DIM));
                }
            });
            // Con poca altura solo se muestran las filas que caben.
            if r.height() < 64.0 {
                return;
            }
            ui.horizontal(|ui| {
                self.flag_buttons(ui, i);
                self.polarity_button(ui, i);
                self.fx_button(ui, i);
                self.route_button(ui, i);
                let t = &mut self.s.tracks[i];
                if self.show_auto {
                    egui::ComboBox::from_id_salt(("auto", i)).width(56.0).selected_text(if t.auto_param == 0 { "Vol" } else { "Pan" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut t.auto_param, 0, tr("Volumen"));
                        ui.selectable_value(&mut t.auto_param, 1, tr("Panorama"));
                    });
                }
            });
            if r.height() < 90.0 {
                return;
            }
            ui.horizontal(|ui| {
                let t = &self.s.tracks[i];
                match t.kind {
                    TrackKind::Midi => {
                        let state = if t.params.monitor.load(Relaxed) || t.params.arm.load(Relaxed) { "teclado MIDI conectado a esta pista" } else { "activa I para tocar con tu teclado MIDI" };
                        ui.label(RichText::new(format!("QUANTUM Synth · {state}")).size(11.0).color(TEXT_DIM));
                    }
                    TrackKind::Click => _ = ui.label(RichText::new(tr("Metrónomo como audio")).size(11.0).color(TEXT_DIM)),
                    TrackKind::Video => {
                        let info = t.video.as_ref().map_or("sin video".to_string(), |v| format!("{:.1} s · visor en Ver → Visor de video", v.duration()));
                        ui.label(RichText::new(info).size(11.0).color(TEXT_DIM));
                    }
                    _ => {
                        let listening = t.params.arm.load(Relaxed) || t.params.monitor.load(Relaxed);
                        widgets::meter_h(ui, if listening { self.in_levels.get(i).copied().unwrap_or(0.0) } else { 0.0 }, 104.0);
                        let p = t.params.clone();
                        let mut d = widgets::db(p.in_gain.get());
                        let drag = egui::DragValue::new(&mut d).range(-60.0..=24.0).speed(0.2).fixed_decimals(1).suffix(" dB");
                        if ui.add(drag).on_hover_text(tr("Ganancia de entrada (doble clic para escribir)")).changed() {
                            p.in_gain.set(10f32.powf(d / 20.0));
                        }
                        self.input_combo(ui, i);
                    }
                }
            });
        });
    }

    /// Selector del canal de la interfaz que alimenta la pista.
    pub fn input_combo(&mut self, ui: &mut egui::Ui, i: usize) {
        let (n, t) = (self.engine.in_channels, &self.s.tracks[i]);
        let stereo = t.kind == TrackKind::AudioStereo;
        let label = |c: u16| match n {
            0 => "Sin entrada".to_string(),
            _ if stereo && (c as usize) + 1 < n => format!("In {}-{}", c + 1, c + 2),
            _ => format!("In {}", c + 1),
        };
        let mut ch = t.input;
        egui::ComboBox::from_id_salt(("input", i)).width(64.0).selected_text(label(ch)).show_ui(ui, |ui| {
            for c in (0..n as u16).step_by(if stereo { 2 } else { 1 }) {
                ui.selectable_value(&mut ch, c, label(c));
            }
        });
        if ch != self.s.tracks[i].input {
            (self.s.tracks[i].input, self.dirty) = (ch, true);
        }
    }

    /// Botones R/M/S/I: afectan a las pistas seleccionadas si esta lo está; si no, solo a ella.
    pub fn flag_buttons(&mut self, ui: &mut egui::Ui, i: usize) {
        let buttons = [(0, "R", METER[2], "Armar grabación"), (1, "M", ACCENT, "Mute"), (2, "S", METER[1], "Solo"), (3, "I", METER[0], "Monitoreo de entrada")];
        for (k, label, color, tip) in buttons {
            let on = flag(&self.s.tracks[i].params, k).load(Relaxed);
            let text = RichText::new(tr(label)).strong().size(12.0).color(if on { BG } else { TEXT_DIM });
            let b = egui::Button::new(text).fill(if on { color } else { BG }).min_size(vec2(24.0, 22.0)).corner_radius(rr(11.0));
            if ui.add(b).on_hover_text(tip).clicked() {
                for m in self.flag_targets(i, k) {
                    flag(&self.s.tracks[m].params, k).store(!on, Relaxed);
                }
            }
        }
    }

    /// Botón Ø (dos círculos entrelazados): invierte la polaridad del audio de la pista.
    pub fn polarity_button(&mut self, ui: &mut egui::Ui, i: usize) {
        let inv = &self.s.tracks[i].params.invert;
        let on = inv.load(Relaxed);
        let (r, resp) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::click());
        let p = ui.painter();
        p.rect_filled(r, rr(11.0), if on { METER[1] } else if resp.hovered() { BORDER } else { BG });
        let c = if on { BG } else { TEXT_DIM };
        for dx in [-3.0, 3.0] {
            p.circle_stroke(r.center() + vec2(dx, 0.0), 5.5, Stroke::new(1.5, c));
        }
        if resp.on_hover_text(tr("Invertir polaridad (fase) del audio")).clicked() {
            inv.store(!on, Relaxed);
            self.dirty = true;
        }
    }

    pub fn fx_button(&mut self, ui: &mut egui::Ui, i: usize) {
        let n = self.s.tracks[i].fx.len();
        let text = RichText::new(if n > 0 { format!("FX {n}") } else { "FX".into() }).size(12.0).color(if n > 0 { ACCENT } else { TEXT_DIM });
        if ui.add(egui::Button::new(text).fill(BG).min_size(vec2(34.0, 22.0)).corner_radius(rr(11.0))).on_hover_text(tr("Instrumento y efectos de la pista")).clicked() {
            self.fx_window = Some(i);
        }
    }

    pub fn track_menu(&mut self, ui: &mut egui::Ui, i: usize) {
        if ui.button(tr("Instrumento y efectos…")).clicked() {
            self.fx_window = Some(i);
        }
        if ui.button(tr("Ruteo (ROUTE)…")).clicked() {
            self.route_window = Some(i);
        }
        if ui.button(tr("Duplicar pista")).clicked() {
            self.duplicate_track(i);
        }
        if ui.add_enabled(i > 0, egui::Button::new(tr("Mover arriba"))).clicked() {
            self.move_track(i, i - 1);
        }
        if ui.add_enabled(i + 1 < self.s.tracks.len(), egui::Button::new(tr("Mover abajo"))).clicked() {
            self.move_track(i, i + 2);
        }
        if ui.button(tr("Borrar automatización")).clicked() {
            self.edit();
            (self.s.tracks[i].vol_auto, self.s.tracks[i].pan_auto) = (vec![], vec![]);
        }
        if self.s.tracks[i].takes.len() > 1 {
            ui.separator();
            ui.label(RichText::new(tr("Tomas (comping)")).strong());
            ui.checkbox(&mut self.s.tracks[i].show_takes, tr("Mostrar carriles de tomas"));
            ui.add(egui::DragValue::new(&mut self.xfade_ms).range(0.0..=200.0).suffix(" ms").prefix("Fundido entre tomas: "));
            if ui.button(tr("Consolidar comp (quitar tomas)")).clicked() {
                self.edit();
                let t = &mut self.s.tracks[i];
                (t.takes, t.comp, t.show_takes) = (vec![], vec![], false);
            }
        }
        ui.menu_button(tr("Grupo"), |ui| {
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
            if ui.button(tr("Quitar del grupo")).clicked() {
                self.edit();
                self.s.tracks[i].group = None;
            }
        });
        ui.separator();
        if ui.button(RichText::new(tr("Eliminar pista")).color(METER[2])).clicked() {
            self.delete_tracks(|j, _| j == i);
        }
        self.delete_selected_item(ui);
    }

    /// «Eliminar todas las pistas seleccionadas», cuando hay más de una seleccionada.
    pub fn delete_selected_item(&mut self, ui: &mut egui::Ui) {
        let n = self.s.tracks.iter().filter(|t| t.selected).count();
        if n > 1 && ui.button(RichText::new(format!("{} ({n})", tr("Eliminar todas las pistas seleccionadas"))).color(METER[2])).clicked() {
            self.delete_tracks(|_, t| t.selected);
        }
    }
}
