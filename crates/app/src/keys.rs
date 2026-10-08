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
}

impl Cmd {
    pub const ALL: [Cmd; 27] = [
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
            Cmd::Delete => self.delete(),
            Cmd::Save => self.save(),
            Cmd::SaveAs => {
                let r = self.save_as();
                self.report("Proyecto guardado", r);
            }
            Cmd::Split => self.split(self.pos()),
            Cmd::Undo => self.undo(),
            Cmd::Redo => self.redo(),
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
            Cmd::Mixer => (self.show_mixer, self.bottom_tab) = (!(self.show_mixer && self.bottom_tab == 0), 0),
            Cmd::Library => self.show_lib = !self.show_lib,
            Cmd::ZoomIn => self.zoom(1.25),
            Cmd::ZoomOut => self.zoom(0.8),
            Cmd::Cancel => (self.time_sel, self.razor_lanes, self.tool) = (None, None, Tool::Select),
            Cmd::Marker => self.add_marker(self.pos()),
        }
    }

    pub fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || matches!(self.dialog, Some(Dialog::Keys(Some(_)))) {
            return;
        }
        let typing = self.roll.typing && self.show_mixer && self.bottom_tab == 1;
        for e in ctx.input(|i| i.events.clone()) {
            match e {
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
                        self.engine.send_midi(if pressed { [0x90, note, 100] } else { [0x80, note, 0] });
                        self.roll.sounding = pressed.then_some(note);
                        if pressed && self.roll.step {
                            self.step_note(note);
                        }
                    }
                },
                Event::Key { key, pressed: true, repeat: false, modifiers, .. } => {
                    let text = key_text(key, modifiers);
                    if let Some(c) = Cmd::ALL.into_iter().find(|c| self.key_for(*c) == text) {
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
    pub fn keys_editor(&mut self, ui: &mut egui::Ui, capture: &mut Option<Capture>) {
        ui.heading(tr("Atajos de teclado y ratón"));
        ui.label(RichText::new(tr("Haz clic en un atajo y pulsa la nueva combinación. Esc cancela, Retroceso lo deja vacío.")).size(11.0).color(TEXT_DIM));
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
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            egui::Grid::new("keys").num_columns(2).striped(true).spacing([24.0, 6.0]).show(ui, |ui| {
                ui.label(RichText::new(tr("Rueda del ratón")).strong());
                ui.end_row();
                for w in Wheel::ALL {
                    ui.label(tr(w.label()));
                    let current = self.wheel_for(w);
                    let text = match (*capture == Some(Capture::Wheel(w)), current.as_str()) {
                        (true, _) => "Mantén los modificadores y gira la rueda…".to_string(),
                        (false, "") => "Rueda".to_string(),
                        (false, m) => format!("{m} + rueda"),
                    };
                    if ui.button(text).clicked() {
                        *capture = Some(Capture::Wheel(w));
                    }
                    ui.end_row();
                }
                ui.label(RichText::new(tr("Teclado")).strong());
                ui.end_row();
                for c in Cmd::ALL {
                    ui.label(tr(c.label()));
                    let current = self.key_for(c);
                    let dup = !current.is_empty() && Cmd::ALL.iter().filter(|o| self.key_for(**o) == current).count() > 1;
                    let text = if *capture == Some(Capture::Key(c)) {
                        "Pulsa la combinación…".to_string()
                    } else if current.is_empty() {
                        "—".into()
                    } else {
                        current
                    };
                    let b = ui.button(RichText::new(text).color(if dup { METER[2] } else { TEXT }));
                    if b.on_hover_text(if dup { "Atajo repetido en otro comando" } else { "Clic para cambiar" }).clicked() {
                        *capture = Some(Capture::Key(c));
                    }
                    ui.end_row();
                }
            });
        });
        if ui.button(tr("Restaurar valores por defecto")).clicked() {
            (self.config.keys, self.config.wheel) = Default::default();
            self.config.save();
        }
    }
}
