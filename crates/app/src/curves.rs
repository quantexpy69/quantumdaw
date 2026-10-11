//! QUANTUM Curves: visor interactivo (espectro en vivo, resonancias atenuadas, curva del EQ con nodos
//! arrastrables), presets por instrumento y detección automática de frecuencias molestas.
use crate::*;
use egui::{Align2, FontId, Mesh, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use engine::{BANDAS, curves_banda, curves_response, frecuencia_banda};
use rustfft::{FftPlanner, num_complex::Complex};

/// Presets: corte graves, graves dB/Hz, banda 1 dB/Hz/Q, banda 2 dB/Hz/Q, banda 3 dB/Hz/Q,
/// agudos dB/Hz, corte agudos, resonancias, sensibilidad, salida dB y mezcla.
pub const PRESETS: [(&str, [f32; 19]); 8] = [
    ("Plano", [20.0, 0.0, 120.0, 0.0, 300.0, 1.0, 0.0, 1200.0, 1.0, 0.0, 4000.0, 1.0, 0.0, 10000.0, 20000.0, 0.0, 0.5, 0.0, 1.0]),
    ("Batería", [30.0, 2.5, 80.0, -3.0, 400.0, 1.4, 1.5, 3500.0, 1.2, 2.0, 8000.0, 0.8, 2.5, 12000.0, 20000.0, 0.35, 0.5, 0.0, 1.0]),
    ("Guitarra acústica", [80.0, -1.5, 150.0, -2.5, 250.0, 1.6, 1.5, 3000.0, 1.0, 2.0, 10000.0, 0.8, 2.0, 12000.0, 20000.0, 0.45, 0.55, 0.0, 1.0]),
    ("Guitarra eléctrica", [90.0, 0.0, 120.0, -2.0, 300.0, 1.2, 2.0, 1800.0, 1.0, -2.5, 4500.0, 3.0, -1.0, 9000.0, 12000.0, 0.4, 0.5, 0.0, 1.0]),
    ("Guitarra eléctrica ambiente", [120.0, -2.0, 200.0, -2.0, 450.0, 1.0, 1.5, 2500.0, 0.8, 2.5, 7000.0, 0.7, 3.0, 11000.0, 18000.0, 0.3, 0.45, 0.0, 1.0]),
    ("Voz principal", [90.0, -1.0, 150.0, -2.5, 300.0, 1.2, 2.0, 3000.0, 1.0, -2.0, 6500.0, 4.0, 2.5, 12000.0, 20000.0, 0.6, 0.6, 0.0, 1.0]),
    ("Coros", [150.0, -3.0, 200.0, -3.0, 400.0, 1.0, 1.0, 2500.0, 1.0, -2.0, 6000.0, 3.0, 2.0, 11000.0, 16000.0, 0.5, 0.55, 0.0, 1.0]),
    ("Synth", [40.0, 1.5, 90.0, -2.0, 350.0, 1.2, 0.0, 1500.0, 1.0, 2.0, 5000.0, 0.9, 2.0, 12000.0, 20000.0, 0.3, 0.5, 0.0, 1.0]),
];

/// Nodos del visor: (nombre, parámetro de ganancia, de frecuencia y de Q).
const NODOS: [(&str, Option<usize>, usize, Option<usize>); 7] = [
    ("Corte graves", None, 0, None),
    ("Graves", Some(1), 2, None),
    ("Banda 1", Some(3), 4, Some(5)),
    ("Banda 2", Some(6), 7, Some(8)),
    ("Banda 3", Some(9), 10, Some(11)),
    ("Agudos", Some(12), 13, None),
    ("Corte agudos", None, 14, None),
];
const ROJO: Color32 = Color32::from_rgb(0xFF, 0x4D, 0x6D);
/// Color de cada banda (curva propia y nodo).
const COLORES: [Color32; 7] = [
    Color32::from_rgb(0x9A, 0xA4, 0xB8),
    Color32::from_rgb(0xFF, 0xB0, 0x20),
    Color32::from_rgb(0x3D, 0xDC, 0x84),
    Color32::from_rgb(0x2E, 0xD8, 0xC3),
    Color32::from_rgb(0x5B, 0x9C, 0xFF),
    Color32::from_rgb(0xB0, 0x8C, 0xF2),
    Color32::from_rgb(0x9A, 0xA4, 0xB8),
];

fn fijar(fx: &engine::Fx, k: usize, v: f32) {
    let (_, min, max, _) = FxKind::Curves.params()[k];
    fx.params[k].set(v.clamp(min, max));
}

/// Aplica un preset de la lista.
pub fn aplicar_preset(fx: &engine::Fx, i: usize) {
    for (k, v) in PRESETS[i].1.iter().enumerate() {
        fijar(fx, k, *v);
    }
}

/// Visor interactivo: espectro, resonancias atenuadas (rojo), curva del EQ y nodos arrastrables.
pub fn visor(ui: &mut egui::Ui, fx: &engine::Fx, sr: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 240.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, widgets::rr(10.0), Color32::from_rgb(0x0C, 0x0C, 0x10));
    let r = rect.shrink2(vec2(10.0, 14.0));
    let x_de = |f: f32| r.left() + ((f / 20.0).log10() / 3.0).clamp(0.0, 1.0) * r.width();
    let f_de = |x: f32| 20.0 * 1000f32.powf(((x - r.left()) / r.width()).clamp(0.0, 1.0));
    let y_eq = |db: f32| r.center().y - (db / 24.0).clamp(-1.0, 1.0) * r.height() / 2.0;
    let db_de = |y: f32| (r.center().y - y) / (r.height() / 2.0) * 24.0;
    let y_nivel = |db: f32| r.bottom() - ((db + 90.0) / 90.0).clamp(0.0, 1.0) * r.height();
    // Rejilla.
    for (f, t) in [(50.0, "50"), (100.0, "100"), (200.0, "200"), (500.0, "500"), (1000.0, "1k"), (2000.0, "2k"), (5000.0, "5k"), (10000.0, "10k")] {
        p.vline(x_de(f), r.y_range(), Stroke::new(1.0, Color32::from_white_alpha(10)));
        p.text(pos2(x_de(f) + 3.0, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, t, FontId::proportional(9.5), TEXT_DIM);
    }
    for db in [-18.0, -12.0, -6.0, 6.0, 12.0, 18.0] {
        p.hline(r.x_range(), y_eq(db), Stroke::new(1.0, Color32::from_white_alpha(6)));
        p.text(pos2(rect.left() + 2.0, y_eq(db)), Align2::LEFT_CENTER, format!("{db:+.0}"), FontId::proportional(8.5), TEXT_DIM.gamma_multiply(0.7));
    }
    p.hline(r.x_range(), y_eq(0.0), Stroke::new(1.0, Color32::from_white_alpha(22)));
    // Espectro en vivo (relleno) y reducción de resonancias (barras rojas desde la línea de 0 dB).
    let crudos: Vec<f32> = fx.espectro.iter().map(|a| a.get()).collect();
    // Interpola las bandas sin datos (graves) y suaviza con sus vecinas para un trazo limpio.
    let validas: Vec<usize> = (0..BANDAS).filter(|&b| crudos[b] > -119.0).collect();
    let interpolados: Vec<f32> = (0..BANDAS)
        .map(|b| match (validas.iter().rev().find(|&&v| v <= b), validas.iter().find(|&&v| v >= b)) {
            (Some(&a), Some(&z)) if a != z => crudos[a] + (crudos[z] - crudos[a]) * (b - a) as f32 / (z - a) as f32,
            (Some(&a), _) => crudos[a],
            (_, Some(&z)) => crudos[z],
            _ => -120.0,
        })
        .collect();
    let niveles: Vec<f32> = (0..BANDAS).map(|b| (interpolados[b.saturating_sub(1)] + 2.0 * interpolados[b] + interpolados[(b + 1).min(BANDAS - 1)]) / 4.0).collect();
    if niveles.iter().any(|&v| v > -100.0) {
        let puntos: Vec<Pos2> = (0..BANDAS).map(|b| pos2(x_de(frecuencia_banda(b)), y_nivel(niveles[b]))).collect();
        let mut malla = Mesh::default();
        for w in puntos.windows(2) {
            let (a, b) = (w[0], w[1]);
            let base = malla.vertices.len() as u32;
            for (pt, alfa) in [(a, 55), (b, 55), (pos2(b.x, r.bottom()), 6), (pos2(a.x, r.bottom()), 6)] {
                malla.colored_vertex(pt, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alfa));
            }
            malla.add_triangle(base, base + 1, base + 2);
            malla.add_triangle(base, base + 2, base + 3);
        }
        p.add(malla);
        p.add(egui::Shape::line(puntos, Stroke::new(1.2, color.gamma_multiply(0.8))));
    }
    for b in 0..BANDAS {
        let red = fx.reduccion[b].get();
        if red > 0.3 {
            let x = x_de(frecuencia_banda(b));
            let ancho = r.width() / BANDAS as f32 + 1.0;
            p.rect_filled(Rect::from_x_y_ranges(x - ancho / 2.0..=x + ancho / 2.0, y_eq(0.0)..=y_eq(-red)), 1.0, ROJO.gamma_multiply(0.75));
        }
    }
    // Curva de cada banda activa (relleno suave en su color) y curva total del EQ.
    let valores: Vec<f32> = fx.params.iter().map(|a| a.get()).collect();
    for (b, (_, ganancia, freq, _)) in NODOS.iter().enumerate() {
        let activa = ganancia.map_or(if b == 0 { valores[*freq] > 21.0 } else { valores[*freq] < 19_900.0 }, |k| valores[k].abs() > 0.05);
        if !activa {
            continue;
        }
        let col = COLORES[b];
        let pts: Vec<Pos2> = (0..=160).map(|k| r.left() + k as f32 / 160.0 * r.width()).map(|x| pos2(x, y_eq(curves_banda(&valores, b, f_de(x), sr)))).collect();
        let mut malla = Mesh::default();
        let cero = y_eq(0.0);
        for w in pts.windows(2) {
            let base = malla.vertices.len() as u32;
            let tinte = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 38);
            for pt in [w[0], w[1], pos2(w[1].x, cero), pos2(w[0].x, cero)] {
                malla.colored_vertex(pt, tinte);
            }
            malla.add_triangle(base, base + 1, base + 2);
            malla.add_triangle(base, base + 2, base + 3);
        }
        p.add(malla);
        p.add(egui::Shape::line(pts, Stroke::new(1.3, col.gamma_multiply(0.9))));
    }
    let curva: Vec<Pos2> = (0..=220).map(|k| r.left() + k as f32 / 220.0 * r.width()).map(|x| pos2(x, y_eq(curves_response(&valores, f_de(x), sr)))).collect();
    p.add(egui::Shape::line(curva, Stroke::new(2.5, color)));
    // Nodos arrastrables: horizontal = frecuencia, vertical = ganancia, rueda = Q, doble clic = 0 dB.
    for (n, (nombre, ganancia, freq, q)) in NODOS.iter().enumerate() {
        let f = valores[*freq];
        let g = ganancia.map_or(curves_response(&valores, f, sr) - valores[17], |k| valores[k]);
        let c = pos2(x_de(f), y_eq(g));
        let resp = ui.interact(Rect::from_center_size(c, vec2(22.0, 22.0)), egui::Id::new(("curves-nodo", fx as *const _ as usize, n)), Sense::click_and_drag());
        if resp.dragged()
            && let Some(m) = resp.interact_pointer_pos()
        {
            fijar(fx, *freq, f_de(m.x));
            if let Some(k) = ganancia {
                fijar(fx, *k, db_de(m.y));
            }
        }
        if resp.double_clicked()
            && let Some(k) = ganancia
        {
            fijar(fx, *k, 0.0);
        }
        if let Some(kq) = q
            && resp.hovered()
        {
            let rueda = ui.input(|i| i.smooth_scroll_delta.y);
            if rueda != 0.0 {
                fijar(fx, *kq, valores[*kq] * 1.003f32.powf(rueda));
            }
        }
        let activo = resp.hovered() || resp.dragged();
        let relleno = if ganancia.is_some_and(|k| valores[k].abs() > 0.05) || (*freq == 0 && f > 21.0) || (*freq == 14 && f < 19_900.0) { COLORES[n] } else { Color32::from_gray(70) };
        p.circle_filled(c, if activo { 10.0 } else { 8.0 }, relleno);
        p.circle_stroke(c, if activo { 10.0 } else { 8.0 }, Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.8)));
        p.text(c, Align2::CENTER_CENTER, (n + 1).to_string(), FontId::proportional(10.0), BG);
        let detalle = match (ganancia, q) {
            (Some(k), Some(kq)) => format!("{} · {:.0} Hz · {:+.1} dB · Q {:.1}", tr(nombre), f, valores[*k], valores[*kq]),
            (Some(k), None) => format!("{} · {:.0} Hz · {:+.1} dB", tr(nombre), f, valores[*k]),
            _ => format!("{} · {:.0} Hz", tr(nombre), f),
        };
        resp.on_hover_text(format!("{detalle}\n{}", tr("Arrastra: frecuencia y ganancia · rueda: ancho (Q) · doble clic: 0 dB")));
    }
    p.text(rect.right_top() + vec2(-10.0, 6.0), Align2::RIGHT_TOP, tr("rojo: resonancias atenuadas"), FontId::proportional(10.0), ROJO);
    p.rect_stroke(rect, widgets::rr(10.0), Stroke::new(1.0, BORDER), StrokeKind::Inside);
    ui.ctx().request_repaint();
}

