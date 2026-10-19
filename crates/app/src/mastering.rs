//! Ventana «Mastering y loudness»: medidor de sonoridad del master (LUFS, true peak, rango y
//! correlación), objetivos por plataforma y la cadena de efectos del master.
use crate::*;
use egui::{Align2, FontId, RichText, Sense, Stroke, pos2, vec2};
use widgets::rr;

/// Objetivos de sonoridad: (plataforma, LUFS integrados, true peak máximo en dBTP).
const OBJETIVOS: [(&str, f32, f32); 9] = [
    ("Spotify", -14.0, -1.0),
    ("YouTube", -14.0, -1.0),
    ("Apple Music", -16.0, -1.0),
    ("Amazon Music", -14.0, -2.0),
    ("Tidal", -14.0, -1.0),
    ("Deezer", -15.0, -1.0),
    ("SoundCloud", -14.0, -1.0),
    ("Radio y TV (EBU R128)", -23.0, -1.0),
    ("Máster fuerte (club, CD)", -9.0, -0.3),
];

/// Estado de la ventana: abierta, objetivo elegido y último resumen calculado (integrado, LRA).
pub struct Mastering {
    pub abierta: bool,
    objetivo: usize,
    resumen: (f32, f32),
    calculado: std::time::Instant,
}

impl Default for Mastering {
    fn default() -> Self {
        Self { abierta: false, objetivo: 0, resumen: (-120.0, 0.0), calculado: std::time::Instant::now() }
    }
}

/// Número grande con su etiqueta.
fn cifra(ui: &mut egui::Ui, etiqueta: &str, valor: f32, unidad: &str, color: Color32) {
    ui.vertical(|ui| {
        ui.label(RichText::new(etiqueta).size(11.0).color(TEXT_DIM()));
        let texto = if valor <= -119.0 { "—".to_string() } else { format!("{valor:.1}") };
        ui.label(RichText::new(format!("{texto} {unidad}")).size(22.0).strong().color(color));
    });
}

