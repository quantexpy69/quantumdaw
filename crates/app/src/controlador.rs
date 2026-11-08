//! Controlador MIDI (teclados como el Roland A-49): las perillas, botones y pedales que mandan
//! controles (CC) se asignan con MIDI Learn — clic derecho en un fader o una perilla → «Aprender MIDI»
//! y se mueve la perilla del teclado. Las asignaciones a pistas se guardan en el proyecto; las de la
//! pista elegida, el master y el transporte, en la configuración (valen para todos los proyectos).
use crate::{App, i18n::tr, widgets::rr};
use eframe::egui::{self, Response, Stroke};
use engine::AtomicF32;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Nombre, mínimo, máximo y valor por defecto de un control de plugin.
type Def = (&'static str, f32, f32, f32);

/// Dónde va un control. Las pistas se nombran por su id.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Destino {
    Volumen(u64),
    Panorama(u64),
    /// Un control de un efecto, del instrumento LV2 o del QUANTUM Synth de la pista.
    Control(u64, Ranura, usize),
    VolumenElegida,
    PanoramaElegida,
    VolumenMaster,
    Reproducir,
    Grabar,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ranura {
    Efecto(usize),
    Plugin,
    Synth,
}

/// Lo que se pide asignar desde un menú: un destino conocido o una perilla de plugin (por la
/// dirección de su valor; la app averigua de qué pista y efecto es).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pedido {
    Destino(Destino),
    Perilla(usize),
}

/// Asignaciones de la pista elegida, el master y el transporte que se ofrecen en Preferencias.
pub const GLOBALES: [(Destino, &str); 5] = [
    (Destino::VolumenElegida, "Volumen de la pista elegida"),
    (Destino::PanoramaElegida, "Panorama de la pista elegida"),
    (Destino::VolumenMaster, "Volumen del master"),
    (Destino::Reproducir, "Reproducir / parar"),
    (Destino::Grabar, "Grabar"),
];

impl Destino {
    pub fn global(&self) -> bool {
        !matches!(self, Destino::Volumen(_) | Destino::Panorama(_) | Destino::Control(..))
    }

    fn pista(&self) -> Option<u64> {
        match *self {
            Destino::Volumen(id) | Destino::Panorama(id) | Destino::Control(id, ..) => Some(id),
            _ => None,
        }
    }

    fn con_pista(self, id: u64) -> Self {
        match self {
            Destino::Volumen(_) => Destino::Volumen(id),
            Destino::Panorama(_) => Destino::Panorama(id),
            Destino::Control(_, r, k) => Destino::Control(id, r, k),
            d => d,
        }
    }

    /// Texto para guardar; en las de pista, `pista` es su posición (los ids cambian al abrir).
    fn clave(&self, pista: usize) -> String {
        match self {
            Destino::Volumen(_) => format!("vol:{pista}"),
            Destino::Panorama(_) => format!("pan:{pista}"),
            Destino::Control(_, Ranura::Efecto(f), k) => format!("fx:{pista}:{f}:{k}"),
            Destino::Control(_, Ranura::Plugin, k) => format!("plugin:{pista}:{k}"),
            Destino::Control(_, Ranura::Synth, k) => format!("synth:{pista}:{k}"),
            Destino::VolumenElegida => "sel:vol".into(),
            Destino::PanoramaElegida => "sel:pan".into(),
            Destino::VolumenMaster => "master:vol".into(),
            Destino::Reproducir => "play".into(),
            Destino::Grabar => "rec".into(),
        }
    }

    /// Lee una clave guardada; en las de pista devuelve también la posición de la pista.
    fn desde_clave(s: &str) -> Option<(Self, usize)> {
        let partes: Vec<&str> = s.split(':').collect();
        let n = |k: usize| partes.get(k).and_then(|x| x.parse::<usize>().ok());
        Some(match partes[..] {
            ["vol", _] => (Destino::Volumen(0), n(1)?),
            ["pan", _] => (Destino::Panorama(0), n(1)?),
            ["fx", _, _, _] => (Destino::Control(0, Ranura::Efecto(n(2)?), n(3)?), n(1)?),
            ["plugin", _, _] => (Destino::Control(0, Ranura::Plugin, n(2)?), n(1)?),
            ["synth", _, _] => (Destino::Control(0, Ranura::Synth, n(2)?), n(1)?),
            _ => (GLOBALES.iter().map(|g| g.0).find(|d| d.clave(0) == s)?, 0),
        })
    }
}