/// Espectro promedio (dB) por banda de un audio, para la detección.
fn espectro_de(frames: &[[f32; 2]], sr: f32) -> [f32; BANDAS] {
    const N: usize = 4096;
    let fft = FftPlanner::new().plan_fft_forward(N);
    let mut suma = [0f64; BANDAS];
    let mut cuenta = [0u32; BANDAS];
    let ventana: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
    let mut buf = vec![Complex::default(); N];
    // Hasta 90 s de material, saltando bloques para que sea rápido.
    for bloque in frames.as_chunks::<N>().0.iter().step_by(2).take(600) {
        for (i, s) in bloque.iter().enumerate() {
            buf[i] = Complex::new((s[0] + s[1]) * 0.5 * ventana[i], 0.0);
        }
        fft.process(&mut buf);
        for (k, c) in buf.iter().enumerate().take(N / 2).skip(1) {
            let f = k as f32 * sr / N as f32;
            if (20.0..20000.0).contains(&f) {
                let b = (((f / 20.0).log10() / 3.0) * BANDAS as f32) as usize;
                suma[b] += c.norm_sqr() as f64;
                cuenta[b] += 1;
            }
        }
    }
    std::array::from_fn(|b| if cuenta[b] == 0 { -120.0 } else { 10.0 * ((suma[b] / cuenta[b] as f64).max(1e-12)).log10() as f32 })
}

