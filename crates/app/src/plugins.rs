//! Ventana de plugins QUANTUM: instrumento y cadena de efectos, con visor, perillas y firma.
use crate::{
    widgets::{self, rr},
    *,
};
use egui::{Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use engine::{AtomicF32, SYNTH_PARAMS, eq_response};

const SIGNATURE: &str = "QUANTUM";
const DISCRETE: [&str; 8] = ["Tonalidad", "Nota manual", "Escala", "Modo", "Onda", "Ping-pong", "Sincronía", "División"];
/// Ancho de cada dispositivo en la pestaña Dispositivos; Curves (visor y controles en fila) es ancho.
const ANCHO_DISPOSITIVO: f32 = 380.0;
const ANCHO_CURVES: f32 = 820.0;

/// En fila, cada dispositivo ocupa una columna de ancho fijo con su propio desplazamiento vertical.
fn columna(ui: &mut egui::Ui, en_fila: bool, ancho: f32, add: impl FnOnce(&mut egui::Ui)) {
    if !en_fila {
        return add(ui);
    }
    ui.allocate_ui_with_layout(vec2(ancho, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(ancho);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, add);
    });
}

/// Color de acento de cada plugin (None = instrumento).
pub fn kind_color(kind: Option<FxKind>) -> Color32 {
    match kind {
        Some(FxKind::Eq) => Color32::from_rgb(0x2E, 0xD8, 0xC3),
        Some(FxKind::Compressor) => Color32::from_rgb(0xFF, 0xB0, 0x20),
        Some(FxKind::Delay) => Color32::from_rgb(0x5B, 0x9C, 0xFF),
        Some(FxKind::Reverb) => Color32::from_rgb(0xB0, 0x8C, 0xF2),
        Some(FxKind::Drive) => Color32::from_rgb(0xFF, 0x7A, 0x45),
        Some(FxKind::Tune) => Color32::from_rgb(0x3D, 0xDC, 0x84),
        Some(FxKind::Curves) => Color32::from_rgb(0xFF, 0x6B, 0x9A),
        Some(FxKind::Limiter) => Color32::from_rgb(0xFF, 0x4D, 0x4F),
        Some(FxKind::Gate) => Color32::from_rgb(0x9C, 0xD6, 0x4A),
        Some(FxKind::Chorus) => Color32::from_rgb(0x4F, 0xC3, 0xF7),
        Some(FxKind::DeEsser) => Color32::from_rgb(0xF2, 0xC9, 0x4C),
        Some(FxKind::TapePro) => Color32::from_rgb(0xFF, 0xA8, 0x4A),
        Some(FxKind::SpacePro) => Color32::from_rgb(0x9D, 0x7C, 0xFF),
        Some(FxKind::VocalPro) => Color32::from_rgb(0xFF, 0x5C, 0xA8),
        Some(FxKind::MasterPro) => Color32::from_rgb(0x2E, 0xE6, 0xC8),
        None => Color32::from_rgb(0xF2, 0x8C, 0xD0),
    }
}