struct Asignacion {
    cc: u8,
    destino: Destino,
    /// Dirección del valor de la perilla (para reconocerla en su menú); 0 si no es una perilla.
    perilla: usize,
}

#[derive(Default)]
struct Estado {
    asignaciones: Vec<Asignacion>,
    aprender: Option<Pedido>,
    /// Pistas movidas desde el teclado y cuándo: mientras tanto cuentan como «tocadas» (automatización).
    tocadas: HashMap<u64, Instant>,
    /// Cambios hechos desde un menú que la app tiene que guardar (globales, del proyecto).
    guardar: (bool, bool),
}

static ESTADO: Mutex<Option<Estado>> = Mutex::new(None);

fn con<R>(f: impl FnOnce(&mut Estado) -> R) -> R {
    let mut g = ESTADO.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Estado::default))
}

/// Los controles asignados dejan de llegar a los instrumentos.
fn avisar_motor(e: &Estado) {
    engine::tomar_controles(e.asignaciones.iter().map(|a| a.cc));
}

pub fn aprendiendo() -> Option<Pedido> {
    con(|e| e.aprender)
}

pub fn aprender(p: Pedido) {
    con(|e| e.aprender = Some(p));
}

pub fn cancelar() {
    con(|e| e.aprender = None);
}

/// Control asignado a lo que pide el menú.
pub fn asignado(p: Pedido) -> Option<u8> {
    con(|e| e.asignaciones.iter().find(|a| es_de(a, p)).map(|a| a.cc))
}

/// Anota que hay que guardar las asignaciones que cumplen `cual`.
fn por_guardar(e: &mut Estado, cual: impl Fn(&Asignacion) -> bool) {
    for a in e.asignaciones.iter().filter(|a| cual(a)) {
        if a.destino.global() { e.guardar.0 = true } else { e.guardar.1 = true }
    }
}

pub fn quitar(p: Pedido) {
    con(|e| {
        por_guardar(e, |a| es_de(a, p));
        e.asignaciones.retain(|a| !es_de(a, p));
        avisar_motor(e);
    });
}

fn es_de(a: &Asignacion, p: Pedido) -> bool {
    match p {
        Pedido::Destino(d) => a.destino == d,
        Pedido::Perilla(ptr) => a.perilla == ptr,
    }
}

/// Todas las asignaciones (control, destino).
pub fn lista() -> Vec<(u8, Destino)> {
    con(|e| e.asignaciones.iter().map(|a| (a.cc, a.destino)).collect())
}

pub fn quitar_cc(cc: u8, destino: Destino) {
    con(|e| {
        e.asignaciones.retain(|a| !(a.cc == cc && a.destino == destino));
        avisar_motor(e);
    });
}

/// Clic derecho en un fader o una perilla: aprender o quitar su control MIDI. Mientras espera el
/// control, el elemento queda marcado.
pub fn menu(r: &Response, p: Pedido) {
    if aprendiendo() == Some(p) {
        let pintor = r.ctx.layer_painter(r.layer_id);
        let fase = (r.ctx.input(|i| i.time) * 4.0).sin() as f32 * 0.5 + 0.5;
        pintor.rect_stroke(r.rect.expand(3.0), rr(6.0), Stroke::new(2.0, crate::ACCENT().gamma_multiply(0.4 + 0.6 * fase)), egui::StrokeKind::Outside);
        r.ctx.request_repaint();
    }
    r.context_menu(|ui| {
        // Qué se va a controlar: la perilla redonda del canal es el panorama (izquierda/derecha), no el volumen.
        let que = match p {
            Pedido::Destino(Destino::Volumen(_) | Destino::VolumenMaster) => Some("Volumen"),
            Pedido::Destino(Destino::Panorama(_)) => Some("Panorama (izquierda / derecha)"),
            _ => None,
        };
        if let Some(que) = que {
            ui.label(egui::RichText::new(tr(que)).strong());
        }
        let actual = asignado(p);
        if let Some(cc) = actual {
            ui.label(egui::RichText::new(format!("{} CC {cc}", tr("Control MIDI:"))).color(crate::TEXT_DIM()));
        }
        if aprendiendo() == Some(p) {
            if ui.button(tr("Cancelar aprendizaje MIDI")).clicked() {
                cancelar();
                ui.close();
            }
        } else if ui.button(tr("Aprender MIDI…")).on_hover_text(tr("Después mueve una perilla, botón o pedal del teclado")).clicked() {
            aprender(p);
            ui.close();
        }
        if actual.is_some() && ui.button(tr("Quitar control MIDI")).clicked() {
            quitar(p);
            ui.close();
        }
    });
}