/// Coloca las campanas sobre las frecuencias que más sobresalen del espectro y sugiere el corte de
/// graves; devuelve la descripción de lo detectado.
fn sugerir(fx: &engine::Fx, niveles: &[f32; BANDAS]) -> String {
    // Rellena huecos (bandas sin datos en graves) con la banda vecina.
    let mut n = *niveles;
    for b in 1..BANDAS {
        if n[b] <= -119.0 {
            n[b] = n[b - 1];
        }
    }
    let envolvente: Vec<f32> = (0..BANDAS).map(|b| {
        let (a, z) = (b.saturating_sub(5), (b + 6).min(BANDAS));
        n[a..z].iter().sum::<f32>() / (z - a) as f32
    }).collect();
    let exceso: Vec<f32> = (0..BANDAS).map(|b| n[b] - envolvente[b]).collect();
    let mut picos: Vec<usize> = (1..BANDAS - 1)
        .filter(|&b| (100.0..12_000.0).contains(&frecuencia_banda(b)) && exceso[b] > 1.5 && n[b] >= n[b - 1] && n[b] >= n[b + 1])
        .collect();
    picos.sort_by(|a, b| exceso[*b].total_cmp(&exceso[*a]));
    let mut elegidos: Vec<usize> = vec![];
    for b in picos {
        if elegidos.iter().all(|e| e.abs_diff(b) >= 6) {
            elegidos.push(b);
        }
        if elegidos.len() == 3 {
            break;
        }
    }
    elegidos.sort();
    for (i, &b) in elegidos.iter().enumerate() {
        let base = 3 + i * 3;
        fijar(fx, base, -(exceso[b] * 0.9).clamp(1.5, 9.0));
        fijar(fx, base + 1, frecuencia_banda(b));
        fijar(fx, base + 2, 5.0);
    }
    // Corte de graves: por debajo de donde la señal empieza a tener energía útil.
    let maximo = n.iter().copied().fold(-120f32, f32::max);
    if let Some(b) = (0..BANDAS).find(|&b| n[b] > maximo - 30.0) {
        fijar(fx, 0, (frecuencia_banda(b) * 0.7).clamp(20.0, 300.0));
    }
    if fx.params[15].get() < 0.2 {
        fijar(fx, 15, 0.4);
    }
    if elegidos.is_empty() {
        tr("No se encontraron frecuencias molestas destacadas").to_string()
    } else {
        let lista: Vec<String> = elegidos.iter().map(|&b| { let f = frecuencia_banda(b); if f >= 1000.0 { format!("{:.1} kHz", f / 1000.0) } else { format!("{f:.0} Hz") } }).collect();
        format!("{} {}", tr("Frecuencias molestas atenuadas:"), lista.join(", "))
    }
}

