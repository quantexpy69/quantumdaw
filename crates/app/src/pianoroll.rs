//! Piano roll: edición de notas de una región MIDI con teclado, rejilla y carril de velocidades.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Id, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

const KEYS_W: f32 = 64.0;
/// Instrumentos preestablecidos del QUANTUM Synth: (nombre, parámetros en el orden de `SYNTH_PARAMS`).
/// Preset que convierte el sintetizador en caja de ritmos (sonidos de batería por nota).
pub const CAJA_RITMOS: &str = "Caja de ritmos";
pub const PRESETS: [(&str, [f32; 10]); 8] = [
    ("Piano eléctrico", [3.0, 4.0, 3500.0, 0.1, 0.3, 2.0, 900.0, 0.25, 400.0, -6.0]),
    ("Bajo sintetizado", [0.0, 6.0, 700.0, 0.45, 0.5, 2.0, 250.0, 0.6, 120.0, -4.0]),
    ("Pad cálido", [0.0, 14.0, 1800.0, 0.2, 0.2, 600.0, 1500.0, 0.8, 1500.0, -10.0]),
    ("Lead brillante", [1.0, 10.0, 5000.0, 0.35, 0.4, 5.0, 300.0, 0.7, 200.0, -9.0]),
    ("Órgano", [2.0, 3.0, 6000.0, 0.05, 0.0, 5.0, 50.0, 1.0, 80.0, -8.0]),
    ("Pluck", [0.0, 7.0, 1200.0, 0.5, 0.9, 1.0, 220.0, 0.0, 200.0, -6.0]),
    ("Cuerdas", [0.0, 18.0, 2600.0, 0.15, 0.15, 300.0, 800.0, 0.85, 900.0, -10.0]),
    (CAJA_RITMOS, [3.0, 0.0, 16000.0, 0.0, 0.0, 1.0, 50.0, 1.0, 50.0, -6.0]),
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
    /// Notas copiadas (inicio relativo a la primera) y si el ratón estuvo sobre el piano roll.
    portapapeles: Vec<Note>,
    hover: bool,
    /// Crear acordes: tipo de acorde que escribe cada nota nueva (None = notas sueltas) e inversión.
    modo_acorde: Option<usize>,
    inversion: usize,
    /// Ventanas del círculo de quintas y del instrumento de la pista.
    pub circulo: bool,
    pub instrumento: bool,
    /// Tonalidad elegida en el círculo y progresión automática (progresión, ritmo, compases por acorde).
    tono: u8,
    menor: bool,
    progresion: usize,
    ritmo: usize,
    compases: u32,
    /// Bloqueo de escala (tónica = `tono`) y opciones de arpegio, rasgueo y melodía.
    escala_on: bool,
    escala: usize,
    arp_patron: usize,
    arp_div: u32,
    strum_ms: f32,
    strum_abajo: bool,
    mel_compases: u32,
    mel_densidad: usize,
    semilla: u64,
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
        Self { target: None, ppb: 90.0, key_h: 12.0, grid: 4, scroll: vec2(0.0, (127.0 - 76.0) * 12.0), sel: Vec::new(), drag: None, last_len: 1.0, sounding: None, typing: false, octave: 5, step: false, lane: Carril::Velocidad, lane_h: 110.0, lane_prev: None, portapapeles: Vec::new(), hover: false, modo_acorde: None, inversion: 0, circulo: false, instrumento: false, tono: 0, menor: false, progresion: 0, ritmo: 0, compases: 1, escala_on: false, escala: 0, arp_patron: 0, arp_div: 4, strum_ms: 30.0, strum_abajo: false, mel_compases: 2, mel_densidad: 1, semilla: 0x5EED }
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
        for k in self.teclas_nota(key) {
            notes.push(Note::new((at as i64 - origin).max(0) as u64, step, k, 100));
        }
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
        let current = match (instruments::es_real(&t.instrument), t.instrument.strip_prefix("synth:")) {
            (true, _) if t.sampler.is_some() => instruments::nombre(&t.instrument),
            (true, _) => format!("{} (preparando…)", instruments::nombre(&t.instrument)),
            (false, Some(p)) => format!("Synth · {p}"),
            _ => "QUANTUM Synth".into(),
        };
        let mut crear_kit = false;
        let mut pick = None;
        egui::ComboBox::from_id_salt(("inst", i)).width(200.0).height(520.0).selected_text(current).show_ui(ui, |ui| {
            ui.label(RichText::new(tr("INSTRUMENTOS REALES · gratis y libres")).size(10.0).color(TEXT_DIM()));
            let mut family = "";
            for e in instruments::CATALOG.iter() {
                if e.family != family {
                    family = e.family;
                    ui.label(RichText::new(family).strong().size(11.0).color(ACCENT()));
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
            ui.label(RichText::new(tr("MIS KITS")).size(10.0).color(TEXT_DIM()));
            for (id, nombre) in instruments::kits() {
                if ui.selectable_label(self.s.tracks[i].instrument == id, &nombre).on_hover_text(tr("Kit de batería propio")).clicked()
                    && let Some(id) = instruments::id_estatico(&id)
                {
                    pick = Some(Pick::Real(id));
                }
            }
            if ui.button(tr("+ Crear kit desde una carpeta…")).clicked() {
                crear_kit = true;
            }
            ui.separator();
            ui.label(RichText::new(tr("QUANTUM SYNTH")).size(10.0).color(TEXT_DIM()));
            for (k, (name, _)) in PRESETS.iter().enumerate() {
                if ui.selectable_label(self.s.tracks[i].instrument == format!("synth:{name}"), *name).clicked() {
                    pick = Some(Pick::Preset(k));
                }
            }
        });
        if crear_kit {
            self.abrir_kits();
        }
        if pick.is_some() {
            self.roll.instrumento = true;
        }
        match pick {
            Some(Pick::Real(id)) => self.set_instrument(i, id),
            Some(Pick::Preset(k)) => {
                let (name, values) = PRESETS[k];
                let t = &mut self.s.tracks[i];
                if let Some(s) = &t.synth {
                    s.params.iter().zip(values).for_each(|(a, v)| a.set(v));
                    s.kit.store(name == CAJA_RITMOS, Relaxed);
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

    /// Notas que escribe una tecla: ella sola o el acorde elegido con esa raíz (ajustada a la escala).
    fn teclas_nota(&self, key: u8) -> Vec<u8> {
        let key = self.en_escala(key);
        self.roll.modo_acorde.map_or_else(|| vec![key], |t| acordes::notas(key, t, self.roll.inversion))
    }

    /// Con el bloqueo de escala activo, la nota más cercana de la escala.
    fn en_escala(&self, key: u8) -> u8 {
        if self.roll.escala_on { creativo::ajustar(key, self.roll.tono, self.roll.escala) } else { key }
    }

    /// Reemplaza las notas seleccionadas por `f(seleccionadas)` y deja seleccionado el resultado.
    fn transformar_seleccion(&mut self, f: impl FnOnce(&[Note]) -> Vec<Note>) {
        let Some((i, j)) = self.roll_clip().filter(|_| !self.roll.sel.is_empty()) else {
            self.status = tr("Selecciona primero las notas (o acordes)").into();
            return;
        };
        let notes = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
        let elegidas: Vec<Note> = self.roll.sel.iter().filter_map(|&k| notes.get(k).copied()).collect();
        let mut resto: Vec<Note> = notes.iter().enumerate().filter(|(k, _)| !self.roll.sel.contains(k)).map(|(_, n)| *n).collect();
        self.edit();
        let primera = resto.len();
        resto.extend(f(&elegidas));
        self.roll.sel = (primera..resto.len()).collect();
        self.set_notes(resto);
    }

    /// Menú «Herramientas MIDI»: escala, arpegiador, rasgueo y generador de melodías.
    fn menu_creativo(&mut self, ui: &mut egui::Ui) {
        let beat = self.engine.beat_frames();
        let sr = self.engine.sample_rate as f32;
        ui.menu_button(tr("Herramientas MIDI"), |ui| {
            widgets::menu_style(ui);
            let r = &mut self.roll;
            ui.label(RichText::new(tr("Escala")).strong());
            ui.checkbox(&mut r.escala_on, tr("Bloquear a la escala")).on_hover_text(tr("Las notas nuevas y las que muevas caen solo en notas de la escala; las demás filas se oscurecen"));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("roll-tonica").width(60.0).selected_text(acordes::RAICES[r.tono as usize % 12]).show_ui(ui, |ui| {
                    for (k, n) in acordes::RAICES.iter().enumerate() {
                        ui.selectable_value(&mut r.tono, k as u8, *n);
                    }
                });
                egui::ComboBox::from_id_salt("roll-escala").width(200.0).height(420.0).selected_text(tr(creativo::ESCALAS[r.escala].0)).show_ui(ui, |ui| {
                    for (k, (n, _)) in creativo::ESCALAS.iter().enumerate() {
                        ui.selectable_value(&mut r.escala, k, tr(n));
                    }
                });
            });
            ui.separator();
            ui.label(RichText::new(tr("Arpegiar la selección")).strong());
            ui.horizontal(|ui| {
                for (k, n) in creativo::PATRONES_ARPEGIO.iter().enumerate() {
                    ui.selectable_value(&mut r.arp_patron, k, tr(n));
                }
            });
            ui.horizontal(|ui| {
                for (d, n) in [(2, "1/8"), (4, "1/16"), (8, "1/32")] {
                    ui.selectable_value(&mut r.arp_div, d, n);
                }
            });
            let arpegiar = ui.button(tr("Arpegiar")).clicked();
            ui.separator();
            ui.label(RichText::new(tr("Rasgueo (strum)")).strong());
            ui.add(egui::Slider::new(&mut r.strum_ms, 5.0..=150.0).suffix(" ms").text(tr("Separación")));
            ui.checkbox(&mut r.strum_abajo, tr("Hacia abajo (de la nota más aguda a la más grave)"));
            let rasguear = ui.button(tr("Rasguear")).clicked();
            ui.separator();
            ui.label(RichText::new(tr("Generar melodía en la escala")).strong());
            ui.horizontal(|ui| {
                ui.label(tr("Compases"));
                for c in [1, 2, 4, 8] {
                    ui.selectable_value(&mut r.mel_compases, c, c.to_string());
                }
            });
            ui.horizontal(|ui| {
                for (k, n) in creativo::DENSIDADES.iter().enumerate() {
                    ui.selectable_value(&mut r.mel_densidad, k, tr(n));
                }
            });
            let generar = ui.button(tr("Generar en el cursor")).on_hover_text(tr("Cada clic genera una melodía distinta; Ctrl+Z la quita")).clicked();
            let (patron, div, ms, abajo) = (r.arp_patron, r.arp_div, r.strum_ms, r.strum_abajo);
            r.semilla = r.semilla.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let semilla = r.semilla;
            if arpegiar {
                self.transformar_seleccion(|n| creativo::arpegiar(n, (beat / div as f64) as u64, patron, semilla));
            }
            if rasguear {
                self.transformar_seleccion(|n| creativo::rasguear(n, (ms * sr / 1000.0) as u64, abajo));
            }
            if generar {
                self.generar_melodia(semilla);
            }
        });
    }

    /// Acciones del menú circular de notas.
    pub fn pie_notas(&mut self, a: pie::Accion) {
        use pie::Accion;
        let beat = self.engine.beat_frames();
        let (div, patron, ms, abajo, sr) = (self.roll.arp_div, self.roll.arp_patron, self.roll.strum_ms, self.roll.strum_abajo, self.engine.sample_rate as f32);
        let paso = beat / self.roll.grid as f64;
        match a {
            Accion::Cuantizar => {
                if !self.roll.has_selection()
                    && let Some((i, j)) = self.roll_clip()
                {
                    self.roll.sel = (0..self.s.tracks[i].clips[j].notes().map_or(0, |n| n.len())).collect();
                }
                self.transformar_seleccion(|n| n.iter().map(|x| Note { start: ((x.start as f64 / paso).round() * paso) as u64, ..*x }).collect());
            }
            Accion::OctavaArriba => self.transformar_seleccion(|n| n.iter().map(|x| Note { key: x.key.saturating_add(12).min(127), ..*x }).collect()),
            Accion::OctavaAbajo => self.transformar_seleccion(|n| n.iter().map(|x| Note { key: x.key.saturating_sub(12), ..*x }).collect()),
            Accion::Arpegiar => self.transformar_seleccion(|n| creativo::arpegiar(n, (beat / div as f64) as u64, patron, 7)),
            Accion::Rasguear => self.transformar_seleccion(|n| creativo::rasguear(n, (ms * sr / 1000.0) as u64, abajo)),
            Accion::BorrarNotas => self.roll_delete_selected(),
            Accion::DuplicarNotas => self.transformar_seleccion(|n| {
                let (a, b) = (n.iter().map(|x| x.start).min().unwrap_or(0), n.iter().map(|x| x.start + x.len).max().unwrap_or(0));
                n.iter().copied().chain(n.iter().map(|x| Note { start: x.start + (b - a), ..*x })).collect()
            }),
            _ => {}
        }
    }

    /// Escribe una melodía nueva desde el cursor en la tonalidad y escala elegidas.
    fn generar_melodia(&mut self, semilla: u64) {
        let Some((i, j)) = self.roll_clip() else { return };
        let beat = self.engine.beat_frames();
        let compas = (beat * self.engine.beats.load(Relaxed) as f64) as u64;
        let paso = (beat / self.roll.grid as f64) as u64;
        let c = &self.s.tracks[i].clips[j];
        let inicio = (self.pos().max(c.start) as i64 - c.origin()).max(0) as u64;
        let nuevas = creativo::melodia(inicio, self.roll.mel_compases, compas, paso, (self.roll.tono, self.roll.escala), self.roll.mel_densidad, semilla);
        let mut notes = c.notes().map(|n| (**n).clone()).unwrap_or_default();
        self.edit();
        let primera = notes.len();
        notes.extend(nuevas);
        self.roll.sel = (primera..notes.len()).collect();
        self.set_notes(notes);
    }

    /// Los atajos de copiar/pegar van al piano roll si el ratón estuvo sobre él en el último cuadro.
    pub fn roll_tomar_foco(&mut self) -> bool {
        std::mem::take(&mut self.roll.hover) && self.roll_clip().is_some()
    }

    /// Copia (o corta) las notas seleccionadas.
    pub fn roll_copiar(&mut self, cortar: bool) {
        let Some((i, j)) = self.roll_clip() else { return };
        let notes = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
        let mut sel: Vec<Note> = self.roll.sel.iter().filter_map(|&k| notes.get(k).copied()).collect();
        let Some(base) = sel.iter().map(|n| n.start).min() else { return };
        sel.iter_mut().for_each(|n| n.start -= base);
        self.roll.portapapeles = sel;
        // egui solo emite el evento de pegar si el portapapeles del sistema tiene texto.
        self.ctx.copy_text("quantum-daw:notas".into());
        if cortar {
            self.roll_delete_selected();
        }
        self.status = format!("{} nota(s) copiada(s)", self.roll.portapapeles.len());
    }

    /// Pega las notas copiadas en el cursor (dentro de la región) y las deja seleccionadas.
    pub fn roll_pegar(&mut self) {
        let Some((i, j)) = self.roll_clip().filter(|_| !self.roll.portapapeles.is_empty()) else { return };
        let c = &self.s.tracks[i].clips[j];
        let en = (self.pos().max(c.start) as i64 - c.origin()).max(0) as u64;
        let mut notes = c.notes().map(|n| (**n).clone()).unwrap_or_default();
        self.edit();
        let primera = notes.len();
        notes.extend(self.roll.portapapeles.iter().map(|n| Note { start: n.start + en, ..*n }));
        self.roll.sel = (primera..notes.len()).collect();
        self.set_notes(notes);
    }

    /// Escribe acordes (inicio absoluto, duración, notas) en la región abierta y los selecciona.
    pub(crate) fn insertar_acordes(&mut self, acordes: Vec<(u64, u64, Vec<u8>)>) {
        let Some((i, j)) = self.roll_clip() else { return };
        let c = &self.s.tracks[i].clips[j];
        let origin = c.origin();
        let mut notes = c.notes().map(|n| (**n).clone()).unwrap_or_default();
        self.edit();
        let primera = notes.len();
        for (at, len, teclas) in acordes {
            notes.extend(teclas.into_iter().map(|k| Note::new((at as i64 - origin).max(0) as u64, len, k, 100)));
        }
        self.roll.sel = (primera..notes.len()).collect();
        self.set_notes(notes);
    }

    /// Escribe un acorde de un compás en el cursor y avanza el cursor (clics seguidos arman una progresión).
    fn acorde_en_cursor(&mut self, raiz: u8, tipo: usize) {
        let Some((i, j)) = self.roll_clip() else { return };
        let compas = (self.engine.beat_frames() * self.engine.beats.load(Relaxed) as f64) as u64;
        let at = self.pos().max(self.s.tracks[i].clips[j].start);
        self.insertar_acordes(vec![(at, compas, acordes::notas(acordes::base_central(raiz), tipo, self.roll.inversion))]);
        self.go(at + compas);
    }

    /// Escribe la progresión elegida desde el cursor con el ritmo elegido.
    fn escribir_progresion(&mut self) {
        let Some((i, j)) = self.roll_clip() else { return };
        let beat = self.engine.beat_frames();
        let tiempos = self.engine.beats.load(Relaxed) as u64;
        let largo = (beat * (tiempos * self.roll.compases as u64) as f64) as u64;
        let grados = acordes::diatonicos(self.roll.tono, self.roll.menor);
        let mut at = self.pos().max(self.s.tracks[i].clips[j].start);
        let mut todo = Vec::new();
        for &g in acordes::PROGRESIONES[self.roll.progresion].1 {
            let (raiz, tipo, _) = grados[g];
            let teclas = acordes::notas(acordes::base_central(raiz), tipo, self.roll.inversion);
            match self.roll.ritmo {
                // Negras: el acorde en cada tiempo.
                1 => (0..tiempos * self.roll.compases as u64).for_each(|b| todo.push((at + (b as f64 * beat) as u64, (beat * 0.9) as u64, teclas.clone()))),
                // Arpegio: las notas del acorde de una en una, en corcheas.
                2 => (0..tiempos * self.roll.compases as u64 * 2).for_each(|b| todo.push((at + (b as f64 * beat / 2.0) as u64, (beat / 2.0) as u64, vec![teclas[b as usize % teclas.len()]]))),
                _ => todo.push((at, largo, teclas)),
            }
            at += largo;
        }
        self.insertar_acordes(todo);
        self.go(at);
    }

    /// Ventana del círculo de quintas: tonalidad, sus acordes y progresiones automáticas.
    pub fn ventana_acordes(&mut self, ctx: &egui::Context) {
        if !self.roll.circulo || self.roll_clip().is_none() {
            return;
        }
        let mut abierta = true;
        let mut accion = None;
        egui::Window::new(RichText::new(tr("Círculo de quintas y acordes")).strong()).id(Id::new("roll-circulo")).open(&mut abierta).default_size([360.0, 640.0]).show(ctx, |ui| {
            let r = &mut self.roll;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut r.menor, false, tr("Mayor"));
                ui.selectable_value(&mut r.menor, true, tr("Menor"));
                ui.label(RichText::new(format!("{} {}", tr("Tonalidad"), acordes::tonalidad(r.tono, r.menor))).color(ACCENT()).strong());
            });
            if let Some((raiz, tipo)) = acordes::circulo(ui, &mut r.tono, &mut r.menor) {
                accion = Some((raiz, tipo));
            }
            ui.label(RichText::new(tr("Acordes de la tonalidad: clic para escribirlo en el cursor (un compás); el cursor avanza.")).size(11.0).color(TEXT_DIM()));
            ui.horizontal_wrapped(|ui| {
                for (raiz, tipo, grado) in acordes::diatonicos(r.tono, r.menor) {
                    let b = egui::Button::new(RichText::new(format!("{grado}\n{}", acordes::nombre(raiz, tipo))).size(12.0)).min_size(vec2(44.0, 38.0));
                    if ui.add(b).clicked() {
                        accion = Some((raiz, tipo));
                    }
                }
            });
            ui.separator();
            ui.label(RichText::new(tr("Progresión automática")).strong());
            egui::ComboBox::from_id_salt("roll-prog").width(300.0).selected_text(tr(acordes::PROGRESIONES[r.progresion].0)).show_ui(ui, |ui| {
                for (k, (n, _)) in acordes::PROGRESIONES.iter().enumerate() {
                    ui.selectable_value(&mut r.progresion, k, tr(n));
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Ritmo"));
                for (k, n) in acordes::RITMOS.iter().enumerate() {
                    ui.selectable_value(&mut r.ritmo, k, tr(n));
                }
            });
            ui.horizontal(|ui| {
                ui.label(tr("Compases por acorde"));
                for c in [1, 2] {
                    ui.selectable_value(&mut r.compases, c, c.to_string());
                }
            });
            if ui.add(egui::Button::new(RichText::new(tr("Escribir progresión en el cursor")).color(BG()).strong()).fill(ACCENT())).clicked() {
                accion = Some((u8::MAX, 0));
            }
        });
        match accion {
            Some((u8::MAX, _)) => self.escribir_progresion(),
            Some((raiz, tipo)) => self.acorde_en_cursor(raiz, tipo),
            None => {}
        }
        self.roll.circulo &= abierta;
    }

    /// Ventana flotante con el instrumento de la pista del piano roll (se abre al elegir uno).
    pub fn ventana_instrumento(&mut self, ctx: &egui::Context) {
        let Some((i, _)) = self.roll_clip().filter(|_| self.roll.instrumento) else {
            return;
        };
        let mut abierta = true;
        let t = &self.s.tracks[i];
        let titulo = format!("{} · {}", tr("Instrumento"), t.name);
        egui::Window::new(RichText::new(titulo).strong()).id(Id::new("roll-instrumento")).open(&mut abierta).default_width(420.0).show(ctx, |ui| {
            if let Some(s) = &t.sampler {
                let e = instruments::find(&t.instrument);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("QUANTUM").size(10.0).strong().color(ACCENT()));
                    ui.label(RichText::new(e.map_or(s.name.as_str(), |e| e.name)).size(16.0).strong());
                });
                let mut db = widgets::db(s.gain.get());
                if ui.add(egui::Slider::new(&mut db, -30.0..=12.0).suffix(" dB").text(tr("Volumen"))).changed() {
                    s.gain.set(10f32.powf(db / 20.0));
                }
                if let Some(e) = e {
                    ui.label(RichText::new(format!("Muestras: {} · licencia {}", e.credit, e.license)).size(10.0).color(TEXT_DIM()));
                }
            } else if let Some(s) = &t.synth {
                let preset = t.instrument.strip_prefix("synth:").unwrap_or("QUANTUM Synth");
                ui.horizontal(|ui| {
                    ui.label(RichText::new("QUANTUM SYNTH").size(10.0).strong().color(ACCENT()));
                    ui.label(RichText::new(preset).size(16.0).strong());
                });
                ui.horizontal_wrapped(|ui| {
                    for (k, def) in engine::SYNTH_PARAMS.iter().enumerate() {
                        plugins::param_knob(ui, None, *def, &s.params[k]);
                    }
                });
            } else {
                ui.label(RichText::new(tr("Esta pista no tiene instrumento.")).color(TEXT_DIM()));
            }
            ui.add_space(6.0);
            if ui.button(tr("Cerrar")).clicked() {
                self.roll.instrumento = false;
            }
        });
        self.roll.instrumento &= abierta;
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
                ui.label(RichText::new(tr("Haz doble clic en una región MIDI para editarla, o en un carril MIDI vacío para crear una.")).color(TEXT_DIM()));
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
                ui.label(RichText::new(format!("Octava Do{}", self.roll.octave - 1)).size(11.0).color(ACCENT()));
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
        // Segunda fila: acorde que suena y herramientas de acordes.
        ui.horizontal(|ui| {
            let Some((i, j)) = self.roll_clip() else { return };
            let c = &self.s.tracks[i].clips[j];
            let notes: Vec<Note> = c.notes().map(|n| (**n).clone()).unwrap_or_default();
            // Notas seleccionadas o, si no hay, las que suenan en el cursor.
            let teclas: Vec<u8> = if self.roll.sel.len() >= 2 {
                self.roll.sel.iter().filter_map(|&k| notes.get(k).map(|n| n.key)).collect()
            } else {
                let rel = self.pos() as i64 - c.origin();
                notes.iter().filter(|n| (n.start as i64..(n.start + n.len) as i64).contains(&rel)).map(|n| n.key).collect()
            };
            ui.label(RichText::new(tr("Acorde")).color(TEXT_DIM()));
            ui.label(RichText::new(acordes::detectar(&teclas).unwrap_or_else(|| "—".into())).size(15.0).strong().color(ACCENT()));
            ui.separator();
            ui.label(tr("Crear acordes"));
            let actual = self.roll.modo_acorde.map_or(tr("Notas sueltas").to_string(), |t| tr(acordes::TIPOS[t].1).to_string());
            egui::ComboBox::from_id_salt("roll-modo-acorde").width(150.0).selected_text(actual).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.roll.modo_acorde, None, tr("Notas sueltas"));
                for (k, (_, n, _)) in acordes::TIPOS.iter().enumerate() {
                    ui.selectable_value(&mut self.roll.modo_acorde, Some(k), tr(n));
                }
            })
            .response
            .on_hover_text(tr("Con un tipo elegido, cada nota que crees (doble clic, escribir por pasos) escribe el acorde completo con esa nota como raíz"));
            egui::ComboBox::from_id_salt("roll-inversion").width(110.0).selected_text(tr(acordes::INVERSIONES[self.roll.inversion])).show_ui(ui, |ui| {
                for (k, n) in acordes::INVERSIONES.iter().enumerate() {
                    ui.selectable_value(&mut self.roll.inversion, k, tr(n));
                }
            });
            if ui.add(egui::Button::new(RichText::new(tr("Generar…")).color(BG()).strong()).fill(ACCENT())).on_hover_text(tr("Generar progresión de acordes por estilo")).clicked() {
                self.generador.abierto = true;
            }
            if ui.add(egui::Button::selectable(self.roll.circulo, tr("Círculo de quintas"))).on_hover_text(tr("Tonalidad, sus acordes y progresiones automáticas")).clicked() {
                self.roll.circulo = !self.roll.circulo;
            }
            if ui.add(egui::Button::selectable(self.roll.instrumento, tr("Ver instrumento"))).clicked() {
                self.roll.instrumento = !self.roll.instrumento;
            }
            self.menu_creativo(ui);
        });
        let Some((i, j)) = self.roll_clip() else {
            return;
        };
        let area = ui.available_rect_before_wrap();
        if ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| area.contains(p)) {
            self.roll.hover = true;
        }
        ui.allocate_rect(area, Sense::hover());
        let lane_h = self.roll.lane_h.clamp(50.0, (area.height() * 0.6).max(50.0));
        // Regla arriba (compases y cursor de reproducción), teclado y rejilla debajo.
        const REGLA_H: f32 = 18.0;
        let regla = Rect::from_min_max(pos2(area.left() + KEYS_W, area.top()), pos2(area.right(), area.top() + REGLA_H));
        let keys = Rect::from_min_max(pos2(area.left(), area.top() + REGLA_H), pos2(area.left() + KEYS_W, area.bottom() - lane_h));
        let grid = Rect::from_min_max(pos2(keys.right(), area.top() + REGLA_H), pos2(area.right(), area.bottom() - lane_h));
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

        // Cursor de reproducción: clic o arrastre en la regla lo mueven; la línea también se arrastra
        // desde la rejilla (si no hay una nota debajo).
        let cursor_x = x_of(self.pos() as f64 - clip.start as f64);
        let frame_en = |x: f32| (rel_at(x) + clip.start as f64).max(0.0) as u64;
        let rr_ = ui.interact(regla, Id::new("roll-regla"), Sense::click_and_drag());
        if rr_.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if (rr_.clicked() || rr_.dragged() || rr_.drag_started())
            && let Some(p) = ui.input(|i| i.pointer.interact_pos())
        {
            self.go(frame_en(p.x.max(grid.left())));
        }
        let linea_id = Id::new("roll-cursor-linea");
        let arrastrando_linea = ui.data(|d| d.get_temp::<bool>(linea_id)).unwrap_or(false);
        let sobre_linea = |p: egui::Pos2| (p.x - cursor_x).abs() <= 5.0 && grid.contains(p) && !notes.iter().any(|n| note_rect(n).contains(p));
        if let Some(p) = ui.input(|i| i.pointer.hover_pos())
            && (sobre_linea(p) || arrastrando_linea)
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        let (pulsado, abajo, puntero) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.interact_pos()));
        if pulsado && puntero.is_some_and(sobre_linea) {
            ui.data_mut(|d| d.insert_temp(linea_id, true));
        }
        if arrastrando_linea {
            if abajo {
                if let Some(p) = puntero {
                    self.go(frame_en(p.x.max(grid.left())));
                }
            } else {
                ui.data_mut(|d| d.insert_temp(linea_id, false));
            }
        }
        let linea_activa = arrastrando_linea || (pulsado && puntero.is_some_and(sobre_linea));

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
            for k in self.teclas_nota(n.key) {
                v.push(Note { key: k, ..n });
            }
            self.roll.sel = (notes.len()..v.len()).collect();
            self.set_notes(v);
            self.roll.drag = None;
            self.preview(Some(n.key));
        } else if gr.hovered()
            && pressed
            && !linea_activa
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
            && !linea_activa
            && let Some(d) = &mut self.roll.drag
        {
            let delta = gr.drag_delta();
            let mut v = self.s.tracks[i].clips[j].notes().map(|n| (**n).clone()).unwrap_or_default();
            let mut key = None;
            let snap = |f: f64| (f / step).round() * step;
            let (escala_on, tono, escala) = (self.roll.escala_on, self.roll.tono, self.roll.escala);
            match d {
                RollDrag::Move(orig, acc) => {
                    *acc += delta;
                    let dk = -(acc.y / kh).round() as i32;
                    let dt = snap((acc.x / ppb) as f64 * beat);
                    for (k, o) in orig.iter() {
                        if let Some(n) = v.get_mut(*k) {
                            let new_key = (o.key as i32 + dk).clamp(0, 127) as u8;
                            let new_key = if escala_on { creativo::ajustar(new_key, tono, escala) } else { new_key };
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
            // En una capa por encima de la rejilla y las notas (que se pintan después).
            let capa = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("roll-recuadro"))).with_clip_rect(grid);
            capa.rect(r, rr(2.0), ACCENT().gamma_multiply(0.15), Stroke::new(1.5, ACCENT()), egui::StrokeKind::Inside);
        }
        if !ui.input(|i| i.pointer.primary_down()) && self.roll.drag.take().is_some() {
            self.preview(None);
        }
        // Ctrl+A: seleccionar todas las notas de la región.
        if gr.hovered() && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::A)) {
            self.roll.sel = (0..notes.len()).collect();
        }
        // Clic derecho: menú circular de notas (sobre una nota, la selecciona si no lo estaba).
        if gr.secondary_clicked()
            && let Some(p) = ptr
        {
            if let Some(k) = notes.iter().rposition(|n| note_rect(n).contains(p))
                && !self.roll.sel.contains(&k)
            {
                self.roll.sel = vec![k];
            }
            self.abrir_pie_notas(p);
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
        p.rect_filled(area, 0.0, BG());
        let gp = p.with_clip_rect(grid);
        for key in 0..128u8 {
            let y = y_of(key);
            if y > grid.bottom() || y + kh < grid.top() {
                continue;
            }
            if self.roll.escala_on && !creativo::en_escala(key, self.roll.tono, self.roll.escala) {
                gp.rect_filled(Rect::from_x_y_ranges(grid.x_range(), y..=y + kh), 0.0, Color32::from_black_alpha(130));
            } else if self.roll.escala_on && key % 12 == self.roll.tono % 12 {
                gp.rect_filled(Rect::from_x_y_ranges(grid.x_range(), y..=y + kh), 0.0, ACCENT().gamma_multiply(0.10));
            } else if BLACK[key as usize % 12] {
                gp.rect_filled(Rect::from_x_y_ranges(grid.x_range(), y..=y + kh), 0.0, Color32::from_black_alpha(60));
            }
            if key % 12 == 0 {
                gp.hline(grid.x_range(), y + kh, Stroke::new(1.0, BORDER()));
            }
        }
        let beats = self.engine.beats.load(Relaxed).max(1) as i64;
        let first = (rel_at(grid.left()) / step) as i64;
        let mut k = first;
        while x_of(k as f64 * step) < grid.right() {
            let x = x_of(k as f64 * step);
            let per_beat = self.roll.grid as i64;
            let stroke = if k % (per_beat * beats) == 0 {
                Stroke::new(1.0, TEXT_DIM().gamma_multiply(0.5))
            } else if k % per_beat == 0 {
                Stroke::new(1.0, BORDER())
            } else {
                Stroke::new(1.0, PANEL())
            };
            gp.vline(x, grid.y_range(), stroke);
            if k % (per_beat * beats) == 0 {
                gp.text(pos2(x + 3.0, grid.top() + 2.0), Align2::LEFT_TOP, format!("{}", k / (per_beat * beats) + 1), FontId::monospace(10.0), TEXT_DIM());
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
                gp.rect_stroke(r, rr(3.0), Stroke::new(1.5, TEXT()), StrokeKind::Inside);
            }
            if r.width() > 26.0 && kh >= 10.0 {
                gp.text(r.left_center() + vec2(3.0, 0.0), Align2::LEFT_CENTER, NOTE_NAMES[n.key as usize % 12], FontId::proportional(9.0), BG());
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
            gp.vline(px, grid.y_range(), Stroke::new(1.5, TEXT()));
        }
        // Nombres de las piezas (batería y percusión) sobre sus teclas.
        if let Some(s) = &self.s.tracks[i].sampler {
            for (key, name) in &s.key_names {
                gp.text(pos2(grid.left() + 4.0, y_of(*key) + kh / 2.0), Align2::LEFT_CENTER, instruments::spanish(name), FontId::proportional(10.0), TEXT_DIM());
            }
        }
        // Teclado del piano.
        let kp = p.with_clip_rect(keys);
        for key in 0..128u8 {
            let y = y_of(key);
            let black = BLACK[key as usize % 12];
            let fill = if self.roll.sounding == Some(key) {
                ACCENT()
            } else if black {
                Color32::from_gray(30)
            } else {
                Color32::from_gray(225)
            };
            kp.rect_filled(Rect::from_x_y_ranges(keys.left()..=keys.right() - if black { 20.0 } else { 0.0 }, y..=y + kh - 1.0), rr(2.0), fill);
            if key % 12 == 0 {
                kp.text(pos2(keys.right() - 4.0, y + kh / 2.0), Align2::RIGHT_CENTER, format!("Do{}", key as i32 / 12 - 1), FontId::proportional(9.0), BG());
            }
        }
        // Carril de control.
        let full = Rect::from_min_max(pos2(area.left(), grid.bottom()), area.max);
        let vp = p.with_clip_rect(full);
        vp.rect_filled(full, 0.0, PANEL());
        vp.hline(full.x_range(), grid.bottom() + 3.0, Stroke::new(2.0, if sr.hovered() || sr.dragged() { ACCENT() } else { BG() }));
        let y_val = |v: u8| vel.bottom() - 4.0 - v as f32 / 127.0 * (vel.height() - 8.0);
        // Líneas guía: 25 %, 50 %, 75 % y máximo, con su valor a la izquierda.
        for v in [32u8, 64, 96, 127] {
            vp.hline(vel.x_range(), y_val(v), Stroke::new(1.0, Color32::from_white_alpha(14)));
        }
        let mut tab_y = lane_tabs.top() + 4.0;
        for (c, nombre) in Carril::TODOS {
            let r = Rect::from_min_size(pos2(lane_tabs.left() + 4.0, tab_y), vec2(lane_tabs.width() - 8.0, 20.0));
            let activo = c == carril;
            vp.rect_filled(r, rr(4.0), if activo { ACCENT() } else { Color32::from_white_alpha(8) });
            vp.text(r.center(), Align2::CENTER_CENTER, tr(nombre), FontId::proportional(11.0), if activo { BG() } else { TEXT_DIM() });
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
        // Regla con números de compás y cursor de reproducción sobre rejilla y carril.
        let rp = p.with_clip_rect(regla);
        rp.rect_filled(regla, 0.0, PANEL());
        let compas = beat * self.engine.beats.load(Relaxed).max(1) as f64;
        let primero = ((rel_at(grid.left()) + clip.start as f64 - origin as f64) / compas).floor().max(0.0) as i64;
        for b in primero..primero + 400 {
            let x = x_of(origin as f64 + b as f64 * compas - clip.start as f64);
            if x > regla.right() {
                break;
            }
            rp.vline(x, regla.center().y..=regla.bottom(), Stroke::new(1.0, BORDER()));
            rp.text(pos2(x + 3.0, regla.top() + 1.0), Align2::LEFT_TOP, (b + 1).to_string(), FontId::monospace(10.0), TEXT_DIM());
        }
        let cx = x_of(self.pos() as f64 - clip.start as f64);
        if (regla.left()..=regla.right()).contains(&cx) {
            let lp = p.with_clip_rect(Rect::from_min_max(regla.min, pos2(area.right(), area.bottom())));
            lp.vline(cx, regla.top()..=area.bottom(), Stroke::new(1.5, TEXT()));
            let t = regla.top();
            lp.add(egui::Shape::convex_polygon(vec![pos2(cx - 6.0, t), pos2(cx + 6.0, t), pos2(cx + 6.0, t + 6.0), pos2(cx, t + 12.0), pos2(cx - 6.0, t + 6.0)], ACCENT(), Stroke::new(1.0, BG())));
        }
    }
}
