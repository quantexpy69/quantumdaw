//! Secuenciador de patrones (pestaña PATRÓN): la región MIDI abierta como cuadrícula de pasos, un
//! carril por sonido (kit General MIDI y cualquier otra nota de la región). Pasos de 1/8, 1/16 o
//! 1/32, herramientas por carril (llenar, euclidiano, rotar, aleatorio, borrar) y gráfico de
//! velocidad, ganancia, ataque o repeticiones (redobles dentro del paso) del carril elegido.
use crate::creativo::Azar;
use crate::*;
use egui::{Align2, FontId, RichText, Sense, Stroke, pos2, vec2};

/// Carriles del kit (nombre, nota General MIDI).
const KIT: [(&str, u8); 11] = [
    ("Bombo", 36),
    ("Caja", 38),
    ("Palmas", 39),
    ("Aro", 37),
    ("Hi-hat cerrado", 42),
    ("Hi-hat abierto", 46),
    ("Tom bajo", 45),
    ("Tom alto", 50),
    ("Crash", 49),
    ("Ride", 51),
    ("Cencerro", 56),
];
const COLORES: [Color32; 8] = [
    Color32::from_rgb(0xF2, 0x8C, 0x5C),
    Color32::from_rgb(0xF2, 0xD2, 0x5C),
    Color32::from_rgb(0x6D, 0xE0, 0x7A),
    Color32::from_rgb(0x5C, 0xE0, 0xB8),
    Color32::from_rgb(0x5C, 0xD0, 0xF2),
    Color32::from_rgb(0x6D, 0x9C, 0xF2),
    Color32::from_rgb(0xB0, 0x8C, 0xF2),
    Color32::from_rgb(0xF2, 0x8C, 0xD0),
];
const GRAFICOS: [&str; 4] = ["Vel", "Ganancia", "Ataque", "Rep"];

pub struct Patron {
    /// Pasos por tiempo (2 = 1/8, 4 = 1/16, 8 = 1/32).
    div: u32,
    carril: usize,
    grafico: usize,
    euclid: u32,
    /// Arrastre pintando celdas: encender (true) o apagar.
    pintando: Option<bool>,
    semilla: u64,
}

impl Default for Patron {
    fn default() -> Self {
        Self { div: 4, carril: 0, grafico: 0, euclid: 5, pintando: None, semilla: 0xBEA7 }
    }
}

/// Nombre de una nota con octava («C1», «F#1»).
fn nombre_nota(k: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[k as usize % 12], k as i32 / 12 - 2)
}

/// Pasos con golpe de un ritmo euclidiano (k golpes repartidos en n pasos).
pub fn euclidiano(k: u32, n: u32) -> Vec<bool> {
    (0..n).map(|i| k > 0 && (i * k) % n.max(1) < k).collect()
}

impl App {
    /// Datos de la región del patrón: (pista, región, notas, inicio relativo de la región, largo del paso, pasos).
    fn datos_patron(&self) -> Option<(usize, usize, Vec<Note>, i64, f64, usize)> {
        let (i, j) = self.roll.target.filter(|&(i, j)| self.s.tracks.get(i).is_some_and(|t| t.clips.get(j).is_some_and(|c| c.notes().is_some())))?;
        let c = &self.s.tracks[i].clips[j];
        let paso = self.engine.beat_frames() / self.patron.div as f64;
        let notas = c.notes().map(|n| (**n).clone()).unwrap_or_default();
        Some((i, j, notas, c.start as i64 - c.origin(), paso, ((c.len as f64 / paso).round() as usize).max(1)))
    }

    /// Carriles: el kit y las notas de la región que no están en él.
    fn carriles(&self, notas: &[Note]) -> Vec<(String, u8)> {
        let mut v: Vec<(String, u8)> = KIT.iter().map(|(n, k)| (tr(n).to_string(), *k)).collect();
        let mut otras: Vec<u8> = notas.iter().map(|n| n.key).filter(|k| !KIT.iter().any(|x| x.1 == *k)).collect();
        otras.sort();
        otras.dedup();
        v.extend(otras.into_iter().map(|k| (nombre_nota(k), k)));
        v
    }

