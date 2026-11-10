//! QUANTUM EQ: visor interactivo del ecualizador paramétrico de 8 bandas. Espectro en vivo detrás,
//! curva de cada banda en su color y la curva total encima, con nodos que se arrastran (frecuencia y
//! ganancia), rueda del ratón para la Q, doble clic para añadir o quitar bandas y clic derecho para
//! el tipo y la pendiente. Debajo, los controles numéricos de la banda elegida.
//!
//! Las curvas se calculan con los mismos coeficientes que filtran el audio (`engine::eq_pro`): lo que
//! se ve es exactamente lo que suena.
use crate::*;
use egui::{Align2, FontId, Id, Mesh, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use engine::eq_pro::{self, BANDAS_EQ, P_ANALIZADOR, P_SALIDA, POR_BANDA, Tipo};
use engine::{BANDAS, frecuencia_banda};

/// Color de cada banda (nodo, curva propia y panel).
const COLORES: [Color32; BANDAS_EQ] = [
    Color32::from_rgb(0xFF, 0x5C, 0x5C),
    Color32::from_rgb(0xFF, 0x9F, 0x40),
    Color32::from_rgb(0xFF, 0xD6, 0x4A),
    Color32::from_rgb(0x6B, 0xE0, 0x6B),
    Color32::from_rgb(0x3F, 0xD8, 0xD0),
    Color32::from_rgb(0x4F, 0x9D, 0xFF),
    Color32::from_rgb(0x9B, 0x7B, 0xFF),
    Color32::from_rgb(0xF2, 0x6B, 0xD5),
];
const F_MIN: f32 = 20.0;
const F_MAX: f32 = 20_000.0;
/// Escalas del eje de ganancia (± dB).
const ESCALAS: [f32; 4] = [6.0, 12.0, 24.0, 30.0];

/// Banda elegida y escala del visor; se guardan en la memoria de egui, una por EQ.
#[derive(Clone, Copy)]
struct Vista {
    elegida: Option<usize>,
    escala: f32,
}

impl Default for Vista {
    fn default() -> Self {
        Self { elegida: None, escala: 24.0 }
    }
}

fn id_de(fx: &engine::Fx) -> Id {
    Id::new(("quantum-eq", fx as *const engine::Fx as usize))
}

fn vista(ui: &egui::Ui, fx: &engine::Fx) -> Vista {
    ui.data(|d| d.get_temp(id_de(fx))).unwrap_or_default()
}

fn guardar_vista(ui: &egui::Ui, fx: &engine::Fx, v: Vista) {
    ui.data_mut(|d| d.insert_temp(id_de(fx), v));
}

fn valor(fx: &engine::Fx, k: usize) -> f32 {
    fx.params.get(k).map_or(eq_pro::PARAMS[k].3, |a| a.get())
}

fn fijar(fx: &engine::Fx, k: usize, v: f32) {
    let (_, min, max, _) = eq_pro::PARAMS[k];
    if let Some(a) = fx.params.get(k) {
        a.set(v.clamp(min, max));
    }
}

fn valores(fx: &engine::Fx) -> Vec<f32> {
    (0..eq_pro::N_PARAMS).map(|k| valor(fx, k)).collect()
}

/// «1,25 kHz», «80 Hz».
pub fn hz(f: f32) -> String {
    if f >= 1000.0 { format!("{:.2} kHz", f / 1000.0).replace(".00 ", " ").replace('.', ",") } else { format!("{f:.0} Hz") }
}

/// Activa la primera banda libre en `f` con el tipo que corresponde a esa zona.
fn anadir_banda(fx: &engine::Fx, f: f32, db: f32) -> Option<usize> {
    let b = (0..BANDAS_EQ).find(|&b| valor(fx, b * POR_BANDA) < 0.5)?;
    let tipo = if f < 30.0 {
        Tipo::CorteGraves
    } else if f > 16_000.0 {
        Tipo::CorteAgudos
    } else {
        Tipo::Campana
    };
    let base = b * POR_BANDA;
    fijar(fx, base, 1.0);
    fijar(fx, base + 1, tipo.valor());
    fijar(fx, base + 2, f);
    fijar(fx, base + 3, if tipo.usa_ganancia() { db } else { 0.0 });
    fijar(fx, base + 4, if tipo.es_corte() { 0.707 } else { 1.0 });
    Some(b)
}

/// Escala, analizador y salida, sobre el visor.
pub fn barra(ui: &mut egui::Ui, fx: &engine::Fx) {
    let mut v = vista(ui, fx);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(tr("Escala")).size(11.0).color(TEXT_DIM()));
        for e in ESCALAS {
            if ui.selectable_label(v.escala == e, RichText::new(format!("±{e:.0}")).size(11.0)).clicked() {
                v.escala = e;
            }
        }
        ui.separator();
        ui.label(RichText::new(tr("Analizador")).size(11.0).color(TEXT_DIM()));
        let modo = valor(fx, P_ANALIZADOR).round() as i32;
        for (k, texto) in [(0, "Apagado"), (1, "Antes"), (2, "Después")] {
            if ui.selectable_label(modo == k, RichText::new(tr(texto)).size(11.0)).on_hover_text(tr("Espectro de la señal antes o después del EQ")).clicked() {
                fijar(fx, P_ANALIZADOR, k as f32);
                if k == 0 {
                    fx.espectro.iter().for_each(|a| a.set(-120.0));
                }
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut salida = valor(fx, P_SALIDA);
            let r = ui.add(egui::DragValue::new(&mut salida).range(-24.0..=24.0).speed(0.1).suffix(" dB").fixed_decimals(1)).on_hover_text(tr("Ganancia de salida (doble clic: 0 dB)"));
            if r.changed() {
                fijar(fx, P_SALIDA, salida);
            }
            if r.double_clicked() {
                fijar(fx, P_SALIDA, 0.0);
            }
            ui.label(RichText::new(tr("Salida")).size(11.0).color(TEXT_DIM()));
        });
    });
    guardar_vista(ui, fx, v);
}