impl App {
    /// Detecta frecuencias molestas para el Curves `j` de la pista `i`: con lo que el plugin escuchó
    /// al reproducir o, si aún no sonó, con el audio de las regiones de la pista.
    pub fn curves_detectar(&mut self, i: usize, j: usize) -> String {
        let fx = self.s.tracks[i].fx[j].clone();
        let escuchado: [f32; BANDAS] = std::array::from_fn(|b| fx.promedio[b].get());
        if escuchado.iter().filter(|&&v| v > -100.0).count() > BANDAS / 2 {
            return sugerir(&fx, &escuchado);
        }
        let frames: Vec<[f32; 2]> = self.s.tracks[i].clips.iter().filter_map(|c| c.buf().map(|b| (b, c))).flat_map(|(b, c)| b.frames[c.offset as usize..((c.offset + c.len) as usize).min(b.frames.len())].iter().copied()).collect();
        if frames.len() < 8192 {
            return tr("Reproduce la canción unos segundos para que Curves escuche la pista").into();
        }
        sugerir(&fx, &espectro_de(&frames, self.engine.sample_rate as f32))
    }
}

/// Encabezado de Curves: presets, detección automática y pista clave del sidechain.
pub fn barra(ui: &mut egui::Ui, fx: &engine::Fx, pistas: &[(u64, String)], detectar: &mut bool, sincronizar: &mut bool) {
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Preset")).size(12.0).color(TEXT_DIM));
        egui::ComboBox::from_id_salt(("curves-preset", fx as *const _ as usize)).selected_text(tr("Elegir…")).width(170.0).show_ui(ui, |ui| {
            for (i, (nombre, _)) in PRESETS.iter().enumerate() {
                if ui.selectable_label(false, tr(nombre)).clicked() {
                    aplicar_preset(fx, i);
                }
            }
        });
        let boton = egui::Button::new(RichText::new(tr("Detectar frecuencias")).strong().color(BG)).fill(Color32::from_rgb(0xFF, 0x6B, 0x9A));
        if ui.add(boton).on_hover_text(tr("Analiza la pista y atenúa las frecuencias que sobresalen (resonancias molestas)")).clicked() {
            *detectar = true;
        }
        // Sidechain: otra pista (por ejemplo la voz) hace que esta le abra espacio en sus frecuencias.
        ui.label(RichText::new(tr("Sidechain")).size(12.0).color(TEXT_DIM));
        let actual = fx.sidechain.load(Relaxed);
        let nombre = pistas.iter().find(|p| p.0 == actual).map_or_else(|| tr("Ninguno").to_string(), |p| p.1.clone());
        egui::ComboBox::from_id_salt(("curves-sc", fx as *const _ as usize))
            .selected_text(nombre)
            .width(130.0)
            .show_ui(ui, |ui| {
                for (id, nombre) in std::iter::once((0, tr("Ninguno").to_string())).chain(pistas.iter().cloned()) {
                    if ui.selectable_label(actual == id, nombre).clicked() && actual != id {
                        fx.sidechain.store(id, Relaxed);
                        *sincronizar = true;
                    }
                }
            })
            .response
            .on_hover_text(tr("Cuando la pista elegida suena, Curves atenúa esta pista en esas mismas frecuencias para dejarle espacio"));
    });
}