    /// Reescribe las notas de un carril con `f(pasos)` (cada paso: None vacío o Some(nota base)).
    fn editar_carril(&mut self, key: u8, f: impl FnOnce(Vec<Option<Note>>) -> Vec<Option<(Note, u32)>>) {
        let Some((i, j, notas, inicio, paso, n)) = self.datos_patron() else { return };
        let indice = |x: &Note| ((x.start as i64 - inicio) as f64 / paso).floor();
        let mut pasos: Vec<Option<Note>> = vec![None; n];
        for x in notas.iter().filter(|x| x.key == key) {
            let k = indice(x);
            if k >= 0.0 && (k as usize) < n && pasos[k as usize].is_none() {
                pasos[k as usize] = Some(*x);
            }
        }
        let mut resto: Vec<Note> = notas.into_iter().filter(|x| x.key != key || indice(x) < 0.0 || indice(x) as usize >= n).collect();
        for (k, c) in f(pasos).into_iter().enumerate() {
            if let Some((base, rep)) = c {
                // Repeticiones: el paso se divide en `rep` golpes iguales.
                let sub = paso / rep.max(1) as f64;
                for r in 0..rep.max(1) {
                    let start = (inicio as f64 + k as f64 * paso + r as f64 * sub).max(0.0) as u64;
                    resto.push(Note { start, len: (sub * 0.9) as u64, key, ..base });
                }
            }
        }
        let _ = (i, j);
        self.set_notes(resto);
    }

    /// Notas por paso de un carril: (nota base, repeticiones).
    fn pasos_carril(notas: &[Note], key: u8, inicio: i64, paso: f64, n: usize) -> Vec<Option<(Note, u32)>> {
        let mut v: Vec<Option<(Note, u32)>> = vec![None; n];
        for x in notas.iter().filter(|x| x.key == key) {
            let k = ((x.start as i64 - inicio) as f64 / paso).floor();
            if k >= 0.0 && (k as usize) < n {
                match &mut v[k as usize] {
                    Some((_, r)) => *r += 1,
                    hueco => *hueco = Some((*x, 1)),
                }
            }
        }
        v
    }

    /// Crea una pista de batería con la caja de ritmos y un patrón vacío de dos compases.
    fn crear_patron(&mut self) {
        self.add_track(tr("Batería").into(), TrackKind::Midi);
        let i = self.s.tracks.len() - 1;
        let t = &mut self.s.tracks[i];
        t.instrument = format!("synth:{}", pianoroll::CAJA_RITMOS);
        if let Some(s) = &t.synth {
            let (_, v) = pianoroll::PRESETS.iter().find(|p| p.0 == pianoroll::CAJA_RITMOS).copied().unwrap_or(pianoroll::PRESETS[0]);
            s.params.iter().zip(v).for_each(|(a, v)| a.set(v));
            s.kit.store(true, Relaxed);
        }
        let (at, largo) = (self.pos(), self.bar_frames() * 2);
        self.s.tracks[i].clips.push(Clip::new(Source::Midi(Arc::new(vec![])), at, largo));
        let j = self.s.tracks[i].clips.len() - 1;
        self.roll.open(i, j);
        self.escuchar_en(i);
        self.dirty = true;
    }

