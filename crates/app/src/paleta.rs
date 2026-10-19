//! Paleta de comandos (Ctrl/Cmd+K): busca cualquier acción por su nombre y la ejecuta con Enter.
use crate::keys::Cmd;
use crate::*;
use egui::{Key, RichText};

#[derive(Clone, Copy)]
enum Accion {
    Cmd(Cmd),
    AudioConfig,
    Atajos,
    Circulo,
    Instrumento,
    PianoRoll,
    Registros,
    Informe,
    Tema(usize),
    Mastering,
    Espacio(usize),
    Panel,
}

/// Estado de la paleta abierta: texto buscado y fila elegida.
#[derive(Default)]
pub struct Paleta {
    texto: String,
    fila: usize,
}

/// Minúsculas y sin tildes, para buscar «configuracion» y encontrar «Configuración».
fn normal(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c {
        'á' | 'à' | 'â' | 'ã' => 'a',
        'é' | 'ê' => 'e',
        'í' => 'i',
        'ó' | 'ô' | 'õ' => 'o',
        'ú' | 'ü' => 'u',
        'ç' => 'c',
        c => c,
    }).collect()
}

fn acciones() -> Vec<(String, &'static str, Accion)> {
    let mut v: Vec<(String, &str, Accion)> = Cmd::ALL.iter().map(|&c| (tr(c.label()).to_string(), tr("Comando"), Accion::Cmd(c))).collect();
    v.extend([
        (tr("Configuración de audio…").to_string(), tr("Herramientas"), Accion::AudioConfig),
        (tr("Atajos de teclado y ratón…").to_string(), tr("Herramientas"), Accion::Atajos),
        (tr("Círculo de quintas y acordes").to_string(), tr("Piano roll"), Accion::Circulo),
        (tr("Ver instrumento").to_string(), tr("Piano roll"), Accion::Instrumento),
        (tr("Piano roll").to_string(), tr("Ver"), Accion::PianoRoll),
        (tr("Abrir carpeta de registros").to_string(), tr("Herramientas"), Accion::Registros),
        (tr("Mastering y loudness…").to_string(), tr("Ver"), Accion::Mastering),
        (tr("Enviar informe de error…").to_string(), tr("Herramientas"), Accion::Informe),
    ]);
    v.extend(crate::welcome::ESPACIOS.iter().enumerate().map(|(k, e)| (format!("{}: {}", tr("Espacio de trabajo"), tr(e.0)), tr("Ver"), Accion::Espacio(k))));
    v.push((tr("Pantalla inicial (panel)").to_string(), tr("Archivo"), Accion::Panel));
    v.extend(TEMAS.iter().enumerate().map(|(k, (n, _))| (format!("{}: {}", tr("Tema"), tr(n)), tr("Ver"), Accion::Tema(k))));
    v
}

impl App {
    pub fn ventana_paleta(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::K)) {
            self.paleta = if self.paleta.is_some() { None } else { Some(Paleta::default()) };
        }
        let Some(p) = &mut self.paleta else { return };
        let buscado = normal(&p.texto);
        let lista: Vec<_> = acciones().into_iter().filter(|(n, g, _)| buscado.split_whitespace().all(|w| normal(n).contains(w) || normal(g).contains(w))).collect();
        let (arriba, abajo, enter, esc) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::ArrowUp), i.consume_key(egui::Modifiers::NONE, Key::ArrowDown), i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if abajo {
            p.fila = (p.fila + 1).min(lista.len().saturating_sub(1));
        }
        if arriba {
            p.fila = p.fila.saturating_sub(1);
        }
        p.fila = p.fila.min(lista.len().saturating_sub(1));
        let mut elegida = enter.then(|| lista.get(p.fila).map(|a| a.2)).flatten();
        egui::Window::new("paleta").title_bar(false).resizable(false).collapsible(false).anchor(egui::Align2::CENTER_TOP, [0.0, 90.0]).fixed_size([540.0, 0.0]).show(ctx, |ui| {
            let campo = ui.add(egui::TextEdit::singleline(&mut p.texto).hint_text(tr("Buscar una acción… (Esc para cerrar)")).desired_width(f32::INFINITY).font(egui::TextStyle::Heading));
            campo.request_focus();
            if campo.changed() {
                p.fila = 0;
            }
            ui.separator();
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                if lista.is_empty() {
                    ui.label(RichText::new(tr("Nada coincide con la búsqueda.")).color(TEXT_DIM()));
                }
                for (k, (nombre, grupo, accion)) in lista.iter().enumerate() {
                    let activa = k == p.fila;
                    let r = ui.horizontal(|ui| {
                        ui.set_width(ui.available_width());
                        let b = ui.add(egui::Button::selectable(activa, RichText::new(nombre).size(14.0)).min_size(egui::vec2(400.0, 26.0)));
                        ui.label(RichText::new(*grupo).size(11.0).color(TEXT_DIM()));
                        b
                    });
                    if activa {
                        r.response.scroll_to_me(None);
                    }
                    if r.inner.clicked() {
                        elegida = Some(*accion);
                    }
                }
            });
        });
        if esc || elegida.is_some() {
            self.paleta = None;
        }
        match elegida {
            Some(Accion::Cmd(c)) => self.run(c),
            Some(Accion::AudioConfig) => self.dialog = Some(Dialog::Settings(self.audio.clone(), self.rec_bits, engine::audio_devices(), engine::midi_ports(), self.config.midi_port.clone())),
            Some(Accion::Atajos) => self.dialog = Some(Dialog::Keys(None)),
            Some(Accion::Circulo) => self.roll.circulo = true,
            Some(Accion::Instrumento) => self.roll.instrumento = true,
            Some(Accion::PianoRoll) => (self.show_mixer, self.bottom_tab) = (true, 1),
            Some(Accion::Espacio(k)) => self.aplicar_espacio(k),
            Some(Accion::Panel) => self.open_main_menu(0),
            Some(Accion::Mastering) => self.mastering.abierta = true,
            Some(Accion::Registros) => informes::abrir_carpeta(&informes::carpeta()),
            Some(Accion::Informe) => {
                informes::guardar("error", tr("Informe enviado por el usuario desde el menú"), "");
                self.informe = informes::pendiente();
            }
            Some(Accion::Tema(k)) => {
                self.config.tema = k;
                TEMA.store(k, Relaxed);
                theme(ctx);
                self.config.save();
            }
            None => {}
        }
    }
}
