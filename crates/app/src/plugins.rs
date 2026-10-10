//! Ventana de plugins QUANTUM: instrumento y cadena de efectos, con visor, perillas y firma.
use crate::{
    widgets::{self, rr},
    *,
};
use egui::{Align2, FontId, Id, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use engine::{AtomicF32, SYNTH_PARAMS, eq_response};

const PLUGIN_BG: Color32 = Color32::from_rgb(0x15, 0x15, 0x19);
const SIGNATURE: &str = "QUANTUM";
const DISCRETE: [&str; 8] = ["Tonalidad", "Nota manual", "Escala", "Modo", "Onda", "Ping-pong", "Sincronía", "División"];

/// Color de acento de cada plugin (None = instrumento).
pub fn kind_color(kind: Option<FxKind>) -> Color32 {
    match kind {
        Some(FxKind::Eq) => Color32::from_rgb(0x2E, 0xD8, 0xC3),
        Some(FxKind::Compressor) => Color32::from_rgb(0xFF, 0xB0, 0x20),
        Some(FxKind::Delay) => Color32::from_rgb(0x5B, 0x9C, 0xFF),
        Some(FxKind::Reverb) => Color32::from_rgb(0xB0, 0x8C, 0xF2),
        Some(FxKind::Drive) => Color32::from_rgb(0xFF, 0x7A, 0x45),
        Some(FxKind::Tune) => Color32::from_rgb(0x3D, 0xDC, 0x84),
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
    ui.label(RichText::new(format!("{} {}", k.params().len(), tr("parámetros"))).size(11.0).color(TEXT_DIM));
}

/// Texto del valor de un parámetro con su unidad (o su nombre, en los discretos).
fn value_text(kind: Option<FxKind>, name: &str, v: f32) -> String {
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
        "Corte" | "Tono" | "Frec. graves" | "Frec. medios" | "Frec. agudos" | "Corte graves" | "Corte agudos" => hz(v),
        "Q medios" => format!("Q {v:.2}"),
        "Desafinar" => format!("{v:.0} ct"),
        "Ratio" => format!("{v:.1}:1"),
        "Ataque" | "Caída" | "Liberación" => format!("{v:.0} ms"),
        n if n.ends_with("dB") => format!("{v:+.1} dB"),
        n if n.ends_with("ms") => format!("{v:.1} ms"),
        _ => format!("{:.0} %", v * 100.0),
    }
}

/// Perilla de consola: escala logarítmica en rangos amplios (frecuencias, tiempos), color del plugin.
fn param_knob(ui: &mut egui::Ui, kind: Option<FxKind>, (name, min, max, def): (&str, f32, f32, f32), a: &AtomicF32) {
    let log = min > 0.0 && max / min > 10.0;
    let to_t = |v: f32| {
        if log { (v / min).ln() / (max / min).ln() } else { (v - min) / (max - min) }
    };
    let from_t = |t: f32| {
        if log { min * (max / min).powf(t) } else { min + t * (max - min) }
    };
    let color = kind_color(kind);
    ui.allocate_ui(vec2(84.0, 102.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut t = to_t(a.get());
            let r = widgets::console_knob(ui, &mut t, to_t(def), color);
            if r.dragged() || r.double_clicked() {
                let v = from_t(t);
                a.set(if DISCRETE.contains(&name) { v.round() } else { v });
            }
            ui.label(RichText::new(value_text(kind, name, a.get())).monospace().size(11.0).strong().color(color));
            ui.label(RichText::new(tr(name.trim_end_matches(" dB").trim_end_matches(" ms"))).size(10.5).color(TEXT));
        });
    });
}