    /// Pestaña PATRÓN.
    pub fn vista_patron(&mut self, ui: &mut egui::Ui) {
        let Some((i, j, notas, inicio, paso, n)) = self.datos_patron() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Secuenciador de patrones")).size(18.0).strong());
                ui.label(RichText::new(tr("Abre una región MIDI (doble clic) para editarla por pasos, o crea un patrón de batería nuevo.")).color(TEXT_DIM()));
                if ui.add(egui::Button::new(RichText::new(tr("+ Crear patrón de batería")).color(BG()).strong()).fill(ACCENT())).clicked() {
                    self.crear_patron();
                }
            });
            return;
        };
        let carriles = self.carriles(&notas);
        self.patron.carril = self.patron.carril.min(carriles.len() - 1);
        let (sel_nombre, sel_key) = carriles[self.patron.carril].clone();
        let tiempos = self.engine.beats.load(Relaxed).max(1) as f64;
        // Barra superior: pista, largo, resolución, herramientas y reproducción.
        ui.horizontal(|ui| {
            let t = &self.s.tracks[i];
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            ui.painter().rect_filled(r, widgets::rr(5.0), t.color);
            ui.label(RichText::new(&t.name).strong());
            ui.separator();
            ui.label(RichText::new(tr("LARGO")).size(11.0).color(TEXT_DIM()));
            let compas = self.bar_frames();
            if ui.small_button("−").clicked() && self.s.tracks[i].clips[j].len > compas {
                self.edit();
                self.s.tracks[i].clips[j].len -= compas;
            }
            ui.label(RichText::new(format!("{n} {} / {:.2} {}", tr("pasos"), n as f64 / self.patron.div as f64 / tiempos, tr("compases"))).monospace());
            if ui.small_button("+").clicked() {
                self.edit();
                self.s.tracks[i].clips[j].len += compas;
            }
            ui.separator();
            for (d, texto) in [(2, "1/8"), (4, "1/16"), (8, "1/32")] {
                if ui.add(egui::Button::selectable(self.patron.div == d, texto)).clicked() {
                    self.patron.div = d;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(RichText::new(tr("Colocar en el cursor")).color(BG()).strong()).fill(ACCENT())).on_hover_text(tr("Pone una copia del patrón donde está el cursor")).clicked() {
                    self.edit();
                    let mut copia = self.s.tracks[i].clips[j].clone();
                    copia.start = self.pos();
                    copia.selected = false;
                    self.s.tracks[i].clips.push(copia);
                    self.dirty = true;
                }
                let sonando = self.playing();
                if ui.add(egui::Button::new(if sonando { tr("Detener") } else { tr("Reproducir en bucle") })).clicked() {
                    if sonando {
                        self.stop();
                    } else {
                        let c = &self.s.tracks[i].clips[j];
                        let (a, b) = (c.start, c.end());
                        self.set_loop(a, b);
                        self.engine.looping.store(true, Relaxed);
                        self.go(a);
                        self.escuchar_en(i);
                        self.toggle_play();
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            let semilla = self.patron.semilla;
            if ui.button(tr("Llenar")).clicked() {
                self.edit();
                self.editar_carril(sel_key, |p| p.into_iter().map(|x| Some((x.unwrap_or(Note::new(0, 0, sel_key, 100)), 1))).collect());
            }
            if ui.button(tr("Euclidiano")).on_hover_text(tr("Reparte los golpes lo más parejo posible en el patrón")).clicked() {
                let golpes = euclidiano(self.patron.euclid, n as u32);
                self.edit();
                self.editar_carril(sel_key, |_| golpes.into_iter().map(|g| g.then_some((Note::new(0, 0, sel_key, 100), 1))).collect());
            }
            ui.add(egui::DragValue::new(&mut self.patron.euclid).range(1..=n as u32).suffix(" golpes"));
            for (texto, dir) in [("◀", -1i64), ("▶", 1)] {
                if ui.button(texto).on_hover_text(tr("Rotar el carril un paso")).clicked() {
                    self.edit();
                    let actual = Self::pasos_carril(&notas, sel_key, inicio, paso, n);
                    self.editar_carril(sel_key, |_| (0..n).map(|k| actual[((k as i64 - dir).rem_euclid(n as i64)) as usize]).collect());
                }
            }
            if ui.button(tr("Aleatorio")).clicked() {
                self.edit();
                self.patron.semilla = semilla.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let mut az = Azar::new(semilla);
                self.editar_carril(sel_key, |p| p.iter().map(|_| (az.hasta(100) < 35).then(|| (Note::new(0, 0, sel_key, 60 + az.hasta(67) as u8), 1))).collect());
            }
            if ui.button(tr("Borrar carril")).clicked() {
                self.edit();
                self.editar_carril(sel_key, |p| p.iter().map(|_| None).collect());
            }
            ui.add_space(8.0);
            let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, COLORES[self.patron.carril % COLORES.len()]);
            ui.label(RichText::new(&sel_nombre).color(TEXT_DIM()));
        });
        ui.separator();
        // Cuadrícula.
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let cab = 190.0;
            let celda = ((ui.available_width() - cab - 10.0) / n as f32).clamp(10.0, 34.0);
            // Números de tiempo arriba.
            let (r, _) = ui.allocate_exact_size(vec2(cab + celda * n as f32, 16.0), Sense::hover());
            for k in (0..n).step_by(self.patron.div as usize) {
                let tiempo = k / self.patron.div as usize;
                ui.painter().text(pos2(r.left() + cab + k as f32 * celda + 2.0, r.center().y), Align2::LEFT_CENTER, (tiempo + 1).to_string(), FontId::proportional(10.0), TEXT_DIM());
            }
            let soltar = !ui.input(|i| i.pointer.primary_down());
            if soltar {
                self.patron.pintando = None;
            }
            for (li, (nombre, key)) in carriles.iter().enumerate() {
                let color = COLORES[li % COLORES.len()];
                let pasos = Self::pasos_carril(&notas, *key, inicio, paso, n);
                let elegido = li == self.patron.carril;
                let (fila, _) = ui.allocate_exact_size(vec2(cab + celda * n as f32, celda.max(22.0) + 4.0), Sense::hover());
                if elegido {
                    ui.painter().rect_stroke(fila, widgets::rr(6.0), Stroke::new(1.0, color.gamma_multiply(0.6)), egui::StrokeKind::Inside);
                }
                // Cabecera del carril: punto, nombre (clic: elegir y escuchar) y nota.
                let cab_r = egui::Rect::from_min_size(fila.min, vec2(cab, fila.height()));
                let rc = ui.interact(cab_r, egui::Id::new(("carril", li)), Sense::click());
                if rc.clicked() {
                    self.patron.carril = li;
                    self.escuchar_en(i);
                    self.engine.send_midi([0x90, *key, 110]);
                    self.engine.send_midi([0x80, *key, 0]);
                }
                let p = ui.painter();
                p.circle_filled(cab_r.left_center() + vec2(12.0, 0.0), 5.0, color);
                p.text(cab_r.left_center() + vec2(26.0, 0.0), Align2::LEFT_CENTER, nombre, FontId::proportional(12.5), if elegido { TEXT() } else { TEXT_DIM() });
                let chip = egui::Rect::from_center_size(cab_r.right_center() - vec2(26.0, 0.0), vec2(36.0, 20.0));
                p.rect_filled(chip, widgets::rr(6.0), color.gamma_multiply(0.18));
                p.text(chip.center(), Align2::CENTER_CENTER, nombre_nota(*key), FontId::proportional(11.0), color);
                // Celdas.
                for (k, c) in pasos.iter().enumerate() {
                    let cr = egui::Rect::from_min_size(pos2(fila.left() + cab + k as f32 * celda, fila.top() + 2.0), vec2(celda - 3.0, fila.height() - 4.0));
                    let resp = ui.interact(cr, egui::Id::new(("celda", li, k)), Sense::click_and_drag());
                    let tiempo_fuerte = (k / self.patron.div as usize).is_multiple_of(2);
                    let fondo = match c {
                        Some((x, _)) => color.gamma_multiply(0.45 + 0.55 * x.vel as f32 / 127.0),
                        None if tiempo_fuerte => ELEVATED(),
                        None => PANEL().lerp_to_gamma(ELEVATED(), 0.5),
                    };
                    ui.painter().rect_filled(cr, widgets::rr(5.0), fondo);
                    if let Some((_, rep)) = c.filter(|c| c.1 > 1) {
                        for r in 1..rep {
                            let x = cr.left() + cr.width() * r as f32 / rep as f32;
                            ui.painter().vline(x, cr.shrink(4.0).y_range(), Stroke::new(1.5, BG()));
                        }
                    }
                    // Pintar: el primer clic decide si se encienden o se apagan las celdas que recorre.
                    let pulsada = resp.is_pointer_button_down_on() || (self.patron.pintando.is_some() && ui.input(|i| i.pointer.hover_pos()).is_some_and(|q| cr.contains(q)));
                    if pulsada {
                        let encender = *self.patron.pintando.get_or_insert(c.is_none());
                        if encender != c.is_some() {
                            if resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_pressed()) {
                                self.edit();
                            }
                            self.patron.carril = li;
                            self.editar_carril(*key, |p| p.into_iter().enumerate().map(|(x, v)| if x == k { encender.then(|| (v.unwrap_or(Note::new(0, 0, *key, 100)), 1)) } else { v.map(|v| (v, 1)) }).collect());
                            return;
                        }
                    }
                }
                // Gráfico del carril elegido.
                if elegido {
                    let alto = 110.0;
                    let (g, _) = ui.allocate_exact_size(vec2(cab + celda * n as f32, alto), Sense::hover());
                    let area = egui::Rect::from_min_max(pos2(g.left() + cab, g.top()), g.max);
                    ui.painter().rect_filled(area, widgets::rr(4.0), BG());
                    let valor = |x: &Note, rep: u32| match self.patron.grafico {
                        0 => x.vel as f32 / 127.0,
                        1 => x.gain as f32 / 127.0,
                        2 => x.attack as f32 / 127.0,
                        _ => rep as f32 / 4.0,
                    };
                    for (k, c) in pasos.iter().enumerate() {
                        if let Some((x, rep)) = c {
                            let h = valor(x, *rep) * (alto - 6.0);
                            let br = egui::Rect::from_min_max(pos2(area.left() + k as f32 * celda, area.bottom() - h), pos2(area.left() + (k + 1) as f32 * celda - 3.0, area.bottom()));
                            ui.painter().rect_filled(br, 2.0, color);
                        }
                    }
                    let rg = ui.interact(area, egui::Id::new("patron-grafico"), Sense::click_and_drag());
                    if (rg.dragged() || rg.clicked())
                        && let Some(q) = rg.interact_pointer_pos()
                    {
                        let k = ((q.x - area.left()) / celda).floor() as usize;
                        let v = ((area.bottom() - q.y) / (alto - 6.0)).clamp(0.0, 1.0);
                        if rg.drag_started() || rg.clicked() {
                            self.edit();
                        }
                        let tipo = self.patron.grafico;
                        let actual = pasos.clone();
                        self.editar_carril(*key, |_| {
                            actual
                                .into_iter()
                                .enumerate()
                                .map(|(x, c)| {
                                    let (mut nota, mut rep) = c?;
                                    if x == k {
                                        let b = (v * 127.0).round() as u8;
                                        match tipo {
                                            0 => nota.vel = b.max(1),
                                            1 => nota.gain = b,
                                            2 => nota.attack = b,
                                            _ => rep = (v * 4.0).ceil().clamp(1.0, 4.0) as u32,
                                        }
                                    }
                                    Some((nota, rep))
                                })
                                .collect()
                        });
                        return;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(cab);
                        for (k, t) in GRAFICOS.iter().enumerate() {
                            if ui.add(egui::Button::selectable(self.patron.grafico == k, tr(t))).clicked() {
                                self.patron.grafico = k;
                            }
                        }
                    });
                }
            }
        });
    }
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn ritmo_euclidiano() {
        let r = super::euclidiano(3, 8);
        assert_eq!(r.iter().filter(|x| **x).count(), 3);
        assert_eq!(super::euclidiano(4, 16).iter().filter(|x| **x).count(), 4);
        assert!(super::euclidiano(4, 16)[0]);
    }
}
