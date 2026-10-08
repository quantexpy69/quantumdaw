//! Ruteo estilo REAPER: salida al master, envíos post-fader entre canales y buses.
use crate::{widgets::rr, *};
use egui::{Id, RichText};

impl App {
    fn index_of(&self, id: u64) -> Option<usize> {
        self.s.tracks.iter().position(|t| t.id == id)
    }

    /// ¿Crearía un ciclo enviar de `from` a `to`? (si `to` ya llega a `from` por sus envíos)
    pub fn would_cycle(&self, from: usize, to: usize) -> bool {
        let mut stack = vec![to];
        let mut seen = vec![false; self.s.tracks.len()];
        while let Some(i) = stack.pop() {
            if i == from {
                return true;
            }
            if !std::mem::replace(&mut seen[i], true) {
                stack.extend(self.s.tracks[i].sends.iter().filter_map(|s| self.index_of(s.0)));
            }
        }
        false
    }

    /// Orden de proceso: cada canal antes que los canales a los que envía (orden topológico).
    pub fn process_order(&self) -> Vec<usize> {
        let n = self.s.tracks.len();
        let mut incoming = vec![0; n];
        for t in &self.s.tracks {
            t.sends.iter().filter_map(|s| self.index_of(s.0)).for_each(|j| incoming[j] += 1);
        }
        let mut ready: Vec<usize> = (0..n).filter(|&i| incoming[i] == 0).rev().collect();
        let mut order = vec![];
        while let Some(i) = ready.pop() {
            order.push(i);
            for j in self.s.tracks[i].sends.iter().filter_map(|s| self.index_of(s.0)) {
                incoming[j] -= 1;
                if incoming[j] == 0 {
                    ready.push(j);
                }
            }
        }
        // Por seguridad, cualquier pista que quedara fuera (ciclo) se añade al final.
        let missing: Vec<usize> = (0..n).filter(|i| !order.contains(i)).collect();
        order.extend(missing);
        order
    }

    pub fn route_button(&mut self, ui: &mut egui::Ui, i: usize) {
        let t = &self.s.tracks[i];
        let routed = !t.sends.is_empty() || !t.to_master;
        let text = RichText::new(tr("ROUTE")).size(10.5).strong().color(if routed { ACCENT } else { TEXT_DIM });
        if ui.add(egui::Button::new(text).fill(BG).min_size(egui::vec2(46.0, 22.0)).corner_radius(rr(11.0))).on_hover_text(tr("Ruteo: salida, envíos y buses")).clicked() {
            self.route_window = Some(i);
        }
    }

    /// Crea un bus con reverb y envía la pista `i` hacia él.
    fn new_reverb_bus(&mut self, i: usize) {
        self.add_track("Bus Reverb".into(), TrackKind::AudioStereo);
        let bus = self.s.tracks.len() - 1;
        let fx = Fx::new(FxKind::Reverb, self.engine.sample_rate);
        fx.params[2].set(1.0);
        self.s.tracks[bus].fx.push(Arc::new(fx));
        let id = self.s.tracks[bus].id;
        self.s.tracks[i].sends.push((id, 0.5));
        self.s.tracks[i].selected = true;
    }

    pub fn route_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.route_window.filter(|&i| i < self.s.tracks.len()) else {
            return;
        };
        enum Act {
            Add(usize),
            Remove(usize),
            Bus,
        }
        let (mut open, mut act) = (true, None);
        let title = RichText::new(format!("ROUTE · {} {}", i + 1, self.s.tracks[i].name)).strong();
        egui::Window::new(title).id(Id::new("route")).open(&mut open).default_width(380.0).show(ctx, |ui| {
            let t = &mut self.s.tracks[i];
            if ui.checkbox(&mut t.to_master, tr("Salida al master (Stereo Out)")).changed() {
                self.dirty = true;
            }
            ui.separator();
            ui.label(RichText::new(tr("ENVÍOS (post-fader)")).size(10.0).color(TEXT_DIM));
            if self.s.tracks[i].sends.is_empty() {
                ui.label(RichText::new(tr("Sin envíos. Envía a un bus para compartir efectos (reverb, delay…).")).color(TEXT_DIM));
            }
            for k in 0..self.s.tracks[i].sends.len() {
                let (id, mut g) = self.s.tracks[i].sends[k];
                let Some(j) = self.index_of(id) else { continue };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("→ {} {}", j + 1, self.s.tracks[j].name)).color(self.s.tracks[j].color).strong());
                    let mut db = widgets::db(g);
                    if ui.add(egui::Slider::new(&mut db, -60.0..=6.0).suffix(" dB").show_value(true)).changed() {
                        g = if db <= -60.0 { 0.0 } else { 10f32.powf(db / 20.0) };
                        self.s.tracks[i].sends[k].1 = g;
                        self.dirty = true;
                    }
                    if ui.small_button(tr("Quitar")).clicked() {
                        act = Some(Act::Remove(k));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.menu_button(tr("+ Añadir envío a…"), |ui| {
                    for j in (0..self.s.tracks.len()).filter(|&j| j != i && !self.s.tracks[j].midi()) {
                        let ok = !self.would_cycle(i, j) && !self.s.tracks[i].sends.iter().any(|s| s.0 == self.s.tracks[j].id);
                        if ui.add_enabled(ok, egui::Button::new(format!("{} {}", j + 1, self.s.tracks[j].name))).clicked() {
                            act = Some(Act::Add(j));
                        }
                    }
                });
                if ui.button(tr("+ Nuevo bus con reverb")).clicked() {
                    act = Some(Act::Bus);
                }
            });
            let id = self.s.tracks[i].id;
            let receives: Vec<String> = self.s.tracks.iter().enumerate().filter(|(_, t)| t.sends.iter().any(|s| s.0 == id)).map(|(j, t)| format!("{} {}", j + 1, t.name)).collect();
            if !receives.is_empty() {
                ui.separator();
                ui.label(RichText::new(format!("RECIBE DE: {}", receives.join(", "))).size(11.0).color(TEXT_DIM));
            }
        });
        match act {
            Some(Act::Add(j)) => {
                self.edit();
                let id = self.s.tracks[j].id;
                self.s.tracks[i].sends.push((id, 0.5));
            }
            Some(Act::Remove(k)) => {
                self.edit();
                self.s.tracks[i].sends.remove(k);
            }
            Some(Act::Bus) => self.new_reverb_bus(i),
            None => {}
        }
        if !open {
            self.route_window = None;
        }
    }
}