/// Visor de cada plugin: curvas de respuesta, medidores o la nota detectada.
fn display(ui: &mut egui::Ui, kind: Option<FxKind>, p: &[f32], readout: [f32; 2], color: Color32, sr: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 96.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, rr(8.0), BG);
    let r = rect.shrink(8.0);
    let line = |pts: Vec<Pos2>| painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
    let curve = |f: &dyn Fn(f32) -> f32| line((0..=120).map(|k| k as f32 / 120.0).map(|t| pos2(r.left() + t * r.width(), r.bottom() - f(t).clamp(0.0, 1.0) * r.height())).collect());
    let label = |pos: Pos2, align: Align2, text: String, size: f32, c: Color32| painter.text(pos, align, text, FontId::proportional(size), c);
    painter.hline(r.x_range(), r.center().y, Stroke::new(1.0, PANEL));
    match kind {
        Some(FxKind::Eq) => {
            curve(&|t| 0.5 + eq_response(p, 20.0 * 1000f32.powf(t), sr) / 36.0);
            for (f, text) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
                let x = r.left() + (f / 20.0f32).log10() / 3.0 * r.width();
                painter.vline(x, r.y_range(), Stroke::new(1.0, PANEL));
                label(pos2(x + 3.0, r.bottom()), Align2::LEFT_BOTTOM, text.into(), 9.0, TEXT_DIM);
            }
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
                label(pos2(r.center().x - 60.0, r.center().y), Align2::CENTER_CENTER, name(readout[0]), 30.0, TEXT_DIM);
                label(r.center(), Align2::CENTER_CENTER, "→".into(), 22.0, TEXT_DIM);
                label(pos2(r.center().x + 60.0, r.center().y), Align2::CENTER_CENTER, name(readout[1]), 30.0, color);
                label(pos2(r.center().x, r.bottom()), Align2::CENTER_BOTTOM, format!("{cents:+} cents corregidos"), 11.0, TEXT_DIM);
            } else {
                label(r.center(), Align2::CENTER_CENTER, "Canta o toca una nota para ver la corrección".into(), 13.0, TEXT_DIM);
            }
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
        enum Act {
            Add(FxKind),
            Remove(usize),
            Up(usize),
            MyPlugins,
        }
        let (mut open, mut act, sr) = (true, None, self.engine.sample_rate as f32);
        let t = &self.s.tracks[i];
        let title = RichText::new(format!("QUANTUM · {}", t.name)).strong();
        egui::Window::new(title).id(Id::new("fx")).open(&mut open).default_width(580.0).default_height(680.0).vscroll(true).show(ctx, |ui| {
            let plugin = |ui: &mut egui::Ui,
                          name: &str,
                          kind: Option<FxKind>,
                          params: &[AtomicF32],
                          defs: &[(&str, f32, f32, f32)],
                          bypass: Option<&AtomicBool>,
                          readout: [f32; 2],
                          j: Option<usize>,
                          act: &mut Option<Act>| {
                let accent = kind_color(kind);
                egui::Frame::new().fill(PLUGIN_BG).corner_radius(rr(12.0)).stroke(Stroke::new(1.5, accent.gamma_multiply(0.6))).inner_margin(10.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // Franja de color del plugin.
                    let (strip, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
                    ui.painter().rect_filled(strip, rr(2.0), accent);
                    ui.horizontal(|ui| {
                        if let Some(b) = bypass {
                            let on = !b.load(Relaxed);
                            let power = egui::Button::new(RichText::new(if on { "ON" } else { "OFF" }).size(10.0).strong().color(if on { BG } else { TEXT_DIM }))
                                .fill(if on { METER[0] } else { BG })
                                .corner_radius(rr(10.0));
                            if ui.add(power).on_hover_text(if on { "Activo · clic para bypass" } else { "En bypass · clic para activar" }).clicked() {
                                b.store(on, Relaxed);
                            }
                        }
                        ui.label(RichText::new(tr("QUANTUM")).size(10.0).strong().color(accent));
                        ui.label(RichText::new(tr(name).trim_start_matches("QUANTUM ")).size(17.0).strong().color(TEXT));
                        if let Some(j) = j {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button(tr("Quitar")).clicked() {
                                    *act = Some(Act::Remove(j));
                                }
                                if ui.add_enabled(j > 0, egui::Button::new("⏶").small()).on_hover_text(tr("Subir en la cadena")).clicked() {
                                    *act = Some(Act::Up(j));
                                }
                            });
                        }
                    });
                    let values: Vec<f32> = params.iter().map(|a| a.get()).collect();
                    display(ui, kind, &values, readout, accent, sr);
                    ui.horizontal_wrapped(|ui| {
                        for (k, def) in defs.iter().enumerate() {
                            param_knob(ui, kind, *def, &params[k]);
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| ui.label(RichText::new(SIGNATURE).size(9.5).italics().color(TEXT_DIM)));
                });
                ui.add_space(6.0);
            };
            if let Some(s) = &t.sampler {
                // Instrumento real (sampler): volumen y créditos de la biblioteca.
                ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM));
                egui::Frame::new().fill(PLUGIN_BG).corner_radius(rr(12.0)).stroke(Stroke::new(1.0, BORDER)).inner_margin(10.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let e = instruments::find(&t.instrument);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr("QUANTUM")).size(10.0).strong().color(ACCENT));
                        ui.label(RichText::new(e.map_or(s.name.as_str(), |e| e.name)).size(16.0).strong());
                    });
                    let mut db = widgets::db(s.gain.get());
                    if ui.add(egui::Slider::new(&mut db, -30.0..=12.0).suffix(" dB").text(tr("Volumen"))).changed() {
                        s.gain.set(10f32.powf(db / 20.0));
                    }
                    if let Some(e) = e {
                        ui.label(RichText::new(format!("Muestras: {} · licencia {}", e.credit, e.license)).size(10.0).color(TEXT_DIM));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| ui.label(RichText::new(SIGNATURE).size(9.5).italics().color(TEXT_DIM)));
                });
                ui.add_space(6.0);
            } else if let Some(s) = &t.synth {
                ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM));
                plugin(ui, "Synth", None, &s.params, &SYNTH_PARAMS, None, [0.0; 2], None, &mut act);
            }
            ui.label(RichText::new(tr("EFECTOS DE AUDIO")).size(10.0).color(TEXT_DIM));
            if t.fx.is_empty() {
                ui.label(RichText::new(tr("Sin efectos. Añade uno abajo o arrástralo desde la Biblioteca.")).color(TEXT_DIM));
            }
            for (j, fx) in t.fx.iter().enumerate() {
                let readout = [fx.readout[0].get(), fx.readout[1].get()];
                plugin(ui, fx.kind.name(), Some(fx.kind), &fx.params, fx.kind.params(), Some(&fx.bypass), readout, Some(j), &mut act);
            }
            ui.menu_button(tr("+ Añadir efecto"), |ui| {
                for k in FxKind::ALL {
                    if ui.button(k.name()).clicked() {
                        act = Some(Act::Add(k));
                    }
                }
                ui.separator();
                if ui.button(tr("Agregar mis efectos…")).on_hover_text(tr("Plugins VST3 / LV2 / CLAP instalados")).clicked() {
                    act = Some(Act::MyPlugins);
                }
            });
        });
        match act {
            Some(Act::Add(k)) => self.add_fx(i, k),
            Some(Act::MyPlugins) => (self.show_lib, self.lib_tab) = (true, library::PLUGINS_TAB),
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
        if !open {
            self.fx_window = None;
        }
        // Los visores (reducción de ganancia, nota detectada) se actualizan en vivo.
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}