/// Vúmetro analógico con aguja (nivel RMS en dB VU, -20 a +3).
fn vumetro(ui: &mut egui::Ui, id: egui::Id, etiqueta: &str, nivel: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(118.0, 78.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, widgets::rr(8.0), Color32::from_rgb(0xF2, 0xE6, 0xC8));
    p.rect_stroke(rect, widgets::rr(8.0), Stroke::new(2.0, Color32::from_gray(40)), StrokeKind::Inside);
    // 0 VU = -18 dBFS RMS; la aguja tiene inercia (≈ 300 ms) como un VU real.
    let vu = 20.0 * (nivel.max(1e-6)).log10() + 18.0;
    let objetivo = ((vu + 20.0) / 23.0).clamp(0.0, 1.0);
    let t = ui.ctx().animate_value_with_time(id, objetivo, 0.3);
    let centro = pos2(rect.center().x, rect.bottom() - 10.0);
    let radio = 56.0;
    let ang = |x: f32| (-50.0 + 100.0 * x).to_radians();
    let punto = |x: f32, r: f32| centro + vec2(ang(x).sin(), -ang(x).cos()) * r;
    for (db, marca) in [(-20.0, "20"), (-10.0, "10"), (-7.0, "7"), (-5.0, "5"), (-3.0, "3"), (0.0, "0"), (3.0, "+3")] {
        let x = (db + 20.0) / 23.0;
        let col = if db > 0.0 { Color32::from_rgb(0xC8, 0x2A, 0x2A) } else { Color32::from_gray(40) };
        p.line_segment([punto(x, radio - 8.0), punto(x, radio - 2.0)], Stroke::new(1.4, col));
        p.text(punto(x, radio + 4.0), Align2::CENTER_BOTTOM, marca, FontId::proportional(8.0), col);
    }
    p.add(egui::Shape::line((0..=30).map(|k| punto(20.0 / 23.0 + k as f32 / 30.0 * 3.0 / 23.0, radio - 5.0)).collect(), Stroke::new(3.0, Color32::from_rgb(0xC8, 0x2A, 0x2A))));
    p.line_segment([centro, punto(t, radio - 1.0)], Stroke::new(1.8, Color32::from_gray(25)));
    p.circle_filled(centro, 4.0, Color32::from_gray(30));
    p.text(pos2(rect.center().x, rect.bottom() - 22.0), Align2::CENTER_CENTER, "VU", FontId::proportional(10.0), Color32::from_gray(60));
    p.text(rect.left_top() + vec2(6.0, 4.0), Align2::LEFT_TOP, etiqueta, FontId::proportional(9.5), Color32::from_gray(60));
}