/// Visor interactivo.
pub fn visor(ui: &mut egui::Ui, fx: &engine::Fx, sr: f32, color: Color32, alto: f32) {
    let mut v = vista(ui, fx);
    let (rect, fondo) = ui.allocate_exact_size(vec2(ui.available_width(), alto), Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, widgets::rr(10.0), Color32::from_rgb(0x0B, 0x0C, 0x11));
    let r = Rect::from_min_max(rect.min + vec2(30.0, 12.0), rect.max - vec2(30.0, 18.0));
    let escala = v.escala;
    let x_de = |f: f32| r.left() + ((f / F_MIN).log10() / (F_MAX / F_MIN).log10()).clamp(0.0, 1.0) * r.width();
    let f_de = |x: f32| F_MIN * (F_MAX / F_MIN).powf(((x - r.left()) / r.width()).clamp(0.0, 1.0));
    let y_eq = |db: f32| r.center().y - (db / escala).clamp(-1.05, 1.05) * r.height() / 2.0;
    let db_de = |y: f32| (r.center().y - y) / (r.height() / 2.0) * escala;
    // El espectro va de 0 dB (arriba) a −90 dB (abajo), con su propia escala a la derecha.
    let y_nivel = |db: f32| r.top() + (-db / 90.0).clamp(0.0, 1.0) * r.height();

    // Rejilla de frecuencias y ganancias.
    for f in [20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 200.0, 300.0, 400.0, 500.0, 600.0, 700.0, 800.0, 900.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0, 7000.0, 8000.0, 9000.0, 10_000.0, 20_000.0] {
        let fuerte = [100.0, 1000.0, 10_000.0].contains(&f);
        p.vline(x_de(f), r.y_range(), Stroke::new(1.0, Color32::from_white_alpha(if fuerte { 16 } else { 7 })));
    }
    for (f, t) in [(50.0, "50"), (100.0, "100"), (200.0, "200"), (500.0, "500"), (1000.0, "1k"), (2000.0, "2k"), (5000.0, "5k"), (10_000.0, "10k"), (20_000.0, "20k")] {
        p.text(pos2(x_de(f), rect.bottom() - 3.0), Align2::CENTER_BOTTOM, t, FontId::proportional(9.5), TEXT_DIM());
    }
    let paso = if escala <= 6.0 { 2.0 } else if escala <= 12.0 { 3.0 } else { 6.0 };
    let mut db = -escala;
    while db <= escala + 0.01 {
        let y = y_eq(db);
        p.hline(r.x_range(), y, Stroke::new(1.0, Color32::from_white_alpha(if db == 0.0 { 26 } else { 7 })));
        p.text(pos2(rect.left() + 4.0, y), Align2::LEFT_CENTER, format!("{db:+.0}"), FontId::proportional(9.0), if db == 0.0 { TEXT() } else { TEXT_DIM() });
        db += paso;
    }
    for db in [-20.0, -40.0, -60.0, -80.0] {
        p.text(pos2(rect.right() - 4.0, y_nivel(db)), Align2::RIGHT_CENTER, format!("{db:.0}"), FontId::proportional(8.5), TEXT_DIM().gamma_multiply(0.6));
    }

    // Espectro en vivo (relleno degradado y contorno), suavizado con las bandas vecinas.
    let analizador = valor(fx, P_ANALIZADOR).round() as i32 != 0;
    if analizador && fx.espectro.len() == BANDAS {
        let crudos: Vec<f32> = fx.espectro.iter().map(|a| a.get()).collect();
        let niveles: Vec<f32> = (0..BANDAS).map(|b| (crudos[b.saturating_sub(1)] + 2.0 * crudos[b] + crudos[(b + 1).min(BANDAS - 1)]) / 4.0).collect();
        if niveles.iter().any(|&n| n > -100.0) {
            let puntos: Vec<Pos2> = (0..BANDAS).map(|b| pos2(x_de(frecuencia_banda(b)), y_nivel(niveles[b]))).collect();
            let mut malla = Mesh::default();
            for w in puntos.windows(2) {
                let base = malla.vertices.len() as u32;
                for (pt, alfa) in [(w[0], 70), (w[1], 70), (pos2(w[1].x, r.bottom()), 8), (pos2(w[0].x, r.bottom()), 8)] {
                    malla.colored_vertex(pt, Color32::from_rgba_unmultiplied(0x9C, 0xB4, 0xE8, alfa));
                }
                malla.add_triangle(base, base + 1, base + 2);
                malla.add_triangle(base, base + 2, base + 3);
            }
            p.add(malla);
            p.add(egui::Shape::line(puntos, Stroke::new(1.0, Color32::from_rgba_unmultiplied(0xC8, 0xD6, 0xFF, 110))));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }

    // Curva de cada banda activa (relleno tenue) y la total encima.
    let vals = valores(fx);
    let n = (r.width() / 2.0).max(60.0) as usize;
    let xs: Vec<f32> = (0..=n).map(|k| r.left() + k as f32 / n as f32 * r.width()).collect();
    let bandas: Vec<eq_pro::Banda> = (0..BANDAS_EQ).map(|b| eq_pro::banda(&vals, b)).collect();
    let cero = y_eq(0.0);
    for b in (0..BANDAS_EQ).filter(|&b| bandas[b].on) {
        let col = COLORES[b];
        let elegida = v.elegida == Some(b);
        let pts: Vec<Pos2> = xs.iter().map(|&x| pos2(x, y_eq(eq_pro::respuesta_banda(&vals, b, f_de(x), sr)))).collect();
        let mut malla = Mesh::default();
        let tinte = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), if elegida { 60 } else { 30 });
        for w in pts.windows(2) {
            let base = malla.vertices.len() as u32;
            for pt in [w[0], w[1], pos2(w[1].x, cero), pos2(w[0].x, cero)] {
                malla.colored_vertex(pt, tinte);
            }
            malla.add_triangle(base, base + 1, base + 2);
            malla.add_triangle(base, base + 2, base + 3);
        }
        p.add(malla);
        p.add(egui::Shape::line(pts, Stroke::new(if elegida { 1.6 } else { 1.0 }, col.gamma_multiply(if elegida { 1.0 } else { 0.6 }))));
    }
    let total: Vec<Pos2> = xs.iter().map(|&x| pos2(x, y_eq(eq_pro::respuesta(&vals, f_de(x), sr)))).collect();
    p.add(egui::Shape::line(total, Stroke::new(2.2, Color32::from_rgb(0xF2, 0xF4, 0xFA))));

    // Nodos: arrastrar mueve frecuencia y ganancia; rueda, Q; doble clic quita; clic derecho, menú.
    let base_id = id_de(fx);
    let mut arrastrando = false;
    let mut quitar = None;
    for b in (0..BANDAS_EQ).filter(|&b| bandas[b].on) {
        let bd = bandas[b];
        let centro = pos2(x_de(bd.freq), if bd.tipo.usa_ganancia() { y_eq(bd.gain) } else { cero });
        let zona = Rect::from_center_size(centro, vec2(20.0, 20.0));
        let resp = ui.interact(zona, base_id.with(("nodo", b)), Sense::click_and_drag());
        let k = b * POR_BANDA;
        if resp.drag_started() || resp.clicked() || resp.secondary_clicked() {
            v.elegida = Some(b);
        }
        if resp.dragged()
            && let Some(pt) = ui.ctx().pointer_interact_pos()
        {
            arrastrando = true;
            // Con Shift solo se mueve la ganancia; con Ctrl, solo la frecuencia.
            let mods = ui.input(|i| i.modifiers);
            if !mods.shift {
                fijar(fx, k + 2, f_de(pt.x));
            }
            if bd.tipo.usa_ganancia() && !mods.command {
                fijar(fx, k + 3, db_de(pt.y));
            }
        }
        if resp.hovered() {
            let rueda = ui.input(|i| i.smooth_scroll_delta.y);
            if rueda != 0.0 {
                fijar(fx, k + 4, bd.q * (rueda * 0.004).exp());
                v.elegida = Some(b);
            }
        }
        if resp.double_clicked() {
            quitar = Some(b);
        }
        resp.context_menu(|ui| {
            widgets::menu_style(ui);
            ui.label(RichText::new(format!("{} {}", tr("Banda"), b + 1)).strong().color(COLORES[b]));
            for t in Tipo::TODOS {
                if ui.radio(bd.tipo == t, tr(t.nombre())).clicked() {
                    fijar(fx, k + 1, t.valor());
                    ui.close();
                }
            }
            if bd.tipo.es_corte() {
                ui.separator();
                for (s, texto) in ["12 dB/oct", "24 dB/oct", "36 dB/oct", "48 dB/oct"].iter().enumerate() {
                    if ui.radio(bd.pendiente == s, *texto).clicked() {
                        fijar(fx, k + 5, s as f32);
                        ui.close();
                    }
                }
            }
            ui.separator();
            if ui.button(tr("Quitar banda")).clicked() {
                quitar = Some(b);
                ui.close();
            }
        });
        let elegida = v.elegida == Some(b);
        let caliente = resp.hovered() || resp.dragged() || elegida;
        let col = COLORES[b];
        p.circle_filled(centro, if caliente { 8.0 } else { 6.5 }, col.gamma_multiply(if caliente { 1.0 } else { 0.85 }));
        p.circle_stroke(centro, if caliente { 8.0 } else { 6.5 }, Stroke::new(if elegida { 2.0 } else { 1.0 }, Color32::WHITE.gamma_multiply(if elegida { 1.0 } else { 0.5 })));
        p.text(centro, Align2::CENTER_CENTER, (b + 1).to_string(), FontId::proportional(9.0), Color32::BLACK);
        if resp.hovered() || resp.dragged() {
            let mut texto = format!("{} · {}", tr(bd.tipo.nombre()), hz(bd.freq));
            if bd.tipo.usa_ganancia() {
                texto += &format!(" · {:+.1} dB", bd.gain);
            }
            texto += &if bd.tipo.es_corte() { format!(" · {} dB/oct", 12 * (bd.pendiente + 1)) } else { format!(" · Q {:.2}", bd.q) };
            etiqueta(&p, centro + vec2(0.0, -16.0), &texto, col, r);
        }
    }
    if let Some(b) = quitar {
        fijar(fx, b * POR_BANDA, 0.0);
        if v.elegida == Some(b) {
            v.elegida = None;
        }
    }

    // Fondo: doble clic añade una banda donde se hizo; clic, deja de elegir.
    if fondo.double_clicked()
        && !arrastrando
        && quitar.is_none()
        && let Some(pt) = fondo.interact_pointer_pos()
        && r.contains(pt)
    {
        v.elegida = anadir_banda(fx, f_de(pt.x), db_de(pt.y));
    } else if fondo.clicked() && !arrastrando {
        v.elegida = None;
    }
    // Frecuencia y ganancia bajo el puntero.
    if let Some(pt) = fondo.hover_pos().filter(|pt| r.contains(*pt)) {
        p.text(pos2(r.right() - 4.0, r.top() + 2.0), Align2::RIGHT_TOP, format!("{}  {:+.1} dB", hz(f_de(pt.x)), db_de(pt.y)), FontId::proportional(10.5), TEXT_DIM());
    }
    if !bandas.iter().any(|b| b.on) {
        p.text(r.center() - vec2(0.0, 22.0), Align2::CENTER_CENTER, tr("Doble clic en la curva para añadir una banda"), FontId::proportional(13.0), TEXT_DIM());
    }
    let _ = color;
    guardar_vista(ui, fx, v);
}

