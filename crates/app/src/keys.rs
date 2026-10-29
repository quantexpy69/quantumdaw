//! Atajos configurables: comandos de teclado, acciones de la rueda del ratón y teclado musical.
//! Las asignaciones se guardan en ~/.config/quantum-daw/config.json.
use crate::*;
use egui::{Modifiers, MouseWheelUnit, RichText};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    Play,
    Record,
    Home,
    End,
    Delete,
    Save,
    SaveAs,
    Split,
    Undo,
    Redo,
    SelectAll,
    Automation,
    NewTrack,
    Import,
    Export,
    Loop,
    Metronome,
    Select,
    Pencil,
    Blade,
    Snap,
    Mixer,
    Library,
    ZoomIn,
    ZoomOut,
    Cancel,
    Marker,
    Capture,
    Inspector,
}

impl Cmd {
    pub const ALL: [Cmd; 29] = [
        Cmd::Play,
        Cmd::Record,
        Cmd::Home,
        Cmd::End,
        Cmd::Delete,
        Cmd::Save,
        Cmd::SaveAs,
        Cmd::Split,
        Cmd::Undo,
        Cmd::Redo,
        Cmd::SelectAll,
        Cmd::Automation,
        Cmd::NewTrack,
        Cmd::Import,
        Cmd::Export,
        Cmd::Loop,
        Cmd::Metronome,
        Cmd::Select,
        Cmd::Pencil,
        Cmd::Blade,
        Cmd::Snap,
        Cmd::Mixer,
        Cmd::Library,
        Cmd::ZoomIn,
        Cmd::ZoomOut,
        Cmd::Cancel,
        Cmd::Marker,
        Cmd::Capture,
        Cmd::Inspector,
    ];

    fn id(self) -> String {
        format!("{self:?}")
    }

    pub fn label(self) -> &'static str {
        match self {
            Cmd::Play => "Reproducir / pausa",
            Cmd::Record => "Grabar",
            Cmd::Home => "Ir al inicio",
            Cmd::End => "Ir al final",
            Cmd::Delete => "Eliminar selección",
            Cmd::Save => "Guardar",
            Cmd::SaveAs => "Guardar como",
            Cmd::Split => "Dividir en el cursor",
            Cmd::Undo => "Deshacer",
            Cmd::Redo => "Rehacer",
            Cmd::SelectAll => "Seleccionar todo",
            Cmd::Automation => "Mostrar automatización",
            Cmd::NewTrack => "Nueva pista",
            Cmd::Import => "Importar audio",
            Cmd::Export => "Exportar",
            Cmd::Loop => "Loop",
            Cmd::Metronome => "Metrónomo",
            Cmd::Select => "Herramienta selección",
            Cmd::Pencil => "Herramienta lápiz",
            Cmd::Blade => "Herramienta cuchilla",
            Cmd::Snap => "Ajustar a la rejilla",
            Cmd::Mixer => "Mostrar mixer",
            Cmd::Library => "Mostrar biblioteca",
            Cmd::ZoomIn => "Acercar",
            Cmd::ZoomOut => "Alejar",
            Cmd::Cancel => "Cancelar / quitar selección",
            Cmd::Marker => "Añadir marca en el cursor",
            Cmd::Capture => "Capturar lo último tocado (grabación retrospectiva)",
            Cmd::Inspector => "Mostrar el inspector",
        }
    }

    fn default_key(self) -> &'static str {
        match self {
            Cmd::Play => "Space",
            Cmd::Record => "Ctrl+R",
            Cmd::Home => "Home",
            Cmd::End => "End",
            Cmd::Delete => "Delete",
            Cmd::Save => "Ctrl+S",
            Cmd::SaveAs => "Ctrl+Shift+S",
            Cmd::Split => "S",
            Cmd::Undo => "Ctrl+Z",
            Cmd::Redo => "Ctrl+Shift+Z",
            Cmd::SelectAll => "Ctrl+A",
            Cmd::Automation => "A",
            Cmd::NewTrack => "Ctrl+T",
            Cmd::Import => "Ctrl+I",
            Cmd::Export => "Ctrl+E",
            Cmd::Loop => "L",
            Cmd::Metronome => "K",
            Cmd::Select => "V",
            Cmd::Pencil => "P",
            Cmd::Blade => "B",
            Cmd::Snap => "N",
            Cmd::Mixer => "X",
            Cmd::Library => "Y",
            Cmd::ZoomIn => "Plus",
            Cmd::ZoomOut => "Minus",
            Cmd::Cancel => "Escape",
            Cmd::Marker => "M",
            Cmd::Capture => "Shift+R",
            Cmd::Inspector => "I",
        }
    }
}

