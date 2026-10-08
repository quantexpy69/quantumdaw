//! Controles propios estilo consola: fader con escala en dB, medidores y perilla.
use crate::*;
use egui::{Align2, FontId, Painter, Rect, Response, Sense, Stroke, pos2, vec2};
use engine::{AtomicF32, fader_gain, fader_pos};

/// Preferencia "interfaz redondeada" (afecta a widgets de egui y a lo dibujado a mano).
pub static ROUND: AtomicBool = AtomicBool::new(true);
pub fn rr(r: f32) -> f32 {
    if ROUND.load(Relaxed) { r } else { r.min(2.0) }
}

pub fn db(gain: f32) -> f32 {
    20.0 * gain.max(1e-6).log10()
}
pub fn db_text(gain: f32) -> String {
    if gain <= 0.001 { "-∞".into() } else { format!("{:+.1}", db(gain)) }
}
fn gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Medidor vertical con la misma curva que el fader: verde < -12 dB < ámbar < -3 dB < rojo.
fn meter_v(p: &Painter, r: Rect, level: f32) {
    p.rect_filled(r, rr(2.0), BG);
    let y = |g: f32| r.bottom() - fader_pos(g) * r.height();
    for (lo, hi, c) in [(-60.0, -12.0, METER[0]), (-12.0, -3.0, METER[1]), (-3.0, 6.0, METER[2])] {
        if db(level) > lo {
            let top = y(level.min(gain(hi)));
            p.rect_filled(Rect::from_x_y_ranges(r.x_range(), top..=y(gain(lo))), rr(1.0), c);
        }
    }
}

/// Fader vertical con escala en dB y medidor. Arrastrar mueve, doble clic = 0 dB.
pub fn fader(ui: &mut egui::Ui, g: &AtomicF32, level: f32, h: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(64.0, h), Sense::click_and_drag());
    let track = Rect::from_center_size(pos2(rect.left() + 34.0, rect.center().y), vec2(4.0, h - 14.0));
    let y_of = |pos: f32| track.bottom() - pos * track.height();
    if resp.dragged() {
        g.set(fader_gain((fader_pos(g.get()) - resp.drag_delta().y / track.height()).clamp(0.0, 1.0)));
    }
    if resp.double_clicked() {
        g.set(1.0);
    }
    let p = ui.painter();
    // En faders bajos se omiten las marcas que no caben (mínimo 10 px entre números).
    let mut last = f32::MIN;
    for d in [6.0, 0.0, -6.0, -12.0, -18.0, -24.0, -36.0, -48.0] {
        let y = y_of(fader_pos(gain(d)));
        if y - last < 10.0 || track.bottom() - y < 10.0 {
            continue;
        }
        last = y;
        p.text(pos2(rect.left() + 18.0, y), Align2::RIGHT_CENTER, format!("{d}"), FontId::monospace(9.0), if d == 0.0 { TEXT } else { TEXT_DIM });
        p.hline(rect.left() + 20.0..=rect.left() + 25.0, y, Stroke::new(1.0, BORDER));
    }
    p.text(pos2(rect.left() + 18.0, track.bottom()), Align2::RIGHT_CENTER, "-∞", FontId::monospace(9.0), TEXT_DIM);
    p.rect_filled(track, rr(2.0), BG);
    meter_v(p, Rect::from_x_y_ranges(rect.right() - 12.0..=rect.right() - 4.0, track.y_range()), level);
    let cap = Rect::from_center_size(pos2(track.center().x, y_of(fader_pos(g.get()))), vec2(22.0, 13.0));
    p.rect_filled(cap, rr(4.0), if resp.dragged() || resp.hovered() { TEXT } else { Color32::from_gray(190) });
    p.hline(cap.shrink(4.0).x_range(), cap.center().y, Stroke::new(1.5, BG));
    resp.on_hover_text(tr("Volumen · doble clic: 0 dB"))
}

