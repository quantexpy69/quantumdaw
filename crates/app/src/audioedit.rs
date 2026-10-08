//! Editor de audio del panel inferior (doble clic en una región de audio): procesos sobre la
//! selección o la región entera, Time & Pitch Machine, ajuste de tempo por selección y localizadores,
//! y búsqueda de picos y silencios. Cada proceso escribe un audio nuevo y se puede deshacer.
use crate::{widgets::rr, *};
use egui::{Align2, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use engine::PEAK_BLOCK;

#[derive(Clone, Copy, PartialEq)]
pub enum Process {
    Normalize,
    Gain(f32),
    FadeIn,
    FadeOut,
    Silence,
    InvertPhase,
    Reverse,
    Trim,
    RemoveDc,
    /// Duración (factor) y transposición (semitonos).
    TimePitch(f64, f32),
}

impl Process {
    fn tag(self) -> &'static str {
        match self {
            Process::Normalize => "normalizado",
            Process::Gain(_) => "ganancia",
            Process::FadeIn => "fade-in",
            Process::FadeOut => "fade-out",
            Process::Silence => "silencio",
            Process::InvertPhase => "fase",
            Process::Reverse => "invertido",
            Process::Trim => "recortado",
            Process::RemoveDc => "sin-dc",
            Process::TimePitch(..) => "time-pitch",
        }
    }
}

/// Aplica un proceso a un trozo de audio (puede cambiar su duración).
fn process(mut a: Vec<[f32; 2]>, op: Process) -> Vec<[f32; 2]> {
    let n = a.len().max(1) as f32;
    let scale = |a: &mut Vec<[f32; 2]>, g: &dyn Fn(usize) -> f32| a.iter_mut().enumerate().for_each(|(k, s)| *s = [s[0] * g(k), s[1] * g(k)]);
    match op {
        Process::Normalize => {
            let g = 0.966 / engine::peak(&a).max(1e-6);
            scale(&mut a, &|_| g);
        }
        Process::Gain(db) => scale(&mut a, &|_| 10f32.powf(db / 20.0)),
        Process::FadeIn => scale(&mut a, &|k| k as f32 / n),
        Process::FadeOut => scale(&mut a, &|k| 1.0 - k as f32 / n),
        Process::Silence => a.iter_mut().for_each(|s| *s = [0.0; 2]),
        Process::InvertPhase => scale(&mut a, &|_| -1.0),
        Process::Reverse => a.reverse(),
        Process::Trim => {}
        Process::RemoveDc => {
            let mean = a.iter().fold([0.0f64; 2], |m, s| [m[0] + s[0] as f64, m[1] + s[1] as f64]).map(|v| (v / n as f64) as f32);
            a.iter_mut().for_each(|s| *s = [s[0] - mean[0], s[1] - mean[1]]);
        }
        Process::TimePitch(f, st) => {
            if (f - 1.0).abs() > 1e-4 {
                a = engine::stretch(&a, f);
            }
            if st.abs() > 1e-3 {
                a = engine::pitch_shift(&a, st);
            }
        }
    }
    a
}

const INTERVALS: [(f32, &str); 13] = [
    (-12.0, "Octava abajo"),
    (-7.0, "Quinta abajo"),
    (-5.0, "Cuarta abajo"),
    (-2.0, "Tono abajo"),
    (-1.0, "Semitono abajo"),
    (0.0, "Sin transponer"),
    (1.0, "Semitono arriba"),
    (2.0, "Tono arriba"),
    (3.0, "Tercera menor"),
    (4.0, "Tercera mayor"),
    (5.0, "Cuarta justa"),
    (7.0, "Quinta justa"),
    (12.0, "Octava arriba"),
];

/// Región abierta en el editor y sus ajustes.
pub struct AudioEdit {
    track: u64,
    clip: usize,
    /// Selección en frames relativos al inicio de la región.
    sel: Option<(u64, u64)>,
    drag: Option<u64>,
    tpm: bool,
    semis: f32,
    cents: f32,
    /// Duración resultante en % de la original.
    length_pct: f64,
    gain_db: f32,
    bars: f32,
    peak_db: f32,
    silence_db: f32,
    silence_ms: f32,
    peaks: Vec<u64>,
    silences: Vec<(u64, u64)>,
}