/// Pide el MIDI Learn de una perilla de plugin (se la reconoce por la dirección de su valor).
pub fn menu_perilla(r: &Response, a: &AtomicF32) {
    menu(r, Pedido::Perilla(a as *const AtomicF32 as usize));
}

/// Posición 0..1 del valor de un control MIDI (0..127); 64 queda justo en el centro.
fn fraccion(v: u8) -> f32 {
    if v <= 64 { v as f32 / 128.0 } else { 0.5 + (v - 64) as f32 / 126.0 }
}

impl App {
    /// Ids de pista y control de una perilla de plugin, buscándola por la dirección de su valor.
    fn ubicar_perilla(&self, ptr: usize) -> Option<Destino> {
        let es = |p: &[AtomicF32]| p.iter().position(|a| a as *const AtomicF32 as usize == ptr);
        self.s.tracks.iter().find_map(|t| {
            let en_fx = t.fx.iter().enumerate().find_map(|(f, fx)| Some(Destino::Control(t.id, Ranura::Efecto(f), es(&fx.params)?)));
            en_fx
                .or_else(|| Some(Destino::Control(t.id, Ranura::Plugin, es(&t.plugin.as_ref()?.params)?)))
                .or_else(|| Some(Destino::Control(t.id, Ranura::Synth, es(&t.synth.as_ref()?.params)?)))
        })
    }

    /// Valor, rango y nombre de un control de plugin asignado.
    fn control(&self, id: u64, ranura: Ranura, k: usize) -> Option<(&AtomicF32, Def)> {
        let t = self.s.tracks.iter().find(|t| t.id == id)?;
        match ranura {
            Ranura::Efecto(f) => t.fx.get(f).and_then(|fx| Some((fx.params.get(k)?, *fx.defs().get(k)?))),
            Ranura::Plugin => t.plugin.as_ref().and_then(|fx| Some((fx.params.get(k)?, *fx.defs().get(k)?))),
            Ranura::Synth => t.synth.as_ref().and_then(|s| Some((s.params.get(k)?, engine::param_def(k)?))),
        }
    }

    fn nombre_pista(&self, id: u64) -> String {
        self.s.tracks.iter().find(|t| t.id == id).map_or_else(|| tr("pista borrada").to_string(), |t| t.name.clone())
    }

    /// Descripción de un destino para la lista de Preferencias y los avisos.
    pub fn describir_control(&self, d: Destino) -> String {
        match d {
            Destino::Volumen(id) => format!("{} · {}", self.nombre_pista(id), tr("Volumen")),
            Destino::Panorama(id) => format!("{} · {}", self.nombre_pista(id), tr("Panorama")),
            Destino::Control(id, r, k) => {
                let que = self.control(id, r, k).map_or_else(|| tr("control").to_string(), |(_, def)| tr(def.0).to_string());
                format!("{} · {que}", self.nombre_pista(id))
            }
            d => GLOBALES.iter().find(|g| g.0 == d).map_or_else(String::new, |g| tr(g.1).to_string()),
        }
    }

    fn elegida(&self) -> Option<usize> {
        self.s.tracks.iter().position(|t| t.selected).or(self.last_track.filter(|&i| i < self.s.tracks.len()))
    }