/// Etiqueta con fondo, dentro del visor.
fn etiqueta(p: &egui::Painter, pos: Pos2, texto: &str, color: Color32, limite: Rect) {
    let g = p.layout_no_wrap(texto.to_string(), FontId::proportional(10.5), Color32::WHITE);
    let mut caja = Align2::CENTER_BOTTOM.anchor_size(pos, g.size()).expand2(vec2(6.0, 3.0));
    // Que no se salga por los lados.
    let dx = (limite.left() - caja.left()).max(0.0) + (limite.right() - caja.right()).min(0.0);
    caja = caja.translate(vec2(dx, 0.0));
    p.rect(caja, widgets::rr(4.0), Color32::from_black_alpha(210), Stroke::new(1.0, color), StrokeKind::Inside);
    p.galley(caja.min + vec2(6.0, 3.0), g, Color32::WHITE);
}

/// Controles de la banda elegida: tipo, frecuencia, ganancia, Q, pendiente y quitar.
pub fn panel_banda(ui: &mut egui::Ui, fx: &engine::Fx, accent: Color32) {
    let mut v = vista(ui, fx);
    ui.add_space(6.0);
    // Selector de bandas: un botón por banda (las apagadas, tenues).
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for b in 0..BANDAS_EQ {
            let on = valor(fx, b * POR_BANDA) >= 0.5;
            let elegida = v.elegida == Some(b);
            let col = COLORES[b];
            let boton = egui::Button::new(RichText::new((b + 1).to_string()).strong().size(11.0).color(if on { Color32::BLACK } else { TEXT_DIM() }))
                .fill(if on { col.gamma_multiply(if elegida { 1.0 } else { 0.7 }) } else { Color32::from_white_alpha(8) })
                .stroke(Stroke::new(if elegida { 2.0 } else { 0.0 }, Color32::WHITE))
                .min_size(vec2(26.0, 22.0))
                .corner_radius(widgets::rr(5.0));
            if ui.add(boton).on_hover_text(if on { tr("Elegir esta banda") } else { tr("Banda apagada: clic para elegirla y encenderla") }).clicked() {
                v.elegida = Some(b);
                if !on {
                    fijar(fx, b * POR_BANDA, 1.0);
                }
            }
        }
    });
    ui.add_space(4.0);
    let Some(b) = v.elegida else {
        ui.label(RichText::new(tr("Arrastra los puntos para mover frecuencia y ganancia · rueda del ratón: Q · Shift: solo ganancia · Ctrl: solo frecuencia · doble clic: añadir o quitar · clic derecho: tipo")).size(11.0).color(TEXT_DIM()));
        guardar_vista(ui, fx, v);
        return;
    };
    let k = b * POR_BANDA;
    let bd = eq_pro::banda(&valores(fx), b);
    egui::Frame::new().fill(COLORES[b].gamma_multiply(0.08)).stroke(Stroke::new(1.0, COLORES[b].gamma_multiply(0.5))).corner_radius(widgets::rr(8.0)).inner_margin(8.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
            let mut on = bd.on;
            if ui.checkbox(&mut on, RichText::new(format!("{} {}", tr("Banda"), b + 1)).strong().color(COLORES[b])).changed() {
                fijar(fx, k, if on { 1.0 } else { 0.0 });
            }
            egui::ComboBox::from_id_salt(("eq-tipo", id_de(fx), b)).width(140.0).selected_text(tr(bd.tipo.nombre())).show_ui(ui, |ui| {
                for t in Tipo::TODOS {
                    if ui.selectable_label(bd.tipo == t, tr(t.nombre())).clicked() {
                        fijar(fx, k + 1, t.valor());
                        fijar(fx, k, 1.0);
                    }
                }
            });
            let campo = |ui: &mut egui::Ui, nombre: &str, idx: usize, velocidad: f64, sufijo: &str, decimales: usize| {
                ui.label(RichText::new(tr(nombre)).size(11.0).color(TEXT_DIM()));
                let (_, min, max, defecto) = eq_pro::PARAMS[idx];
                let mut x = valor(fx, idx);
                let r = ui.add(egui::DragValue::new(&mut x).range(min..=max).speed(velocidad).suffix(sufijo).fixed_decimals(decimales));
                if r.changed() {
                    fijar(fx, idx, x);
                }
                if r.double_clicked() {
                    fijar(fx, idx, defecto);
                }
            };
            // La frecuencia se arrastra en proporción a su valor (igual de fino en graves y agudos).
            ui.label(RichText::new(tr("Frecuencia")).size(11.0).color(TEXT_DIM()));
            let mut f = bd.freq;
            let velocidad = (bd.freq * 0.004) as f64;
            let r = ui.add(egui::DragValue::new(&mut f).range(10.0..=22_000.0).speed(velocidad).custom_formatter(|v, _| hz(v as f32)).custom_parser(|s| {
                let s = s.trim().to_lowercase().replace(',', ".");
                let (num, mult) = if let Some(x) = s.strip_suffix("khz").or_else(|| s.strip_suffix('k')) { (x.trim().to_string(), 1000.0) } else { (s.trim_end_matches("hz").trim().to_string(), 1.0) };
                num.parse::<f64>().ok().map(|v| v * mult)
            }));
            if r.changed() {
                fijar(fx, k + 2, f);
            }
            if bd.tipo.usa_ganancia() {
                campo(ui, "Ganancia", k + 3, 0.05, " dB", 1);
            }
            campo(ui, "Q", k + 4, 0.01, "", 2);
            if bd.tipo.es_corte() {
                ui.label(RichText::new(tr("Pendiente")).size(11.0).color(TEXT_DIM()));
                for (s, texto) in ["12", "24", "36", "48"].iter().enumerate() {
                    if ui.selectable_label(bd.pendiente == s, format!("{texto} dB/oct")).clicked() {
                        fijar(fx, k + 5, s as f32);
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(RichText::new(tr("Quitar")).color(accent)).clicked() {
                    fijar(fx, k, 0.0);
                    v.elegida = None;
                }
            });
        });
    });
    guardar_vista(ui, fx, v);
}
