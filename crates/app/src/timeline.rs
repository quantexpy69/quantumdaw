//! Timeline: regla, cabeceras, regiones (audio/MIDI), fades, tomas (comping), razor, automatización
//! y edición de muestras con zoom profundo.
use crate::{keys::Wheel, library::LibItem, widgets::rr, *};
use egui::{Align2, CornerRadius, CursorIcon, FontId, Id, Mesh, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};
use engine::{PEAK_BLOCK, fader_pos};

/// Ancho de la columna del número de pista en la cabecera.
const NUMERO_W: f32 = 22.0;

/// Ancho por defecto, mínimo y máximo de la columna de cabeceras.
pub const HEADER_W: f32 = 356.0;
pub const HEADER_MIN: f32 = 280.0;
pub const HEADER_MAX: f32 = 720.0;
const RULER_H: f32 = 24.0;
const TITLE_H: f32 = 20.0;
const TEMPO_H: f32 = 20.0;
const MARK_H: f32 = 22.0;
const SPAN_H: f32 = 20.0;
const MARK: Color32 = Color32::from_rgb(0xFF, 0xC8, 0x57);
/// Columna de grupos (barra vertical con el nombre, como en Ardour).
const GROUP_W: f32 = 20.0;
const TAKE_H: f32 = 44.0;
/// Barra de arriba de cada grabación en un carril de tomas (se agarra para moverla).
const TOMA_BARRA: f32 = 12.0;
/// Altura mínima de una pista: entra la fila del nombre con sus botones y la de la entrada.
pub const MIN_H: f32 = 52.0;
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
    /// Grabación de una toma movida desde su barra: pista, toma, región y frames desde su inicio al punto agarrado e inicio original.
    Toma(usize, usize, usize, u64, u64),
    /// Borde de una grabación de toma: pista, toma, región, borde izquierdo y la grabación original.
    TomaBorde(usize, usize, usize, bool, Clip),
    /// Mover el contenido del razor: desplazamiento acumulado en frames.
    RazorMove(f64),
    /// Tramo elegido en el carril de tempo (inicio, fin).
    Tempo(u64, u64),
    /// Cursor de reproducción arrastrado desde su tirador en la regla.
    Cursor,
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
        let h = (peak * half).min(half).max(0.5);
        mesh.add_colored_rect(Rect::from_x_y_ranges(x as f32..=x as f32 + 1.0, mid - h..=mid + h), color);
    }
    mesh
}