/// Acciones de la rueda del ratón, elegidas según los modificadores pulsados.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Wheel {
    ScrollV,
    ScrollH,
    ZoomH,
    ZoomV,
}

impl Wheel {
    pub const ALL: [Wheel; 4] = [Wheel::ScrollV, Wheel::ScrollH, Wheel::ZoomH, Wheel::ZoomV];
    fn id(self) -> String {
        format!("{self:?}")
    }
    pub fn label(self) -> &'static str {
        match self {
            Wheel::ScrollV => "Desplazar pistas (vertical)",
            Wheel::ScrollH => "Desplazar en el tiempo (horizontal)",
            Wheel::ZoomH => "Zoom horizontal",
            Wheel::ZoomV => "Zoom de altura de pistas",
        }
    }
    fn default_mods(self) -> &'static str {
        match self {
            Wheel::ScrollV => "",
            Wheel::ScrollH => "Shift",
            Wheel::ZoomH => "Ctrl",
            Wheel::ZoomV => "Ctrl+Shift",
        }
    }
}

/// Qué se está capturando en el editor de atajos.
#[derive(Clone, Copy, PartialEq)]
pub enum Capture {
    Key(Cmd),
    Wheel(Wheel),
}

/// "Ctrl+Shift+Alt" según los modificadores (Ctrl equivale a Cmd en macOS).
fn mods_text(m: Modifiers) -> String {
    [(m.command || m.ctrl, "Ctrl"), (m.shift, "Shift"), (m.alt, "Alt")].iter().filter(|x| x.0).map(|x| x.1).collect::<Vec<_>>().join("+")
}

fn key_text(key: Key, m: Modifiers) -> String {
    let mods = mods_text(m);
    if mods.is_empty() { key.name().to_string() } else { format!("{mods}+{}", key.name()) }
}

/// Teclado musical (como en Logic): A W S E D F T G Y H U J K O L P = notas cromáticas.
const MUSIC_KEYS: [Key; 16] = [Key::A, Key::W, Key::S, Key::E, Key::D, Key::F, Key::T, Key::G, Key::Y, Key::H, Key::U, Key::J, Key::K, Key::O, Key::L, Key::P];

impl App {
    fn key_for(&self, c: Cmd) -> String {
        self.config.keys.get(&c.id()).cloned().unwrap_or_else(|| c.default_key().into())
    }
    fn wheel_for(&self, w: Wheel) -> String {
        self.config.wheel.get(&w.id()).cloned().unwrap_or_else(|| w.default_mods().into())
    }

    /// Acción de la rueda para los modificadores actuales.
    pub fn wheel_action(&self, m: Modifiers) -> Option<Wheel> {
        Wheel::ALL.into_iter().find(|w| self.wheel_for(*w) == mods_text(m))
    }

    /// Eventos de rueda convertidos a puntos, con la acción que les corresponde.
    pub fn wheel_events(&self, ui: &egui::Ui) -> Vec<(Wheel, egui::Vec2)> {
        ui.input(|i| i.events.clone())
            .into_iter()
            .filter_map(|e| match e {
                Event::MouseWheel { unit, delta, modifiers, .. } => {
                    let scale = match unit {
                        MouseWheelUnit::Point => 1.0,
                        MouseWheelUnit::Line => 40.0,
                        MouseWheelUnit::Page => 400.0,
                    };
                    Some((self.wheel_action(modifiers)?, delta * scale))
                }
                _ => None,
            })
            .collect()
    }

