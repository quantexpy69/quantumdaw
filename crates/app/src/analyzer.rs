//! Analizador de espectro del master (FFT con ventana de Hann) con opciones de visualización.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Mesh, Pos2, Rect, RichText, Sense, Stroke, pos2};
use rustfft::{FftPlanner, num_complex::Complex};

pub struct Analyzer {
    size: usize,
    smooth: f32,
    /// Pendiente de compensación en dB/octava (centrada en 1 kHz), como en los analizadores de mastering.
    slope: f32,
    hold: bool,
    ring: Vec<f32>,
    spec: Vec<f32>,
    peak: Vec<f32>,
    planner: FftPlanner<f32>,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self { size: 8192, smooth: 0.75, slope: 3.0, hold: true, ring: vec![], spec: vec![], peak: vec![], planner: FftPlanner::new() }
    }
}

const FLOOR: f32 = -90.0;
const CEIL: f32 = 6.0;

impl Analyzer {
    /// Calcula el espectro (dB por bin) con la última ventana de muestras.
    fn update(&mut self) {
        let n = self.size;
        if self.ring.len() < n {
            return;
        }
        let start = self.ring.len() - n;
        let mut buf: Vec<Complex<f32>> = self.ring[start..].iter().enumerate().map(|(i, &x)| Complex::new(x * (std::f32::consts::PI * i as f32 / n as f32).sin().powi(2), 0.0)).collect();
        self.planner.plan_fft_forward(n).process(&mut buf);
        if self.spec.len() != n / 2 {
            (self.spec, self.peak) = (vec![FLOOR; n / 2], vec![FLOOR; n / 2]);
        }
        for (k, c) in buf.iter().take(n / 2).enumerate() {
            let db = 20.0 * (c.norm() * 4.0 / n as f32).max(1e-9).log10();
            self.spec[k] = self.spec[k] * self.smooth + db * (1.0 - self.smooth);
            self.peak[k] = if self.hold { self.peak[k].max(self.spec[k]) } else { self.spec[k] };
        }
        self.ring.drain(..start);
    }
}

impl App {
    pub fn analyzer(&mut self, ui: &mut egui::Ui) {
        if let Ok(mut rx) = self.engine.scope_rx.lock() {
            while let Ok(s) = rx.pop() {
                self.analyzer.ring.push(s);
            }
        }
        self.analyzer.update();
        let sr = self.sr() as f32;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(190.0);
                let a = &mut self.analyzer;
                ui.label(RichText::new(tr("ANALIZADOR")).size(10.0).strong().color(ACCENT));
                egui::ComboBox::from_label(tr("Resolución")).selected_text(format!("{}", a.size)).show_ui(ui, |ui| {
                    for s in [2048, 4096, 8192, 16384] {
                        ui.selectable_value(&mut a.size, s, format!("{s} puntos"));
                    }
                });
                ui.add(egui::Slider::new(&mut a.smooth, 0.0..=0.95).text(tr("Suavizado")));
                egui::ComboBox::from_label(tr("Pendiente")).selected_text(format!("{} dB/oct", a.slope)).show_ui(ui, |ui| {
                    for s in [0.0, 3.0, 4.5] {
                        ui.selectable_value(&mut a.slope, s, format!("{s} dB/oct"));
                    }
                });
                ui.checkbox(&mut a.hold, tr("Retener picos"));
                if ui.button(tr("Reiniciar picos")).clicked() {
                    a.peak.iter_mut().for_each(|p| *p = FLOOR);
                }
                ui.separator();
                let master = self.levels.last().copied().unwrap_or(0.0);
                let peak = self.peaks.last().copied().unwrap_or(0.0);
                ui.label(RichText::new(format!("Nivel master: {} dB", widgets::db_text(master))).monospace().size(11.0));
                ui.label(RichText::new(format!("Pico máximo: {} dB", widgets::db_text(peak))).monospace().size(11.0).color(if widgets::db(peak) > 0.0 { METER[2] } else { TEXT }));
            });
            let rect = ui.available_rect_before_wrap();
            ui.allocate_rect(rect, Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, rr(8.0), BG);
            let r = rect.shrink2(egui::vec2(30.0, 12.0));
            let x_of = |f: f32| r.left() + (f / 20.0).log10() / 3.0 * r.width();
            let y_of = |db: f32| r.top() + (CEIL - db.clamp(FLOOR, CEIL)) / (CEIL - FLOOR) * r.height();
            for f in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
                p.vline(x_of(f), r.y_range(), Stroke::new(1.0, PANEL));
                let text = if f >= 1000.0 { format!("{}k", f / 1000.0) } else { format!("{f}") };
                p.text(pos2(x_of(f), r.bottom() + 1.0), Align2::CENTER_TOP, text, FontId::monospace(9.0), TEXT_DIM);
            }
            for db in (-84..=0).step_by(12) {
                p.hline(r.x_range(), y_of(db as f32), Stroke::new(1.0, PANEL));
                p.text(pos2(r.left() - 4.0, y_of(db as f32)), Align2::RIGHT_CENTER, db.to_string(), FontId::monospace(9.0), TEXT_DIM);
            }
            let a = &self.analyzer;
            if a.spec.is_empty() {
                p.text(r.center(), Align2::CENTER_CENTER, tr("Reproduce el proyecto para ver el espectro"), FontId::proportional(13.0), TEXT_DIM);
                return;
            }
            let bins = a.spec.len();
            let value = |data: &[f32], f: f32| {
                let k = (f * a.size as f32 / sr).clamp(1.0, bins as f32 - 1.0) as usize;
                data[k] + a.slope * (f / 1000.0).log2()
            };
            let cols = r.width() as usize;
            let freq = |c: usize| 20.0 * 1000f32.powf(c as f32 / cols as f32);
            let mut mesh = Mesh::default();
            let mut line: Vec<Pos2> = vec![];
            for c in 0..cols {
                let (x, y) = (r.left() + c as f32, y_of(value(&a.spec, freq(c))));
                mesh.add_colored_rect(Rect::from_x_y_ranges(x..=x + 1.0, y..=r.bottom()), ACCENT.gamma_multiply(0.25));
                line.push(pos2(x, y));
            }
            p.add(mesh);
            p.add(egui::Shape::line(line, Stroke::new(1.5, ACCENT)));
            if a.hold {
                let peaks: Vec<Pos2> = (0..cols).map(|c| pos2(r.left() + c as f32, y_of(value(&a.peak, freq(c))))).collect();
                p.add(egui::Shape::line(peaks, Stroke::new(1.0, METER[1].gamma_multiply(0.8))));
            }
        });
    }
}