/// Qué hace cada efecto (vista previa en la biblioteca).
pub fn summary(k: FxKind) -> &'static str {
    match k {
        FxKind::Eq => "Ecualizador de 3 bandas con frecuencias ajustables: realza o atenúa graves, medios y agudos.",
        FxKind::Compressor => "Controla la dinámica bajando los picos sobre el umbral. Ataque, release, rodilla suave y mezcla paralela.",
        FxKind::Delay => "Repeticiones con feedback, ping-pong estéreo, filtros en las repeticiones y sincronía al tempo.",
        FxKind::Reverb => "Simula un espacio: tamaño, amortiguación, pre-delay, corte de graves y anchura estéreo.",
        FxKind::Drive => "Saturación analógica suave, dura o de válvula, con control de tono y salida.",
        FxKind::Tune => "Afinación de voz automática a la tonalidad y escala, o manual a una nota fija.",
        FxKind::Curves => "EQ de 7 bandas con analizador en vivo, detección automática y supresor de resonancias molestas, con presets por instrumento.",
        FxKind::Limiter => "Limitador con anticipación: sube el volumen sin pasar del techo. Ideal al final de la cadena del master.",
        FxKind::Gate => "Puerta de ruido: silencia el ruido de fondo entre frases o golpes, con retención y rango.",
        FxKind::Chorus => "Duplica la señal con pequeñas variaciones de tiempo para un sonido más ancho y envolvente.",
        FxKind::DeEsser => "Baja las eses y sibilancias de las voces: detecta los agudos fuertes y reduce el volumen solo en ese instante.",
        FxKind::TapePro => "Cinta y válvula de estudio: saturación, golpe de cabeza, wow, flutter y siseo, con VU analógicos y tres velocidades de cinta.",
        FxKind::SpacePro => "Reverb de 8 líneas (sala, placa, catedral y shimmer) con eco sincronizado, ducking y congelar, en una sala 3D animada.",
        FxKind::VocalPro => "Cadena de voz completa: limpieza, de-esser, nivelador, dos compresores, tono, afinación y espacio, con un macro de carácter y presets.",
        FxKind::MasterPro => "Mastering en 3 bandas: compresión e imagen por banda, saturación, limitador y ganancia automática hacia Spotify, YouTube o Apple Music, con A/B al mismo volumen.",
    }
}

/// Vista previa de un efecto (visor con sus valores por defecto), para mostrar al pasar el ratón.
pub fn preview(ui: &mut egui::Ui, k: FxKind, sr: f32) {
    ui.set_width(300.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("QUANTUM")).size(10.0).strong().color(kind_color(Some(k))));
        ui.label(RichText::new(tr(k.name()).trim_start_matches("QUANTUM ")).size(15.0).strong());
    });
    let defaults: Vec<f32> = k.params().iter().map(|p| p.3).collect();
    display(ui, Some(k), &defaults, [0.0; 2], kind_color(Some(k)), sr);
    ui.label(RichText::new(tr(summary(k))).size(12.5));
    ui.label(RichText::new(format!("{} {}", k.params().len(), tr("parámetros"))).size(11.0).color(TEXT_DIM()));
}

/// Texto del valor de un parámetro con su unidad (o su nombre, en los discretos).
pub(crate) fn value_text(kind: Option<FxKind>, name: &str, v: f32) -> String {
    let pick = |names: &[&str]| tr(names[(v.round().max(0.0) as usize).min(names.len() - 1)]).to_string();
    let hz = |v: f32| {
        if v >= 1000.0 { format!("{:.1} kHz", v / 1000.0) } else { format!("{v:.0} Hz") }
    };
    match name {
        "Tonalidad" | "Nota manual" => pick(&NOTE_NAMES),
        "Escala" => pick(&["Cromática", "Mayor", "Menor"]),
        "Modo" if kind == Some(FxKind::Drive) => pick(&["Suave", "Duro", "Válvula"]),
        "Modo" => pick(&["Automático", "Manual"]),
        "Onda" => pick(&["Sierra", "Cuadrada", "Triángulo", "Seno"]),
        "Ping-pong" | "Sincronía" => pick(&["No", "Sí"]),
        "División" => pick(&["1/1", "1/2", "1/4", "1/8", "1/16", "1/8 ·"]),
        "Volumen" => format!("{v:+.1} dB"),
        "Frecuencia" | "Corte" | "Tono" | "Frec. graves" | "Frec. medios" | "Frec. agudos" | "Corte graves" | "Corte agudos" | "Frec. eses" | "Cruce graves" | "Cruce agudos" => hz(v),
        "Q medios" => format!("Q {v:.2}"),
        "Desafinar" => format!("{v:.0} ct"),
        "Ratio" => format!("{v:.1}:1"),
        "Ataque" | "Caída" | "Liberación" => format!("{v:.0} ms"),
        n if n.ends_with(" Hz") => hz(v),
        n if n.ends_with(" Q") => format!("Q {v:.2}"),
        n if n.ends_with("dB") => format!("{v:+.1} dB"),
        n if n.ends_with("ms") => format!("{v:.1} ms"),
        n if n.ends_with(" s") => format!("{v:.1} s"),
        _ => format!("{:.0} %", v * 100.0),
    }
}