    pub fn run(&mut self, c: Cmd) {
        match c {
            Cmd::Play => self.toggle_play(),
            Cmd::Record => self.toggle_record(),
            Cmd::Home => self.go(0),
            Cmd::End => self.go(self.project_end()),
            Cmd::Delete if self.bottom_tab == 1 && self.roll.has_selection() => self.roll_delete_selected(),
            Cmd::Delete if self.aedit_active() => self.aedit_edit(audioedit::EditAct::Delete),
            Cmd::Delete => self.delete(),
            Cmd::Save => self.save(),
            Cmd::SaveAs => {
                let r = self.save_as();
                self.report("Proyecto guardado", r);
            }
            Cmd::Split => self.split(self.pos()),
            Cmd::Undo => self.undo(),
            Cmd::Redo => self.redo(),
            Cmd::SelectAll if self.aedit_active() => self.aedit_edit(audioedit::EditAct::SelectAll),
            Cmd::SelectAll => self.select_all(),
            Cmd::Automation => self.show_auto = !self.show_auto,
            Cmd::NewTrack => self.dialog = Some(Dialog::NewTrack(String::new(), TrackKind::AudioStereo, 1, None)),
            Cmd::Import => self.import(rfd::FileDialog::new().pick_files().unwrap_or_default()),
            Cmd::Export => self.dialog = Some(Dialog::Export(Format::Wav24, 0, 0, 0)),
            Cmd::Loop => self.toggle_loop(),
            Cmd::Metronome => _ = self.engine.metronome.fetch_xor(true, Relaxed),
            Cmd::Select => self.set_tool(Tool::Select),
            Cmd::Pencil => self.set_tool(Tool::Pencil),
            Cmd::Blade => self.set_tool(Tool::Blade),
            Cmd::Snap => self.snap = !self.snap,
            Cmd::Mixer => (self.show_mixer, self.bottom_tab, self.vista_mix) = (!(self.show_mixer && self.bottom_tab == 0), 0, false),
            Cmd::Library => self.show_lib = !self.show_lib,
            Cmd::ZoomIn => self.zoom(1.25),
            Cmd::ZoomOut => self.zoom(0.8),
            Cmd::Cancel => (self.time_sel, self.razor_lanes, self.tool) = (None, None, Tool::Select),
            Cmd::Marker => self.add_marker(self.pos()),
            Cmd::Capture => self.capturar(),
            Cmd::Inspector => self.show_inspector = !self.show_inspector,
        }
    }

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        // Mientras se captura un atajo nuevo (en su ventana o en Preferencias), las teclas son para él.
        if ctx.egui_wants_keyboard_input() || matches!(self.dialog, Some(Dialog::Keys(Some(_)))) || self.welcome.as_ref().is_some_and(|w| w.captura.is_some()) {
            return;
        }
        let typing = self.roll.typing && self.show_mixer && self.bottom_tab == 1;
        let en_roll = self.roll_tomar_foco();
        // Con el ratón sobre QUANTUM Partitura, el teclado es para la partitura.
        if self.partitura_app.foco {
            return;
        }
        for e in ctx.input(|i| i.events.clone()) {
            match e {
                // Con el ratón sobre el piano roll, copiar/cortar/pegar actúan sobre las notas.
                Event::Copy if en_roll => self.roll_copiar(false),
                Event::Cut if en_roll => self.roll_copiar(true),
                Event::Paste(_) if en_roll => self.roll_pegar(),
                Event::Copy if self.aedit_active() => self.aedit_edit(audioedit::EditAct::Copy),
                Event::Cut if self.aedit_active() => self.aedit_edit(audioedit::EditAct::Cut),
                Event::Paste(_) if self.aedit_active() => self.aedit_edit(audioedit::EditAct::Paste),
                Event::Copy => self.copy(false),
                Event::Cut => self.copy(true),
                Event::Paste(_) => self.paste(),
                // Teclado musical: las teclas tocan notas (con repetición ignorada) y Z/X cambian de octava.
                Event::Key { key, pressed, repeat: false, modifiers, .. } if typing && modifiers.is_none() && (MUSIC_KEYS.contains(&key) || matches!(key, Key::Z | Key::X)) => match (key, pressed) {
                    (Key::Z, true) => self.roll.octave = (self.roll.octave - 1).max(0),
                    (Key::X, true) => self.roll.octave = (self.roll.octave + 1).min(8),
                    (Key::Z | Key::X, false) => {}
                    (k, _) => {
                        let note = (self.roll.octave * 12 + MUSIC_KEYS.iter().position(|m| *m == k).unwrap_or(0) as i32).clamp(0, 127) as u8;
                        let e = if pressed { [0x90, note, 100] } else { [0x80, note, 0] };
                        engine::registrar_llegada(e);
                        self.engine.send_midi(e);
                        self.roll.sounding = pressed.then_some(note);
                        if pressed && self.roll.step {
                            self.step_note(note);
                        }
                    }
                },
                Event::Key { key, pressed: true, repeat: false, modifiers, .. } => {
                    let text = key_text(key, modifiers);
                    if let Some(c) = Cmd::ALL.into_iter().find(|c| self.key_for(*c) == text) {
                        // Con el ratón sobre Q Drums, su transporte usa la barra espaciadora.
                        if c == Cmd::Play && self.espacio_propio {
                            continue;
                        }
                        self.run(c);
                    } else if key == Key::Backspace && modifiers.is_none() {
                        self.run(Cmd::Delete);
                    } else if key == Key::Y && modifiers.command {
                        self.run(Cmd::Redo);
                    }
                }
                _ => {}
            }
        }
    }

    /// Editor de atajos: clic en un atajo y pulsa la nueva combinación (o gira la rueda con los modificadores).
    /// Editor de atajos: en su ventana (con desplazamiento propio) o `embebido` en Preferencias → Atajos,
    /// donde desplaza la página entera. Mismo diseño que el resto de las preferencias.
    pub fn keys_editor(&mut self, ui: &mut egui::Ui, capture: &mut Option<Capture>, embebido: bool) {
        use crate::preferencias::{fila, tarjeta};
        if !embebido {
            ui.heading(tr("Atajos de teclado y ratón"));
            ui.label(RichText::new(tr("Haz clic en un atajo y pulsa la nueva combinación. Esc cancela, Retroceso lo deja vacío.")).size(11.0).color(TEXT_DIM()));
        }
        // Captura de la combinación pulsada.
        if let Some(c) = *capture {
            for e in ui.input(|i| i.events.clone()) {
                match (c, e) {
                    (Capture::Key(_), Event::Key { key: Key::Escape, pressed: true, .. }) | (Capture::Wheel(_), Event::Key { key: Key::Escape, pressed: true, .. }) => *capture = None,
                    (Capture::Key(cmd), Event::Key { key, pressed: true, modifiers, .. }) => {
                        let text = if key == Key::Backspace { String::new() } else { key_text(key, modifiers) };
                        self.config.keys.insert(cmd.id(), text);
                        *capture = None;
                    }
                    (Capture::Wheel(w), Event::MouseWheel { modifiers, .. }) => {
                        self.config.wheel.insert(w.id(), mods_text(modifiers));
                        *capture = None;
                    }
                    _ => {}
                }
            }
            if capture.is_none() {
                self.config.save();
            }
        }
        // Filtro de acciones (se recuerda mientras la ventana está abierta).
        let id_filtro = egui::Id::new("filtro-atajos");
        let mut filtro: String = ui.data_mut(|d| d.get_temp_mut_or_default::<String>(id_filtro).clone());
        let busca = crate::preferencias::sin_tildes(filtro.trim());
        let coincide = |texto: &str, atajo: &str| busca.is_empty() || crate::preferencias::sin_tildes(tr(texto)).contains(&busca) || atajo.to_lowercase().contains(&busca);
        // Botón de atajo: tecla con aspecto de chip; en rojo si se repite en otro comando.
        let chip = |ui: &mut egui::Ui, texto: String, activo: bool, repetido: bool| {
            let color = if repetido { METER[2] } else if activo { ACCENT() } else { TEXT() };
            ui.add(egui::Button::new(RichText::new(texto).size(13.0).color(color)).fill(if activo { ACCENT().gamma_multiply(0.15) } else { BG() }).stroke(egui::Stroke::new(1.0, if activo { ACCENT() } else { BORDER() })).min_size(egui::vec2(240.0, 28.0)).corner_radius(widgets::rr(7.0)))
        };
        let mut cuerpo = |app: &mut Self, ui: &mut egui::Ui| {
            tarjeta(ui, "BUSCAR", |ui| {
                fila(ui, "Acción o tecla", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut filtro).hint_text(tr("Por ejemplo: grabar, Ctrl+S…")).desired_width(ui.available_width()).min_size(egui::vec2(0.0, 28.0)).margin(egui::Margin::symmetric(10, 4)).vertical_align(egui::Align::Center));
                });
            });
            let ruedas: Vec<Wheel> = Wheel::ALL.into_iter().filter(|w| coincide(w.label(), &app.wheel_for(*w))).collect();
            if !ruedas.is_empty() {
                tarjeta(ui, "RUEDA DEL RATÓN", |ui| {
                    for w in ruedas {
                        let current = app.wheel_for(w);
                        let activo = *capture == Some(Capture::Wheel(w));
                        let text = match (activo, current.as_str()) {
                            (true, _) => tr("Mantén los modificadores y gira la rueda…").to_string(),
                            (false, "") => tr("Rueda").to_string(),
                            (false, m) => format!("{m} + {}", tr("rueda")),
                        };
                        if fila(ui, w.label(), |ui| chip(ui, text, activo, false)).clicked() {
                            *capture = Some(Capture::Wheel(w));
                        }
                    }
                });
            }
            let comandos: Vec<Cmd> = Cmd::ALL.into_iter().filter(|c| coincide(c.label(), &app.key_for(*c))).collect();
            tarjeta(ui, "TECLADO", |ui| {
                if comandos.is_empty() {
                    ui.label(RichText::new(tr("Ningún atajo coincide con la búsqueda.")).color(TEXT_DIM()));
                }
                for c in comandos {
                    let current = app.key_for(c);
                    let dup = !current.is_empty() && Cmd::ALL.iter().filter(|o| app.key_for(**o) == current).count() > 1;
                    let activo = *capture == Some(Capture::Key(c));
                    let text = if activo {
                        tr("Pulsa la combinación…").to_string()
                    } else if current.is_empty() {
                        "—".into()
                    } else {
                        current
                    };
                    let b = fila(ui, c.label(), |ui| chip(ui, text, activo, dup));
                    if b.on_hover_text(tr(if dup { "Atajo repetido en otro comando" } else { "Clic para cambiar" })).clicked() {
                        *capture = Some(Capture::Key(c));
                    }
                }
                ui.add_space(4.0);
                crate::preferencias::a_la_derecha(ui, |ui| {
                    if crate::preferencias::boton(ui, "Restaurar valores por defecto").clicked() {
                        (app.config.keys, app.config.wheel) = Default::default();
                        app.config.save();
                    }
                });
            });
        };
        if embebido {
            cuerpo(self, ui);
        } else {
            egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| cuerpo(self, ui));
        }
        ui.data_mut(|d| d.insert_temp(id_filtro, filtro));
    }
}