impl App {
    pub fn timeline(&mut self, ui: &mut egui::Ui) {
        let full = ui.max_rect();
        let (sr, n) = (self.sr(), self.s.tracks.len());
        // Con poco lugar (inspector, lanzador y biblioteca abiertos) las cabeceras se angostan.
        let header_w = self.header_w().min((full.width() * 0.42).max(HEADER_MIN * 0.8));
        let x0 = full.left() + header_w;
        let (arr_h, chord_h) = (if self.show_arrange { SPAN_H } else { 0.0 }, if self.show_chords { SPAN_H } else { 0.0 });
        let lanes_top = full.top() + MARK_H + RULER_H + arr_h + chord_h + TEMPO_H;
        // Carril de marcas arriba de todo, con los números de compás.
        let mark_lane = Rect::from_min_max(pos2(x0, full.top()), pos2(full.right(), full.top() + MARK_H));
        let body = Rect::from_min_max(pos2(x0, lanes_top), full.max);
        let ruler = Rect::from_min_max(pos2(x0, mark_lane.bottom()), pos2(full.right(), mark_lane.bottom() + RULER_H));
        // Carril de tempo, siempre visible sobre la pista 1.
        // Carriles de arreglo (Intro, Estrofa, Coro…) y de acordes, bajo la regla.
        let arr_lane = Rect::from_min_max(pos2(x0, ruler.bottom()), pos2(full.right(), ruler.bottom() + arr_h));
        let chord_lane = Rect::from_min_max(pos2(x0, arr_lane.bottom()), pos2(full.right(), arr_lane.bottom() + chord_h));
        let tempo_lane = Rect::from_min_max(pos2(x0, chord_lane.bottom()), pos2(full.right(), lanes_top));
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

        // La vista sigue al cursor al reproducir o grabar: cuando llega al borde derecho pasa de página.
        // Si se movió la vista a mano lejos del cursor, no salta (salvo al grabar, que siempre se ve).
        if self.playing() && !matches!(self.drag, Some(Drag::Cursor)) {
            let ancho = (full.right() - x0) as f64;
            let px = self.pos() as f64 / sr * self.pps as f64 - self.sx;
            let fuera = px < 0.0 || px > ancho - 24.0;
            if fuera && (self.recording() || (px > 0.0 && px < ancho * 1.5)) {
                self.sx = (px + self.sx - ancho * 0.08).max(0.0);
            }
        }
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
        // Paso de la rejilla en tiempos del compás (compás entero, 1/2 … 1/32).
        let paso = self.paso_rejilla();
        let snap = move |f: f64| (if snap_on { engine::frame_at_beat(&tmap, sr, unit, (engine::beats_at(&tmap, sr, unit, f) / paso).round() * paso) } else { f }).max(0.0) as u64;
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
        if mark_r.double_clicked() {
            match mark_hit(&self.s, x) {
                Some(k) => self.dialog = Some(Dialog::Name(NameKind::Marker, k, self.s.markers[k].1.clone())),
                None => self.add_marker(snap(frame_at(x))),
            }
        } else if mark_r.clicked() {
            self.go(mark_hit(&self.s, x).map_or(frame_at(x) as u64, |k| self.s.markers[k].0));
        }
        if mark_r.secondary_clicked() {
            self.mark_ctx = Some((snap(frame_at(x)), mark_hit(&self.s, x)));
        }
        widgets::edit_menu(&mark_r, |ui| {
            let Some((f, hit)) = self.mark_ctx else { return };
            ui.label(RichText::new(tr("Marcas")).strong());
            match hit.filter(|&k| k < self.s.markers.len()) {
                Some(k) => {
                    ui.text_edit_singleline(&mut self.s.markers[k].1);
                    if ui.button(tr("Renombrar…")).clicked() {
                        self.dialog = Some(Dialog::Name(NameKind::Marker, k, self.s.markers[k].1.clone()));
                        ui.close();
                    }
                    if ui.button(tr("Ir a la marca")).clicked() {
                        self.go(self.s.markers[k].0);
                        ui.close();
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
                        ui.close();
                    }
                }
            }
            if ui.add_enabled(!self.s.markers.is_empty(), egui::Button::new(tr("Quitar todas las marcas"))).clicked() {
                self.edit();
                self.s.markers.clear();
                ui.close();
            }
        });
        for (lane, kind) in [(arr_lane, NameKind::Section), (chord_lane, NameKind::Chord)] {
            if lane.height() > 0.0 {
                self.span_lane(ui, lane, kind, &x_of, &frame_at, &snap, press, ptr);
            }
        }
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
        // Regla: clic = mover cursor, arrastrar = definir rango de loop; arrastrar sobre el cursor
        // (o su tirador) lo mueve.
        let cerca_cursor = |x: f32| (x - x_of(self.pos())).abs() <= 8.0;
        if ruler_r.hover_pos().is_some_and(|p| cerca_cursor(p.x)) || matches!(self.drag, Some(Drag::Cursor)) {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if ruler_r.drag_started() && press.is_some_and(|p| cerca_cursor(p.x)) {
            self.drag = Some(Drag::Cursor);
        } else if ruler_r.drag_started() {
            self.drag = Some(Drag::Loop(snap(frame_at(press.unwrap_or_default().x))));
        } else if ruler_r.clicked() {
            self.go(frame_at(ptr.unwrap_or_default().x) as u64);
        }

        if let Some(item) = body_r.dnd_release_payload::<LibItem>() {
            let p = ptr.unwrap_or_default();
            self.drop_item(&item, lane_at(p.y), snap(frame_at(p.x)));
        }

        if self.tool == Tool::Ganancia {
            // Línea de ganancia de una región o de una grabación de toma: clic agrega un punto (o agarra el
            // que está cerca), arrastrar lo mueve y clic derecho lo borra. Para toda la pista:
            // automatización → Ganancia.
            let p = ptr.unwrap_or_default();
            // Destino: pista, toma (None = la región de la pista) y región.
            let clip_de = |s: &Session, (i, k, j): (usize, Option<usize>, usize)| match k {
                Some(k) => s.tracks.get(i).and_then(|t| t.takes.get(k)).and_then(|t| t.get(j)).cloned(),
                None => s.tracks.get(i).and_then(|t| t.clips.get(j)).cloned(),
            };
            // (offset, inicio, largo, alto útil, base) de la línea en pantalla.
            let geo = |s: &Session, d: (usize, Option<usize>, usize)| {
                let c = clip_de(s, d)?;
                Some(match d.1 {
                    Some(k) => {
                        let fila = lane_y(d.0) + main(d.0) + k as f32 * TAKE_H;
                        (c.offset, c.start, c.len, TAKE_H - 9.0 - TOMA_BARRA, fila + TAKE_H - 4.0)
                    }
                    None => {
                        let r = clip_rect(d.0, &c);
                        (c.offset, c.start, c.len, r.height() - TITLE_H - 8.0, r.bottom() - 4.0)
                    }
                })
            };
            let cerca = |s: &Session, d| {
                let (off, ini, _, alto, base) = geo(s, d)?;
                clip_de(s, d)?.ganancia.as_deref().and_then(|l| l.iter().position(|&(f, db)| (x_of(ini + f.saturating_sub(off)) - p.x).abs() < 8.0 && (base - (db + 24.0) / 36.0 * alto - p.y).abs() < 8.0))
            };
            let punto = |s: &Session, d| {
                let (off, ini, len, alto, base) = geo(s, d)?;
                let f = (off as f64 + (frame_at(p.x) - ini as f64).clamp(0.0, len as f64)) as u64;
                Some((f, ganancia_db((base - p.y) / alto.max(1.0))))
            };
            let destino = match take_at(p) {
                Some((i, k)) => {
                    let f = frame_at(p.x) as u64;
                    self.s.tracks[i].takes[k].iter().position(|c| c.start <= f && f < c.end()).map(|j| (i, Some(k), j))
                }
                None => zone_at(&self.s, p).map(|z| (z.0, None, z.1)),
            }
            .filter(|&d| clip_de(&self.s, d).is_some_and(|c| c.buf().is_some()));
            if body_r.hovered()
                && (pressed || body_r.secondary_clicked())
                && let Some(d) = destino
                && let Some(q) = punto(&self.s, d)
            {
                self.edit();
                let mut l = clip_de(&self.s, d).and_then(|c| c.ganancia.as_deref().cloned()).unwrap_or_default();
                match (body_r.secondary_clicked(), cerca(&self.s, d)) {
                    (true, Some(k)) => _ = l.remove(k),
                    (true, None) => {}
                    (false, Some(k)) => self.gain_drag = Some((d, k)),
                    (false, None) => {
                        let k = l.partition_point(|x| x.0 < q.0);
                        l.insert(k, q);
                        self.gain_drag = Some((d, k));
                    }
                }
                self.poner_ganancia(d, l);
            }
            match self.gain_drag {
                Some((d, k)) if down => {
                    if let (Some(q), Some(mut l)) = (punto(&self.s, d), clip_de(&self.s, d).and_then(|c| c.ganancia.as_deref().cloned()).filter(|l| k < l.len())) {
                        // Entre el punto anterior y el siguiente (no se cruzan).
                        let (min, max) = (if k > 0 { l[k - 1].0 + 1 } else { 0 }, l.get(k + 1).map_or(u64::MAX, |x| x.0.saturating_sub(1)));
                        l[k] = (q.0.clamp(min, max.max(min)), q.1);
                        self.poner_ganancia(d, l);
                    }
                }
                _ => self.gain_drag = None,
            }
        } else if self.tool == Tool::Pencil {
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
            if body_r.secondary_clicked() {
                self.ctx_toma = None;
            }
            if let Some((i, k)) = take_at(p) {
                // Carriles de tomas: barrer elige ese tramo; un clic usa esa grabación entera; la barra de
                // arriba mueve la grabación; clic derecho: opciones. Nada de esto borra las otras tomas.
                let f = frame_at(p.x) as u64;
                let bajo = self.s.tracks[i].takes[k].iter().rposition(|c| c.start <= f && f < c.end());
                let en_barra = p.y - (lane_y(i) + main(i) + k as f32 * TAKE_H) < 3.0 + TOMA_BARRA;
                // Grabación con un borde a menos de 6 px: se recorta o se prolonga arrastrándolo.
                let borde = self.s.tracks[i].takes[k].iter().enumerate().find_map(|(j, c)| {
                    if (x_of(c.start) - p.x).abs() < 6.0 {
                        Some((j, true))
                    } else {
                        ((x_of(c.end()) - p.x).abs() < 6.0).then_some((j, false))
                    }
                });
                if body_r.secondary_clicked() {
                    self.ctx_region = None;
                    self.ctx_toma = bajo.map(|j| (i, k, j));
                } else if let (Tool::Blade, true, Some(j)) = (self.tool, body_r.clicked(), bajo) {
                    // Cuchilla: corta la grabación en dos dentro de la misma toma.
                    self.edit();
                    let c = self.s.tracks[i].takes[k].remove(j);
                    let partes: Vec<_> = [c.trim(0, at), c.trim(at, u64::MAX)].into_iter().flatten().collect();
                    for (n, parte) in partes.into_iter().enumerate() {
                        self.s.tracks[i].takes[k].insert(j + n, parte);
                    }
                    self.dirty = true;
                    self.status = format!("{} {}", tr("Toma cortada:"), k + 1);
                } else if let (true, Some((j, izq))) = (body_r.drag_started(), borde) {
                    self.edit();
                    let orig = self.s.tracks[i].takes[k][j].clone();
                    self.drag = Some(Drag::TomaBorde(i, k, j, izq, orig));
                } else if let (true, true, Some(j)) = (body_r.drag_started(), en_barra, bajo) {
                    self.edit();
                    let inicio = self.s.tracks[i].takes[k][j].start;
                    self.drag = Some(Drag::Toma(i, k, j, f.saturating_sub(inicio), inicio));
                } else if body_r.drag_started() {
                    self.drag = Some(Drag::Swipe(i, k, at, at));
                } else if let (true, Some(j)) = (body_r.clicked(), bajo) {
                    self.edit();
                    let c = &self.s.tracks[i].takes[k][j];
                    self.comp_take(i, c.start, c.end(), k);
                    self.status = format!("{} {} · {}", tr("Suena esta grabación de la toma"), k + 1, self.s.tracks[i].name);
                }
            } else if body_r.drag_started() && self.tool == Tool::Select && in_razor(self.time_sel, self.razor_lanes, p) {
                self.drag = Some(Drag::RazorMove(0.0));
            } else {
                match zone_at(&self.s, p) {
                    Some((i, j, _)) if body_r.secondary_clicked() => {
                        // Fuera de la selección de tiempo, el menú actúa sobre esta región (si no,
                        // «Eliminar» borraría el rango razor en lugar de la región elegida).
                        if !in_razor(self.time_sel, self.razor_lanes, p) {
                            (self.time_sel, self.razor_lanes) = (None, None);
                        }
                        if !self.s.tracks[i].clips[j].selected {
                            self.deselect_clips();
                            self.s.tracks[i].clips[j].selected = true;
                        }
                        (self.ctx_region, self.ctx_lane) = (Some((i, j)), Some((i, at)));
                        self.abrir_pie_region(p, i, j, false);
                    }
                    None if body_r.secondary_clicked() => (self.ctx_region, self.ctx_lane) = (None, lane_at(p.y).map(|i| (i, at))),
                    Some((i, j, _)) if self.tool == Tool::Blade && body_r.clicked() => {
                        self.edit();
                        let c = self.s.tracks[i].clips.remove(j);
                        // Las dos partes quedan sin seleccionar y en su orden: así lo que se elija
                        // después (clic derecho, o clic con la herramienta Selección) es lo que se borra.
                        self.deselect_clips();
                        (self.time_sel, self.razor_lanes, self.ctx_region) = (None, None, None);
                        let partes = [c.trim(0, at), c.trim(at, u64::MAX)].into_iter().flatten().map(|mut c| {
                            c.selected = false;
                            c
                        });
                        self.s.tracks[i].clips.extend(partes);
                        self.fundir_solapes([i]);
                        self.dirty = true;
                        self.status = tr("Región dividida: elige una parte (V = Selección, o clic derecho) y pulsa Suprimir").into();
                    }
                    Some((i, j, _)) if body_r.double_clicked() && self.s.tracks[i].midi() => self.open_roll(i, j),
                    Some((i, _, _)) if body_r.double_clicked() && self.s.tracks[i].kind == TrackKind::Video => self.open_video(i),
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
                        // Shift+clic: todas las regiones desde la seleccionada hasta esta (también entre
                        // pistas), como en una lista de archivos.
                        let ancla = self.s.tracks.iter().enumerate().find_map(|(ti, t)| t.clips.iter().find(|c| c.selected).map(|c| (ti, c.start, c.end())));
                        if body_r.clicked() && mods.shift && !mods.command && ancla.is_some() {
                            let (ia, sa, ea) = ancla.unwrap_or_default();
                            let c = &self.s.tracks[i].clips[j];
                            let (desde, hasta) = (sa.min(c.start), ea.max(c.end()));
                            for t in self.s.tracks[ia.min(i)..=ia.max(i)].iter_mut() {
                                t.clips.iter_mut().filter(|c| c.start < hasta && c.end() > desde).for_each(|c| c.selected = true);
                            }
                        } else if body_r.clicked() && (mods.command || mods.shift) {
                            self.s.tracks[i].clips[j].selected = !selected;
                        } else if !selected || body_r.clicked() {
                            // Un clic simple deja seleccionada solo esta región (aunque ya lo estuviera);
                            // arrastrar una ya seleccionada mueve toda la selección.
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
        // Sobre una región se abre el menú circular; en un lugar vacío, el menú clásico; sobre una
        // grabación de un carril de tomas, sus opciones.
        if self.pie.is_none() && self.ctx_region.is_none() {
            body_r.context_menu(|ui| {
                widgets::menu_style(ui);
                match self.ctx_toma {
                    Some((i, k, j)) => self.menu_toma(ui, i, k, j),
                    None => self.region_menu(ui),
                }
            });
        }
        // Q mantenida sobre una región: menú rápido (se elige al soltar la tecla).
        if self.pie.is_none()
            && !ui.ctx().egui_wants_keyboard_input()
            && ui.input(|i| i.key_pressed(Key::Q) && i.modifiers.is_none())
            && let Some(p) = body_r.hover_pos()
            && let Some((i, j, _)) = zone_at(&self.s, p)
        {
            if !self.s.tracks[i].clips[j].selected {
                self.deselect_clips();
                self.s.tracks[i].clips[j].selected = true;
            }
            self.ctx_region = Some((i, j));
            self.abrir_pie_region(p, i, j, true);
        }
        if body_r.dragged()
            && let Some(Drag::Toma(i, k, j, agarre, _)) = self.drag
            && let Some(c) = self.s.tracks[i].takes.get_mut(k).and_then(|tk| tk.get_mut(j))
        {
            // Mientras se arrastra solo se mueve en su carril; al soltar se mueve lo que suena de ella.
            c.start = snap(frame_at(ptr.unwrap_or_default().x) - agarre as f64);
        }
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
                        let (lo, hi) = (orig.start as i64 - orig.offset as i64, orig.end() as i64 - 64);
                        let edge = (snap(at) as i64).min(hi).max(lo).max(0) as u64;
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
                Some(Drag::Toma(..)) => {}
                Some(Drag::TomaBorde(i, k, j, izq, orig)) => {
                    let max = orig.buf().map_or(u64::MAX, |b| b.frames.len() as u64);
                    if let Some(c) = self.s.tracks[*i].takes.get_mut(*k).and_then(|t| t.get_mut(*j)) {
                        if *izq {
                            // Hasta el inicio del audio grabado y dejando al menos 64 frames.
                            let lo = (orig.origin()).max(0) as u64;
                            let edge = snap(at).clamp(lo, orig.end().saturating_sub(64).max(lo));
                            let d = edge as i64 - orig.start as i64;
                            (c.start, c.offset, c.len) = (edge, (orig.offset as i64 + d).max(0) as u64, (orig.len as i64 - d).max(64) as u64);
                        } else {
                            c.len = snap(at).saturating_sub(orig.start).clamp(64, max.saturating_sub(orig.offset).max(64));
                        }
                    }
                }
                Some(Drag::RazorMove(acc)) => *acc += body_r.drag_delta().x as f64 / pps * sr,
                Some(Drag::Tempo(_, b)) => *b = snap(at),
                Some(Drag::Cursor) => self.go(if mods.shift { snap(at) } else { at as u64 }),
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
                Drag::Toma(i, k, j, _, antes) => self.grabacion_movida(i, k, j, antes),
                Drag::TomaBorde(i, k, j, _, orig) => self.borde_de_toma_soltado(i, k, j, &orig),
                // Las tomas siguen a las regiones movidas que salen de ellas.
                Drag::Clips(orig, _, _) => {
                    let mut pistas: Vec<usize> = orig.iter().map(|o| o.0).collect();
                    for (i, j, antes) in orig {
                        self.toma_sigue_a_region(i, j, antes);
                    }
                    // Al soltar: la región que queda después va encima y los solapes llevan crossfade.
                    pistas.sort_unstable();
                    pistas.dedup();
                    self.fundir_solapes(pistas);
                }
                Drag::Edge(i, ..) => self.fundir_solapes([i]),
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
                (Tool::Ganancia, _) => CursorIcon::Crosshair,
                _ if take_at(p).is_some_and(|(i, k)| self.s.tracks[i].takes[k].iter().any(|c| (x_of(c.start) - p.x).abs() < 6.0 || (x_of(c.end()) - p.x).abs() < 6.0)) => CursorIcon::ResizeHorizontal,
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
            // Fotogramas dentro de cada región (arriba), dejando ver la onda de su audio debajo.
            for c in self.s.tracks[i].clips.clone() {
                let r = clip_rect(i, &c);
                let area = Rect::from_min_max(pos2(r.left(), r.top() + TITLE_H + 2.0), pos2(r.right(), r.bottom() - 2.0));
                let h = if c.buf().is_some() { area.height() * 0.62 } else { area.height() };
                let tw = (h * v.w as f32 / v.h as f32).max(8.0);
                let (xs, xe) = (area.left().max(x0), area.right().min(full.right()));
                let mut x = area.left() + ((xs - area.left()) / tw).floor() * tw;
                while x < xe {
                    if let Some((tex, _)) = self.video_texture(i, frame_at(x + tw / 2.0) as u64) {
                        thumbs.push((Rect::from_min_size(pos2(x, area.top()), vec2(tw, h)), tex, area.intersect(body)));
                    }
                    x += tw;
                }
            }
        }

        // ---------- Dibujo ----------
        let painter = ui.painter_at(full);
        painter.rect_filled(full, 0.0, BG());
        painter.rect_filled(ruler, 0.0, PANEL());
        let lp = painter.with_clip_rect(body);
        let drop_lane = body_r.dnd_hover_payload::<LibItem>().and(ptr).and_then(|p| lane_at(p.y));
        for (i, t) in self.s.tracks.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x0, lane_y(i)), vec2(body.width(), lane_h(i)));
            if t.selected {
                lp.rect_filled(r, 0.0, PANEL());
            }
            lp.hline(r.x_range(), r.bottom(), Stroke::new(1.0, BORDER()));
            if drop_lane == Some(i) {
                lp.rect_stroke(r.shrink(2.0), rr(6.0), Stroke::new(2.0, ACCENT()), StrokeKind::Inside);
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
        marks.rect_filled(mark_lane, 0.0, chrome::FONDO_HONDO);
        let mut k = first as u64 / step * step;
        while x_of(bar_at(k as f64) as u64) < full.right() {
            let x = x_of(bar_at(k as f64) as u64);
            grid.vline(x, ruler.top() + 4.0..=full.bottom(), Stroke::new(1.0, BORDER()));
            grid.text(pos2(x + 4.0, ruler.top() + 3.0), Align2::LEFT_TOP, (k + 1).to_string(), FontId::proportional(11.0), TEXT());
            marks.vline(x, mark_lane.bottom() - 4.0..=mark_lane.bottom(), Stroke::new(1.0, BORDER()));
            if step == 1 && beat / sr * pps > 14.0 {
                for b in 1..beats as u64 {
                    grid.vline(x_of(self.frame_of(k as f64 * beats + b as f64) as u64), lanes_top..=full.bottom(), Stroke::new(1.0, PANEL()));
                }
            }
            k += step;
        }
        // Carril de tempo: un tramo por cambio de tempo, con su BPM.
        let tp = painter.with_clip_rect(tempo_lane);
        tp.rect_filled(tempo_lane, 0.0, PANEL());
        for (k, &(s, bpm)) in self.s.tempo.iter().enumerate() {
            let (xa, xb) = (x_of(s), self.s.tempo.get(k + 1).map_or(full.right(), |n| x_of(n.0)));
            tp.rect_filled(Rect::from_x_y_ranges(xa..=xb, tempo_lane.y_range()), 0.0, ACCENT().gamma_multiply(if k % 2 == 0 { 0.12 } else { 0.22 }));
            tp.vline(xa, tempo_lane.y_range(), Stroke::new(2.0, ACCENT()));
            tp.text(pos2(xa + 5.0, tempo_lane.center().y), Align2::LEFT_CENTER, format!("{bpm:.1} BPM"), FontId::proportional(10.5), TEXT());
        }
        for (lane, kind) in [(arr_lane, NameKind::Section), (chord_lane, NameKind::Chord)] {
            if lane.height() > 0.0 {
                self.draw_spans(ui, lane, kind, &x_of);
            }
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
            // Rango de loop en toda la altura de la regla (como en las consolas), con sus bordes marcados
            // y un velo sobre las pistas cuando está activo.
            let color = if on { ACCENT() } else { TEXT_DIM() };
            let r = Rect::from_x_y_ranges(x_of(ls)..=x_of(le), ruler.top() + 1.0..=ruler.bottom() - 1.0);
            grid.rect_filled(r, rr(2.0), color.gamma_multiply(if on { 0.28 } else { 0.12 }));
            grid.rect_filled(Rect::from_x_y_ranges(r.x_range(), r.bottom() - 3.0..=r.bottom()), 0.0, color);
            for x in [r.left(), r.right()] {
                grid.vline(x, r.y_range(), Stroke::new(2.0, color));
            }
            if on {
                lp.rect_filled(Rect::from_x_y_ranges(r.x_range(), body.y_range()), 0.0, ACCENT().gamma_multiply(0.06));
                for x in [r.left(), r.right()] {
                    lp.vline(x, body.y_range(), Stroke::new(1.0, ACCENT().gamma_multiply(0.5)));
                }
            }
        }
        if let Some((a, b)) = self.time_sel {
            let ys = self.razor_lanes.map_or(body.y_range(), |(l0, l1)| (lane_y(l0)..=lane_y(l1) + lane_h(l1)).into());
            let r = Rect::from_x_y_ranges(x_of(a)..=x_of(b), ys);
            lp.rect_filled(r, rr(4.0), Color32::from_white_alpha(16));
            lp.rect_stroke(r, rr(4.0), Stroke::new(1.0, TEXT_DIM()), StrokeKind::Inside);
            if let Some(Drag::RazorMove(acc)) = &self.drag {
                let dx = (*acc / sr * pps) as f32;
                lp.rect_stroke(r.translate(vec2(dx, 0.0)), rr(4.0), Stroke::new(2.0, ACCENT()), StrokeKind::Inside);
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
                lp.text(pos2(x0 + 12.0, top + lh / 2.0), Align2::LEFT_CENTER, tr("Doble clic para crear una región MIDI"), FontId::proportional(12.0), TEXT_DIM());
            }
            if t.kind == TrackKind::Video && t.video.is_none() {
                lp.text(pos2(x0 + 12.0, top + lh / 2.0), Align2::LEFT_CENTER, tr("Video no disponible"), FontId::proportional(12.0), TEXT_DIM());
            }
            if t.kind == TrackKind::Click {
                lp.text(pos2(x0 + 12.0, top + lh / 2.0), Align2::LEFT_CENTER, tr("Clic del metrónomo · sigue el tempo y la métrica · se exporta como stem"), FontId::proportional(12.0), TEXT_DIM());
            }
            for (j, c) in t.clips.iter().enumerate() {
                let r = clip_rect(i, c);
                if r.right() < x0 || r.left() > full.right() || r.bottom() < lanes_top || r.top() > full.bottom() {
                    continue;
                }
                // Cuerpo: el color de la pista oscurecido; barra de título con el color pleno y el nombre
                // en blanco; la forma de onda y las notas en un tono claro del mismo color.
                let cuerpo = color.lerp_to_gamma(BG(), if c.selected { 0.45 } else { 0.62 });
                lp.rect_filled(r, rr(6.0), cuerpo.gamma_multiply(dim));
                let title = Rect::from_min_max(r.min, pos2(r.right(), r.top() + TITLE_H));
                lp.rect_filled(title, CornerRadius { nw: k6, ne: k6, sw: 0, se: 0 }, color.gamma_multiply(dim));
                let label = format!("{} · {:.0} BPM", t.name, engine::bpm_at(&self.s.tempo, c.start as f64));
                lp.with_clip_rect(title.intersect(body)).text(title.left_center() + vec2(7.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(12.0), Color32::WHITE);
                lp.rect_stroke(r, rr(6.0), Stroke::new(if c.selected { 2.0 } else { 1.0 }, if c.selected { Color32::WHITE } else { color.gamma_multiply(0.9) }), StrokeKind::Inside);
                let inner = Rect::from_min_max(pos2(r.left(), title.bottom() + 3.0), pos2(r.right(), r.bottom() - 3.0));
                let wave = color.lerp_to_gamma(Color32::WHITE, 0.35).gamma_multiply(dim);
                // Estirando con el asa inferior derecha: el contenido se dibuja ya escalado al nuevo largo
                // (k = largo original / largo actual), así se ve en tiempo real cómo se estira o comprime.
                let estirando = match &self.drag {
                    Some(Drag::Edge(di, dj, false, true, orig)) if (*di, *dj) == (i, j) => Some((orig.len as f64 / c.len.max(1) as f64, orig.len)),
                    _ => None,
                };
                let k = estirando.map_or(1.0, |e| e.0);
                match &c.src {
                    Source::Audio(b) if estirando.is_some() => _ = lp.add(peak_mesh(c, b, inner, xr, fpp * k, wave)),
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
                                pts.iter().for_each(|p| _ = lp.circle_filled(*p, 2.5, TEXT()));
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
                            let rel = (origin + nt.start as i64 - c.start as i64) as f64 / k;
                            let s = (c.start as f64 + rel).max(0.0) as u64;
                            let y = inner.bottom() - (nt.key.saturating_sub(lo) as f32 + 0.5) / span * inner.height();
                            p.hline(x_of(s)..=x_of(s + (nt.len as f64 / k) as u64).max(x_of(s) + 2.0), y, Stroke::new(3.0, wave));
                        }
                    }
                }
                // Guía del estiramiento: largo original (línea discontinua) y porcentaje junto al asa.
                if let Some((_, largo)) = estirando {
                    let xo = x_of(c.start + largo);
                    let guia = lp.with_clip_rect(body);
                    for y in (r.top() as i32..r.bottom() as i32).step_by(8) {
                        guia.vline(xo, y as f32..=(y as f32 + 4.0).min(r.bottom()), Stroke::new(1.5, TEXT().gamma_multiply(0.7)));
                    }
                    let pct = format!("{:.1} %", 100.0 / k);
                    let pos = pos2(r.right() - 6.0, r.bottom() - 18.0);
                    let galeria = guia.layout_no_wrap(pct.clone(), FontId::proportional(13.0), BG());
                    guia.rect_filled(Rect::from_min_size(pos2(pos.x - galeria.size().x - 8.0, pos.y - 10.0), vec2(galeria.size().x + 10.0, 20.0)), rr(5.0), METER[1]);
                    guia.text(pos, Align2::RIGHT_CENTER, pct, FontId::proportional(13.0), BG());
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
                        lp.add(egui::Shape::line(pts, Stroke::new(1.5, TEXT())));
                    }
                    if (c.selected || hover == Some((i, j))) && !out {
                        // Asa de time-stretch en la esquina inferior derecha.
                        let k = r.right_bottom() - vec2(4.0, 4.0);
                        for d in [4.0, 8.0] {
                            lp.line_segment([k - vec2(d, 0.0), k - vec2(0.0, d)], Stroke::new(1.5, TEXT()));
                        }
                    }
                    if c.selected || hover == Some((i, j)) {
                        let x = if out { x_of(c.end() - c.fade_out) } else { x_of(c.start + c.fade_in) };
                        // En una región muy angosta (menos de 10 px) el tirador queda al centro: clamp con min > max cierra la app.
                        lp.circle_filled(pos2(x.min(r.right() - 5.0).max(r.left() + 5.0).min(r.center().x.max(r.left())), title.center().y), 4.5, TEXT());
                    }
                }
                // Línea de ganancia de la región (con la herramienta Ganancia, o si tiene puntos).
                if c.buf().is_some() && (self.tool == Tool::Ganancia || c.ganancia.is_some()) {
                    let (alto, base) = (r.height() - TITLE_H - 8.0, r.bottom() - 4.0);
                    let y = |db: f32| base - (db + 24.0) / 36.0 * alto;
                    let x = |f: u64| x_of(c.start + f.saturating_sub(c.offset));
                    let puntos = c.ganancia.as_deref().map(Vec::as_slice).unwrap_or_default();
                    let mut linea = vec![pos2(r.left(), y(engine::automation_at(puntos, c.offset).unwrap_or(0.0)))];
                    linea.extend(puntos.iter().filter(|q| q.0 >= c.offset && q.0 <= c.offset + c.len).map(|&(f, db)| pos2(x(f), y(db))));
                    linea.push(pos2(r.right(), y(engine::automation_at(puntos, c.offset + c.len).unwrap_or(0.0))));
                    let color = METER[0];
                    let gp = lp.with_clip_rect(r.intersect(lp.clip_rect()));
                    gp.add(egui::Shape::line(linea, Stroke::new(if puntos.is_empty() { 1.0 } else { 2.0 }, color.gamma_multiply(if puntos.is_empty() { 0.55 } else { 1.0 }))));
                    for &(f, db) in puntos {
                        gp.circle_filled(pos2(x(f), y(db)), 3.5, color);
                        if self.tool == Tool::Ganancia {
                            gp.text(pos2(x(f) + 5.0, y(db) - 4.0), Align2::LEFT_BOTTOM, format!("{db:+.1} dB"), FontId::proportional(10.0), color);
                        }
                    }
                }
            }
            self.draw_takes(&lp, i, lane_y(i) + lh, xr, &x_of, fpp);
            if self.recording() && t.params.arm.load(Relaxed) {
                self.draw_live_take(&lp, i, Rect::from_x_y_ranges(x0..=full.right(), top + 4.0..=top + lh - 4.0), &x_of, &frame_at);
            }
            // Curva de automatización (volumen = ámbar, panorama = azul, ganancia = verde).
            if self.show_auto {
                let (pts, c, cur) = match t.auto_param {
                    0 => (&t.vol_auto, METER[1], fader_pos(t.params.gain.get())),
                    2 => (&t.gain_auto, METER[0], 2.0 / 3.0),
                    _ => (&t.pan_auto, ACCENT(), (t.params.pan.get() + 1.0) / 2.0),
                };
                let norm = |v: f32| {
                    if t.auto_param == 1 { (v + 1.0) / 2.0 } else { v }
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
                let label = match t.auto_param {
                    0 => tr("Volumen").to_string(),
                    2 => format!("{} ({})", tr("Ganancia"), tr("toda la pista")),
                    _ => tr("Panorama").to_string(),
                };
                lp.text(pos2(x0 + 8.0, top + lh - 8.0), Align2::LEFT_BOTTOM, label, FontId::proportional(11.0), c);
                // Valor en dB de cada punto de la ganancia.
                if t.auto_param == 2 {
                    for &(f, v) in pts.iter().filter(|p| (x0..full.right()).contains(&x_of(p.0))) {
                        lp.text(pos2(x_of(f) + 5.0, y(v) - 4.0), Align2::LEFT_BOTTOM, format!("{:+.1} dB", ganancia_db(v)), FontId::proportional(10.0), c);
                    }
                }
            }
        }
        for (r, tex, clip) in &thumbs {
            let tp = lp.with_clip_rect(*clip);
            tp.image(tex.id(), *r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            tp.rect_stroke(*r, 0.0, Stroke::new(1.0, BG()), StrokeKind::Inside);
        }
        if let Some(Drag::Swipe(i, k, a, b)) = &self.drag {
            let y = lane_y(*i) + main(*i) + *k as f32 * TAKE_H;
            lp.rect_stroke(Rect::from_x_y_ranges(x_of((*a).min(*b))..=x_of((*a).max(*b)), y + 2.0..=y + TAKE_H - 2.0), rr(4.0), Stroke::new(2.0, ACCENT()), StrokeKind::Inside);
        }

        // Marcas: línea vertical sobre todas las pistas y etiqueta con el nombre.
        for (k, (f, name)) in self.s.markers.iter().enumerate() {
            let x = x_of(*f);
            if x < x0 - 80.0 || x > full.right() {
                continue;
            }
            grid.vline(x, ruler.top()..=full.bottom(), Stroke::new(1.0, MARK.gamma_multiply(0.45)));
            let font = FontId::proportional(11.0);
            let w = ui.fonts_mut(|fo| fo.layout_no_wrap(name.clone(), font.clone(), BG()).size().x);
            // La etiqueta no pisa la siguiente marca: se recorta hasta ella.
            let libre = self.s.markers.iter().map(|m| x_of(m.0)).filter(|&nx| nx > x + 1.0).fold(f32::INFINITY, f32::min) - x - 3.0;
            let pill = Rect::from_min_size(pos2(x, mark_lane.top() + 3.0), vec2((w.min(140.0) + 12.0).min(libre.max(14.0)), MARK_H - 6.0));
            let on = self.mark_drag == Some(k);
            marks.rect_filled(pill, CornerRadius { nw: 0, ne: 3, sw: 0, se: 3 }, if on { TEXT() } else { MARK });
            marks.with_clip_rect(pill.intersect(mark_lane)).text(pill.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, name, font, BG());
        }

        // Cursor de reproducción.
        let px = x_of(self.pos());
        if px >= x0 {
            painter.vline(px, ruler.top()..=full.bottom(), Stroke::new(1.5, TEXT()));
            // Tirador en la regla para arrastrar el cursor.
            let t = ruler.top();
            painter.add(egui::Shape::convex_polygon(vec![pos2(px - 7.0, t), pos2(px + 7.0, t), pos2(px + 7.0, t + 7.0), pos2(px, t + 13.0), pos2(px - 7.0, t + 7.0)], ACCENT(), Stroke::new(1.0, BG())));
        }

        // Cabeceras de pista (widgets). El fondo de la columna tiene su propio menú contextual.
        painter.rect_filled(Rect::from_min_max(full.min, pos2(x0, full.bottom())), 0.0, PANEL());
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
                for (label, h) in [("Compacta", MIN_H), ("Normal", TRACK_H), ("Grande", 160.0)] {
                    if ui.button(tr(label)).clicked() {
                        self.s.tracks.iter_mut().for_each(|t| t.height = h);
                    }
                }
            });
            ui.separator();
            self.delete_selected_item(ui);
        });
        for (i, &takes) in takes_n.iter().enumerate() {
            // Pegadas al borde izquierdo; solo las pistas de un grupo dejan sitio a la barra del grupo.
            let left = if self.s.tracks[i].group.is_some() { GROUP_W } else { 0.0 };
            let r = Rect::from_min_size(pos2(full.left() + left, lane_y(i)), vec2(header_w - left, main(i))).shrink2(vec2(4.0, 3.0));
            if r.bottom() > heads.top() && r.top() < heads.bottom() {
                self.track_header(ui, i, r, heads);
            }
            // Las tomas quedan agrupadas bajo su pista: una llave del color de la pista las une.
            if takes > 0 {
                let llave = Rect::from_x_y_ranges(full.left() + 16.0..=full.left() + 19.0, lane_y(i) + main(i) + 4.0..=lane_y(i) + lane_h(i) - 4.0);
                ui.painter().with_clip_rect(heads).rect_filled(llave, rr(2.0), self.s.tracks[i].color.gamma_multiply(0.8));
            }
            // Botones de las tomas, uno por carril.
            for k in 0..takes {
                let take_r = Rect::from_min_size(pos2(full.left() + 34.0, lane_y(i) + main(i) + k as f32 * TAKE_H + 8.0), vec2(header_w - 44.0, TAKE_H - 16.0));
                if take_r.bottom() > heads.top() && take_r.top() < heads.bottom() {
                    let active = tomas_en_uso(&self.s.tracks[i]).contains(&k);
                    let b = egui::Button::new(RichText::new(format!("{} {}", tr("Toma"), k + 1)).size(11.0).color(if active { BG() } else { TEXT_DIM() }))
                        .fill(if active { self.s.tracks[i].color } else { ELEVATED() });
                    let r = ui.put(take_r, b).on_hover_text(tr("Clic: que suene esta toma en todo lo que grabó (las otras siguen donde no se pisan) · en su carril: clic en una grabación para usarla, barrer para usar un tramo, arrastrar la barra de arriba para moverla"));
                    if r.clicked() {
                        self.edit();
                        self.usar_toma(i, k);
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
                let galley = p.layout_no_wrap(grp.name.clone(), FontId::proportional(12.0), if light { BG() } else { TEXT() });
                let (gw, gh) = (galley.size().x.min(bar.height() - 6.0), galley.size().y);
                let pos = pos2(bar.center().x - gh / 2.0, bar.center().y + gw / 2.0);
                p.with_clip_rect(bar.intersect(heads)).add(egui::epaint::TextShape::new(pos, galley, TEXT()).with_angle(-std::f32::consts::FRAC_PI_2));
                let r = ui
                    .interact(bar, Id::new(("group-bar", first)), Sense::click())
                    .on_hover_text(format!("Grupo «{}» · clic: seleccionar sus pistas · clic derecho: nombre, color y opciones", grp.name));
                if r.clicked() {
                    self.s.tracks.iter_mut().for_each(|t| t.selected = t.group == Some(g));
                }
                widgets::edit_menu(&r, |ui| self.group_menu(ui, g));
            }
            k += 1;
        }
        if let Some((_, to)) = self.track_drag {
            painter.hline(full.x_range(), lane_y(to), Stroke::new(2.0, ACCENT()));
        }
        let add = Rect::from_min_size(pos2(full.left() + 5.0, lane_y(n) + 8.0), vec2(header_w - 10.0, 30.0));
        if add.top() < full.bottom() && ui.put(add, egui::Button::new(RichText::new(tr("+  Nueva pista")).color(TEXT_DIM()))).clicked() {
            self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None));
        }
        painter.rect_filled(Rect::from_min_max(full.min, pos2(x0, lanes_top)), 0.0, PANEL());
        // Nombres de los carriles globales con un punto de su color (las ayudas están en el tooltip).
        let etiqueta = |lane: Rect, texto: &str, color: Color32| {
            painter.circle_filled(pos2(full.left() + 12.0, lane.center().y), 3.0, color);
            painter.text(pos2(full.left() + 21.0, lane.center().y), Align2::LEFT_CENTER, tr(texto), FontId::proportional(11.0), TEXT_DIM());
        };
        etiqueta(mark_lane, "Marcas", MARK);
        etiqueta(Rect::from_min_max(pos2(full.left(), ruler.top()), pos2(x0, ruler.bottom())), "Compases", TEXT_DIM());
        if arr_lane.height() > 0.0 {
            etiqueta(arr_lane, "Arreglo", METER[1]);
        }
        if chord_lane.height() > 0.0 {
            etiqueta(chord_lane, "Acordes", Color32::from_rgb(0xE0, 0xB0, 0x3A));
        }
        etiqueta(tempo_lane, "Tempo", ACCENT());
        let linea = |y: f32| painter.hline(full.left()..=x0, y, Stroke::new(1.0, BORDER().gamma_multiply(0.6)));
        for y in [mark_lane.bottom(), ruler.bottom(), arr_lane.bottom(), chord_lane.bottom(), lanes_top] {
            linea(y);
        }
        // Divisor entre cabeceras y regiones: se arrastra para ensanchar o estrechar las cabeceras.
        let div = ui.interact(Rect::from_x_y_ranges(x0 - 4.0..=x0 + 3.0, full.y_range()), Id::new("header-divider"), Sense::click_and_drag()).on_hover_text(tr("Arrastra para cambiar el ancho de las pistas"));
        if div.hovered() || div.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if div.dragged() {
            self.config.header_w = (header_w + div.drag_delta().x).clamp(HEADER_MIN, HEADER_MAX);
        }
        if div.drag_stopped() {
            self.config.save();
        }
        if div.double_clicked() {
            self.config.header_w = 0.0;
        }
        let hot = div.hovered() || div.dragged();
        painter.vline(x0 - 0.5, full.y_range(), Stroke::new(if hot { 3.0 } else { 1.0 }, if hot { ACCENT() } else { BORDER() }));
    }

    /// Carril de acordes o de arreglo: doble clic añade (o renombra), arrastrar mueve o estira por los
    /// bordes, clic en una sección selecciona su rango de tiempo, clic derecho abre las opciones.
    #[allow(clippy::too_many_arguments)]
    fn span_lane(&mut self, ui: &mut egui::Ui, lane: Rect, kind: NameKind, x_of: &dyn Fn(u64) -> f32, frame_at: &dyn Fn(f32) -> f64, snap: &dyn Fn(f64) -> u64, press: Option<Pos2>, ptr: Option<Pos2>) {
        let chord = kind == NameKind::Chord;
        let list = |s: &Session| if chord { s.chords.clone() } else { s.sections.clone() };
        let spans = list(&self.s);
        let hit = |x: f32| spans.iter().rposition(|sp| x >= x_of(sp.0) - 4.0 && x < x_of(sp.1) + 4.0);
        let tip = if chord {
            "Acordes: doble clic para añadir o cambiar · arrastra para mover o estirar por los bordes · clic derecho: opciones"
        } else {
            "Arreglo: doble clic para añadir (Coro, Estrofa…) · clic: seleccionar su rango · arrastra para mover o estirar · clic derecho: opciones"
        };
        let r = ui.interact(lane, Id::new(("span-lane", chord)), Sense::click_and_drag()).on_hover_text(tr(tip));
        let edge = |x: f32, k: usize| match () {
            _ if (x - x_of(spans[k].0)).abs() < 6.0 => 1u8,
            _ if (x - x_of(spans[k].1)).abs() < 6.0 => 2,
            _ => 0,
        };
        if let Some(p) = r.hover_pos()
            && let Some(k) = hit(p.x)
            && edge(p.x, k) > 0
        {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
        }
        if r.drag_started()
            && let Some(p) = press
            && let Some(k) = hit(p.x)
        {
            self.edit();
            let (a, b) = (spans[k].0, spans[k].1);
            self.span_drag = Some((kind, k, edge(p.x, k), frame_at(p.x) as i64 - a as i64, (a, b)));
        }
        let min_len = self.engine.beat_frames() as u64;
        if r.dragged()
            && let (Some((kd, k, mode, grab, (a, b))), Some(p)) = (self.span_drag, ptr)
            && kd == kind
        {
            let f = frame_at(p.x);
            let sp = &mut (if chord { &mut self.s.chords } else { &mut self.s.sections })[k];
            match mode {
                0 => {
                    let na = snap((f - grab as f64).max(0.0));
                    (sp.0, sp.1) = (na, na + (b - a));
                }
                1 => sp.0 = snap(f).min(b.saturating_sub(min_len)),
                _ => sp.1 = snap(f).max(a + min_len),
            }
        }
        if r.drag_stopped() && self.span_drag.is_some_and(|d| d.0 == kind) {
            self.span_drag = None;
            (if chord { &mut self.s.chords } else { &mut self.s.sections }).sort_by_key(|s| s.0);
        }
        let x = ptr.unwrap_or_default().x;
        if r.double_clicked() {
            match hit(x) {
                Some(k) => self.dialog = Some(Dialog::Name(kind, k, spans[k].2.clone())),
                None => {
                    let a = snap(frame_at(x));
                    self.add_span(kind, a, a + self.bar_frames() * if chord { 1 } else { 4 });
                }
            }
        } else if r.clicked()
            && let Some(k) = hit(x)
        {
            if !chord {
                (self.time_sel, self.razor_lanes) = (Some((spans[k].0, spans[k].1)), None);
            }
            self.go(spans[k].0);
        }
        if r.secondary_clicked() {
            self.span_ctx = Some((kind, snap(frame_at(x)), hit(x)));
        }
        widgets::edit_menu(&r, |ui| {
            let Some((kd, f, hit)) = self.span_ctx.filter(|c| c.0 == kind) else { return };
            ui.label(RichText::new(tr(if chord { "Acordes" } else { "Arreglo" })).strong());
            match hit.filter(|&k| k < spans.len()) {
                Some(k) => {
                    let (a, b) = (spans[k].0, spans[k].1);
                    if ui.button(tr("Renombrar…")).clicked() {
                        self.dialog = Some(Dialog::Name(kd, k, spans[k].2.clone()));
                        ui.close();
                    }
                    if !chord {
                        if ui.button(tr("Seleccionar su rango")).clicked() {
                            (self.time_sel, self.razor_lanes) = (Some((a, b)), None);
                            ui.close();
                        }
                        if ui.button(tr("Loop de esta sección")).clicked() {
                            self.set_loop(a, b);
                            ui.close();
                        }
                    }
                    if ui.button(tr("Duplicar a continuación")).clicked() {
                        self.edit();
                        let list = if chord { &mut self.s.chords } else { &mut self.s.sections };
                        list.push((b, b + (b - a), spans[k].2.clone()));
                        list.sort_by_key(|s| s.0);
                        ui.close();
                    }
                    if ui.button(RichText::new(tr("Quitar")).color(METER[2])).clicked() {
                        self.edit();
                        (if chord { &mut self.s.chords } else { &mut self.s.sections }).remove(k);
                        ui.close();
                    }
                }
                None => {
                    if ui.button(tr(if chord { "Añadir acorde aquí" } else { "Añadir sección aquí" })).clicked() {
                        self.add_span(kind, f, f + self.bar_frames() * if chord { 1 } else { 4 });
                        ui.close();
                    }
                }
            }
            if ui.add_enabled(!spans.is_empty(), egui::Button::new(tr("Quitar todos"))).clicked() {
                self.edit();
                (if chord { &mut self.s.chords } else { &mut self.s.sections }).clear();
                ui.close();
            }
        });
    }

    /// Dibujo de un carril de acordes o arreglo: secciones con color según su nombre; acordes en azul.
    fn draw_spans(&self, ui: &egui::Ui, lane: Rect, kind: NameKind, x_of: &dyn Fn(u64) -> f32) {
        let chord = kind == NameKind::Chord;
        let list = if chord { &self.s.chords } else { &self.s.sections };
        let p = ui.painter().with_clip_rect(lane);
        p.rect_filled(lane, 0.0, Color32::from_rgb(0x1A, 0x1A, 0x1F));
        p.hline(lane.x_range(), lane.bottom() - 0.5, Stroke::new(1.0, BORDER()));
        for (k, (a, b, name)) in list.iter().enumerate() {
            let rect = Rect::from_x_y_ranges(x_of(*a)..=x_of(*b), lane.top() + 2.0..=lane.bottom() - 2.0);
            let color = if chord { Color32::from_rgb(0x8A, 0xB4, 0xFF) } else { rgb(PALETTE[name.to_lowercase().bytes().map(|b| b as usize).sum::<usize>() % PALETTE.len()]) };
            let on = self.span_drag.is_some_and(|d| d.0 == kind && d.1 == k);
            p.rect_filled(rect, rr(5.0), color.gamma_multiply(if on { 0.6 } else { 0.32 }));
            p.rect_stroke(rect, rr(5.0), Stroke::new(1.0, color), StrokeKind::Inside);
            p.with_clip_rect(rect.intersect(lane)).text(rect.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, name, FontId::proportional(if chord { 13.5 } else { 12.5 }), TEXT());
        }
    }

    /// Menú de un grupo: nombre, color, edición vinculada, selección y disolver.
    pub fn group_menu(&mut self, ui: &mut egui::Ui, g: usize) {
        let Some(grp) = self.s.groups.get_mut(g) else {
            return;
        };
        ui.label(RichText::new(tr("Grupo")).strong());
        ui.add(egui::TextEdit::singleline(&mut grp.name).hint_text(tr("Nombre del grupo")).desired_width(200.0));
        if ui.button(tr("Renombrar…")).clicked() {
            self.dialog = Some(Dialog::Name(NameKind::Group, g, grp.name.clone()));
            ui.close();
        }
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
            p.rect_filled(row, 0.0, if k % 2 == 0 { BG() } else { PANEL() });
            for c in &t.takes[k] {
                let r = Rect::from_x_y_ranges(x_of(c.start)..=x_of(c.end()), row.top() + 3.0..=row.bottom() - 3.0);
                if r.right() < xr.0 || r.left() > xr.1 {
                    continue;
                }
                p.rect_filled(r, rr(4.0), t.color.gamma_multiply(0.06));
                // Lo que suena de esta toma, resaltado con el color de la pista.
                for u in t.clips.iter().filter(|u| sale_de(u, c)) {
                    p.rect_filled(Rect::from_x_y_ranges(x_of(u.start).max(r.left())..=x_of(u.end()).min(r.right()), r.y_range()), rr(4.0), t.color.gamma_multiply(0.28));
                }
                p.rect_stroke(r, rr(4.0), Stroke::new(1.0, t.color.gamma_multiply(0.7)), StrokeKind::Inside);
                // Barra de arriba: se agarra para mover la grabación en el tiempo.
                let barra = Rect::from_min_max(r.min, pos2(r.right(), r.top() + TOMA_BARRA));
                p.rect_filled(barra, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, t.color.gamma_multiply(0.55));
                p.with_clip_rect(barra.intersect(row)).text(barra.left_center() + vec2(5.0, 0.0), Align2::LEFT_CENTER, format!("{} {}", tr("Toma"), k + 1), FontId::proportional(9.5), TEXT());
                if let Some(b) = c.buf() {
                    let onda = Rect::from_min_max(pos2(r.left(), barra.bottom()), r.max);
                    p.add(peak_mesh(c, b, onda, xr, fpp.max(1.0), t.color.gamma_multiply(0.8)));
                }
                // Línea de ganancia de la grabación (misma escala que en las regiones).
                if c.buf().is_some() && (self.tool == Tool::Ganancia || c.ganancia.is_some()) {
                    let (alto, base) = (TAKE_H - 9.0 - TOMA_BARRA, row.top() + TAKE_H - 4.0);
                    let y = |db: f32| base - (db + 24.0) / 36.0 * alto;
                    let x = |f: u64| x_of(c.start + f.saturating_sub(c.offset));
                    let puntos = c.ganancia.as_deref().map(Vec::as_slice).unwrap_or_default();
                    let mut linea = vec![pos2(r.left(), y(engine::automation_at(puntos, c.offset).unwrap_or(0.0)))];
                    linea.extend(puntos.iter().filter(|q| q.0 >= c.offset && q.0 <= c.offset + c.len).map(|&(f, db)| pos2(x(f), y(db))));
                    linea.push(pos2(r.right(), y(engine::automation_at(puntos, c.offset + c.len).unwrap_or(0.0))));
                    let gp = p.with_clip_rect(r.intersect(p.clip_rect()));
                    gp.add(egui::Shape::line(linea, Stroke::new(if puntos.is_empty() { 1.0 } else { 1.8 }, METER[0].gamma_multiply(if puntos.is_empty() { 0.55 } else { 1.0 }))));
                    for &(f, db) in puntos {
                        gp.circle_filled(pos2(x(f), y(db)), 3.0, METER[0]);
                    }
                }
            }
            p.hline(row.x_range(), row.bottom(), Stroke::new(1.0, BORDER()));
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
        p.with_clip_rect(title.intersect(lane)).text(title.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(11.0), TEXT());
        let lane = Rect::from_min_max(pos2(lane.left(), title.bottom()), lane.max);
        if t.midi() {
            let mut held: HashMap<u8, u64> = HashMap::new();
            let note_y = |key: u8| lane.bottom() - (key.saturating_sub(36) as f32 / 60.0).min(1.0) * lane.height();
            for &(at, e) in &self.rec_midi {
                match (e[0] & 0xF0, e[2]) {
                    (0x90, v) if v > 0 => _ = held.insert(e[1], at),
                    (0x80 | 0x90, _) => {
                        if let Some(s) = held.remove(&e[1]) {
                            p.hline(x_of(s)..=x_of(at).max(x_of(s) + 2.0), note_y(e[1]), Stroke::new(3.0, TEXT()));
                        }
                    }
                    _ => {}
                }
            }
            held.into_iter().for_each(|(key, s)| _ = p.hline(x_of(s)..=x_of(self.pos()), note_y(key), Stroke::new(3.0, TEXT())));
            return;
        }
        let ich = self.engine.in_channels.max(1);
        let ((l, der), g) = (canales_entrada(t, ich), t.params.in_gain.get());
        let frames = self.rec_data.len() / ich;
        let (mid, half) = (lane.center().y, lane.height() / 2.0 - 4.0);
        let mut mesh = Mesh::default();
        for x in (r.left().max(lane.left()) as i32)..(r.right().min(lane.right()) as i32) {
            let f0 = (frame_at(x as f32) as u64).saturating_sub(start) as usize;
            let f1 = ((frame_at(x as f32 + 1.0) as u64).saturating_sub(start) as usize).min(frames);
            let stride = ((f1.saturating_sub(f0)) / 64).max(1);
            // Lo que entra por cualquiera de los dos lados (un micrófono suele ir en una sola entrada).
            let peak = (f0..f1).step_by(stride).map(|k| self.rec_data[k * ich + l].abs().max(self.rec_data[k * ich + der].abs()) * g).fold(0f32, f32::max);
            // En dB (desde -72): la voz o un instrumento se ven aunque entren bajos.
            let h = (((20.0 * peak.max(1e-6).log10() + 72.0) / 72.0).clamp(0.0, 1.0) * half).max(0.5);
            mesh.add_colored_rect(Rect::from_x_y_ranges(x as f32..=x as f32 + 1.0, mid - h..=mid + h), TEXT());
        }
        p.add(mesh);
    }

    /// Menú contextual del timeline: herramientas de región o acciones en el espacio vacío.
    pub(crate) fn region_menu(&mut self, ui: &mut egui::Ui) {
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
                if !midi {
                    ui.menu_button(tr("Edición de audio"), |ui| {
                        widgets::menu_style(ui);
                        let mut corte = None;
                        ui.menu_button(tr("Quitar silencios"), |ui| {
                            for (n, db) in [("Suave (-50 dB)", -50.0), ("Normal (-40 dB)", -40.0), ("Fuerte (-30 dB)", -30.0)] {
                                if ui.button(tr(n)).clicked() {
                                    corte = Some(edicion::Corte::Silencios(db));
                                }
                            }
                        });
                        if ui.button(tr("Cerrar huecos")).on_hover_text(tr("Junta las regiones seleccionadas de cada pista una detrás de otra")).clicked() {
                            corte = Some(edicion::Corte::CerrarHuecos);
                        }
                        if ui.button(tr("Cortar en los transientes")).on_hover_text(tr("Una región por cada golpe o ataque")).clicked() {
                            corte = Some(edicion::Corte::Transientes(0.5));
                        }
                        if ui.button(tr("Cuantizar audio a la rejilla")).on_hover_text(tr("Corta en los ataques y lleva cada golpe a la semicorchea más cercana")).clicked() {
                            corte = Some(edicion::Corte::Cuantizar(0.5));
                        }
                        if let Some(c) = corte {
                            ui.close();
                            let r = self.cortar_audio(c);
                            self.report(r.as_ref().map_or(String::new(), |n| format!("{n} {}", tr("región(es) resultantes"))), r);
                        }
                    });
                }
                if midi && ui.button(tr("Exportar región a MIDI…")).clicked() {
                    ui.close();
                    self.exportar_midi(Some((i, j)));
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
        if resp.double_clicked() && self.s.tracks[i].kind == TrackKind::Video {
            self.open_video(i);
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
        // Fila plana como en las consolas: la seleccionada se aclara, con una franja de color a la izquierda.
        p.rect_filled(r, rr(2.0), if t.selected { BORDER() } else { PANEL() });
        if resp.dnd_hover_payload::<LibItem>().is_some() {
            p.rect_stroke(r, rr(2.0), Stroke::new(1.0, ACCENT()), StrokeKind::Inside);
        }
        p.rect_filled(Rect::from_min_size(r.min, vec2(4.0, r.height())), CornerRadius { nw: 2, sw: 2, ne: 0, se: 0 }, t.color);
        p.hline(r.x_range(), r.bottom() + 1.5, Stroke::new(1.0, BG()));
        // Número de la pista, tenue, a la izquierda del icono.
        p.text(pos2(r.left() + 7.0 + NUMERO_W / 2.0, r.top() + 15.0), Align2::CENTER_CENTER, (i + 1).to_string(), FontId::proportional(if i + 1 >= 100 { 11.0 } else { 13.0 }), TEXT_DIM());
        let proxy = self.usa_proxy(i);
        let subtitulo = {
            let t = &self.s.tracks[i];
            let base = match t.kind {
                TrackKind::Midi => instruments::find(&t.instrument).map_or_else(|| pianoroll::nombre_synth(&t.instrument), |e| e.name.to_string()),
                TrackKind::Click => tr("Metrónomo como audio").to_string(),
                TrackKind::Video => t.video.as_ref().map_or(tr("sin video").to_string(), |v| format!("Video · {:.1} s", v.duration())),
                _ => format!("{} {}", t.clips.len(), tr("regiones")),
            };
            if t.congelada {
                format!("{base} · {}", tr("CONGELADA"))
            } else if proxy {
                format!("{base} · PROXY")
            } else {
                base
            }
        };
        let zona = Rect::from_min_max(r.min + vec2(NUMERO_W + 8.0, 4.0), r.max - vec2(6.0, 4.0));
        ui.scope_builder(UiBuilder::new().max_rect(zona), |ui| {
            ui.set_clip_rect(clip.intersect(r));
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            // Fila 1: icono, nombre, plugins y el menú «…».
            ui.horizontal(|ui| {
                {
                    let t = &mut self.s.tracks[i];
                    ui.allocate_ui_with_layout(vec2(20.0, 20.0), egui::Layout::centered_and_justified(egui::Direction::TopDown), |ui| icons::picker(ui, t, 18.0));
                }
                let t = &mut self.s.tracks[i];
                let nombre = egui::TextEdit::singleline(&mut t.name).frame(egui::Frame::NONE).font(FontId::proportional(13.5)).text_color(TEXT()).desired_width((zona.width() - 92.0).max(60.0));
                ui.add(nombre).on_hover_text(tr("Nombre de la pista (clic para editar)"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mas = ui.add(egui::Button::new(RichText::new("···").size(13.0).color(TEXT_DIM())).frame(false)).on_hover_text(tr("Opciones de la pista"));
                    egui::Popup::menu(&mas).show(|ui| {
                        widgets::menu_style(ui);
                        self.track_menu(ui, i);
                    });
                    if !self.config.ocultar_plugins_pista && self.admite_efectos(i) {
                        self.boton_plugins(ui, i);
                    }
                });
            });
            // Fila 2: M S ● I R W en teclas cuadradas y el medidor horizontal.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let lado = vec2(widgets::TECLA_CHICA, widgets::TECLA_CHICA);
                for (k, texto, ayuda) in [(1, "M", "Mute"), (2, "S", "Solo"), (0, "", "Armar grabación"), (3, "I", "Monitoreo de entrada: escucharte en vivo por los auriculares o monitores")] {
                    let on = flag(&self.s.tracks[i].params, k).load(Relaxed);
                    // Grabar: un punto dibujado (la fuente no trae el círculo).
                    let tecla = if k == 0 {
                        widgets::tecla_dibujo(ui, lado, on, widgets::color_bandera(k), |p, r, tinta| _ = p.circle_filled(r.center(), 4.0, if on { tinta } else { METER[2] }))
                    } else {
                        widgets::tecla(ui, lado, on, widgets::color_bandera(k), texto)
                    };
                    if tecla.on_hover_text(tr(ayuda)).clicked() {
                        for m in self.flag_targets(i, k) {
                            flag(&self.s.tracks[m].params, k).store(!on, Relaxed);
                        }
                    }
                }
                // R y W: leer y escribir automatización (los modos finos siguen en el carril de automatización).
                let modo = self.s.tracks[i].auto_mode;
                if widgets::tecla(ui, lado, modo != 1, ACCENT(), "R").on_hover_text(tr("Leer la automatización")).clicked() {
                    self.s.tracks[i].auto_mode = if modo == 1 { 0 } else { 1 };
                }
                if widgets::tecla(ui, lado, modo == 4, METER[1], "W").on_hover_text(tr("Escribir la automatización mientras suena")).clicked() {
                    self.s.tracks[i].auto_mode = if modo == 4 { 0 } else { 4 };
                }
                if r.height() < 74.0 {
                    // Pista compacta: el volumen (con su medidor) va en esta misma fila.
                    let ancho = ui.available_width().max(30.0);
                    self.fader_de_pista(ui, i, ancho, false);
                    return;
                }
                // Medidor de la pista en una ranura oscura (entrada si está armada o monitoreando).
                let t = &self.s.tracks[i];
                let entrada = matches!(t.kind, TrackKind::AudioMono | TrackKind::AudioStereo) && (t.params.arm.load(Relaxed) || t.params.monitor.load(Relaxed));
                let nivel = if entrada { self.in_levels.get(i) } else { self.levels.get(i) }.copied().unwrap_or(0.0);
                let (ranura, _) = ui.allocate_exact_size(vec2(ui.available_width().max(10.0), 8.0), Sense::hover());
                ui.painter().rect_filled(ranura, rr(2.0), chrome::FONDO_HONDO);
                let db = widgets::db(nivel);
                let fill = ((db + 60.0) / 66.0).clamp(0.0, 1.0);
                ui.painter().rect_filled(Rect::from_min_max(ranura.min, pos2(ranura.left() + fill * ranura.width(), ranura.bottom())), rr(2.0), METER[(db > -12.0) as usize + (db > -3.0) as usize]);
            });
            if r.height() < 74.0 {
                return;
            }
            // Fila 3: volumen y panorama, visibles en la altura normal (también en el inspector y el mixer).
            ui.horizontal(|ui| {
                let ancho = (zona.width() - 48.0).max(60.0);
                self.fader_de_pista(ui, i, ancho, true);
            });
            if r.height() < 104.0 {
                return;
            }
            // Fila 4: entrada, tomas y el subtítulo.
            ui.horizontal(|ui| {
                if matches!(self.s.tracks[i].kind, TrackKind::AudioMono | TrackKind::AudioStereo) {
                    self.input_combo(ui, i);
                }
                // Versiones grabadas (tomas), con la flecha que pliega o despliega sus carriles.
                if self.s.tracks[i].takes.len() > 1 {
                    let t = &mut self.s.tracks[i];
                    let ayuda = if t.show_takes { "Plegar las tomas (lo elegido sigue sonando igual)" } else { "Desplegar las tomas de esta pista" };
                    // Botón con triángulo dibujado (la fuente no trae flechas): abajo = desplegadas.
                    let texto = format!("{} {}", t.takes.len(), tr("tomas"));
                    let b = egui::Button::new(RichText::new(format!("    {texto}")).size(11.0).color(TEXT())).fill(if t.show_takes { t.color.gamma_multiply(0.35) } else { ELEVATED() }).corner_radius(rr(5.0)).min_size(vec2(0.0, 18.0));
                    let f = ui.add(b);
                    let (c, k) = (f.rect.left_center() + vec2(10.0, 0.0), 4.0);
                    let pts = if t.show_takes { vec![c + vec2(-k, -k * 0.6), c + vec2(k, -k * 0.6), c + vec2(0.0, k * 0.7)] } else { vec![c + vec2(-k * 0.6, -k), c + vec2(k * 0.7, 0.0), c + vec2(-k * 0.6, k)] };
                    ui.painter().add(egui::Shape::convex_polygon(pts, TEXT(), Stroke::NONE));
                    if f.on_hover_text(tr(ayuda)).clicked() {
                        t.show_takes = !t.show_takes;
                    }
                    self.boton_versiones(ui, i);
                }
                ui.add(egui::Label::new(RichText::new(&subtitulo).size(11.0).color(if proxy { METER[0] } else { TEXT_DIM() })).truncate());
            });
            if self.show_auto && r.height() >= 134.0 {
                let t = &mut self.s.tracks[i];
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("auto", i)).width(70.0).selected_text(tr(["Volumen", "Panorama", "Ganancia"][t.auto_param.min(2) as usize])).show_ui(ui, |ui| {
                        ui.selectable_value(&mut t.auto_param, 0, tr("Volumen"));
                        ui.selectable_value(&mut t.auto_param, 1, tr("Panorama"));
                        ui.selectable_value(&mut t.auto_param, 2, tr("Ganancia")).on_hover_text(tr("Línea de ganancia de toda la pista, antes de los efectos (de -24 a +12 dB)"));
                    });
                    // Modo de escritura: con la vista de automatización abierta, «Lectura» graba como «Toque».
                    const MODOS: [&str; 5] = ["Toque", "Apagado", "Toque", "Retención", "Escritura"];
                    egui::ComboBox::from_id_salt(("auto-modo", i)).width(80.0).selected_text(tr(MODOS[t.auto_mode.min(4) as usize])).show_ui(ui, |ui| {
                        for (k, ayuda) in [(2u8, "Graba mientras mueves el fader, la perilla o un control del teclado"), (3, "Graba desde que tocas un control hasta parar"), (4, "Graba siempre mientras se reproduce"), (1, "Ignora la automatización")] {
                            ui.selectable_value(&mut t.auto_mode, k, tr(MODOS[k as usize])).on_hover_text(tr(ayuda));
                        }
                    });
                });
            }
        });
    }

    /// Fader de volumen de la cabecera (con el medidor dentro) y, si `con_pan`, la perilla de panorama.
    /// Armada o monitoreando, una pista de audio muestra lo que entra por su entrada (para ver si llega
    /// sonido antes de grabar); si no, lo que suena.
    fn fader_de_pista(&mut self, ui: &mut egui::Ui, i: usize, ancho: f32, con_pan: bool) {
        let params = self.s.tracks[i].params.clone();
        let id = self.s.tracks[i].id;
        let t = &self.s.tracks[i];
        let entrada = matches!(t.kind, TrackKind::AudioMono | TrackKind::AudioStereo) && (params.arm.load(Relaxed) || params.monitor.load(Relaxed));
        let nivel = if entrada { self.in_levels.get(i) } else { self.levels.get(i) }.copied();
        let f = widgets::fader_h(ui, &params.gain, ancho, nivel);
        crate::controlador::menu(&f, crate::controlador::Pedido::Destino(crate::controlador::Destino::Volumen(id)));
        if con_pan {
            let mut pan = params.pan.get();
            let p = widgets::pan_knob_chica(ui, &mut pan);
            crate::controlador::menu(&p, crate::controlador::Pedido::Destino(crate::controlador::Destino::Panorama(id)));
            if p.changed() {
                params.pan.set(pan);
            }
        }
    }

    /// Botón de la cabecera de pista: el instrumento y los plugins que usa, para abrirlos, apagarlos,
    /// quitarlos o agregar otros sin salir de la pista.
    fn boton_plugins(&mut self, ui: &mut egui::Ui, i: usize) {
        let n = self.s.tracks[i].fx.len();
        let midi = self.s.tracks[i].midi();
        let activo = n > 0 || midi;
        let texto = if n > 0 { format!("FX {n}") } else { "FX".to_string() };
        // Misma altura que las teclas M, S y R de al lado.
        (ui.spacing_mut().interact_size.y, ui.spacing_mut().button_padding.y) = (widgets::TECLA_CHICA, 0.0);
        let b = egui::Button::new(RichText::new(texto).strong().size(10.5).color(if activo { BG() } else { TEXT_DIM() })).fill(if activo { ACCENT() } else { Color32::from_white_alpha(10) }).min_size(vec2(2.0 * widgets::TECLA_CHICA, widgets::TECLA_CHICA)).corner_radius(rr((widgets::TECLA_CHICA * 0.27).round()));
        let (mut quitar, mut agregar, mut abrir, mut instrumento) = (None, None, false, false);
        egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
            widgets::menu_style(ui);
            if midi {
                ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM()));
                let t = &self.s.tracks[i];
                let nombre = if instruments::es_real(&t.instrument) { instruments::nombre(&t.instrument) } else { pianoroll::nombre_synth(&t.instrument) };
                if ui.button(nombre).on_hover_text(tr("Abrir el instrumento")).clicked() {
                    instrumento = true;
                }
                ui.separator();
            }
            ui.label(RichText::new(tr("PLUGINS")).size(10.0).color(TEXT_DIM()));
            if n == 0 {
                ui.label(RichText::new(tr("Sin plugins en esta pista")).size(11.5).color(TEXT_DIM()));
            }
            for (k, fx) in self.s.tracks[i].fx.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut on = !fx.bypass.load(Relaxed);
                    if ui.checkbox(&mut on, "").on_hover_text(tr("Encendido / bypass")).changed() {
                        fx.bypass.store(!on, Relaxed);
                    }
                    if ui.button(fx.nombre()).on_hover_text(tr("Abrir el plugin")).clicked() {
                        abrir = true;
                    }
                    if ui.small_button("✕").on_hover_text(tr("Quitar")).clicked() {
                        quitar = Some(k);
                    }
                });
            }
            ui.menu_button(tr("+ Agregar plugin"), |ui| {
                widgets::menu_style(ui);
                for k in FxKind::disponibles() {
                    if ui.button(k.name()).clicked() {
                        agregar = Some(k);
                    }
                }
            });
        });
        if instrumento {
            self.abrir_instrumento(i);
        }
        if abrir {
            self.abrir_efectos(i);
        }
        if let Some(k) = quitar {
            self.edit();
            self.s.tracks[i].fx.remove(k);
            self.dirty = true;
        }
        if let Some(k) = agregar {
            self.add_fx(i, k);
            self.abrir_efectos(i);
        }
    }

    /// Al soltar una grabación de la toma `k` que se movió desde `antes`: lo que sonaba de ella en la
    /// pista se mueve con ella (y recorta lo que quede debajo en su nuevo lugar).
    fn grabacion_movida(&mut self, i: usize, k: usize, j: usize, antes: u64) {
        let xf = self.xfade();
        let t = &mut self.s.tracks[i];
        let Some(nueva) = t.takes.get(k).and_then(|tk| tk.get(j)).cloned() else { return };
        if nueva.start == antes {
            return;
        }
        let vieja = Clip { start: antes, ..nueva.clone() };
        let mover = |x: u64| (x as i64 + nueva.start as i64 - antes as i64).max(0) as u64;
        let tramos: Vec<_> = t.clips.iter().filter(|c| sale_de(c, &vieja)).map(|c| (mover(c.start), mover(c.end()))).collect();
        t.clips.retain(|c| !sale_de(c, &vieja));
        for (a, b) in tramos {
            poner_tramo(&mut t.clips, &nueva, a, b, xf);
        }
        for s in t.comp.iter_mut().filter(|s| s.2 == k && s.0 >= antes && s.1 <= vieja.end()) {
            (s.0, s.1) = (mover(s.0), mover(s.1));
        }
        self.dirty = true;
    }

    /// La región `j` de la pista `i` se movió desde `antes`: si su audio es el de una grabación de toma,
    /// el mismo tramo de la grabación (el material que usa la región) queda debajo de ella, en su nueva
    /// posición. La grabación se corta en esos bordes, así lo demás que suena de ella queda en su lugar.
    fn toma_sigue_a_region(&mut self, i: usize, j: usize, antes: u64) {
        let Some(t) = self.s.tracks.get_mut(i) else { return };
        let Some(c) = t.clips.get(j).cloned() else { return };
        if c.start == antes || t.takes.is_empty() {
            return;
        }
        let mismo = |a: &Clip, b: &Clip| matches!((a.buf(), b.buf()), (Some(x), Some(y)) if Arc::ptr_eq(x, y) || x.file == y.file);
        // Material (frames del archivo) que usa la región.
        let (m0, m1) = (c.offset, c.offset + c.len);
        for toma in t.takes.iter_mut() {
            let Some(jj) = toma.iter().position(|src| mismo(&c, src) && src.offset < m1 && src.offset + src.len > m0) else { continue };
            let src = toma.remove(jj);
            let en = |m: u64| (src.origin() + m as i64).max(0) as u64;
            let (a, b) = (en(m0.max(src.offset)), en(m1.min(src.offset + src.len)));
            let medio = src.trim(a, b).map(|m| Clip { start: (c.origin() + m.offset as i64).max(0) as u64, ..m });
            let partes: Vec<Clip> = [src.trim(0, a), medio, src.trim(b, u64::MAX)].into_iter().flatten().collect();
            toma.extend(partes);
            toma.sort_by_key(|c| c.start);
            break;
        }
        self.dirty = true;
    }

    /// Se soltó el borde de una grabación de toma: lo que suena de ella en la pista sigue a ese borde
    /// (se prolonga o se acorta) y nada de lo que suena queda fuera de la grabación.
    fn borde_de_toma_soltado(&mut self, i: usize, k: usize, j: usize, orig: &Clip) {
        let t = &mut self.s.tracks[i];
        let Some(nueva) = t.takes.get(k).and_then(|tk| tk.get(j)).cloned() else { return };
        let mut out = vec![];
        for c in t.clips.drain(..) {
            if !sale_de(&c, orig) {
                out.push(c);
                continue;
            }
            let mut c = c;
            if c.end() == orig.end() && nueva.end() > c.start {
                c.len = nueva.end() - c.start;
            }
            if c.start == orig.start && nueva.start < c.end() {
                let d = nueva.start as i64 - c.start as i64;
                (c.start, c.offset, c.len) = (nueva.start, (c.offset as i64 + d).max(0) as u64, (c.len as i64 - d) as u64);
            }
            out.extend(c.trim(nueva.start, nueva.end()));
        }
        out.sort_by_key(|c| c.start);
        t.clips = out;
        self.dirty = true;
    }

    /// Cambia la línea de ganancia de una región o de una grabación de toma (pista, toma, región). En
    /// una toma, lo que suena de esa grabación en la pista toma la misma línea.
    fn poner_ganancia(&mut self, (i, k, j): (usize, Option<usize>, usize), l: Vec<(u64, f32)>) {
        let linea = (!l.is_empty()).then(|| Arc::new(l));
        let Some(t) = self.s.tracks.get_mut(i) else { return };
        match k {
            None => {
                if let Some(c) = t.clips.get_mut(j) {
                    c.ganancia = linea;
                }
            }
            Some(k) => {
                let Some(src) = t.takes.get_mut(k).and_then(|tk| tk.get_mut(j)) else { return };
                src.ganancia = linea.clone();
                let src = src.clone();
                t.clips.iter_mut().filter(|c| sale_de(c, &src)).for_each(|c| c.ganancia = linea.clone());
            }
        }
        self.dirty = true;
    }

    /// Quita de la pista lo que suena de la grabación `j` de la toma `k`.
    fn quitar_lo_que_suena(&mut self, i: usize, k: usize, j: usize) {
        let t = &mut self.s.tracks[i];
        if let Some(src) = t.takes.get(k).and_then(|tk| tk.get(j)).cloned() {
            t.clips.retain(|c| !sale_de(c, &src));
        }
    }

    /// Donde se borró una toma vuelve a sonar lo que había antes: lo que grabaron `tomas` (las que
    /// quedan, la más nueva primero) en esos tramos.
    fn devolver_con(&mut self, i: usize, tomas: &[Vec<Clip>], tramos: &[(u64, u64)]) {
        let (xf, min) = (self.xfade(), (self.sr() / 20.0) as u64);
        let t = &mut self.s.tracks[i];
        // Si quedó una sola toma la lista ya se vació: igual se rellena con ella.
        let guardadas = std::mem::replace(&mut t.takes, tomas.to_vec());
        for &r in tramos {
            completar_tomas(t, xf, min, r);
        }
        t.takes = guardadas;
    }

    /// Quita la toma `k` de la lista (los tramos del comp que la usaban pasan a la primera).
    fn quitar_toma(&mut self, i: usize, k: usize) {
        let t = &mut self.s.tracks[i];
        t.takes.remove(k);
        t.comp = t.comp.iter().map(|&(a, b, x)| (a, b, if x == k { 0 } else if x > k { x - 1 } else { x })).collect();
        if t.takes.len() <= 1 {
            (t.takes, t.comp, t.show_takes) = (vec![], vec![], false);
        }
        self.dirty = true;
    }

    /// Menú de una grabación en un carril de tomas: usarla, escucharla sola o borrarla.
    fn menu_toma(&mut self, ui: &mut egui::Ui, i: usize, k: usize, j: usize) {
        let Some(c) = self.s.tracks.get(i).and_then(|t| t.takes.get(k)).and_then(|tk| tk.get(j)).cloned() else {
            ui.close();
            return;
        };
        ui.label(RichText::new(format!("{} {}", tr("Toma"), k + 1)).strong());
        if ui.button(tr("Usar esta grabación entera")).clicked() {
            self.edit();
            self.comp_take(i, c.start, c.end(), k);
            ui.close();
        }
        if ui.button(tr("Usar esta toma en todo lo que grabó")).clicked() {
            self.edit();
            self.usar_toma(i, k);
            ui.close();
        }
        if ui.button(tr("Escuchar desde aquí")).clicked() {
            self.go(c.start);
            ui.close();
        }
        ui.separator();
        if ui.button(tr("Borrar esta grabación de la toma")).on_hover_text(tr("Quita solo esta grabación; las otras tomas quedan igual")).clicked() {
            self.edit();
            self.quitar_lo_que_suena(i, k, j);
            self.s.tracks[i].takes[k].remove(j);
            let quedan = self.s.tracks[i].takes.clone();
            if self.s.tracks[i].takes[k].is_empty() {
                self.quitar_toma(i, k);
            }
            self.devolver_con(i, &quedan, &[(c.start, c.end())]);
            self.dirty = true;
            ui.close();
        }
    }

    /// La toma `k` manda en todo lo que grabó; fuera de eso suena lo que ya sonaba.
    fn usar_toma(&mut self, i: usize, k: usize) {
        let tramos: Vec<_> = self.s.tracks[i].takes[k].iter().map(|c| (c.start, c.end())).collect();
        for (a, b) in tramos {
            self.comp_take(i, a, b, k);
        }
        self.status = format!("{} {} · {}", tr("Suena la toma"), k + 1, self.s.tracks[i].name);
    }

    /// Selector del canal de la interfaz que alimenta la pista.
    /// Versiones de una pista de audio: cada vez que se graba encima queda una toma nueva en la misma
    /// pista. Se elige cuál suena entera, o se arma una mezcla por tramos en los carriles de tomas.
    fn boton_versiones(&mut self, ui: &mut egui::Ui, i: usize) {
        let t = &self.s.tracks[i];
        let n = t.takes.len();
        // Tomas que suenan en la pista: si es una sola, se nombra; si no, es una mezcla por tramos.
        let usadas = tomas_en_uso(t);
        let entera = (usadas.len() == 1).then(|| usadas[0]);
        let texto = match entera {
            Some(k) => format!("{} {}", tr("Toma"), k + 1),
            None => tr("Mezcla").to_string(),
        };
        let b = egui::Button::new(RichText::new(texto).size(11.0).strong().color(BG())).fill(ACCENT()).corner_radius(rr(5.0)).min_size(vec2(64.0, 18.0));
        let (mut elegida, mut carriles, mut fijar, mut borrar) = (None, false, false, None);
        egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
            widgets::menu_style(ui);
            ui.label(RichText::new(tr("VERSIONES GRABADAS")).size(10.0).color(TEXT_DIM()));
            for k in (0..n).rev() {
                let nombre = if k == 0 { format!("{} 1 · {}", tr("Toma"), tr("original")) } else { format!("{} {}", tr("Toma"), k + 1) };
                ui.horizontal(|ui| {
                    if ui.selectable_label(usadas.contains(&k), nombre).on_hover_text(tr("Que mande esta toma: suena entera, y las otras solo donde esta no grabó")).clicked() {
                        elegida = Some(k);
                        ui.close();
                    }
                    if ui.small_button(tr("Borrar")).on_hover_text(tr("Quita esta toma de la pista")).clicked() {
                        borrar = Some(k);
                        ui.close();
                    }
                });
            }
            ui.separator();
            if ui.button(tr("Quedarme solo con lo que suena…")).on_hover_text(tr("Quita las tomas de la lista y deja la pista como está. Los archivos quedan en el proyecto.")).clicked() {
                fijar = true;
                ui.close();
            }
            let texto = if self.s.tracks[i].show_takes { "Plegar las tomas" } else { "Desplegar las tomas (elegir por tramos)" };
            if ui.button(tr(texto)).on_hover_text(tr("En los carriles de tomas: clic en una grabación para usarla, barre para usar un tramo y arrastra su barra de arriba para moverla. Plegar no cambia lo elegido.")).clicked() {
                carriles = true;
                ui.close();
            }
        })
        .0
        .on_hover_text(tr("Versiones de esta pista: graba encima y elige la mejor toma sin crear otra pista"));
        if let Some(k) = elegida {
            self.edit();
            self.usar_toma(i, k);
        }
        if carriles {
            let t = &mut self.s.tracks[i];
            t.show_takes = !t.show_takes;
        }
        if let Some(k) = borrar {
            self.edit();
            let tramos: Vec<_> = self.s.tracks[i].takes[k].iter().map(|c| (c.start, c.end())).collect();
            // Las tomas que quedan, antes de que la lista se vacíe si queda una sola.
            let quedan: Vec<_> = self.s.tracks[i].takes.iter().enumerate().filter(|e| e.0 != k).map(|e| e.1.clone()).collect();
            for j in 0..self.s.tracks[i].takes[k].len() {
                self.quitar_lo_que_suena(i, k, j);
            }
            self.quitar_toma(i, k);
            self.devolver_con(i, &quedan, &tramos);
            self.status = format!("{} {} {}", tr("Toma"), k + 1, tr("borrada"));
        }
        if fijar {
            self.edit();
            let t = &mut self.s.tracks[i];
            (t.takes, t.comp, t.show_takes) = (vec![], vec![], false);
            self.status = tr("Listo: quedó solo lo que suena; las otras tomas se quitaron de la pista").into();
        }
    }

    /// Botón de entrada de una pista de audio: de qué canales de la interfaz entra el sonido al grabar
    /// o monitorear. Se ve en la cabecera, el mixer y el inspector.
    pub fn input_combo(&mut self, ui: &mut egui::Ui, i: usize) {
        self.input_combo_ancho(ui, i, 58.0);
    }

    /// Selector de entrada con el ancho dado (en el mixer ocupa todo el canal).
    pub fn input_combo_ancho(&mut self, ui: &mut egui::Ui, i: usize, ancho: f32) {
        let (n, t) = (self.engine.in_channels, &self.s.tracks[i]);
        let stereo = t.kind == TrackKind::AudioStereo;
        // Par estéreo (In 1-2) o una entrada sola (In 2: un micrófono, que suena al centro).
        let label = |c: u16| match n {
            0 => tr("Sin entrada").to_string(),
            _ if stereo && c & ENTRADA_MONO == 0 && ((c & 0xFF) as usize) + 1 < n => format!("In {}-{}", c + 1, c + 2),
            _ => format!("In {}", (c & 0xFF) + 1),
        };
        let actual = t.input;
        let texto = RichText::new(label(actual)).size(11.0).strong().color(if n == 0 { METER[1] } else { TEXT() });
        let b = egui::Button::new(texto).fill(ELEVATED()).stroke(Stroke::new(1.0, BORDER())).corner_radius(rr(2.0)).min_size(vec2(ancho, 18.0));
        let (mut elegido, mut configurar) = (None, false);
        egui::containers::menu::MenuButton::from_button(b).ui(ui, |ui| {
            widgets::menu_style(ui);
            ui.label(RichText::new(tr("ENTRADA DE AUDIO")).size(10.0).color(TEXT_DIM()));
            let dispositivo = if self.audio.input.is_empty() { tr("Entrada predeterminada").to_string() } else { self.audio.input.clone() };
            ui.label(RichText::new(dispositivo).size(11.5).color(TEXT_DIM()));
            if n == 0 {
                ui.label(RichText::new(tr("No hay entrada de audio abierta")).color(METER[1]));
            }
            if stereo && n > 1 {
                ui.label(RichText::new(tr("Par estéreo (teclado, línea)")).size(10.0).color(TEXT_DIM()));
                for c in (0..n as u16 - 1).step_by(2) {
                    if ui.selectable_label(c == actual, label(c)).clicked() {
                        elegido = Some(c);
                        ui.close();
                    }
                }
                ui.label(RichText::new(tr("Una entrada (micrófono, guitarra): suena al centro")).size(10.0).color(TEXT_DIM()));
            }
            for c in 0..n as u16 {
                let v = if stereo { c | ENTRADA_MONO } else { c };
                if ui.selectable_label(v == actual || (!stereo && c == actual), label(v)).clicked() {
                    elegido = Some(v);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr("Cambiar dispositivo de entrada…")).clicked() {
                configurar = true;
                ui.close();
            }
        })
        .0
        .on_hover_text(tr("Entrada: de dónde entra el sonido de esta pista"));
        if let Some(c) = elegido.filter(|&c| c != actual) {
            (self.s.tracks[i].input, self.dirty) = (c, true);
        }
        if configurar {
            self.dialog = Some(Dialog::Settings(self.audio.clone(), self.rec_bits, engine::audio_devices(), engine::midi_ports(), self.config.midi_port.clone()));
        }
    }

    /// Botones R/M/S/I: afectan a las pistas seleccionadas si esta lo está; si no, solo a ella.
    pub fn flag_buttons(&mut self, ui: &mut egui::Ui, i: usize) {
        let buttons = [(1, "M", "Mute"), (2, "S", "Solo"), (0, "R", "Armar grabación"), (3, "I", "Monitoreo de entrada")];
        for (k, label, tip) in buttons {
            let on = flag(&self.s.tracks[i].params, k).load(Relaxed);
            if widgets::tecla(ui, vec2(widgets::TECLA, widgets::TECLA), on, widgets::color_bandera(k), label).on_hover_text(tr(tip)).clicked() {
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
        let resp = widgets::tecla_dibujo(ui, vec2(widgets::TECLA, widgets::TECLA), on, METER[1], |p, r, c| {
            for dx in [-2.5, 2.5] {
                p.circle_stroke(r.center() + vec2(dx, 0.0), 4.5, Stroke::new(1.4, c));
            }
        });
        if resp.on_hover_text(tr("Invertir polaridad (fase) del audio")).clicked() {
            inv.store(!on, Relaxed);
            self.dirty = true;
        }
    }

    pub fn fx_button(&mut self, ui: &mut egui::Ui, i: usize) {
        let n = self.s.tracks[i].fx.len();
        let texto = if n > 0 { format!("FX {n}") } else { "FX".into() };
        if widgets::tecla(ui, vec2(2.0 * widgets::TECLA, widgets::TECLA), n > 0, ACCENT(), &texto).on_hover_text(tr("Instrumento y efectos de la pista")).clicked() {
            self.abrir_efectos(i);
        }
    }

    pub fn track_menu(&mut self, ui: &mut egui::Ui, i: usize) {
        if ui.button(tr("Instrumento y efectos…")).clicked() {
            self.abrir_efectos(i);
        }
        if Self::drummer_disponible() && self.s.tracks[i].midi() && ui.button(tr("Abrir Q Drums…")).clicked() {
            self.abrir_drummer(Some(i));
            ui.close();
        }
        if ui.button(tr("Ruteo (ROUTE)…")).clicked() {
            self.route_window = Some(i);
        }
        if self.s.tracks[i].midi() && ui.button(tr("Convertir MIDI en audio")).on_hover_text(tr("Crea una pista de audio con el sonido del instrumento y sus efectos; la pista MIDI queda silenciada")).clicked() {
            let r = self.midi_a_audio(i);
            self.report(tr("MIDI convertido en audio"), r);
            ui.close();
        }
        if self.s.tracks[i].congelada {
            if ui.button(tr("Descongelar la batería")).on_hover_text(tr("Vuelve a tocar el MIDI en vivo para poder editarlo")).clicked() {
                self.descongelar(i);
                ui.close();
            }
        } else if self.s.tracks[i].midi() && !self.canales_de(i).is_empty() && ui.button(tr("Congelar la batería en audio")).on_hover_text(tr("Cada canal suena desde su audio y el instrumento no se calcula: ahorra procesador")).clicked() {
            let r = self.congelar(i);
            self.report(tr("Batería congelada en audio"), r);
            ui.close();
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