/// Perillas en una cuadrícula que reparte el ancho: tantas columnas como entren, separadas por igual
/// y alineadas fila a fila (la última fila queda bajo las primeras columnas).
fn rejilla_perillas(ui: &mut egui::Ui, kind: Option<FxKind>, defs: &[(&str, f32, f32, f32)], params: &[AtomicF32]) {
    const CELDA: f32 = 58.0 + 26.0;
    let ancho = ui.available_width();
    let columnas = (((ancho + 4.0) / (CELDA + 4.0)).floor() as usize).clamp(1, defs.len().max(1));
    let hueco = if columnas > 1 { ((ancho - CELDA * columnas as f32) / (columnas - 1) as f32).max(0.0) } else { 0.0 };
    for (f, fila) in defs.chunks(columnas).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = hueco;
            for (k, def) in fila.iter().enumerate() {
                param_knob(ui, kind, *def, &params[f * columnas + k]);
            }
        });
    }
}

/// Perilla de consola: escala logarítmica en rangos amplios (frecuencias, tiempos), color del plugin.
pub fn param_knob(ui: &mut egui::Ui, kind: Option<FxKind>, def: (&str, f32, f32, f32), a: &AtomicF32) {
    perilla(ui, kind, def, a, 58.0);
}

/// Perilla de un parámetro con su valor y su nombre, del tamaño indicado.
pub fn perilla(ui: &mut egui::Ui, kind: Option<FxKind>, (name, min, max, def): (&str, f32, f32, f32), a: &AtomicF32, size: f32) {
    let log = min > 0.0 && max / min > 10.0;
    let to_t = |v: f32| {
        if log { (v / min).ln() / (max / min).ln() } else { (v - min) / (max - min) }
    };
    let from_t = |t: f32| {
        if log { min * (max / min).powf(t) } else { min + t * (max - min) }
    };
    let color = kind_color(kind);
    ui.allocate_ui(vec2(size + 26.0, size + 44.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut t = to_t(a.get());
            let r = widgets::console_knob_size(ui, &mut t, to_t(def), color, size);
            if r.dragged() || r.double_clicked() {
                let v = from_t(t);
                a.set(if DISCRETE.contains(&name) { v.round() } else { v });
            }
            ui.label(RichText::new(value_text(kind, name, a.get())).monospace().size(11.0).strong().color(color));
            ui.label(RichText::new(tr(name.trim_end_matches(" dB").trim_end_matches(" ms"))).size(10.5).color(TEXT()));
        });
    });
}

