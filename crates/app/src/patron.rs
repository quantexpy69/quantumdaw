//! Secuenciador de patrones (pestaña PATRÓN): la región MIDI abierta como cuadrícula de pasos, un
//! carril por sonido (kit General MIDI y cualquier otra nota de la región). Pasos de 1/8, 1/16 o
//! 1/32; a la derecha, el editor gráfico del carril elegido (velocidad, ganancia, ataque o
//! repeticiones) con sus herramientas; patrones automáticos por estilo; menú circular con el clic
//! derecho; y el patrón se coloca en el cursor o se arrastra a cualquier pista.
use crate::creativo::Azar;
use crate::*;
use egui::{Align2, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};

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
const GRAFICOS: [&str; 4] = ["Velocidad", "Ganancia", "Ataque", "Repeticiones"];

/// Patrones automáticos por estilo: nombre y un compás de 16 semicorcheas por pieza (un dígito es la
/// fuerza del golpe, del 1 al 9; «r» es un redoble doble; «.» nada).
pub const ESTILOS: [(&str, &[(u8, &str)]); 13] = [
    ("Rock", &[(36, "9.......9.8....."), (38, "....9.......9..."), (42, "8.5.7.5.8.5.7.5.")]),
    ("Pop", &[(36, "9......8..8....."), (38, "....8.......8..."), (39, "....6.......6..."), (42, "7.4.6.4.7.4.6.4.")]),
    ("Funk", &[(36, "9..7..8...8..7.."), (38, "....9..3.3..9..3"), (42, "8463846384638463")]),
    ("Hip hop", &[(36, "9......8..8....."), (38, "....9.......9..."), (42, "7.5.7.5.7.5.7.5.")]),
    ("Trap", &[(36, "9......8..8....."), (38, "........9......."), (42, "7.5.7.5.7.5.7.rr")]),
    ("Reggaetón", &[(36, "9...9...9...9..."), (38, "...8..8....8..8."), (42, "7.4.7.4.7.4.7.4.")]),
    ("Cumbia", &[(36, "9.......8......."), (37, "..8...8...8...8."), (42, "8.558.558.558.55")]),
    ("House", &[(36, "9...9...9...9..."), (39, "....8.......8..."), (46, "..8...8...8...8."), (42, "6..46..46..46..4")]),
    ("Disco", &[(36, "9...9...9...9..."), (38, "....8.......8..."), (46, "..7...7...7...7."), (42, "6.4.6.4.6.4.6.4.")]),
    ("Bossa nova", &[(36, "7..67..67..67..6"), (37, "8..8..8...8..8.."), (42, "5.4.5.4.5.4.5.4.")]),
    ("Balada", &[(36, "9.......8.7....."), (37, "....8.......8..."), (42, "6.4.6.4.6.4.6.4.")]),
    ("Metal", &[(36, "8888888888888888"), (38, "....9.......9..."), (49, "9...8...9...8...")]),
    ("Drum & bass", &[(36, "9.........8....."), (38, "....9.......9..."), (42, "6.4.6.4.6.4.6.4.")]),
];

pub struct Patron {
    /// Pasos por tiempo (2 = 1/8, 4 = 1/16, 8 = 1/32).
    div: u32,
    carril: usize,
    grafico: usize,
    euclid: u32,
    /// Arrastre pintando celdas: encender (true) o apagar.
    pintando: Option<bool>,
    semilla: u64,
    /// Estilo elegido para los patrones automáticos.
    estilo: usize,
}