fn smpte(f: u64, sr: f64) -> String {
    let s = f as f64 / sr;
    format!("{:02}:{:02}:{:02}:{:02}", (s / 3600.0) as u64, (s / 60.0) as u64 % 60, s as u64 % 60, (s.fract() * 30.0) as u64)
}

impl App {
    pub fn open_audio_editor(&mut self, i: usize, j: usize) {
        let e = AudioEdit {
            track: self.s.tracks[i].id,
            clip: j,
            sel: None,
            drag: None,
            tpm: true,
            semis: 0.0,
            cents: 0.0,
            length_pct: 100.0,
            gain_db: 3.0,
            bars: 0.0,
            peak_db: -1.0,
            silence_db: -50.0,
            silence_ms: 250.0,
            peaks: vec![],
            silences: vec![],
        };
        self.aedit = Some(e);
        (self.show_mixer, self.bottom_tab) = (true, 3);
    }

    /// Región abierta: (pista, clip, audio).
    fn aedit_target(&self) -> Option<(usize, usize, Arc<AudioBuf>)> {
        let e = self.aedit.as_ref()?;
        let i = self.s.tracks.iter().position(|t| t.id == e.track)?;
        let c = self.s.tracks[i].clips.get(e.clip)?;
        Some((i, e.clip, c.buf()?.clone()))
    }

    /// Material de la región y rango elegido (la selección o toda la región).
    fn aedit_range(&self) -> Option<(Vec<[f32; 2]>, usize, usize)> {
        let (i, j, b) = self.aedit_target()?;
        let c = &self.s.tracks[i].clips[j];
        let part = b.frames[c.offset as usize..((c.offset + c.len) as usize).min(b.frames.len())].to_vec();
        let (a, z) = self.aedit.as_ref()?.sel.map_or((0, part.len()), |(a, z)| (a as usize, (z as usize).min(part.len())));
        (z > a).then_some((part, a, z))
    }

    /// Aplica un proceso a la selección (o a toda la región) y lo escribe como audio nuevo.
    fn aedit_apply(&mut self, op: Process) -> anyhow::Result<()> {
        let (part, a, z) = self.aedit_range().ok_or_else(|| anyhow::anyhow!("la selección está vacía"))?;
        let (i, j, b) = self.aedit_target().ok_or_else(|| anyhow::anyhow!("no hay región abierta"))?;
        let c = self.s.tracks[i].clips[j].clone();
        let sel = process(part[a..z].to_vec(), op);
        let n = sel.len();
        let frames: Vec<[f32; 2]> = if op == Process::Trim { sel } else { part[..a].iter().chain(&sel).chain(&part[z..]).copied().collect() };
        self.edit();
        let buf = self.write_audio(&b.file, op.tag(), frames)?;
        let len = buf.frames.len() as u64;
        let start = if op == Process::Trim { c.start + a as u64 } else { c.start };
        self.s.tracks[i].clips[j] = Clip { fade_in: c.fade_in.min(len / 2), fade_out: c.fade_out.min(len / 2), gain: c.gain, selected: true, ..Clip::audio(buf, start) };
        if let Some(e) = &mut self.aedit {
            e.sel = (op != Process::Trim && (a, z) != (0, part.len())).then_some((a as u64, (a + n) as u64));
            (e.peaks, e.silences) = (vec![], vec![]);
        }
        self.dirty = true;
        Ok(())
    }

    fn aedit_run(&mut self, op: Process) {
        let r = self.aedit_apply(op);
        self.report(format!("{}: {}", tr("Proceso aplicado"), tr(op.tag())), r);
    }

    /// Escucha la selección procesada con la Time & Pitch Machine (en bucle).
    fn aedit_preview(&mut self, op: Process) {
        let Some((part, a, z)) = self.aedit_range() else { return };
        let out = process(part[a..z].to_vec(), op);
        self.engine.preview_pos.store(0, Relaxed);
        self.engine.preview_loop.store(true, Relaxed);
        self.engine.preview.store(Some(Arc::new(out)));
    }