/// Visor de cada plugin: curvas de respuesta, medidores o la nota detectada.
fn display(ui: &mut egui::Ui, kind: Option<FxKind>, p: &[f32], readout: [f32; 2], color: Color32, sr: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, rr(8.0), BG());
    let r = rect.shrink(8.0);
    let line = |pts: Vec<Pos2>| painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
    let curve = |f: &dyn Fn(f32) -> f32| line((0..=120).map(|k| k as f32 / 120.0).map(|t| pos2(r.left() + t * r.width(), r.bottom() - f(t).clamp(0.0, 1.0) * r.height())).collect());
    // Textos del visor sobre una placa del color del fondo: se leen aunque pase una curva por debajo.
    let label = |pos: Pos2, align: Align2, text: String, size: f32, c: Color32| {
        let g = painter.layout_no_wrap(text, FontId::proportional(size), c);
        let caja = align.anchor_size(pos, g.size()).expand2(vec2(4.0, 1.0));
        painter.rect_filled(caja, rr(3.0), BG());
        painter.galley(caja.min + vec2(4.0, 1.0), g, c);
        caja
    };
    painter.hline(r.x_range(), r.center().y, Stroke::new(1.0, PANEL()));
    match kind {
        Some(FxKind::Eq) => {
            curve(&|t| 0.5 + eq_response(p, 20.0 * 1000f32.powf(t), sr) / 36.0);
            for (f, text) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
                let x = r.left() + (f / 20.0f32).log10() / 3.0 * r.width();
                painter.vline(x, r.y_range(), Stroke::new(1.0, PANEL()));
                label(pos2(x + 3.0, r.bottom()), Align2::LEFT_BOTTOM, text.into(), 9.0, TEXT_DIM());
            }
        }
        // Vista previa de Curves (en la biblioteca): curva del EQ; el visor completo está en la ventana del plugin.
        Some(FxKind::Curves) => {
            curve(&|t| 0.5 + engine::curves_response(p, 20.0 * 1000f32.powf(t), sr) / 48.0);
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, tr("espectro, nodos y resonancias en el plugin").into(), 10.0, TEXT_DIM());
        }
        Some(FxKind::Compressor) => {
            curve(&|t| {
                let x = -60.0 + t * 60.0;
                let y = if x > p[0] { p[0] + (x - p[0]) / p[1] } else { x } + p[2];
                (y + 60.0) / 60.0
            });
            let gr = Rect::from_min_max(pos2(r.right() - 10.0, r.top()), r.max);
            painter.rect_filled(Rect::from_min_max(gr.min, pos2(gr.right(), gr.top() + (readout[0] / 24.0).min(1.0) * gr.height())), rr(2.0), METER[1]);
            label(pos2(r.right() - 14.0, r.top()), Align2::RIGHT_TOP, format!("GR -{:.1} dB", readout[0]), 11.0, METER[1]);
        }
        Some(FxKind::Delay) => {
            for k in 0..10 {
                let x = r.left() + (k as f32 * p[0] / 2000.0) * r.width();
                let h = if k == 0 { 1.0 } else { p[2] * p[1].powi(k - 1) };
                if x < r.right() {
                    painter.vline(x, r.bottom() - h * r.height()..=r.bottom(), Stroke::new(3.0, color));
                }
            }
        }
        // Dinámica con lectura de reducción de ganancia.
        Some(FxKind::Limiter | FxKind::Gate | FxKind::DeEsser) => {
            let techo = if kind == Some(FxKind::Limiter) { 1.0 - p[0] / -12.0 } else { 0.5 };
            painter.hline(r.x_range(), r.bottom() - techo * r.height(), Stroke::new(1.5, color));
            let gr = Rect::from_min_max(pos2(r.right() - 10.0, r.top()), r.max);
            painter.rect_filled(Rect::from_min_max(gr.min, pos2(gr.right(), gr.top() + (readout[0] / 24.0).min(1.0) * gr.height())), rr(2.0), METER[1]);
            label(pos2(r.right() - 14.0, r.top()), Align2::RIGHT_TOP, format!("GR -{:.1} dB", readout[0]), 11.0, METER[1]);
        }
        Some(FxKind::Chorus) => {
            for ch in 0..2 {
                curve(&|t| 0.5 + 0.35 * p[1] / 15.0 * (std::f32::consts::TAU * (t * 2.0 + ch as f32 * 0.25 * p[3])).sin());
            }
        }
        Some(FxKind::Reverb) => _ = curve(&|t| p[2].max(0.15) * (-t * 9.0 * (1.05 - p[0])).exp() * (1.0 - 0.5 * p[1] * t)),
        Some(FxKind::Drive) => {
            let g = 10f32.powf(p[0] / 20.0);
            curve(&|t| {
                let x = t * 2.0 - 1.0;
                (x + ((x * g).tanh() / g.sqrt() - x) * p[1] + 1.0) / 2.0
            });
        }
        Some(FxKind::Tune) => {
            let name = |n: f32| format!("{}{}", NOTE_NAMES[(n.round() as i32).rem_euclid(12) as usize], (n.round() as i32) / 12 - 1);
            if readout[0] > 0.0 {
                let cents = ((readout[0] - readout[1]) * 100.0).round();
                label(pos2(r.center().x - 60.0, r.center().y), Align2::CENTER_CENTER, name(readout[0]), 30.0, TEXT_DIM());
                label(r.center(), Align2::CENTER_CENTER, "→".into(), 22.0, TEXT_DIM());
                label(pos2(r.center().x + 60.0, r.center().y), Align2::CENTER_CENTER, name(readout[1]), 30.0, color);
                label(pos2(r.center().x, r.bottom()), Align2::CENTER_BOTTOM, format!("{cents:+} cents corregidos"), 11.0, TEXT_DIM());
            } else {
                label(r.center(), Align2::CENTER_CENTER, "Canta o toca una nota para ver la corrección".into(), 13.0, TEXT_DIM());
            }
        }
        Some(k @ (FxKind::TapePro | FxKind::SpacePro | FxKind::VocalPro | FxKind::MasterPro)) => {
            label(r.center() - vec2(0.0, 10.0), Align2::CENTER_CENTER, tr(k.name()).trim_start_matches("QUANTUM ").to_uppercase(), 22.0, color);
            let estado = if engine::pro::licenciado(k) { tr("Con licencia").to_string() } else { format!("{} · {} US$", tr("Modo demo"), k.pro().map_or(0, |p| p.2)) };
            label(r.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, estado, 11.0, TEXT_DIM());
        }
        None => {
            // Envolvente ADSR del sintetizador.
            let total = p[5] + p[6] + 400.0 + p[8];
            let x = |ms: f32| r.left() + ms / total * r.width();
            let (a, d, s) = (x(p[5]), x(p[5] + p[6]), x(p[5] + p[6] + 400.0));
            let y = |v: f32| r.bottom() - v * r.height();
            line(vec![pos2(r.left(), y(0.0)), pos2(a, y(1.0)), pos2(d, y(p[7])), pos2(s, y(p[7])), pos2(r.right(), y(0.0))]);
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, value_text(None, "Onda", p[0]), 12.0, color);
        }
    }
}