    /// Lee los controles que llegaron del teclado: completa el MIDI Learn pendiente y mueve lo asignado.
    pub fn atender_controles(&mut self) {
        let llegados = engine::controles();
        for ev in llegados {
            let (cc, v) = (ev[1], ev[2]);
            if let Some(p) = aprendiendo() {
                let destino = match p {
                    Pedido::Destino(d) => Some(d),
                    Pedido::Perilla(ptr) => self.ubicar_perilla(ptr),
                };
                cancelar();
                let Some(destino) = destino else {
                    self.status = tr("Esa perilla no se puede asignar a un control MIDI").into();
                    continue;
                };
                let perilla = if let Pedido::Perilla(ptr) = p { ptr } else { 0 };
                con(|e| {
                    e.asignaciones.retain(|a| a.destino != destino);
                    e.asignaciones.push(Asignacion { cc, destino, perilla });
                    avisar_motor(e);
                });
                self.status = format!("CC {cc} → {}", self.describir_control(destino));
                self.guardar_controles(destino.global());
                continue;
            }
            for d in con(|e| e.asignaciones.iter().filter(|a| a.cc == cc).map(|a| a.destino).collect::<Vec<_>>()) {
                self.mover_control(d, v);
            }
        }
        let (globales, del_proyecto) = con(|e| std::mem::take(&mut e.guardar));
        if globales {
            self.guardar_controles(true);
        }
        if del_proyecto {
            self.guardar_controles(false);
        }
        // Las pistas movidas desde el teclado siguen «tocadas» un momento: así Toque y Latch escriben
        // la automatización entre un mensaje y el siguiente.
        let ahora = Instant::now();
        let tocadas = con(|e| {
            e.tocadas.retain(|_, t| ahora.duration_since(*t) < Duration::from_millis(600));
            e.tocadas.keys().copied().collect::<Vec<_>>()
        });
        for t in self.s.tracks.iter_mut().filter(|t| tocadas.contains(&t.id)) {
            t.touching = true;
        }
    }

    fn mover_control(&mut self, d: Destino, v: u8) {
        let x = fraccion(v);
        let id = match d {
            Destino::VolumenElegida | Destino::PanoramaElegida => self.elegida().map(|i| self.s.tracks[i].id),
            d => d.pista(),
        };
        let pista = id.and_then(|id| self.s.tracks.iter().position(|t| t.id == id));
        match d {
            Destino::Volumen(_) | Destino::VolumenElegida => {
                if let Some(i) = pista {
                    self.s.tracks[i].params.gain.set(engine::fader_gain(x));
                }
            }
            Destino::Panorama(_) | Destino::PanoramaElegida => {
                if let Some(i) = pista {
                    self.s.tracks[i].params.pan.set(x * 2.0 - 1.0);
                }
            }
            Destino::Control(id, r, k) => {
                if let Some((a, def)) = self.control(id, r, k) {
                    a.set(crate::plugins::valor_de_perilla(def, x));
                }
            }
            Destino::VolumenMaster => self.engine.master.gain.set(engine::fader_gain(x)),
            // Botones: actúan al apretar (valor alto), no al soltar.
            Destino::Reproducir if v >= 64 => self.toggle_play(),
            Destino::Grabar if v >= 64 => self.toggle_record(),
            _ => {}
        }
        if let Some(id) = id.filter(|_| matches!(d, Destino::Volumen(_) | Destino::Panorama(_) | Destino::VolumenElegida | Destino::PanoramaElegida)) {
            con(|e| _ = e.tocadas.insert(id, Instant::now()));
        }
    }

    /// Guarda las asignaciones globales en la configuración, o marca el proyecto como cambiado.
    pub fn guardar_controles(&mut self, global: bool) {
        if global {
            self.config.controles_midi = con(|e| e.asignaciones.iter().filter(|a| a.destino.global()).map(|a| (a.cc, a.destino.clave(0))).collect());
            self.config.save();
        } else {
            self.cambios = true;
        }
    }

    /// Asignaciones de pista para guardar en el proyecto (con la posición de la pista).
    pub fn controles_del_proyecto(&self) -> Vec<(u8, String)> {
        con(|e| {
            e.asignaciones
                .iter()
                .filter_map(|a| {
                    let i = self.s.tracks.iter().position(|t| Some(t.id) == a.destino.pista())?;
                    Some((a.cc, a.destino.clave(i)))
                })
                .collect()
        })
    }

