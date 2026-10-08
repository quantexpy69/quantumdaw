//! Piano roll: edición de notas de una región MIDI con teclado, rejilla y carril de velocidades.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Id, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

const KEYS_W: f32 = 64.0;
const VEL_H: f32 = 60.0;
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

#[derive(Clone, Copy)]
enum RollDrag {
    Move(usize, Note, egui::Vec2),
    Resize(usize, Note, f32),
}

pub struct Roll {
    pub target: Option<(usize, usize)>,
    /// Píxeles por tiempo, alto de cada tecla y divisiones de la rejilla por tiempo.
    ppb: f32,
    key_h: f32,
    grid: u32,
    scroll: egui::Vec2,
    sel: Option<usize>,
    drag: Option<RollDrag>,
    /// Duración de la última nota (en tiempos), usada al crear notas nuevas.
    last_len: f64,
    pub sounding: Option<u8>,
    /// Tocar con el teclado del computador y su octava base.
    pub typing: bool,
    pub octave: i32,
    /// Escritura por pasos: cada nota tocada se escribe en el cursor y este avanza.
    pub step: bool,
}

impl Default for Roll {
    fn default() -> Self {
        Self { target: None, ppb: 90.0, key_h: 12.0, grid: 4, scroll: vec2(0.0, (127.0 - 76.0) * 12.0), sel: None, drag: None, last_len: 1.0, sounding: None, typing: false, octave: 5, step: false }
    }
}

impl Roll {
    pub fn open(&mut self, i: usize, j: usize) {
        (self.target, self.sel, self.drag) = (Some((i, j)), None, None)
    }
    pub fn close(&mut self) {
        (self.target, self.sel, self.drag) = (None, None, None)
    }
    pub fn has_selection(&self) -> bool {
        self.target.is_some() && self.sel.is_some()
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
        notes.push(Note { start: (at as i64 - origin).max(0) as u64, len: step, key, vel: 100 });
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
        let (Some((i, j)), Some(k)) = (self.roll_clip(), self.roll.sel) else {
            return;
        };
        let mut notes = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
        if k < notes.len() {
            self.edit();
            notes.remove(k);
            self.roll.sel = None;
            self.set_notes(notes);
        }
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
        let keys = Rect::from_min_max(area.min, pos2(area.left() + KEYS_W, area.bottom() - VEL_H));
        let grid = Rect::from_min_max(pos2(keys.right(), area.top()), pos2(area.right(), area.bottom() - VEL_H));
        let vel = Rect::from_min_max(pos2(keys.right(), grid.bottom()), area.max);
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

        // Interacción con la rejilla.
        let gr = ui.interact(grid, Id::new("roll-grid-area"), Sense::click_and_drag());
        let (ptr, pressed) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_pressed()));
        if gr.hovered()
            && pressed
            && let Some(p) = ptr
        {
            self.edit();
            let hit = notes.iter().rposition(|n| note_rect(n).contains(p));
            self.roll.drag = Some(match hit {
                Some(k) if p.x > note_rect(&notes[k]).right() - 6.0 => RollDrag::Resize(k, notes[k], 0.0),
                Some(k) => RollDrag::Move(k, notes[k], egui::Vec2::ZERO),
                None => {
                    let rel = (rel_at(p.x) / step).floor() * step;
                    let n = Note { start: (rel + clip.start as f64 - origin as f64).max(0.0) as u64, len: (self.roll.last_len * beat) as u64, key: key_at(p.y), vel: 100 };
                    let mut v = notes.clone();
                    v.push(n);
                    self.set_notes(v);
                    RollDrag::Resize(notes.len(), n, 0.0)
                }
            });
            self.roll.sel = Some(match self.roll.drag {
                Some(RollDrag::Move(k, ..) | RollDrag::Resize(k, ..)) => k,
                None => 0,
            });
            let key = self.roll.drag.map(|d| match d {
                RollDrag::Move(_, n, _) | RollDrag::Resize(_, n, _) => n.key,
            });
            self.preview(key);
        }
        if gr.dragged()
            && let Some(d) = &mut self.roll.drag
        {
            let delta = gr.drag_delta();
            let mut v = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
            let mut key = None;
            match d {
                RollDrag::Move(k, orig, acc) => {
                    *acc += delta;
                    let start = ((orig.start as f64 + (acc.x / ppb) as f64 * beat) / step).round() * step;
                    let new_key = (orig.key as f32 - (acc.y / kh).round()).clamp(0.0, 127.0) as u8;
                    if let Some(n) = v.get_mut(*k) {
                        if n.key != new_key {
                            key = Some(new_key);
                        }
                        (n.start, n.key) = (start.max(0.0) as u64, new_key);
                    }
                }
                RollDrag::Resize(k, orig, acc) => {
                    *acc += delta.x;
                    let len = (((orig.len as f64 + (*acc / ppb) as f64 * beat) / step).round() * step).max(step);
                    if let Some(n) = v.get_mut(*k) {
                        n.len = len as u64;
                    }
                    self.roll.last_len = len / beat;
                }
            }
            self.set_notes(v);
            if key.is_some() {
                self.preview(key);
            }
        }
        if !ui.input(|i| i.pointer.primary_down()) && self.roll.drag.take().is_some() {
            self.preview(None);
        }
        if gr.secondary_clicked()
            && let Some(k) = ptr.and_then(|p| notes.iter().rposition(|n| note_rect(n).contains(p)))
        {
            self.roll.sel = Some(k);
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

        // Velocidades: arrastrar sobre una barra cambia la velocidad de esa nota.
        let vr = ui.interact(vel, Id::new("roll-vel"), Sense::click_and_drag());
        if (vr.dragged() || vr.clicked())
            && let Some(p) = ptr
        {
            let near = notes.iter().enumerate().min_by(|a, b| (x_of(rel_of(a.1)) - p.x).abs().total_cmp(&(x_of(rel_of(b.1)) - p.x).abs()));
            if let Some((k, n)) = near.filter(|(_, n)| (x_of(rel_of(n)) - p.x).abs() < 12.0) {
                if vr.drag_started() || vr.clicked() {
                    self.edit();
                }
                let mut v = notes.clone();
                v[k].vel = ((1.0 - (p.y - vel.top()) / vel.height()) * 127.0).clamp(1.0, 127.0) as u8;
                let _ = n;
                self.set_notes(v);
            }
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
            if self.roll.sel == Some(k) {
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
        // Carril de velocidades.
        let vp = p.with_clip_rect(vel);
        vp.rect_filled(vel, 0.0, PANEL);
        vp.text(pos2(area.left() + 6.0, vel.center().y), Align2::LEFT_CENTER, tr("Velocidad"), FontId::proportional(10.0), TEXT_DIM);
        for n in &notes {
            let x = x_of(rel_of(n));
            let h = n.vel as f32 / 127.0 * (vel.height() - 6.0);
            vp.vline(x + 1.0, vel.bottom() - h..=vel.bottom(), Stroke::new(3.0, color));
            vp.circle_filled(pos2(x + 1.0, vel.bottom() - h), 3.0, color);
        }
    }
}
