//! Piano roll: edición de notas de una región MIDI con teclado, rejilla y carril de velocidades.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Id, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

const KEYS_W: f32 = 64.0;
/// Instrumentos preestablecidos del QUANTUM Synth: (nombre, parámetros en el orden de `SYNTH_PARAMS`).
pub const PRESETS: [(&str, [f32; 10]); 7] = [
    ("Piano eléctrico", [3.0, 4.0, 3500.0, 0.1, 0.3, 2.0, 900.0, 0.25, 400.0, -6.0]),
    ("Bajo sintetizado", [0.0, 6.0, 700.0, 0.45, 0.5, 2.0, 250.0, 0.6, 120.0, -4.0]),
    ("Pad cálido", [0.0, 14.0, 1800.0, 0.2, 0.2, 600.0, 1500.0, 0.8, 1500.0, -10.0]),
    ("Lead brillante", [1.0, 10.0, 5000.0, 0.35, 0.4, 5.0, 300.0, 0.7, 200.0, -9.0]),
    ("Órgano", [2.0, 3.0, 6000.0, 0.05, 0.0, 5.0, 50.0, 1.0, 80.0, -8.0]),
    ("Pluck", [0.0, 7.0, 1200.0, 0.5, 0.9, 1.0, 220.0, 0.0, 200.0, -6.0]),
    ("Cuerdas", [0.0, 18.0, 2600.0, 0.15, 0.15, 300.0, 800.0, 0.85, 900.0, -10.0]),
];
const BLACK: [bool; 12] = [false, true, false, true, false, false, true, false, true, false, true, false];

/// Arrastre en la rejilla: las notas guardan su estado al empezar (índice, nota original).
#[derive(Clone)]
enum RollDrag {
    Move(Vec<(usize, Note)>, egui::Vec2),
    /// Borde derecho: cambia la duración.
    ResizeEnd(Vec<(usize, Note)>, f32),
    /// Borde izquierdo: mueve el inicio y mantiene el final.
    ResizeStart(Vec<(usize, Note)>, f32),
    /// Recuadro de selección desde un punto; `sumar` conserva la selección previa (Shift).
    Select(egui::Pos2, Vec<usize>),
}

pub struct Roll {
    pub target: Option<(usize, usize)>,
    /// Píxeles por tiempo, alto de cada tecla y divisiones de la rejilla por tiempo.
    ppb: f32,
    key_h: f32,
    grid: u32,
    scroll: egui::Vec2,
    /// Notas seleccionadas (índices).
    sel: Vec<usize>,
    drag: Option<RollDrag>,
    /// Duración de la última nota (en tiempos), usada al crear notas nuevas.
    last_len: f64,
    pub sounding: Option<u8>,
    /// Tocar con el teclado del computador y su octava base.
    pub typing: bool,
    pub octave: i32,
    /// Escritura por pasos: cada nota tocada se escribe en el cursor y este avanza.
    pub step: bool,
    /// Carril de control bajo la rejilla: qué muestra (velocidad, ganancia o ataque) y su alto.
    lane: Carril,
    lane_h: f32,
    /// Último punto del arrastre en el carril, para pintar varias barras de un trazo.
    lane_prev: Option<egui::Pos2>,
}

#[derive(Clone, Copy, PartialEq)]
enum Carril {
    Velocidad,
    Ganancia,
    Ataque,
}