/// Controles esenciales: vúmetros de entrada y salida, perilla grande de intensidad en el centro y
/// sensibilidad, sidechain, salida y mezcla. Devuelve si hay que mostrar todos los controles.
pub fn controles(ui: &mut egui::Ui, fx: &engine::Fx, color: Color32) -> bool {
    let defs = FxKind::Curves.params();
    let clave = egui::Id::new(("curves-avanzado", fx as *const _ as usize));
    let mut avanzado = ui.data(|d| d.get_temp::<bool>(clave).unwrap_or(false));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let ancho = ui.available_width();
        // Centrado: vúmetro · 2 perillas · perilla grande · 2 perillas · vúmetro.
        let total = 118.0 * 2.0 + 84.0 * 4.0 + 146.0 + 6.0 * 12.0;
        ui.add_space(((ancho - total) / 2.0).max(0.0));
        vumetro(ui, clave.with("in"), "IN", fx.vu[0].get());
        ui.vertical(|ui| {
            plugins::perilla(ui, Some(FxKind::Curves), defs[16], &fx.params[16], 58.0);
        });
        ui.vertical(|ui| {
            plugins::perilla(ui, Some(FxKind::Curves), defs[19], &fx.params[19], 58.0);
        });
        ui.vertical(|ui| {
            ui.label(RichText::new(tr("INTENSIDAD")).size(11.0).strong().color(color));
            plugins::perilla(ui, Some(FxKind::Curves), defs[15], &fx.params[15], 120.0);
        });
        ui.vertical(|ui| {
            plugins::perilla(ui, Some(FxKind::Curves), defs[17], &fx.params[17], 58.0);
        });
        ui.vertical(|ui| {
            plugins::perilla(ui, Some(FxKind::Curves), defs[18], &fx.params[18], 58.0);
        });
        vumetro(ui, clave.with("out"), "OUT", fx.vu[1].get());
    });
    ui.horizontal(|ui| {
        if ui.selectable_label(avanzado, RichText::new(tr("Avanzado: todas las bandas")).size(12.0)).clicked() {
            avanzado = !avanzado;
            ui.data_mut(|d| d.insert_temp(clave, avanzado));
        }
    });
    avanzado
}