/// Medidor horizontal compacto con números en dB (entrada de la pista).
pub fn meter_h(ui: &mut egui::Ui, level: f32, w: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    let bar = Rect::from_min_size(rect.min + vec2(0.0, 3.0), vec2(w, 6.0));
    let x = |d: f32| bar.left() + ((d + 60.0) / 60.0).clamp(0.0, 1.0) * w;
    let p = ui.painter();
    p.rect_filled(bar, rr(3.0), BG);
    let ld = db(level);
    for (lo, hi, c) in [(-60.0, -12.0, METER[0]), (-12.0, -3.0, METER[1]), (-3.0, 0.0, METER[2])] {
        if ld > lo {
            p.rect_filled(Rect::from_x_y_ranges(x(lo)..=x(ld.min(hi)), bar.y_range()), rr(3.0), c);
        }
    }
    for d in [-48.0, -24.0, -12.0, -6.0, 0.0] {
        p.text(pos2(x(d), bar.bottom() + 1.0), Align2::CENTER_TOP, format!("{}", -d as i32), FontId::monospace(8.0), TEXT_DIM);
    }
    resp.on_hover_text(format!("Entrada: {} dB", db_text(level)))
}

/// Selector de color compacto: un círculo que abre la paleta al hacer clic.
pub fn color_dot(ui: &mut egui::Ui, color: &mut Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
    ui.painter().circle_filled(rect.center(), 6.5, *color);
    if resp.hovered() {
        ui.painter().circle_stroke(rect.center(), 7.5, Stroke::new(1.0, TEXT));
    }
    egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        egui::widgets::color_picker::color_picker_color32(ui, color, egui::widgets::color_picker::Alpha::Opaque);
    });
    resp.on_hover_text(tr("Color de la pista"))
}

/// Botón con icono dibujado (se ve nítido en cualquier escala); `on` lo resalta.
pub fn icon_button(ui: &mut egui::Ui, on: bool, draw: fn(&Painter, Rect, Color32)) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(38.0, 30.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(
        r,
        rr(10.0),
        if on {
            ACCENT
        } else if resp.hovered() {
            BORDER
        } else {
            ELEVATED
        },
    );
    draw(p, r.shrink2(vec2(11.0, 6.0)), if on { BG } else { TEXT });
    resp
}

/// Botón de grabar: círculo rojo; fondo rojo mientras graba.
pub fn record_button(ui: &mut egui::Ui, on: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(38.0, 30.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(
        r,
        rr(10.0),
        if on {
            METER[2]
        } else if resp.hovered() {
            BORDER
        } else {
            ELEVATED
        },
    );
    p.circle_filled(r.center(), 7.0, if on { TEXT } else { METER[2] });
    resp
}

pub fn draw_play(p: &Painter, r: Rect, c: Color32) {
    let k = r.center();
    p.add(egui::Shape::convex_polygon(vec![k + vec2(-5.0, -8.0), k + vec2(-5.0, 8.0), k + vec2(8.0, 0.0)], c, Stroke::NONE));
}

pub fn draw_pause(p: &Painter, r: Rect, c: Color32) {
    for dx in [-4.0, 4.0] {
        p.rect_filled(Rect::from_center_size(r.center() + vec2(dx, 0.0), vec2(4.0, 15.0)), 1.0, c);
    }
}

pub fn draw_stop(p: &Painter, r: Rect, c: Color32) {
    p.rect_filled(Rect::from_center_size(r.center(), vec2(13.0, 13.0)), rr(2.0), c);
}

pub fn draw_prev(p: &Painter, r: Rect, c: Color32) {
    let k = r.center();
    p.rect_filled(Rect::from_center_size(k + vec2(-7.0, 0.0), vec2(2.5, 14.0)), 1.0, c);
    p.add(egui::Shape::convex_polygon(vec![k + vec2(7.0, -7.0), k + vec2(7.0, 7.0), k + vec2(-5.0, 0.0)], c, Stroke::NONE));
}

pub fn draw_next(p: &Painter, r: Rect, c: Color32) {
    let k = r.center();
    p.rect_filled(Rect::from_center_size(k + vec2(7.0, 0.0), vec2(2.5, 14.0)), 1.0, c);
    p.add(egui::Shape::convex_polygon(vec![k + vec2(-7.0, -7.0), k + vec2(-7.0, 7.0), k + vec2(5.0, 0.0)], c, Stroke::NONE));
}

/// Loop: rectángulo redondeado abierto con punta de flecha.
pub fn draw_loop(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.8, c);
    let b = Rect::from_center_size(r.center(), vec2(18.0, 11.0));
    p.add(egui::Shape::line(
        vec![
            pos2(b.left() + 6.0, b.bottom()),
            pos2(b.left() + 2.0, b.bottom()),
            pos2(b.left(), b.bottom() - 2.0),
            pos2(b.left(), b.top() + 2.0),
            pos2(b.left() + 2.0, b.top()),
            pos2(b.right() - 2.0, b.top()),
            pos2(b.right(), b.top() + 2.0),
            pos2(b.right(), b.bottom() - 2.0),
            pos2(b.right() - 2.0, b.bottom()),
            pos2(b.center().x, b.bottom()),
        ],
        s,
    ));
    p.add(egui::Shape::convex_polygon(vec![pos2(b.left() + 5.0, b.bottom() - 3.5), pos2(b.left() + 5.0, b.bottom() + 3.5), pos2(b.left() + 1.0, b.bottom())], c, Stroke::NONE));
}