impl Carril {
    const TODOS: [(Carril, &'static str); 3] = [(Carril::Velocidad, "Velocidad"), (Carril::Ganancia, "Ganancia"), (Carril::Ataque, "Ataque")];

    fn valor(self, n: &Note) -> u8 {
        match self {
            Carril::Velocidad => n.vel,
            Carril::Ganancia => n.gain,
            Carril::Ataque => n.attack,
        }
    }

    fn poner(self, n: &mut Note, v: u8) {
        match self {
            Carril::Velocidad => n.vel = v.max(1),
            Carril::Ganancia => n.gain = v,
            Carril::Ataque => n.attack = v,
        }
    }

    /// Valor legible: velocidad 1–127, ganancia en dB, ataque en milisegundos.
    fn texto(self, v: u8) -> String {
        match self {
            Carril::Velocidad => v.to_string(),
            Carril::Ganancia if v == 0 => "-∞ dB".into(),
            Carril::Ganancia => format!("{:+.1} dB", 20.0 * engine::ganancia_nota(v).log10()),
            Carril::Ataque if v == 0 => tr("instrumento").into(),
            Carril::Ataque => format!("{:.0} ms", engine::ataque_ms(v)),
        }
    }
}

impl Default for Roll {
    fn default() -> Self {
        Self { target: None, ppb: 90.0, key_h: 12.0, grid: 4, scroll: vec2(0.0, (127.0 - 76.0) * 12.0), sel: Vec::new(), drag: None, last_len: 1.0, sounding: None, typing: false, octave: 5, step: false, lane: Carril::Velocidad, lane_h: 110.0, lane_prev: None }
    }
}

impl Roll {
    pub fn open(&mut self, i: usize, j: usize) {
        (self.target, self.sel, self.drag) = (Some((i, j)), Vec::new(), None)
    }
    pub fn close(&mut self) {
        (self.target, self.sel, self.drag) = (None, Vec::new(), None)
    }
    pub fn has_selection(&self) -> bool {
        self.target.is_some() && !self.sel.is_empty()
    }
}

impl App {
    fn roll_clip(&self) -> Option<(usize, usize)> {
        self.roll.target.filter(|&(i, j)| self.s.tracks.get(i).is_some_and(|t| t.clips.get(j).is_some_and(|c| c.notes().is_some())))
    }

    /// Sustituye las notas de la región (y la alarga si alguna se sale).
    pub fn set_notes(&mut self, notes: Vec<Note>) {
        let Some((i, j)) = self.roll_clip() else {
            return;
        };
        let c = &mut self.s.tracks[i].clips[j];
        let end = notes.iter().map(|n| (c.origin() + (n.start + n.len) as i64).max(0) as u64).max().unwrap_or(0);
        c.len = c.len.max(end.saturating_sub(c.start));
        c.src = Source::Midi(Arc::new(notes));
        self.dirty = true;
    }

    /// Escritura por pasos: añade la nota en el cursor (duración = rejilla) y avanza el cursor.
    pub fn step_note(&mut self, key: u8) {
        let Some((i, j)) = self.roll_clip() else {
            return;
        };
        let step = (self.engine.beat_frames() / self.roll.grid as f64) as u64;
        let c = &self.s.tracks[i].clips[j];
        let (at, origin) = (self.pos().max(c.start), c.origin());
        let mut notes = c.notes().map(|n| (**n).clone()).unwrap_or_default();
        self.edit();
        notes.push(Note::new((at as i64 - origin).max(0) as u64, step, key, 100));
        self.set_notes(notes);
        self.go(at + step);
    }

    /// Selector de instrumento: instrumentos reales (descarga bajo demanda) y sonidos del QUANTUM Synth.
    pub fn instrument_combo(&mut self, ui: &mut egui::Ui, i: usize) {
        enum Pick {
            Preset(usize),
            Real(&'static str),
        }
        let t = &self.s.tracks[i];
        let current = match (instruments::find(&t.instrument), t.instrument.strip_prefix("synth:")) {
            (Some(e), _) if t.sampler.is_some() => e.name.to_string(),
            (Some(e), _) => format!("{} (preparando…)", e.name),
            (None, Some(p)) => format!("Synth · {p}"),
            _ => "QUANTUM Synth".into(),
        };
        let mut pick = None;
        egui::ComboBox::from_id_salt(("inst", i)).width(200.0).height(520.0).selected_text(current).show_ui(ui, |ui| {
            ui.label(RichText::new(tr("INSTRUMENTOS REALES · gratis y libres")).size(10.0).color(TEXT_DIM));
            let mut family = "";
            for e in instruments::CATALOG.iter() {
                if e.family != family {
                    family = e.family;
                    ui.label(RichText::new(family).strong().size(11.0).color(ACCENT));
                }
                let st = self.downloads.get(e.id).and_then(|s| s.lock().ok().map(|s| s.clone())).unwrap_or_default();
                let label = match (instruments::installed(e.id), st.as_str()) {
                    (true, _) => e.name.to_string(),
                    (false, "") | (false, "Listo") => {
                        format!("{} · descargar {} MB", e.name, e.mb)
                    }
                    (false, st) => format!("{} · {st}", e.name),
                };
                if ui.selectable_label(self.s.tracks[i].instrument == e.id, label).on_hover_text(format!("{} · licencia {}", e.credit, e.license)).clicked() {
                    pick = Some(Pick::Real(e.id));
                }
            }
            ui.separator();
            ui.label(RichText::new(tr("QUANTUM SYNTH")).size(10.0).color(TEXT_DIM));
            for (k, (name, _)) in PRESETS.iter().enumerate() {
                if ui.selectable_label(self.s.tracks[i].instrument == format!("synth:{name}"), *name).clicked() {
                    pick = Some(Pick::Preset(k));
                }
            }
        });
        match pick {
            Some(Pick::Real(id)) => self.set_instrument(i, id),
            Some(Pick::Preset(k)) => {
                let (name, values) = PRESETS[k];
                let t = &mut self.s.tracks[i];
                if let Some(s) = &t.synth {
                    s.params.iter().zip(values).for_each(|(a, v)| a.set(v));
                }
                (t.instrument, t.sampler, self.dirty) = (format!("synth:{name}"), None, true);
            }
            None => {}
        }
    }

    pub fn roll_delete_selected(&mut self) {
        let Some((i, j)) = self.roll_clip().filter(|_| !self.roll.sel.is_empty()) else {
            return;
        };
        let notes = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
        self.edit();
        let sel = std::mem::take(&mut self.roll.sel);
        self.set_notes(notes.into_iter().enumerate().filter(|(k, _)| !sel.contains(k)).map(|(_, n)| n).collect());
    }

    fn preview(&mut self, key: Option<u8>) {
        if let Some(k) = self.roll.sounding.take() {
            self.engine.send_midi([0x80, k, 0]);
        }
        if let Some(k) = key {
            self.engine.send_midi([0x90, k, 100]);
            self.roll.sounding = Some(k);
        }
    }

    pub fn piano_roll(&mut self, ui: &mut egui::Ui) {
        let Some((i, j)) = self.roll_clip() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Piano roll")).size(18.0).strong());
                ui.label(RichText::new(tr("Haz doble clic en una región MIDI para editarla, o en un carril MIDI vacío para crear una.")).color(TEXT_DIM));
                if ui.button(tr("+ Crear pista MIDI")).clicked() {
                    self.add_track(format!("MIDI {}", self.s.tracks.len() + 1), TrackKind::Midi);
                    let i = self.s.tracks.len() - 1;
                    self.new_midi_region(i, self.pos());
                }
            });
            return;
        };
        let beat = self.engine.beat_frames();
        let step = beat / self.roll.grid as f64;
        // Barra de herramientas.
        ui.horizontal(|ui| {
            let t = &self.s.tracks[i];
            ui.label(RichText::new(&t.name).strong().color(t.color));
            ui.separator();
            ui.label(tr("Rejilla"));
            egui::ComboBox::from_id_salt("roll-grid").width(60.0).selected_text(format!("1/{}", self.roll.grid * 4)).show_ui(ui, |ui| {
                for g in [1, 2, 4, 8] {
                    ui.selectable_value(&mut self.roll.grid, g, format!("1/{}", g * 4));
                }
            });
            if ui.button(tr("Cuantizar")).on_hover_text(tr("Ajusta el inicio de las notas a la rejilla")).clicked() {
                let notes = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
                self.edit();
                let origin = self.s.tracks[i].clips[j].origin();
                let q = |s: u64| (((origin + s as i64) as f64 / step).round() * step) as i64 - origin;
                self.set_notes(notes.into_iter().map(|n| Note { start: q(n.start).max(0) as u64, ..n }).collect());
            }
            ui.add(egui::Slider::new(&mut self.roll.ppb, 20.0..=400.0).show_value(false).text(tr("Zoom")));
            ui.separator();
            // Instrumento virtual de la pista y teclado del computador para tocar y grabar.
            ui.label(tr("Instrumento"));
            self.instrument_combo(ui, i);
            if ui.button(tr("Editar sonido")).clicked() {
                self.fx_window = Some(i);
            }
            let typing = egui::Button::selectable(self.roll.typing, "Teclado del PC");
            if ui.add(typing).on_hover_text(tr("Toca con A W S E D F T G Y H U J K O L P · Z/X cambian de octava · con R y grabación activa, las notas se escriben en la pista")).clicked() {
                self.roll.typing = !self.roll.typing;
            }
            if self.roll.typing {
                ui.label(RichText::new(format!("Octava Do{}", self.roll.octave - 1)).size(11.0).color(ACCENT));
            }
            let step = egui::Button::selectable(self.roll.step, "Escribir por pasos");
            if ui.add(step).on_hover_text(tr("Cada nota que toques (teclado del PC o teclas de la izquierda) se escribe en el cursor con la duración de la rejilla, y el cursor avanza")).clicked() {
                self.roll.step = !self.roll.step;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Cerrar")).clicked() {
                    self.roll.close();
                }
            });
        });
        let Some((i, j)) = self.roll_clip() else {
            return;
        };
        let area = ui.available_rect_before_wrap();
        ui.allocate_rect(area, Sense::hover());
        let lane_h = self.roll.lane_h.clamp(50.0, (area.height() * 0.6).max(50.0));
        let keys = Rect::from_min_max(area.min, pos2(area.left() + KEYS_W, area.bottom() - lane_h));
        let grid = Rect::from_min_max(pos2(keys.right(), area.top()), pos2(area.right(), area.bottom() - lane_h));
        // Carril de control: separador arrastrable arriba, pestañas a la izquierda y barras a la derecha.
        let vel = Rect::from_min_max(pos2(keys.right(), grid.bottom() + 6.0), area.max);
        let lane_tabs = Rect::from_min_max(pos2(area.left(), grid.bottom() + 6.0), pos2(keys.right(), area.bottom()));
        if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| area.contains(*p)) {
            let (zoom, d) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            let t = (p.x - grid.left() + self.roll.scroll.x) / self.roll.ppb;
            self.roll.ppb = (self.roll.ppb * zoom).clamp(20.0, 400.0);
            self.roll.scroll.x = t * self.roll.ppb - (p.x - grid.left());
            self.roll.scroll -= d;
            self.roll.scroll = vec2(self.roll.scroll.x.max(0.0), self.roll.scroll.y.clamp(0.0, 128.0 * self.roll.key_h - grid.height()));
        }
        let (ppb, kh, scroll) = (self.roll.ppb, self.roll.key_h, self.roll.scroll);
        let clip = self.s.tracks[i].clips[j].clone();
        let (origin, color) = (clip.origin(), self.s.tracks[i].color);
        let notes: Vec<Note> = clip.notes().map(|n| (**n).clone()).unwrap_or_default();
        // Coordenadas: frames relativos al inicio de la región ↔ x; nota MIDI ↔ y.
        let x_of = |rel: f64| grid.left() + (rel / beat) as f32 * ppb - scroll.x;
        let rel_at = |x: f32| ((x - grid.left() + scroll.x) / ppb) as f64 * beat;
        let y_of = |key: u8| grid.top() + (127 - key) as f32 * kh - scroll.y;
        let key_at = |y: f32| (127.0 - ((y - grid.top() + scroll.y) / kh).floor()).clamp(0.0, 127.0) as u8;
        let rel_of = |n: &Note| (origin + n.start as i64 - clip.start as i64) as f64;
        let note_rect = |n: &Note| Rect::from_x_y_ranges(x_of(rel_of(n))..=x_of(rel_of(n) + n.len as f64).max(x_of(rel_of(n)) + 3.0), y_of(n.key) + 1.0..=y_of(n.key) + kh - 1.0);

        // Interacción con la rejilla: clic en una nota la selecciona (Shift suma), sus bordes la estiran
        // o acortan por cualquiera de los dos lados, arrastrar en vacío dibuja un recuadro de selección
        // y el doble clic en vacío crea una nota.
        let gr = ui.interact(grid, Id::new("roll-grid-area"), Sense::click_and_drag());
        let (ptr, pressed, shift) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_pressed(), i.modifiers.shift));
        const BORDE: f32 = 6.0;
        let hit_at = |p: egui::Pos2| notes.iter().rposition(|n| note_rect(n).expand2(vec2(2.0, 0.0)).contains(p));
        let borde = |r: Rect, p: egui::Pos2| {
            let b = BORDE.min(r.width() / 3.0);
            if p.x <= r.left() + b {
                -1
            } else if p.x >= r.right() - b {
                1
            } else {
                0
            }
        };
        if let Some(p) = ptr.filter(|p| grid.contains(*p) && self.roll.drag.is_none())
            && let Some(k) = hit_at(p)
            && borde(note_rect(&notes[k]), p) != 0
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if gr.double_clicked()
            && let Some(p) = ptr.filter(|p| hit_at(*p).is_none())
        {
            self.edit();
            let rel = (rel_at(p.x) / step).floor() * step;
            let n = Note::new((rel + clip.start as f64 - origin as f64).max(0.0) as u64, (self.roll.last_len * beat) as u64, key_at(p.y), 100);
            let mut v = notes.clone();
            v.push(n);
            self.roll.sel = vec![v.len() - 1];
            self.set_notes(v);
            self.roll.drag = None;
            self.preview(Some(n.key));
        } else if gr.hovered()
            && pressed
            && let Some(p) = ptr
        {
            match hit_at(p) {
                Some(k) => {
                    if !self.roll.sel.contains(&k) {
                        if !shift {
                            self.roll.sel.clear();
                        }
                        self.roll.sel.push(k);
                    } else if shift {
                        self.roll.sel.retain(|&x| x != k);
                    }
                    self.edit();
                    let orig: Vec<(usize, Note)> = self.roll.sel.iter().filter_map(|&x| notes.get(x).map(|n| (x, *n))).collect();
                    self.roll.drag = Some(match borde(note_rect(&notes[k]), p) {
                        -1 => RollDrag::ResizeStart(orig, 0.0),
                        1 => RollDrag::ResizeEnd(orig, 0.0),
                        _ => RollDrag::Move(orig, egui::Vec2::ZERO),
                    });
                    self.preview(Some(notes[k].key));
                }
                None => {
                    let previa = if shift { self.roll.sel.clone() } else { Vec::new() };
                    self.roll.sel = previa.clone();
                    self.roll.drag = Some(RollDrag::Select(p, previa));
                }
            }
        }
        if gr.dragged()
            && let Some(d) = &mut self.roll.drag
        {
            let delta = gr.drag_delta();
            let mut v = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
            let mut key = None;
            let snap = |f: f64| (f / step).round() * step;
            match d {
                RollDrag::Move(orig, acc) => {
                    *acc += delta;
                    let dk = -(acc.y / kh).round() as i32;
                    let dt = snap((acc.x / ppb) as f64 * beat);
                    for (k, o) in orig.iter() {
                        if let Some(n) = v.get_mut(*k) {
                            let new_key = (o.key as i32 + dk).clamp(0, 127) as u8;
                            if n.key != new_key && orig.len() == 1 {
                                key = Some(new_key);
                            }
                            (n.start, n.key) = ((o.start as f64 + dt).max(0.0) as u64, new_key);
                        }
                    }
                }
                RollDrag::ResizeEnd(orig, acc) => {
                    *acc += delta.x;
                    let dt = snap((*acc / ppb) as f64 * beat);
                    for (k, o) in orig.iter() {
                        if let Some(n) = v.get_mut(*k) {
                            n.len = (o.len as f64 + dt).max(step) as u64;
                            self.roll.last_len = n.len as f64 / beat;
                        }
                    }
                }
                RollDrag::ResizeStart(orig, acc) => {
                    *acc += delta.x;
                    let dt = snap((*acc / ppb) as f64 * beat);
                    for (k, o) in orig.iter() {
                        if let Some(n) = v.get_mut(*k) {
                            let end = (o.start + o.len) as f64;
                            let start = (o.start as f64 + dt).clamp(0.0, end - step.min(o.len as f64));
                            (n.start, n.len) = (start as u64, (end - start) as u64);
                            self.roll.last_len = n.len as f64 / beat;
                        }
                    }
                }
                RollDrag::Select(from, previa) => {
                    if let Some(p) = ptr {
                        let r = Rect::from_two_pos(*from, p);
                        let mut sel = previa.clone();
                        sel.extend((0..notes.len()).filter(|k| note_rect(&notes[*k]).intersects(r) && !previa.contains(k)));
                        self.roll.sel = sel;
                    }
                    v.clear();
                }
            }
            if !v.is_empty() {
                self.set_notes(v);
            }
            if key.is_some() {
                self.preview(key);
            }
        }
        // Recuadro de selección en curso.
        if let (Some(RollDrag::Select(from, _)), Some(p)) = (&self.roll.drag, ptr) {
            let r = Rect::from_two_pos(*from, p);
            ui.painter().with_clip_rect(grid).rect(r, 0.0, ACCENT.gamma_multiply(0.12), Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
        }
        if !ui.input(|i| i.pointer.primary_down()) && self.roll.drag.take().is_some() {
            self.preview(None);
        }
        // Ctrl+A: seleccionar todas las notas de la región.
        if gr.hovered() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            self.roll.sel = (0..notes.len()).collect();
        }
        if gr.secondary_clicked()
            && let Some(k) = ptr.and_then(|p| notes.iter().rposition(|n| note_rect(n).contains(p)))
        {
            self.roll.sel = vec![k];
            self.roll_delete_selected();
        }

        // Teclado: clic para escuchar.
        let kr = ui.interact(keys, Id::new("roll-keys"), Sense::drag());
        if kr.is_pointer_button_down_on() {
            let k = ptr.map(|p| key_at(p.y));
            if k != self.roll.sounding {
                self.preview(k);
                if let Some(k) = k.filter(|_| self.roll.step) {
                    self.step_note(k);
                }
            }
        } else if self.roll.drag.is_none() && self.roll.sounding.is_some() {
            self.preview(None);
        }

        // Separador: arrastrarlo cambia el alto del carril de control.
        let sep = Rect::from_min_max(pos2(area.left(), grid.bottom()), pos2(area.right(), grid.bottom() + 6.0));
        let sr = ui.interact(sep, Id::new("roll-lane-sep"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if sr.dragged() {
            self.roll.lane_h = (lane_h - sr.drag_delta().y).clamp(50.0, (area.height() * 0.6).max(50.0));
        }
        // Pestañas del carril.
        let mut tab_y = lane_tabs.top() + 4.0;
        for (c, nombre) in Carril::TODOS {
            let r = Rect::from_min_size(pos2(lane_tabs.left() + 4.0, tab_y), vec2(lane_tabs.width() - 8.0, 20.0));
            if ui.interact(r, Id::new(("roll-lane-tab", nombre)), Sense::click()).clicked() {
                self.roll.lane = c;
            }
            tab_y += 23.0;
        }
        // Barras: arrastrar pinta el valor de todas las notas por las que pasa el ratón; en una nota
        // sola basta con hacer clic a la altura deseada.
        let carril = self.roll.lane;
        let valor_en = |y: f32| ((1.0 - (y - vel.top() - 4.0) / (vel.height() - 8.0)) * 127.0).clamp(0.0, 127.0).round() as u8;
        let vr = ui.interact(vel, Id::new("roll-vel"), Sense::click_and_drag());
        if (vr.dragged() || vr.clicked() || vr.drag_started())
            && let Some(p) = ptr
        {
            let prev = self.roll.lane_prev.unwrap_or(p);
            let (x0, x1) = (prev.x.min(p.x) - 4.0, prev.x.max(p.x) + 4.0);
            let mut v = notes.clone();
            let mut cambio = false;
            for n in v.iter_mut() {
                let (a, b) = (x_of(rel_of(n)), x_of(rel_of(n) + n.len as f64).max(x_of(rel_of(n)) + 6.0));
                // La nota está bajo el trazo si su barra se cruza con el tramo recorrido.
                if b >= x0 && a <= x1 {
                    let t = if (p.x - prev.x).abs() > 1.0 { ((a - prev.x) / (p.x - prev.x)).clamp(0.0, 1.0) } else { 1.0 };
                    let nuevo = valor_en(prev.y + (p.y - prev.y) * t);
                    if carril.valor(n) != nuevo {
                        carril.poner(n, nuevo);
                        cambio = true;
                    }
                }
            }
            if vr.drag_started() || vr.clicked() {
                self.edit();
            }
            if cambio {
                self.set_notes(v);
            }
            self.roll.lane_prev = Some(p);
        }
        if !vr.dragged() && !vr.is_pointer_button_down_on() {
            self.roll.lane_prev = None;
        }

        // ---------- Dibujo ----------
        let notes: Vec<Note> = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
        let p = ui.painter_at(area);
        p.rect_filled(area, 0.0, BG);
        let gp = p.with_clip_rect(grid);
        for key in 0..128u8 {
            let y = y_of(key);
            if y > grid.bottom() || y + kh < grid.top() {
                continue;
            }
            if BLACK[key as usize % 12] {
                gp.rect_filled(Rect::from_x_y_ranges(grid.x_range(), y..=y + kh), 0.0, Color32::from_black_alpha(60));
            }
            if key % 12 == 0 {
                gp.hline(grid.x_range(), y + kh, Stroke::new(1.0, BORDER));
            }
        }
        let beats = self.engine.beats.load(Relaxed).max(1) as i64;
        let first = (rel_at(grid.left()) / step) as i64;
        let mut k = first;
        while x_of(k as f64 * step) < grid.right() {
            let x = x_of(k as f64 * step);
            let per_beat = self.roll.grid as i64;
            let stroke = if k % (per_beat * beats) == 0 {
                Stroke::new(1.0, TEXT_DIM.gamma_multiply(0.5))
            } else if k % per_beat == 0 {
                Stroke::new(1.0, BORDER)
            } else {
                Stroke::new(1.0, PANEL)
            };
            gp.vline(x, grid.y_range(), stroke);
            if k % (per_beat * beats) == 0 {
                gp.text(pos2(x + 3.0, grid.top() + 2.0), Align2::LEFT_TOP, format!("{}", k / (per_beat * beats) + 1), FontId::monospace(10.0), TEXT_DIM);
            }
            k += 1;
        }
        let end_x = x_of(clip.len as f64);
        gp.rect_filled(Rect::from_min_max(pos2(end_x, grid.top()), grid.max), 0.0, Color32::from_black_alpha(90));
        for (k, n) in notes.iter().enumerate() {
            let r = note_rect(n);
            let c = color.gamma_multiply(0.45 + 0.55 * n.vel as f32 / 127.0);
            gp.rect_filled(r, rr(3.0), c);
            if self.roll.sel.contains(&k) {
                gp.rect_stroke(r, rr(3.0), Stroke::new(1.5, TEXT), StrokeKind::Inside);
            }
            if r.width() > 26.0 && kh >= 10.0 {
                gp.text(r.left_center() + vec2(3.0, 0.0), Align2::LEFT_CENTER, NOTE_NAMES[n.key as usize % 12], FontId::proportional(9.0), BG);
            }
        }
        // Notas que se están grabando en vivo (teclado MIDI o del PC) sobre esta pista.
        if self.recording() && self.s.tracks[i].params.arm.load(Relaxed) {
            let mut held: HashMap<u8, u64> = HashMap::new();
            let draw = |key: u8, a: u64, b: u64| {
                let xa = x_of(a as f64 - clip.start as f64);
                let r = Rect::from_x_y_ranges(xa..=x_of(b as f64 - clip.start as f64).max(xa + 3.0), y_of(key) + 1.0..=y_of(key) + kh - 1.0);
                gp.rect_filled(r, rr(3.0), METER[2].gamma_multiply(0.8));
            };
            for &(at, e) in &self.rec_midi {
                match (e[0] & 0xF0, e[2]) {
                    (0x90, v) if v > 0 => _ = held.insert(e[1], at),
                    (0x80 | 0x90, _) => {
                        if let Some(s) = held.remove(&e[1]) {
                            draw(e[1], s, at);
                        }
                    }
                    _ => {}
                }
            }
            held.into_iter().for_each(|(key, s)| draw(key, s, self.pos()));
        }
        let px = x_of(self.pos() as f64 - clip.start as f64);
        if (grid.left()..grid.right()).contains(&px) {
            gp.vline(px, grid.y_range(), Stroke::new(1.5, TEXT));
        }
        // Nombres de las piezas (batería y percusión) sobre sus teclas.
        if let Some(s) = &self.s.tracks[i].sampler {
            for (key, name) in &s.key_names {
                gp.text(pos2(grid.left() + 4.0, y_of(*key) + kh / 2.0), Align2::LEFT_CENTER, instruments::spanish(name), FontId::proportional(10.0), TEXT_DIM);
            }
        }
        // Teclado del piano.
        let kp = p.with_clip_rect(keys);
        for key in 0..128u8 {
            let y = y_of(key);
            let black = BLACK[key as usize % 12];
            let fill = if self.roll.sounding == Some(key) {
                ACCENT
            } else if black {
                Color32::from_gray(30)
            } else {
                Color32::from_gray(225)
            };
            kp.rect_filled(Rect::from_x_y_ranges(keys.left()..=keys.right() - if black { 20.0 } else { 0.0 }, y..=y + kh - 1.0), rr(2.0), fill);
            if key % 12 == 0 {
                kp.text(pos2(keys.right() - 4.0, y + kh / 2.0), Align2::RIGHT_CENTER, format!("Do{}", key as i32 / 12 - 1), FontId::proportional(9.0), BG);
            }
        }
        // Carril de control.
        let full = Rect::from_min_max(pos2(area.left(), grid.bottom()), area.max);
        let vp = p.with_clip_rect(full);
        vp.rect_filled(full, 0.0, PANEL);
        vp.hline(full.x_range(), grid.bottom() + 3.0, Stroke::new(2.0, if sr.hovered() || sr.dragged() { ACCENT } else { BG }));
        let y_val = |v: u8| vel.bottom() - 4.0 - v as f32 / 127.0 * (vel.height() - 8.0);
        // Líneas guía: 25 %, 50 %, 75 % y máximo, con su valor a la izquierda.
        for v in [32u8, 64, 96, 127] {
            vp.hline(vel.x_range(), y_val(v), Stroke::new(1.0, Color32::from_white_alpha(14)));
        }
        let mut tab_y = lane_tabs.top() + 4.0;
        for (c, nombre) in Carril::TODOS {
            let r = Rect::from_min_size(pos2(lane_tabs.left() + 4.0, tab_y), vec2(lane_tabs.width() - 8.0, 20.0));
            let activo = c == carril;
            vp.rect_filled(r, rr(4.0), if activo { ACCENT } else { Color32::from_white_alpha(8) });
            vp.text(r.center(), Align2::CENTER_CENTER, tr(nombre), FontId::proportional(11.0), if activo { BG } else { TEXT_DIM });
            tab_y += 23.0;
        }
        let bp = p.with_clip_rect(vel);
        let hover = ptr.filter(|p| vel.contains(*p));
        let mut etiqueta = None;
        for (k, n) in notes.iter().enumerate() {
            let a = x_of(rel_of(n));
            let ancho = (x_of(rel_of(n) + n.len as f64) - a - 2.0).clamp(3.0, 9.0);
            let v = carril.valor(n);
            let top = y_val(v);
            let sel = self.roll.sel.contains(&k);
            let c = if sel { Color32::WHITE } else { color.gamma_multiply(0.55 + 0.45 * v as f32 / 127.0) };
            bp.rect_filled(Rect::from_min_max(pos2(a, top), pos2(a + ancho, vel.bottom() - 4.0)), rr(1.5), c.gamma_multiply(0.75));
            bp.rect_filled(Rect::from_min_max(pos2(a, top - 1.5), pos2(a + ancho, top + 1.5)), 0.0, c);
            if hover.is_some_and(|h| (a - 3.0..=a + ancho + 3.0).contains(&h.x)) || (sel && vr.dragged()) {
                etiqueta = Some((pos2(a + ancho + 4.0, top), v));
            }
        }
        if let Some((pos, v)) = etiqueta {
            let g = bp.layout_no_wrap(carril.texto(v), FontId::proportional(11.0), Color32::WHITE);
            let r = Rect::from_min_size(pos2(pos.x, (pos.y - g.size().y / 2.0).clamp(vel.top(), vel.bottom() - g.size().y)), g.size()).expand(3.0);
            bp.rect_filled(r, rr(3.0), Color32::from_black_alpha(200));
            bp.galley(r.min + vec2(3.0, 3.0), g, Color32::WHITE);
        }
    }
}