    /// Carga las asignaciones globales (configuración) y las del proyecto abierto.
    pub fn cargar_controles(&mut self, del_proyecto: &[(u8, String)]) {
        let mut nuevas = vec![];
        for (cc, clave) in self.config.controles_midi.iter().chain(del_proyecto) {
            let Some((d, i)) = Destino::desde_clave(clave) else { continue };
            let destino = if d.global() {
                d
            } else {
                let Some(t) = self.s.tracks.get(i) else { continue };
                d.con_pista(t.id)
            };
            let perilla = match destino {
                Destino::Control(id, r, k) => self.control(id, r, k).map_or(0, |(a, _)| a as *const AtomicF32 as usize),
                _ => 0,
            };
            nuevas.push(Asignacion { cc: *cc, destino, perilla });
        }
        con(|e| {
            e.asignaciones = nuevas;
            e.aprender = None;
            avisar_motor(e);
        });
    }

    /// Tarjeta de Preferencias → MIDI: asignaciones rápidas y lista de todas.
    pub fn tarjeta_controlador(&mut self, ui: &mut egui::Ui) {
        use crate::preferencias::{boton, fila, nota, parrafo, tarjeta};
        tarjeta(ui, "CONTROLADOR MIDI", |ui| {
            parrafo(ui, "Las perillas, botones y pedales del teclado (por ejemplo C1 y C2) pueden mover lo que quieras. Elige qué controlar con «Aprender» y mueve la perilla; o haz clic derecho en cualquier fader o perilla del programa → «Aprender MIDI…».");
            ui.add_space(2.0);
            for (d, nombre) in GLOBALES {
                let p = Pedido::Destino(d);
                fila(ui, nombre, |ui| {
                    let esperando = aprendiendo() == Some(p);
                    let texto = match (esperando, asignado(p)) {
                        (true, _) => tr("Mueve una perilla…").to_string(),
                        (_, Some(cc)) => format!("CC {cc}"),
                        _ => tr("Sin asignar").to_string(),
                    };
                    ui.label(egui::RichText::new(texto).monospace().color(if esperando { crate::ACCENT() } else { crate::TEXT() }));
                    if asignado(p).is_some() && boton(ui, "Quitar").clicked() {
                        quitar(p);
                    }
                    if esperando {
                        if boton(ui, "Cancelar").clicked() {
                            cancelar();
                        }
                    } else if boton(ui, "Aprender").clicked() {
                        aprender(p);
                    }
                });
            }
            let de_pistas: Vec<(u8, Destino)> = lista().into_iter().filter(|(_, d)| !d.global()).collect();
            if !de_pistas.is_empty() {
                ui.add_space(6.0);
                nota(ui, "Asignados en este proyecto:");
                for (cc, d) in de_pistas {
                    let texto = format!("CC {cc} → {}", self.describir_control(d));
                    fila(ui, &texto, |ui| {
                        if boton(ui, "Quitar").clicked() {
                            quitar_cc(cc, d);
                            self.guardar_controles(false);
                        }
                    });
                }
            }
            ui.add_space(4.0);
            nota(ui, "La rueda de tono, la modulación y el pedal de sustain llegan solos a los instrumentos. Con la automatización en Toque o Latch, mover una perilla asignada al volumen o al panorama mientras suena graba la curva.");
        });
        if aprendiendo().is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn claves_de_ida_y_vuelta() {
        for d in [Destino::Volumen(9), Destino::Panorama(9), Destino::Control(9, Ranura::Efecto(2), 5), Destino::Control(9, Ranura::Plugin, 1), Destino::Control(9, Ranura::Synth, 3)] {
            let (leido, i) = Destino::desde_clave(&d.clave(4)).unwrap();
            assert_eq!((leido.con_pista(9), i), (d, 4));
        }
        for (d, _) in GLOBALES {
            assert_eq!(Destino::desde_clave(&d.clave(0)).map(|x| x.0), Some(d));
        }
        assert_eq!(fraccion(0), 0.0);
        assert_eq!(fraccion(64), 0.5);
        assert_eq!(fraccion(127), 1.0);
    }
}