/// Metrónomo: cuerpo trapezoidal, péndulo y pesa.
/// Cronómetro con un toque (contador de tempo).
pub fn draw_tap(p: &Painter, r: Rect, c: Color32) {
    let k = r.center() + vec2(0.0, 1.5);
    p.circle_stroke(k, 7.0, Stroke::new(1.8, c));
    p.line_segment([k + vec2(0.0, -10.5), k + vec2(0.0, -8.0)], Stroke::new(2.5, c));
    p.line_segment([k, k + vec2(3.5, -4.0)], Stroke::new(1.8, c));
    p.circle_filled(k, 1.5, c);
}

pub fn draw_metronome(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.6, c);
    let (l, rt, t, b, cx) = (r.left(), r.right(), r.top(), r.bottom(), r.center().x);
    p.add(egui::Shape::closed_line(vec![pos2(l + 1.0, b), pos2(rt - 1.0, b), pos2(cx + 2.5, t), pos2(cx - 2.5, t)], s));
    p.line_segment([pos2(cx, b - 3.0), pos2(rt + 1.0, t + 2.0)], s);
    p.rect_filled(Rect::from_center_size(pos2(cx + 3.5, b - 7.5), vec2(4.0, 3.0)), 1.0, c);
}

/// Puntero de selección (flecha).
pub fn draw_pointer(p: &Painter, r: Rect, c: Color32) {
    let (l, t, b) = (r.left() + 3.0, r.top(), r.bottom());
    let pts = vec![pos2(l, t), pos2(l, b - 2.0), pos2(l + 4.0, b - 6.0), pos2(l + 7.0, b), pos2(l + 9.0, b - 1.0), pos2(l + 6.5, b - 7.0), pos2(l + 11.0, b - 7.5)];
    p.add(egui::Shape::closed_line(pts, Stroke::new(1.6, c)));
}

/// Lápiz en diagonal con su punta.
pub fn draw_pencil(p: &Painter, r: Rect, c: Color32) {
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    p.add(egui::Shape::convex_polygon(vec![pos2(l + 3.0, b - 6.0), pos2(rt - 5.0, t + 1.0), pos2(rt - 1.0, t + 5.0), pos2(l + 7.0, b - 2.0)], c, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![pos2(l + 3.0, b - 5.0), pos2(l + 6.0, b - 2.0), pos2(l, b + 1.0)], c, Stroke::NONE));
}

/// Tijeras: dos anillos y dos hojas cruzadas.
pub fn draw_scissors(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.6, c);
    let (a, b) = (pos2(r.left() + 3.0, r.bottom() - 3.0), pos2(r.right() - 3.0, r.bottom() - 3.0));
    p.circle_stroke(a, 2.8, s);
    p.circle_stroke(b, 2.8, s);
    p.line_segment([a + vec2(2.0, -2.0), pos2(r.right() - 1.0, r.top())], s);
    p.line_segment([b + vec2(-2.0, -2.0), pos2(r.left() + 1.0, r.top())], s);
}