impl App {
    pub fn ventana_mastering(&mut self, ctx: &egui::Context) {
        if !self.mastering.abierta {
            return;
        }
        let m = &self.engine.medidor;
        if self.mastering.calculado.elapsed() > Duration::from_millis(500) {
            self.mastering.resumen = m.resumen();
            self.mastering.calculado = std::time::Instant::now();
        }
        let (integrado, lra) = self.mastering.resumen;
        let (momentaneo, corto, tp, corr) = (m.momentaneo.get(), m.corto.get(), m.true_peak.get(), m.correlacion.get());
        let mut abierta = true;
        let mut cambio_cadena = false;
        egui::Window::new(RichText::new(tr("Mastering y loudness")).strong()).id(egui::Id::new("mastering")).open(&mut abierta).default_size([620.0, 680.0]).vscroll(true).show(ctx, |ui| {
            let (objetivo_lufs, objetivo_tp) = (OBJETIVOS[self.mastering.objetivo].1, OBJETIVOS[self.mastering.objetivo].2);
            let color_lufs = |v: f32| if v <= -119.0 { TEXT_DIM() } else if (v - objetivo_lufs).abs() <= 1.0 { METER[0] } else if v > objetivo_lufs { METER[2] } else { METER[1] };
            ui.horizontal(|ui| {
                cifra(ui, tr("Integrado"), integrado, "LUFS", color_lufs(integrado));
                ui.add_space(14.0);
                cifra(ui, tr("Corto plazo (3 s)"), corto, "LUFS", TEXT());
                ui.add_space(14.0);
                cifra(ui, tr("Momentáneo"), momentaneo, "LUFS", TEXT());
            });
            ui.horizontal(|ui| {
                cifra(ui, tr("True peak máx."), tp, "dBTP", if tp > objetivo_tp { METER[2] } else { METER[0] });
                ui.add_space(14.0);
                cifra(ui, tr("Rango (LRA)"), lra, "LU", TEXT());
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Correlación de fase")).size(11.0).color(TEXT_DIM()));
                    let (r, _) = ui.allocate_exact_size(vec2(180.0, 22.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(r, rr(4.0), ELEVATED());
                    p.vline(r.center().x, r.y_range(), Stroke::new(1.0, BORDER()));
                    let x = r.center().x + corr.clamp(-1.0, 1.0) * r.width() / 2.0;
                    p.circle_filled(pos2(x, r.center().y), 6.0, if corr < 0.0 { METER[2] } else { METER[0] });
                    p.text(r.left_center() + vec2(4.0, 0.0), Align2::LEFT_CENTER, "-1", FontId::proportional(10.0), TEXT_DIM());
                    p.text(r.right_center() - vec2(4.0, 0.0), Align2::RIGHT_CENTER, "+1", FontId::proportional(10.0), TEXT_DIM());
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Reiniciar medición")).on_hover_text(tr("Empieza a medir de nuevo el integrado, el rango y el true peak (reproduce la canción entera para medirla)")).clicked() {
                    m.reiniciar.store(true, Relaxed);
                    self.mastering.resumen = (-120.0, 0.0);
                }
                ui.label(RichText::new(tr("Mide la salida del master mientras reproduces.")).size(11.0).color(TEXT_DIM()));
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Objetivo")).strong());
                egui::ComboBox::from_id_salt("objetivo-lufs").width(240.0).selected_text(tr(OBJETIVOS[self.mastering.objetivo].0)).show_ui(ui, |ui| {
                    for (k, (n, l, t)) in OBJETIVOS.iter().enumerate() {
                        ui.selectable_value(&mut self.mastering.objetivo, k, format!("{} · {l} LUFS · {t} dBTP", tr(n)));
                    }
                });
            });
            if integrado > -119.0 {
                let dif = integrado - objetivo_lufs;
                let consejo = if dif.abs() <= 1.0 {
                    tr("La sonoridad está en el objetivo.").to_string()
                } else if dif > 0.0 {
                    format!("{} {dif:.1} dB {}", tr("Está"), tr("por encima: la plataforma la bajará; puedes bajar la ganancia del limitador."))
                } else {
                    format!("{} {:.1} dB {}", tr("Faltan"), -dif, tr("para el objetivo: sube la ganancia del limitador del master."))
                };
                ui.label(RichText::new(consejo).color(color_lufs(integrado)));
                if tp > objetivo_tp {
                    ui.label(RichText::new(format!("{} {objetivo_tp} dBTP: {}", tr("El true peak supera"), tr("baja el techo del limitador."))).color(METER[2]));
                }
            }
            ui.separator();
            // Cadena del master.
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Efectos del master")).strong());
                ui.menu_button(tr("+ Añadir efecto"), |ui| {
                    widgets::menu_style(ui);
                    for k in FxKind::ALL.into_iter().filter(|k| *k != FxKind::Tune) {
                        if ui.button(tr(k.name())).clicked() {
                            self.master_fx.push(Arc::new(Fx::new(k, self.engine.sample_rate)));
                            cambio_cadena = true;
                        }
                    }
                });
                if self.master_fx.is_empty() && ui.button(tr("Cadena de mastering sugerida")).on_hover_text(tr("Curves, compresor suave y limitador a -1 dBTP")).clicked() {
                    let sr = self.engine.sample_rate;
                    let comp = Fx::new(FxKind::Compressor, sr);
                    for (k, v) in [(0, -14.0), (1, 2.0), (3, 20.0), (4, 200.0)] {
                        comp.params[k].set(v);
                    }
                    self.master_fx = vec![Arc::new(Fx::new(FxKind::Curves, sr)), Arc::new(comp), Arc::new(Fx::new(FxKind::Limiter, sr))];
                    cambio_cadena = true;
                }
            });
            if self.master_fx.is_empty() {
                ui.label(RichText::new(tr("Sin efectos en el master.")).color(TEXT_DIM()));
            }
            let mut quitar = None;
            for (k, fx) in self.master_fx.iter().enumerate() {
                let color = plugins::kind_color(Some(fx.kind));
                egui::Frame::new().fill(ELEVATED()).corner_radius(rr(8.0)).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(tr(fx.kind.name())).strong().color(color));
                        let mut activo = !fx.bypass.load(Relaxed);
                        if ui.checkbox(&mut activo, tr("Activo")).changed() {
                            fx.bypass.store(!activo, Relaxed);
                        }
                        if matches!(fx.kind, FxKind::Limiter | FxKind::Compressor | FxKind::Gate | FxKind::DeEsser) {
                            ui.label(RichText::new(format!("GR -{:.1} dB", fx.readout[0].get())).color(METER[1]));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("×").on_hover_text(tr("Quitar")).clicked() {
                                quitar = Some(k);
                            }
                        });
                    });
                    if fx.kind == FxKind::Curves {
                        ui.label(RichText::new(tr("Ábrelo en una pista para ver el visor; aquí se ajustan sus perillas.")).size(10.0).color(TEXT_DIM()));
                    }
                    ui.horizontal_wrapped(|ui| {
                        for (p, def) in fx.kind.params().iter().enumerate() {
                            plugins::param_knob(ui, Some(fx.kind), *def, &fx.params[p]);
                        }
                    });
                });
            }
            if let Some(k) = quitar {
                self.master_fx.remove(k);
                cambio_cadena = true;
            }
        });
        if cambio_cadena {
            self.engine.master_fx.store(Arc::new(self.master_fx.clone()));
            self.cambios = true;
        }
        self.mastering.abierta &= abierta;
    }
}