impl Default for Patron {
    fn default() -> Self {
        Self { div: 4, carril: 0, grafico: 0, euclid: 5, pintando: None, semilla: 0xBEA7, estilo: 0 }
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

    /// Escribe en la región un patrón del estilo elegido: el compás de base repetido y, desde los dos
    /// compases, un relleno de caja en el último tiempo del último compás.
    fn generar_estilo(&mut self, k: usize) {
        let Some((_, _, _, inicio, paso, n)) = self.datos_patron() else { return };
        let beat = self.engine.beat_frames();
        let tiempos = self.engine.beats.load(Relaxed).max(1) as usize;
        let pasos_compas = tiempos * 4;
        let semis = (n as f64 * paso / (beat / 4.0)).round() as usize;
        let compases = semis.div_ceil(pasos_compas).max(1);
        let sub = beat / 4.0;
        let mut notas = vec![];
        for b in 0..compases {
            let relleno = compases >= 2 && b + 1 == compases;
            for &(key, linea) in ESTILOS[k.min(ESTILOS.len() - 1)].1 {
                let c: Vec<char> = linea.chars().collect();
                for s in 0..pasos_compas {
                    let x = b * pasos_compas + s;
                    if x >= semis || (relleno && s >= pasos_compas - 4 && key != 36) {
                        continue;
                    }
                    let (vel, rep) = match c[s % c.len()] {
                        d @ '1'..='9' => ((d as u8 - b'0') * 14, 1),
                        'r' => (90, 2),
                        _ => continue,
                    };
                    for r in 0..rep {
                        let start = (inicio as f64 + x as f64 * sub + r as f64 * sub / rep as f64).max(0.0) as u64;
                        notas.push(Note::new(start, (sub / rep as f64 * 0.9) as u64, key, vel));
                    }
                }
            }
            if relleno {
                for s in 0..4 {
                    let x = b * pasos_compas + pasos_compas - 4 + s;
                    if x < semis {
                        notas.push(Note::new((inicio as f64 + x as f64 * sub).max(0.0) as u64, (sub * 0.9) as u64, 38, 80 + s as u8 * 14));
                    }
                }
            }
        }
        self.edit();
        self.set_notes(notas);
        self.status = format!("{}: {}", tr("Patrón generado"), tr(ESTILOS[k.min(ESTILOS.len() - 1)].0));
    }

    /// Acciones del menú circular de un carril.
    pub fn pie_patron(&mut self, a: pie::Accion) {
        use pie::Accion;
        let Some((_, _, notas, inicio, paso, n)) = self.datos_patron() else { return };
        let carriles = self.carriles(&notas);
        let key = carriles[self.patron.carril.min(carriles.len() - 1)].1;
        let actual = Self::pasos_carril(&notas, key, inicio, paso, n);
        self.edit();
        match a {
            Accion::PatLlenar => self.editar_carril(key, |p| p.into_iter().map(|x| Some((x.unwrap_or(Note::new(0, 0, key, 100)), 1))).collect()),
            Accion::PatEuclidiano => {
                let golpes = euclidiano(self.patron.euclid, n as u32);
                self.editar_carril(key, |_| golpes.into_iter().map(|g| g.then_some((Note::new(0, 0, key, 100), 1))).collect());
            }
            Accion::PatRotarIzq | Accion::PatRotarDer => {
                let dir = if a == Accion::PatRotarDer { 1 } else { -1i64 };
                self.editar_carril(key, |_| (0..n).map(|k| actual[((k as i64 - dir).rem_euclid(n as i64)) as usize]).collect());
            }
            Accion::PatAleatorio => {
                let semilla = self.patron.semilla;
                self.patron.semilla = semilla.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let mut az = Azar::new(semilla);
                self.editar_carril(key, |p| p.iter().map(|_| (az.hasta(100) < 35).then(|| (Note::new(0, 0, key, 60 + az.hasta(67) as u8), 1))).collect());
            }
            Accion::PatHumanizar => {
                let mut az = Azar::new(self.patron.semilla ^ 0x51);
                self.editar_carril(key, |_| actual.iter().map(|c| c.map(|(x, r)| (Note { vel: (x.vel as i32 + az.hasta(25) as i32 - 12).clamp(20, 127) as u8, ..x }, r))).collect());
            }
            Accion::PatBorrar => self.editar_carril(key, |p| p.iter().map(|_| None).collect()),
            Accion::PatDuplicar => self.duplicar_patron(),
            _ => {}
        }
    }

    /// Duplica el largo del patrón y copia lo que había en la segunda mitad.
    fn duplicar_patron(&mut self) {
        let Some((i, j, notas, ..)) = self.datos_patron() else { return };
        let largo = self.s.tracks[i].clips[j].len;
        let mut v = notas.clone();
        v.extend(notas.iter().map(|x| Note { start: x.start + largo, ..*x }));
        self.s.tracks[i].clips[j].len = largo * 2;
        self.set_notes(v);
    }

    /// Abre el menú circular de un carril.
    fn abrir_pie_patron(&mut self, centro: egui::Pos2) {
        use pie::Accion;
        self.pie = Some(pie::Pie {
            centro,
            titulo: "CARRIL",
            items: vec![
                ("Llenar", Accion::PatLlenar, true),
                ("Euclidiano", Accion::PatEuclidiano, true),
                ("Rotar ▶", Accion::PatRotarDer, true),
                ("Humanizar", Accion::PatHumanizar, true),
                ("Duplicar", Accion::PatDuplicar, true),
                ("Borrar carril", Accion::PatBorrar, true),
                ("Aleatorio", Accion::PatAleatorio, true),
                ("◀ Rotar", Accion::PatRotarIzq, true),
            ],
            con_tecla: false,
            region: None,
            en_partitura: false,
            arrastrando: false,
        });
    }

    /// Pestaña PATRÓN.
    pub fn vista_patron(&mut self, ui: &mut egui::Ui) {
        let Some((i, j, notas, inicio, paso, n)) = self.datos_patron() else {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(tr("Secuenciador de patrones")).size(18.0).strong());
                ui.label(RichText::new(tr("Abre una región MIDI (doble clic) para editarla por pasos, o crea un patrón de batería nuevo.")).color(TEXT_DIM()));
                ui.add_space(8.0);
                if ui.add(egui::Button::new(RichText::new(tr("+ Crear patrón de batería")).color(BG()).strong()).fill(ACCENT())).clicked() {
                    self.crear_patron();
                }
            });
            return;
        };
        let carriles = self.carriles(&notas);
        self.patron.carril = self.patron.carril.min(carriles.len() - 1);
        let tiempos = self.engine.beats.load(Relaxed).max(1) as f64;
        // Barra superior: pista, largo, resolución, estilo y cómo llevarlo a la pista.
        ui.horizontal(|ui| {
            let t = &self.s.tracks[i];
            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
            ui.painter().rect_filled(r, widgets::rr(5.0), t.color);
            ui.label(RichText::new(&t.name).strong());
            ui.separator();
            ui.label(RichText::new(tr("LARGO")).size(11.0).color(TEXT_DIM()));
            let compas = self.bar_frames();
            if ui.button("−").on_hover_text(tr("Un compás menos")).clicked() && self.s.tracks[i].clips[j].len > compas {
                self.edit();
                self.s.tracks[i].clips[j].len -= compas;
            }
            ui.label(RichText::new(format!("{:.0} {}", n as f64 / self.patron.div as f64 / tiempos, tr("compases"))).monospace());
            if ui.button("+").on_hover_text(tr("Un compás más")).clicked() {
                self.edit();
                self.s.tracks[i].clips[j].len += compas;
            }
            ui.separator();
            for (d, texto) in [(2, "1/8"), (4, "1/16"), (8, "1/32")] {
                if ui.add(egui::Button::selectable(self.patron.div == d, texto)).clicked() {
                    self.patron.div = d;
                }
            }
            ui.separator();
            // Patrón automático por estilo.
            egui::ComboBox::from_id_salt("patron-estilo").width(120.0).selected_text(tr(ESTILOS[self.patron.estilo].0)).show_ui(ui, |ui| {
                for (k, (nombre, _)) in ESTILOS.iter().enumerate() {
                    ui.selectable_value(&mut self.patron.estilo, k, tr(nombre));
                }
            });
            if ui.button(tr("Generar")).on_hover_text(tr("Escribe un patrón completo del estilo elegido (reemplaza el actual)")).clicked() {
                self.generar_estilo(self.patron.estilo);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Arrastrar el patrón a cualquier pista (o colocarlo en el cursor).
                let arrastre = ui.add(egui::Button::new(RichText::new(tr("Arrastrar a una pista"))).sense(Sense::click_and_drag())).on_hover_text(tr("Arrastra el patrón a la línea de tiempo: se copia en la pista y el compás donde lo sueltes"));
                arrastre.dnd_set_drag_payload(library::LibItem::Patron(i, j));
                if arrastre.dragged() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                }
                if ui.add(egui::Button::new(RichText::new(tr("Colocar en la pista")).color(BG()).strong()).fill(ACCENT())).on_hover_text(tr("Pone una copia del patrón en su pista, donde está el cursor")).clicked() {
                    self.edit();
                    let mut copia = self.s.tracks[i].clips[j].clone();
                    copia.start = self.pos();
                    copia.selected = false;
                    self.s.tracks[i].clips.push(copia);
                    self.dirty = true;
                }
                let sonando = self.playing();
                if ui.add(egui::Button::new(if sonando { tr("■ Detener") } else { tr("▶ Reproducir en bucle") })).clicked() {
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
        ui.separator();
        // Cuadrícula a la izquierda y editor gráfico del carril a la derecha, cada uno en su lugar.
        let zona = ui.available_rect_before_wrap();
        let panel = 310.0f32.min(zona.width() * 0.4);
        let izquierda = Rect::from_min_max(zona.min, pos2(zona.right() - panel - 12.0, zona.bottom()));
        let derecha = Rect::from_min_max(pos2(zona.right() - panel, zona.top()), zona.max);
        ui.allocate_rect(zona, Sense::hover());
        ui.scope_builder(egui::UiBuilder::new().max_rect(izquierda), |ui| self.cuadricula_patron(ui, i, &notas, &carriles, inicio, paso, n));
        ui.scope_builder(egui::UiBuilder::new().max_rect(derecha), |ui| {
            egui::ScrollArea::vertical().id_salt("patron-panel").auto_shrink([false, false]).show(ui, |ui| self.editor_grafico(ui, &notas, &carriles, inicio, paso, n));
        });
    }

    /// Cuadrícula: un carril por sonido; clic o arrastre para pintar, clic derecho para el menú.
    #[allow(clippy::too_many_arguments)]
    fn cuadricula_patron(&mut self, ui: &mut egui::Ui, i: usize, notas: &[Note], carriles: &[(String, u8)], inicio: i64, paso: f64, n: usize) {
        egui::ScrollArea::both().id_salt("patron-rejilla").auto_shrink([false, false]).show(ui, |ui| {
            let cab = 180.0;
            let celda = ((ui.available_width() - cab - 10.0) / n as f32).clamp(12.0, 36.0);
            let alto = celda.clamp(24.0, 30.0);
            // Números de tiempo arriba.
            let (r, _) = ui.allocate_exact_size(vec2(cab + celda * n as f32, 18.0), Sense::hover());
            for k in (0..n).step_by(self.patron.div as usize) {
                let tiempo = k / self.patron.div as usize;
                ui.painter().text(pos2(r.left() + cab + k as f32 * celda + 3.0, r.center().y), Align2::LEFT_CENTER, (tiempo + 1).to_string(), FontId::proportional(10.5), TEXT_DIM());
            }
            if !ui.input(|i| i.pointer.primary_down()) {
                self.patron.pintando = None;
            }
            for (li, (nombre, key)) in carriles.iter().enumerate() {
                let color = COLORES[li % COLORES.len()];
                let pasos = Self::pasos_carril(notas, *key, inicio, paso, n);
                let elegido = li == self.patron.carril;
                let (fila, _) = ui.allocate_exact_size(vec2(cab + celda * n as f32, alto + 4.0), Sense::hover());
                if elegido {
                    ui.painter().rect_filled(fila, widgets::rr(6.0), color.gamma_multiply(0.07));
                    ui.painter().rect_stroke(fila, widgets::rr(6.0), Stroke::new(1.0, color.gamma_multiply(0.6)), egui::StrokeKind::Inside);
                }
                // Cabecera del carril: punto, nombre (clic: elegir y escuchar) y nota.
                let cab_r = Rect::from_min_size(fila.min, vec2(cab, fila.height()));
                let rc = ui.interact(cab_r, egui::Id::new(("carril", li)), Sense::click());
                if rc.clicked() {
                    self.patron.carril = li;
                    self.escuchar_en(i);
                    self.engine.send_midi([0x90, *key, 110]);
                    self.engine.send_midi([0x80, *key, 0]);
                }
                if rc.secondary_clicked() {
                    self.patron.carril = li;
                    if let Some(q) = rc.interact_pointer_pos() {
                        self.abrir_pie_patron(q);
                    }
                }
                let p = ui.painter();
                if rc.hovered() {
                    p.rect_filled(cab_r.shrink(1.0), widgets::rr(5.0), Color32::from_white_alpha(8));
                }
                p.circle_filled(cab_r.left_center() + vec2(12.0, 0.0), 5.0, color);
                p.text(cab_r.left_center() + vec2(26.0, 0.0), Align2::LEFT_CENTER, nombre, FontId::proportional(12.5), if elegido { TEXT() } else { TEXT_DIM() });
                let chip = Rect::from_center_size(cab_r.right_center() - vec2(28.0, 0.0), vec2(38.0, 20.0));
                p.rect_filled(chip, widgets::rr(6.0), color.gamma_multiply(0.18));
                p.text(chip.center(), Align2::CENTER_CENTER, nombre_nota(*key), FontId::proportional(11.0), color);
                // Celdas.
                for (k, c) in pasos.iter().enumerate() {
                    let cr = Rect::from_min_size(pos2(fila.left() + cab + k as f32 * celda + 1.0, fila.top() + 2.0), vec2(celda - 3.0, fila.height() - 4.0));
                    let resp = ui.interact(cr, egui::Id::new(("celda", li, k)), Sense::click_and_drag());
                    let tiempo_fuerte = (k / self.patron.div as usize).is_multiple_of(2);
                    let fondo = match c {
                        Some((x, _)) => color.gamma_multiply(0.45 + 0.55 * x.vel as f32 / 127.0),
                        None if tiempo_fuerte => ELEVATED(),
                        None => PANEL().lerp_to_gamma(ELEVATED(), 0.5),
                    };
                    ui.painter().rect_filled(cr, widgets::rr(5.0), fondo);
                    if resp.hovered() && c.is_none() {
                        ui.painter().rect_stroke(cr, widgets::rr(5.0), Stroke::new(1.0, color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
                    }
                    if let Some((_, rep)) = c.filter(|c| c.1 > 1) {
                        for r in 1..rep {
                            let x = cr.left() + cr.width() * r as f32 / rep as f32;
                            ui.painter().vline(x, cr.shrink(4.0).y_range(), Stroke::new(1.5, BG()));
                        }
                    }
                    if resp.secondary_clicked() {
                        self.patron.carril = li;
                        if let Some(q) = resp.interact_pointer_pos() {
                            self.abrir_pie_patron(q);
                        }
                    }
                    // Pintar: el primer clic decide si se encienden o se apagan las celdas que recorre.
                    let pulsada = resp.is_pointer_button_down_on() && ui.input(|i| i.pointer.primary_down()) || (self.patron.pintando.is_some() && ui.input(|i| i.pointer.hover_pos()).is_some_and(|q| cr.contains(q)));
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
            }
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Clic o arrastre: pintar golpes · clic en el nombre: escuchar · clic derecho: más opciones")).size(11.0).color(TEXT_DIM()));
        });
    }

    /// Editor gráfico del carril elegido (arriba a la derecha): la curva editable de velocidad,
    /// ganancia, ataque o repeticiones y las herramientas del carril.
    fn editor_grafico(&mut self, ui: &mut egui::Ui, notas: &[Note], carriles: &[(String, u8)], inicio: i64, paso: f64, n: usize) {
        let li = self.patron.carril;
        let (nombre, key) = carriles[li].clone();
        let color = COLORES[li % COLORES.len()];
        let pasos = Self::pasos_carril(notas, key, inicio, paso, n);
        egui::Frame::new().fill(ELEVATED()).stroke(Stroke::new(1.0, BORDER())).corner_radius(widgets::rr(10.0)).inner_margin(10.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 6.0, color);
                ui.label(RichText::new(nombre.to_uppercase()).strong());
                ui.label(RichText::new(nombre_nota(key)).size(11.0).color(TEXT_DIM()));
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (k, t) in GRAFICOS.iter().enumerate() {
                    if ui.add(egui::Button::selectable(self.patron.grafico == k, tr(t))).clicked() {
                        self.patron.grafico = k;
                    }
                }
            });
            ui.add_space(4.0);
            // La curva: una barra por paso; arrastrar dibuja los valores.
            let alto = 140.0;
            let (area, rg) = ui.allocate_exact_size(vec2(ui.available_width(), alto), Sense::click_and_drag());
            let p = ui.painter();
            p.rect_filled(area, widgets::rr(6.0), BG());
            for f in [0.25, 0.5, 0.75] {
                p.hline(area.x_range(), area.bottom() - f * area.height(), Stroke::new(1.0, Color32::from_white_alpha(10)));
            }
            let w = area.width() / n as f32;
            let valor = |x: &Note, rep: u32| match self.patron.grafico {
                0 => x.vel as f32 / 127.0,
                1 => x.gain as f32 / 127.0,
                2 => x.attack as f32 / 127.0,
                _ => rep as f32 / 4.0,
            };
            for (k, c) in pasos.iter().enumerate() {
                if let Some((x, rep)) = c {
                    let h = valor(x, *rep) * (alto - 8.0);
                    let br = Rect::from_min_max(pos2(area.left() + k as f32 * w + 1.0, area.bottom() - 4.0 - h), pos2(area.left() + (k + 1) as f32 * w - 1.0, area.bottom() - 4.0));
                    p.rect_filled(br, 2.0, color);
                }
            }
            if (rg.dragged() || rg.clicked())
                && let Some(q) = rg.interact_pointer_pos()
            {
                let k = ((q.x - area.left()) / w).floor().max(0.0) as usize;
                let v = ((area.bottom() - 4.0 - q.y) / (alto - 8.0)).clamp(0.0, 1.0);
                if rg.drag_started() || rg.clicked() {
                    self.edit();
                }
                let tipo = self.patron.grafico;
                let actual = pasos.clone();
                self.editar_carril(key, |_| {
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
            }
            ui.label(RichText::new(tr("Arrastra sobre las barras para dibujar los valores")).size(10.5).color(TEXT_DIM()));
            ui.separator();
            // Herramientas del carril.
            ui.label(RichText::new(tr("HERRAMIENTAS DEL CARRIL")).size(10.0).strong().color(TEXT_DIM()));
            let boton = |ui: &mut egui::Ui, texto: &str| ui.add(egui::Button::new(tr(texto)).min_size(vec2(88.0, 28.0)));
            let mut accion = None;
            ui.horizontal_wrapped(|ui| {
                use pie::Accion;
                for (t, a) in [("Llenar", Accion::PatLlenar), ("Aleatorio", Accion::PatAleatorio), ("Humanizar", Accion::PatHumanizar), ("◀ Rotar", Accion::PatRotarIzq), ("Rotar ▶", Accion::PatRotarDer), ("Borrar", Accion::PatBorrar)] {
                    if boton(ui, t).clicked() {
                        accion = Some(a);
                    }
                }
            });
            ui.horizontal(|ui| {
                if boton(ui, "Euclidiano").on_hover_text(tr("Reparte los golpes lo más parejo posible en el patrón")).clicked() {
                    accion = Some(pie::Accion::PatEuclidiano);
                }
                ui.add(egui::DragValue::new(&mut self.patron.euclid).range(1..=n as u32).suffix(format!(" {}", tr("golpes"))));
            });
            if let Some(a) = accion {
                self.pie_patron(a);
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