impl App {
    pub fn fx_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.fx_window.filter(|&i| i < self.s.tracks.len()) else {
            return;
        };
        let title = format!("QUANTUM · {}", self.s.tracks[i].name);
        let open = self.ventana(ctx, "fx", title, [860.0, 900.0], true, |app, ui| app.cadena(ui, i, false));
        if !open {
            self.fx_window = None;
        }
        // Los visores (reducción de ganancia, nota detectada) se actualizan en vivo.
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    /// Instrumento y cadena de efectos de la pista `i`: en columna (ventana de plugins) o en fila,
    /// un dispositivo junto al otro (pestaña Dispositivos del panel inferior).
    pub fn cadena(&mut self, ui: &mut egui::Ui, i: usize, en_fila: bool) {
        enum Act {
            Add(FxKind),
            Detectar(usize),
            Sincronizar,
            Remove(usize),
            Up(usize),
            MyPlugins,
            Licencias,
            Drums,
        }
        let bateria = Self::drummer_disponible() && self.es_pista_bateria(i);
        let (mut act, sr) = (None, self.engine.sample_rate as f32);
        let mut cuerpo = |app: &Self, ui: &mut egui::Ui| {
            let t = &app.s.tracks[i];
            // Pistas que pueden ser clave del sidechain de Curves (todas menos esta).
            let pistas: Vec<(u64, String)> = app.s.tracks.iter().filter(|x| x.id != t.id).map(|x| (x.id, x.name.clone())).collect();
            let plugin = |ui: &mut egui::Ui,
                          name: &str,
                          kind: Option<FxKind>,
                          params: &[AtomicF32],
                          defs: &[(&str, f32, f32, f32)],
                          bypass: Option<&AtomicBool>,
                          readout: [f32; 2],
                          j: Option<usize>,
                          fx: Option<&engine::Fx>,
                          act: &mut Option<Act>| {
                let accent = kind_color(kind);
                let ancho = if kind == Some(FxKind::Curves) { ANCHO_CURVES } else { ANCHO_DISPOSITIVO };
                // Cabecera: encendido (bypass), nombre con la marca y, a la derecha, subir y quitar.
                let mut accion_cabecera = None;
                let cabecera = |ui: &mut egui::Ui| {
                    if let Some(b) = bypass {
                        let on = !b.load(Relaxed);
                        if widgets::encendido(ui, on).on_hover_text(if on { tr("Activo · clic para bypass") } else { tr("En bypass · clic para activar") }).clicked() {
                            b.store(on, Relaxed);
                        }
                    }
                    ui.label(RichText::new(tr(name).trim_start_matches("QUANTUM ")).size(13.5).strong().color(TEXT()));
                    ui.label(RichText::new(SIGNATURE).size(9.0).strong().color(accent));
                    if let Some(j) = j {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::boton_cabecera(ui, widgets::draw_quitar, true).on_hover_text(tr("Quitar")).clicked() {
                                accion_cabecera = Some(Act::Remove(j));
                            }
                            if widgets::boton_cabecera(ui, widgets::draw_subir, j > 0).on_hover_text(tr("Subir en la cadena")).clicked() {
                                accion_cabecera = Some(Act::Up(j));
                            }
                        });
                    }
                };
                columna(ui, en_fila, ancho, |ui| widgets::dispositivo(ui, accent, en_fila, cabecera, |ui| {
                    ui.set_width(ui.available_width());
                    let values: Vec<f32> = params.iter().map(|a| a.get()).collect();
                    match (kind, fx, j) {
                        // Plugins Pro: su interfaz propia (con todos sus controles).
                        (Some(k), Some(fx), Some(_)) if k.pro().is_some() => {
                            if let Some(licencias::PedidoPro::Licencias) = licencias::panel_pro(ui, fx) {
                                *act = Some(Act::Licencias);
                            }
                        }
                        // Curves: presets, detección y visor interactivo.
                        (Some(FxKind::Curves), Some(fx), Some(j)) => {
                            let (mut detectar, mut sincronizar) = (false, false);
                            curves::barra(ui, fx, &pistas, &mut detectar, &mut sincronizar);
                            if detectar {
                                *act = Some(Act::Detectar(j));
                            }
                            if sincronizar {
                                *act = Some(Act::Sincronizar);
                            }
                            curves::visor(ui, fx, sr, accent);
                        }
                        // Compresor y puerta: también pueden escuchar a otra pista (sidechain).
                        (Some(k @ (FxKind::Compressor | FxKind::Gate)), Some(fx), Some(_)) => {
                            let mut sincronizar = false;
                            let ayuda = if k == FxKind::Compressor { tr("La pista elegida baja esta pista al sonar (por ejemplo, el bombo agacha el bajo)") } else { tr("La puerta se abre cuando suena la pista elegida (por ejemplo, un golpe de batería)") };
                            curves::selector_sidechain(ui, fx, &pistas, &mut sincronizar, ayuda);
                            if sincronizar {
                                *act = Some(Act::Sincronizar);
                            }
                            display(ui, kind, &values, readout, accent, sr);
                        }
                        _ => display(ui, kind, &values, readout, accent, sr),
                    }
                    // Curves muestra solo los controles esenciales; el resto en «Avanzado».
                    let todos = match (kind, fx) {
                        (Some(FxKind::Curves), Some(fx)) => curves::controles(ui, fx, accent),
                        (Some(k), Some(_)) if k.pro().is_some() => false,
                        _ => true,
                    };
                    if todos {
                        ui.add_space(4.0);
                        rejilla_perillas(ui, kind, defs, params);
                    }
                }));
                if accion_cabecera.is_some() {
                    *act = accion_cabecera;
                }
                ui.add_space(6.0);
            };
            if let Some(s) = &t.sampler {
                // Instrumento real (sampler): volumen y créditos de la biblioteca.
                columna(ui, en_fila, ANCHO_DISPOSITIVO, |ui| {
                if !en_fila {
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM()));
                }
                let e = instruments::find(&t.instrument);
                let nombre = if instruments::es_kit(&t.instrument) { instruments::nombre(&t.instrument) } else { e.map_or(s.name.clone(), |e| e.name.to_string()) };
                let mut abrir_drums = false;
                let cabecera = |ui: &mut egui::Ui| {
                    ui.label(RichText::new(&nombre).size(13.5).strong().color(TEXT()));
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(9.0).strong().color(ACCENT()));
                    if bateria {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let b = egui::Button::new(RichText::new(tr("Abrir Q Drums")).strong().color(BG())).fill(Color32::from_rgb(0xFF, 0x6B, 0x3D));
                            abrir_drums = ui.add(b).on_hover_text(tr("Ritmos, kit y piezas de la batería")).clicked();
                        });
                    }
                };
                widgets::dispositivo(ui, ACCENT(), en_fila, cabecera, |ui| {
                    ui.set_width(ui.available_width());
                    let mut db = widgets::db(s.gain.get());
                    if ui.add(egui::Slider::new(&mut db, -30.0..=12.0).suffix(" dB").text(tr("Volumen"))).changed() {
                        s.gain.set(10f32.powf(db / 20.0));
                    }
                    if let Some(e) = e {
                        ui.label(RichText::new(format!("Muestras: {} · licencia {}", e.credit, e.license)).size(10.0).color(TEXT_DIM()));
                    }
                });
                if abrir_drums {
                    act = Some(Act::Drums);
                }
                ui.add_space(6.0);
                });
            } else if let Some(s) = &t.synth {
                if !en_fila {
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM()));
                }
                plugin(ui, "Synth", None, &s.params, &SYNTH_PARAMS, None, [0.0; 2], None, None, &mut act);
            }
            if !en_fila {
                ui.label(RichText::new(tr("EFECTOS DE AUDIO")).size(10.0).color(TEXT_DIM()));
            }
            for (j, fx) in t.fx.iter().enumerate() {
                let readout = [fx.readout[0].get(), fx.readout[1].get()];
                plugin(ui, fx.kind.name(), Some(fx.kind), &fx.params, fx.kind.params(), Some(&fx.bypass), readout, Some(j), Some(fx), &mut act);
            }
            // Al final de la cadena: añadir un efecto (en fila, como una ranura vacía más angosta).
            let mut anadir = |ui: &mut egui::Ui| {
                if t.fx.is_empty() {
                    ui.label(RichText::new(tr("Sin efectos. Añade uno abajo o arrástralo desde la Biblioteca.")).color(TEXT_DIM()));
                }
                ui.menu_button(tr("+ Añadir efecto"), |ui| {
                    for k in FxKind::disponibles() {
                        if ui.button(k.name()).clicked() {
                            act = Some(Act::Add(k));
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Agregar mis efectos…")).on_hover_text(tr("Plugins VST3 / LV2 / CLAP instalados")).clicked() {
                        act = Some(Act::MyPlugins);
                    }
                });
            };
            if en_fila {
                // Ranura vacía al final de la fila: marco del alto del panel con el botón en el centro.
                let alto = (ui.available_height() - 8.0).max(120.0);
                ui.allocate_ui_with_layout(vec2(220.0, alto), egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.set_width(220.0);
                    ui.painter().rect_stroke(egui::Rect::from_min_size(ui.max_rect().min, vec2(220.0, alto)), widgets::rr(12.0), egui::Stroke::new(1.0, BORDER()), egui::StrokeKind::Inside);
                    ui.add_space((alto / 2.0 - 40.0).max(8.0));
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(190.0);
                        anadir(ui);
                    });
                });
            } else {
                anadir(ui);
            }
        };
        if en_fila {
            egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_top(|ui| cuerpo(self, ui));
            });
        } else {
            cuerpo(self, ui);
        }
        match act {
            Some(Act::Add(k)) => self.add_fx(i, k),
            Some(Act::Detectar(j)) => self.status = self.curves_detectar(i, j),
            Some(Act::Sincronizar) => self.dirty = true,
            Some(Act::MyPlugins) => (self.show_lib, self.lib_tab) = (true, library::PLUGINS_TAB),
            Some(Act::Licencias) => self.licencias_abierta = true,
            Some(Act::Drums) => self.abrir_drummer(Some(i)),
            Some(Act::Remove(j)) => {
                self.edit();
                self.s.tracks[i].fx.remove(j);
            }
            Some(Act::Up(j)) => {
                self.edit();
                self.s.tracks[i].fx.swap(j, j - 1);
            }
            None => {}
        }
    }
}