    pub fn audio_editor(&mut self, ui: &mut egui::Ui) {
        let Some((i, j, b)) = self.aedit_target() else {
            self.aedit = None;
            ui.label(RichText::new(tr("Haz doble clic en una región de audio para editarla aquí.")).color(TEXT_DIM));
            return;
        };
        let sr = self.sr();
        let c = self.s.tracks[i].clips[j].clone();
        let (tname, color) = (self.s.tracks[i].name.clone(), self.s.tracks[i].color);
        let per_bar = self.engine.beats.load(Relaxed).max(1) as f64;
        let bars_of = |app: &Self, from: u64, len: u64| (app.beats((from + len) as f64) - app.beats(from as f64)) / per_bar;
        let bpm = engine::bpm_at(&self.s.tempo, c.start as f64);
        let (mut run, mut preview_req, mut go_to, mut tempo_from_sel, mut fit_loop, mut close) = (None, None, None, None, None, false);
        let Some(mut e) = self.aedit.take() else { return };
        let (sa, sz) = e.sel.unwrap_or((0, c.len));
        let sel_len = sz.saturating_sub(sa);
        let tp = Process::TimePitch(e.length_pct / 100.0, e.semis + e.cents / 100.0);

        // Menú superior del editor.
        egui::Frame::new().fill(ELEVATED).corner_radius(rr(8.0)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::menu_style(ui);
                ui.menu_button(RichText::new(tr("Procesar")).strong(), |ui| {
                    widgets::menu_style(ui);
                    for (label, op) in [
                        ("Normalizar audio", Process::Normalize),
                        ("Fundido de entrada", Process::FadeIn),
                        ("Fundido de salida", Process::FadeOut),
                        ("Silencio", Process::Silence),
                        ("Invertir fase", Process::InvertPhase),
                        ("Invertir (reverse)", Process::Reverse),
                        ("Acortar (dejar solo la selección)", Process::Trim),
                        ("Eliminar desplazamiento de CC", Process::RemoveDc),
                    ] {
                        if ui.button(tr(label)).clicked() {
                            run = Some(op);
                        }
                    }
                    ui.menu_button(tr("Cambiar ganancia"), |ui| {
                        ui.add(egui::Slider::new(&mut e.gain_db, -24.0..=24.0).suffix(" dB"));
                        if ui.button(tr("Aplicar")).clicked() {
                            run = Some(Process::Gain(e.gain_db));
                        }
                    });
                });
                ui.menu_button(RichText::new(tr("Tiempo y tono")).strong(), |ui| {
                    widgets::menu_style(ui);
                    ui.checkbox(&mut e.tpm, "Time & Pitch Machine");
                    ui.menu_button(tr("Ajustar tempo por selección y localizadores"), |ui| {
                        widgets::menu_style(ui);
                        ui.set_width(340.0);
                        if e.bars <= 0.0 {
                            e.bars = (bars_of(self, c.start + sa, sel_len)).round().max(1.0) as f32;
                        }
                        ui.horizontal(|ui| {
                            ui.label(tr("La selección dura"));
                            ui.add(egui::DragValue::new(&mut e.bars).range(0.25..=512.0).speed(0.05).max_decimals(2));
                            ui.label(tr("compases"));
                        });
                        let secs = sel_len as f64 / sr;
                        let unit = self.engine.unit.load(Relaxed) as f64;
                        let new_bpm = (e.bars as f64 * per_bar * 60.0 * 4.0 / (unit * secs.max(1e-3))) as f32;
                        ui.label(RichText::new(format!("{} {new_bpm:.2} BPM", tr("Tempo resultante:"))).size(16.0).color(METER[0]));
                        if ui.button(tr("Ajustar el tempo del proyecto a la selección")).clicked() {
                            tempo_from_sel = Some((c.start + sa, new_bpm));
                        }
                        let (ls, le) = (self.engine.loop_start.load(Relaxed), self.engine.loop_end.load(Relaxed));
                        if ui.add_enabled(le > ls, egui::Button::new(tr("Estirar la selección a los localizadores (loop)"))).clicked() {
                            fit_loop = Some((le - ls) as f64 / sel_len.max(1) as f64);
                        }
                    });
                });
                ui.menu_button(RichText::new(tr("Análisis")).strong(), |ui| {
                    widgets::menu_style(ui);
                    ui.horizontal(|ui| {
                        if ui.button(tr("Buscar picos")).clicked() {
                            let part = &b.frames[c.offset as usize..(c.offset + c.len) as usize];
                            let th = engine::peak(part) * 10f32.powf(e.peak_db / 20.0);
                            e.peaks = engine::find_peaks(part, sr as f32, th).into_iter().map(|k| k as u64).collect();
                        }
                        ui.add(egui::DragValue::new(&mut e.peak_db).range(-24.0..=0.0).speed(0.1).suffix(" dB"));
                    });
                    ui.horizontal(|ui| {
                        if ui.button(tr("Buscar silencios")).clicked() {
                            let part = &b.frames[c.offset as usize..(c.offset + c.len) as usize];
                            e.silences = engine::find_silences(part, sr as f32, e.silence_db, e.silence_ms).into_iter().map(|(a, z)| (a as u64, z as u64)).collect();
                        }
                        ui.add(egui::DragValue::new(&mut e.silence_db).range(-90.0..=-20.0).suffix(" dB"));
                        ui.add(egui::DragValue::new(&mut e.silence_ms).range(20.0..=5000.0).suffix(" ms"));
                    });
                    if ui.add_enabled(!e.peaks.is_empty() || !e.silences.is_empty(), egui::Button::new(tr("Limpiar resultados"))).clicked() {
                        (e.peaks, e.silences) = (vec![], vec![]);
                    }
                });
                ui.separator();
                ui.label(RichText::new(format!("{tname} · {:.0} BPM", bpm)).strong().color(color));
                if !e.peaks.is_empty() {
                    ui.label(RichText::new(format!("{} {}", e.peaks.len(), tr("picos"))).color(METER[2]));
                }
                if !e.silences.is_empty() {
                    ui.label(RichText::new(format!("{} {}", e.silences.len(), tr("silencios"))).color(TEXT_DIM));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(tr("Cerrar editor")).clicked() {
                        close = true;
                    }
                });
            });
        });
        // Duraciones de la región y de la selección.
        let info = |len: u64, from: u64| format!("{len} {} · SMPTE {} · {:.2} {}", tr("muestras"), smpte(len, sr), bars_of(self, from, len), tr("compases"));
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}: {}", tr("Región"), info(c.len, c.start))).size(12.0).color(TEXT_DIM));
            if e.sel.is_some() {
                ui.label(RichText::new(format!("{}: {}", tr("Selección"), info(sel_len, c.start + sa))).size(12.0).color(ACCENT));
            }
        });

        let side = if e.tpm { 340.0 } else { 0.0 };
        let wave = Rect::from_min_size(ui.cursor().min, vec2(ui.available_width() - side - 8.0, ui.available_height().max(80.0)));
        let resp = ui.allocate_rect(wave, Sense::click_and_drag());
        let p = ui.painter_at(wave);
        p.rect_filled(wave, rr(8.0), BG);
        let len = c.len.max(1);
        let x_of = |f: u64| wave.left() + f as f32 / len as f32 * wave.width();
        let f_at = |x: f32| (((x - wave.left()) / wave.width()).clamp(0.0, 1.0) * len as f32) as u64;
        if let Some((a, z)) = e.sel {
            p.rect_filled(Rect::from_x_y_ranges(x_of(a)..=x_of(z), wave.y_range()), 0.0, ACCENT.gamma_multiply(0.22));
        }
        for &(a, z) in &e.silences {
            p.rect_filled(Rect::from_x_y_ranges(x_of(a)..=x_of(z), wave.y_range()), 0.0, Color32::from_white_alpha(10));
        }
        // Forma de onda por canal (L arriba, R abajo).
        let fpp = len as f64 / wave.width() as f64;
        for ch in 0..2 {
            let h = wave.height() / 2.0;
            let mid = wave.top() + h * (ch as f32 + 0.5);
            p.hline(wave.x_range(), mid, Stroke::new(1.0, BORDER));
            let mut mesh = egui::Mesh::default();
            for x in 0..wave.width() as usize {
                let (f0, f1) = (c.offset as f64 + x as f64 * fpp, c.offset as f64 + (x + 1) as f64 * fpp);
                let v = if fpp >= PEAK_BLOCK as f64 {
                    b.peaks[(f0 as usize / PEAK_BLOCK).min(b.peaks.len().saturating_sub(1))..(f1 as usize / PEAK_BLOCK).clamp(f0 as usize / PEAK_BLOCK + 1, b.peaks.len())].iter().fold(0f32, |m, v| m.max(*v))
                } else {
                    b.frames[(f0 as usize).min(b.frames.len() - 1)..(f1.ceil() as usize).clamp(f0 as usize + 1, b.frames.len())].iter().fold(0f32, |m, s| m.max(s[ch].abs()))
                };
                let (xx, a) = (wave.left() + x as f32, (v * c.gain).min(1.0) * (h - 4.0));
                mesh.add_colored_rect(Rect::from_x_y_ranges(xx..=xx + 1.0, mid - a..=mid + a.max(0.5)), color);
            }
            p.add(mesh);
        }
        for &k in &e.peaks {
            p.vline(x_of(k), wave.y_range(), Stroke::new(1.0, METER[2]));
        }
        if let Some((a, z)) = e.sel {
            p.rect_filled(Rect::from_x_y_ranges(x_of(a)..=x_of(z), wave.y_range()), 0.0, Color32::from_white_alpha(24));
            for f in [a, z] {
                p.vline(x_of(f), wave.y_range(), Stroke::new(1.5, ACCENT));
            }
        }
        let pos = self.pos();
        if pos >= c.start && pos <= c.end() {
            p.vline(x_of(pos - c.start), wave.y_range(), Stroke::new(1.5, TEXT));
        }
        p.text(wave.left_top() + vec2(8.0, 6.0), Align2::LEFT_TOP, tr("Arrastra para seleccionar · clic: mover el cursor"), FontId::proportional(11.0), TEXT_DIM);
        if let Some(pp) = resp.interact_pointer_pos() {
            let f = f_at(pp.x);
            if resp.drag_started() {
                e.drag = Some(f);
            }
            if let Some(a) = e.drag.filter(|_| resp.dragged()) {
                e.sel = Some((a.min(f), a.max(f))).filter(|(a, z)| z > a);
                e.bars = 0.0;
            }
            if resp.clicked() {
                (e.sel, e.bars) = (None, 0.0);
                go_to = Some(c.start + f);
            }
        }
        if resp.drag_stopped() {
            e.drag = None;
        }

        // Time & Pitch Machine.
        if e.tpm {
            let panel = Rect::from_min_max(pos2(wave.right() + 8.0, wave.top()), pos2(wave.right() + 8.0 + side, wave.bottom()));
            ui.scope_builder(egui::UiBuilder::new().max_rect(panel), |ui| {
                egui::Frame::new().fill(ELEVATED).corner_radius(rr(10.0)).inner_margin(10.0).show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.set_width(side - 24.0);
                        ui.label(RichText::new("Time & Pitch Machine").size(16.0).strong().color(METER[1]));
                        egui::Grid::new("tpm").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            ui.label(tr("Transposición"));
                            let name = INTERVALS.iter().find(|iv| iv.0 == e.semis).map_or_else(|| format!("{:+} st", e.semis), |iv| tr(iv.1).to_string());
                            egui::ComboBox::from_id_salt("interval").selected_text(name).width(150.0).show_ui(ui, |ui| {
                                for (st, label) in INTERVALS {
                                    ui.selectable_value(&mut e.semis, st, format!("{st:+} · {}", tr(label)));
                                }
                            });
                            ui.end_row();
                            ui.label(tr("Semitonos / cents"));
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut e.semis).range(-24.0..=24.0).speed(0.1).fixed_decimals(0));
                                ui.add(egui::DragValue::new(&mut e.cents).range(-100.0..=100.0).suffix(" ct"));
                            });
                            ui.end_row();
                            ui.label(tr("Duración"));
                            ui.add(egui::DragValue::new(&mut e.length_pct).range(25.0..=400.0).speed(0.2).suffix(" %"));
                            ui.end_row();
                            ui.label(tr("Tempo original"));
                            ui.label(format!("{bpm:.2} BPM"));
                            ui.end_row();
                            ui.label(tr("Tempo destino"));
                            let mut target = bpm as f64 * 100.0 / e.length_pct;
                            if ui.add(egui::DragValue::new(&mut target).range(20.0..=400.0).speed(0.1).fixed_decimals(2).suffix(" BPM")).changed() {
                                e.length_pct = bpm as f64 * 100.0 / target.max(1.0);
                            }
                            ui.end_row();
                            let out_len = (sel_len as f64 * e.length_pct / 100.0) as u64;
                            ui.label(tr("Muestras"));
                            let mut samples = out_len;
                            if ui.add(egui::DragValue::new(&mut samples).range(1..=u64::MAX).speed(100.0)).changed() {
                                e.length_pct = samples as f64 * 100.0 / sel_len.max(1) as f64;
                            }
                            ui.end_row();
                            ui.label("SMPTE");
                            ui.label(RichText::new(smpte(out_len, sr)).monospace());
                            ui.end_row();
                            ui.label(tr("Compases"));
                            let src_bars = sel_len as f64 * per_bar.recip() / engine::beat_len(sr, bpm, self.engine.unit.load(Relaxed));
                            let mut out_bars = src_bars * e.length_pct / 100.0;
                            if ui.add(egui::DragValue::new(&mut out_bars).range(0.01..=1024.0).speed(0.01).max_decimals(3)).changed() {
                                e.length_pct = out_bars * 100.0 / src_bars.max(1e-6);
                            }
                            ui.end_row();
                        });
                        e.length_pct = e.length_pct.clamp(25.0, 400.0);
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new(format!("▶ {}", tr("Previsualizar")))).clicked() {
                                preview_req = Some(tp);
                            }
                            if ui.button("■").on_hover_text(tr("Detener")).clicked() {
                                self.engine.preview.store(None);
                            }
                            let go = egui::Button::new(RichText::new(tr("Procesar y pegar")).strong().color(BG)).fill(METER[1]);
                            if ui.add(go).clicked() {
                                run = Some(tp);
                            }
                        });
                        ui.label(RichText::new(tr("Se aplica a la selección o, si no hay, a toda la región.")).size(11.0).color(TEXT_DIM));
                    });
                });
            });
        }
        self.aedit = Some(e);
        if let Some(op) = run {
            self.engine.preview.store(None);
            self.aedit_run(op);
        }
        if let Some(op) = preview_req {
            self.aedit_preview(op);
        }
        if let Some(f) = go_to {
            self.go(f);
        }
        if let Some((at, bpm)) = tempo_from_sel {
            self.edit();
            if let Some(seg) = self.s.tempo.iter_mut().rev().find(|s| s.0 <= at) {
                seg.1 = bpm.clamp(20.0, 400.0);
            }
            self.dirty = true;
            self.status = format!("{} {bpm:.2} BPM", tr("Tempo del proyecto:"));
        }
        if let Some(factor) = fit_loop {
            let ls = self.engine.loop_start.load(Relaxed);
            let r = self.aedit_apply(Process::TimePitch(factor, 0.0));
            if r.is_ok()
                && let Some((i, j, _)) = self.aedit_target()
            {
                self.s.tracks[i].clips[j].start = ls.saturating_sub(sa);
            }
            self.report(tr("Selección ajustada a los localizadores"), r);
        }
        if close {
            (self.aedit, self.bottom_tab) = (None, 0);
            self.engine.preview.store(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn processes() {
        let a: Vec<[f32; 2]> = (0..1000).map(|k| [0.25 + (k as f32 * 0.1).sin() * 0.1, -0.5]).collect();
        let dc = process(a.clone(), Process::RemoveDc);
        assert!(dc.iter().map(|s| s[0] + s[1]).sum::<f32>().abs() < 1.0);
        assert_eq!(process(a.clone(), Process::InvertPhase)[3], [-a[3][0], 0.5]);
        assert!((engine::peak(&process(a.clone(), Process::Normalize)) - 0.966).abs() < 1e-3);
        assert_eq!(process(a.clone(), Process::FadeIn)[0], [0.0, -0.0]);
        let longer = process(a, Process::TimePitch(2.0, 0.0)).len();
        assert!((1900..2100).contains(&longer), "{longer}");
    }
}