/// Perilla de panorama: anillo de fondo, arco de color desde el centro, marcas L/C/R y disco con brillo.
pub fn pan_knob(ui: &mut egui::Ui, v: &mut f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(56.0, 48.0), Sense::click_and_drag());
    if resp.dragged() {
        *v = (*v - resp.drag_delta().y / 120.0).clamp(-1.0, 1.0);
    }
    if resp.double_clicked() {
        *v = 0.0;
    }
    let (c, r) = (rect.center() - vec2(0.0, 2.0), 16.0);
    let angle = |x: f32| (x * 135.0).to_radians();
    let at = |a: f32, rad: f32| c + vec2(a.sin(), -a.cos()) * rad;
    let arc = |a0: f32, a1: f32, rad: f32| -> Vec<egui::Pos2> { (0..=32).map(|i| at(a0 + (a1 - a0) * i as f32 / 32.0, rad)).collect() };
    let p = ui.painter();
    let hot = resp.hovered() || resp.dragged();
    p.add(egui::Shape::line(arc(angle(-1.0), angle(1.0), r + 4.0), Stroke::new(4.0, BG)));
    if v.abs() > 0.005 {
        p.add(egui::Shape::line(arc(0.0, angle(*v), r + 4.0), Stroke::new(4.0, ACCENT)));
    }
    for (x, label) in [(-1.0, "L"), (1.0, "R")] {
        p.text(at(angle(x), r + 10.0) + vec2(0.0, 4.0), Align2::CENTER_CENTER, tr(label), FontId::proportional(9.0), TEXT_DIM);
    }
    p.line_segment([at(0.0, r + 7.0), at(0.0, r + 10.0)], Stroke::new(1.5, TEXT_DIM));
    p.circle_filled(c, r, if hot { BORDER } else { ELEVATED });
    p.circle_filled(c - vec2(0.0, 3.0), r - 5.0, Color32::from_white_alpha(10));
    p.circle_stroke(c, r, Stroke::new(1.0, Color32::from_gray(90)));
    p.line_segment([at(angle(*v), 3.0), at(angle(*v), r - 2.5)], Stroke::new(2.5, TEXT));
    resp
}

/// Perilla de consola: marcas alrededor, pista oscura, arco de color, botón con brillo y puntero.
pub fn console_knob(ui: &mut egui::Ui, t: &mut f32, default: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(58.0, 58.0), Sense::click_and_drag());
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        *t = (*t - resp.drag_delta().y / 180.0 * fine).clamp(0.0, 1.0);
    }
    if resp.double_clicked() {
        *t = default;
    }
    let (c, r) = (rect.center(), 19.0);
    let angle = |x: f32| (-135.0 + 270.0 * x).to_radians();
    let at = |a: f32, rad: f32| c + vec2(a.sin(), -a.cos()) * rad;
    let arc = |a0: f32, a1: f32, rad: f32| -> Vec<egui::Pos2> { (0..=36).map(|i| at(a0 + (a1 - a0) * i as f32 / 36.0, rad)).collect() };
    let p = ui.painter();
    let hot = resp.hovered() || resp.dragged();
    for k in 0..=10 {
        let a = angle(k as f32 / 10.0);
        p.line_segment([at(a, r + 7.0), at(a, r + 10.0)], Stroke::new(if k % 5 == 0 { 1.6 } else { 1.0 }, TEXT_DIM));
    }
    p.add(egui::Shape::line(arc(angle(0.0), angle(1.0), r + 3.0), Stroke::new(4.0, BG)));
    p.add(egui::Shape::line(arc(angle(default), angle(*t), r + 3.0), Stroke::new(4.0, color)));
    if hot {
        p.circle_filled(c, r + 1.0, color.gamma_multiply(0.18));
    }
    p.circle_filled(c, r - 1.0, Color32::from_gray(48));
    p.circle_filled(c - vec2(0.0, 2.5), r - 6.0, Color32::from_gray(64));
    p.circle_stroke(c, r - 1.0, Stroke::new(1.0, Color32::from_gray(100)));
    p.line_segment([at(angle(*t), 4.0), at(angle(*t), r - 4.0)], Stroke::new(3.0, color));
    p.circle_filled(c, 2.5, color);
    resp.on_hover_text(tr("Arrastra arriba/abajo (Shift: ajuste fino) · doble clic: valor por defecto"))
}

/// Menú contextual que solo se cierra al hacer clic fuera (para menús con campos de texto).
pub fn edit_menu(resp: &Response, add: impl FnOnce(&mut egui::Ui)) {
    egui::Popup::context_menu(resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        menu_style(ui);
        add(ui);
    });
}

/// Menús contextuales con tipografía más grande y filas más altas.
pub fn menu_style(ui: &mut egui::Ui) {
    let st = ui.style_mut();
    for ts in [egui::TextStyle::Body, egui::TextStyle::Button] {
        st.text_styles.insert(ts, egui::FontId::proportional(15.0));
    }
    st.spacing.button_padding = vec2(8.0, 4.0);
    st.spacing.item_spacing.y = 5.0;
}
