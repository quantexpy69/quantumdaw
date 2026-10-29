//! Controles propios estilo consola: fader con escala en dB, medidores y perilla.
use crate::*;
use egui::{Align2, FontId, Painter, Pos2, Rect, Response, Sense, Stroke, pos2, vec2};
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
    p.rect_filled(r, rr(2.0), BG());
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
        p.text(pos2(rect.left() + 18.0, y), Align2::RIGHT_CENTER, format!("{d}"), FontId::monospace(9.0), if d == 0.0 { TEXT() } else { TEXT_DIM() });
        p.hline(rect.left() + 20.0..=rect.left() + 25.0, y, Stroke::new(1.0, BORDER()));
    }
    p.text(pos2(rect.left() + 18.0, track.bottom()), Align2::RIGHT_CENTER, "-∞", FontId::monospace(9.0), TEXT_DIM());
    p.rect_filled(track, rr(2.0), BG());
    meter_v(p, Rect::from_x_y_ranges(rect.right() - 12.0..=rect.right() - 4.0, track.y_range()), level);
    let cap = Rect::from_center_size(pos2(track.center().x, y_of(fader_pos(g.get()))), vec2(22.0, 13.0));
    p.rect_filled(cap, rr(4.0), if resp.dragged() || resp.hovered() { TEXT() } else { Color32::from_gray(190) });
    p.hline(cap.shrink(4.0).x_range(), cap.center().y, Stroke::new(1.5, BG()));
    resp.on_hover_text(tr("Volumen · doble clic: 0 dB"))
}

/// Medidor horizontal compacto con números en dB (entrada de la pista).
pub fn meter_h(ui: &mut egui::Ui, level: f32, w: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
    let bar = Rect::from_min_size(rect.min + vec2(0.0, 3.0), vec2(w, 6.0));
    let x = |d: f32| bar.left() + ((d + 60.0) / 60.0).clamp(0.0, 1.0) * w;
    let p = ui.painter();
    p.rect_filled(bar, rr(3.0), BG());
    let ld = db(level);
    for (lo, hi, c) in [(-60.0, -12.0, METER[0]), (-12.0, -3.0, METER[1]), (-3.0, 0.0, METER[2])] {
        if ld > lo {
            p.rect_filled(Rect::from_x_y_ranges(x(lo)..=x(ld.min(hi)), bar.y_range()), rr(3.0), c);
        }
    }
    for d in [-48.0, -24.0, -12.0, -6.0, 0.0] {
        p.text(pos2(x(d), bar.bottom() + 1.0), Align2::CENTER_TOP, format!("{}", -d as i32), FontId::monospace(8.0), TEXT_DIM());
    }
    resp.on_hover_text(format!("Entrada: {} dB", db_text(level)))
}

/// Selector de color compacto: un círculo que abre la paleta al hacer clic.
pub fn color_dot(ui: &mut egui::Ui, color: &mut Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
    ui.painter().circle_filled(rect.center(), 6.5, *color);
    if resp.hovered() {
        ui.painter().circle_stroke(rect.center(), 7.5, Stroke::new(1.0, TEXT()));
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
            ACCENT()
        } else if resp.hovered() {
            BORDER()
        } else {
            ELEVATED()
        },
    );
    draw(p, r.shrink2(vec2(11.0, 6.0)), if on { BG() } else { TEXT() });
    resp
}

/// Botón de herramienta: icono y nombre, resaltado cuando está activa.
pub fn icon_text_button(ui: &mut egui::Ui, on: bool, draw: fn(&Painter, Rect, Color32), texto: &str) -> Response {
    let ink = if on { BG() } else { TEXT() };
    let galley = ui.painter().layout_no_wrap(texto.to_string(), egui::FontId::proportional(12.5), ink);
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 40.0, 30.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, rr(10.0), if on { ACCENT() } else if resp.hovered() { BORDER() } else { ELEVATED() });
    draw(p, Rect::from_center_size(r.left_center() + vec2(17.0, 0.0), vec2(16.0, 18.0)), ink);
    p.galley(pos2(r.left() + 30.0, r.center().y - galley.size().y / 2.0), galley, ink);
    resp
}

/// Botón de icono con color propio cuando está activo (play verde, loop naranja).
pub fn icon_button_color(ui: &mut egui::Ui, on: bool, draw: fn(&Painter, Rect, Color32), color: Color32) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(38.0, 30.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, rr(8.0), if on { color } else if resp.hovered() { BORDER() } else { ELEVATED() });
    draw(p, r.shrink2(vec2(11.0, 6.0)), if on { BG() } else { color });
    resp
}

/// Pestaña de vista en mayúsculas con subrayado del color de acento cuando está activa.
pub fn pestana(ui: &mut egui::Ui, activa: bool, texto: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(texto.to_uppercase(), egui::FontId::proportional(13.5), if activa { TEXT() } else { TEXT_DIM() });
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 18.0, 30.0), Sense::click());
    let color = if activa || resp.hovered() { TEXT() } else { TEXT_DIM() };
    let p = ui.painter();
    p.galley_with_override_text_color(pos2(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0 - 2.0), galley, color);
    if activa {
        p.rect_filled(Rect::from_center_size(pos2(r.center().x, r.bottom() - 2.0), vec2(18.0, 3.0)), rr(1.5), ACCENT());
    }
    resp
}

/// Fader horizontal fino (cabeceras de pista): -60 a +6 dB; doble clic vuelve a 0 dB. Con `nivel`, el
/// carril muestra además el medidor de la pista (verde, amarillo y rojo) bajo el tirador.
pub fn fader_h(ui: &mut egui::Ui, g: &AtomicF32, ancho: f32, nivel: Option<f32>) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ancho, 16.0), Sense::click_and_drag());
    let a_pos = |gain: f32| ((db(gain) + 60.0) / 66.0).clamp(0.0, 1.0);
    if resp.dragged()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let t = ((p.x - r.left()) / r.width()).clamp(0.0, 1.0);
        g.set(if t <= 0.0 { 0.0 } else { 10f32.powf((t * 66.0 - 60.0) / 20.0) });
    }
    if resp.double_clicked() {
        g.set(1.0);
    }
    let p = ui.painter();
    let carril = Rect::from_center_size(r.center(), vec2(r.width(), 6.0));
    p.rect_filled(carril, rr(3.0), BG());
    let x = r.left() + a_pos(g.get()) * r.width();
    p.rect_filled(Rect::from_min_max(carril.min, pos2(x, carril.bottom())), rr(3.0), BORDER());
    if let Some(nivel) = nivel.filter(|&n| n > 0.0) {
        let xn = |d: f32| r.left() + ((d + 60.0) / 66.0).clamp(0.0, 1.0) * r.width();
        let barra = carril.shrink2(vec2(0.0, 1.0));
        for (lo, hi, c) in [(-60.0, -12.0, METER[0]), (-12.0, -3.0, METER[1]), (-3.0, 6.0, METER[2])] {
            if db(nivel) > lo {
                p.rect_filled(Rect::from_x_y_ranges(xn(lo)..=xn(db(nivel).min(hi)), barra.y_range()), rr(2.0), c);
            }
        }
    }
    p.rect_filled(Rect::from_center_size(pos2(x, r.center().y), vec2(3.0, 14.0)), 1.0, if resp.hovered() || resp.dragged() { ACCENT() } else { TEXT() });
    resp.on_hover_text(format!("{} {}", tr("Volumen"), db_text(g.get())))
}

/// Pestaña de texto (biblioteca): clara y subrayada con el acento cuando está activa; se puede arrastrar.
pub fn pestana_texto(ui: &mut egui::Ui, activa: bool, texto: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(texto.to_string(), egui::FontId::proportional(13.0), TEXT());
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 14.0, 28.0), Sense::click_and_drag());
    let color = if activa || resp.hovered() { TEXT() } else { TEXT_DIM() };
    ui.painter().galley_with_override_text_color(pos2(r.center().x - galley.size().x / 2.0, r.center().y - galley.size().y / 2.0 - 2.0), galley, color);
    if activa {
        ui.painter().rect_filled(Rect::from_min_max(pos2(r.left() + 4.0, r.bottom() - 3.0), pos2(r.right() - 4.0, r.bottom() - 1.0)), 1.0, ACCENT());
    }
    resp
}

/// Pestaña tipo píldora (panel inferior): rellena de acento cuando está activa.
pub fn pildora(ui: &mut egui::Ui, activa: bool, texto: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(texto.to_string(), egui::FontId::proportional(13.5), if activa { BG() } else { TEXT() });
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + 26.0, 28.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(r, rr(7.0), if activa { ACCENT() } else if resp.hovered() { BORDER() } else { ELEVATED() });
    p.galley(r.center() - galley.size() / 2.0, galley, TEXT());
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
            BORDER()
        } else {
            ELEVATED()
        },
    );
    p.circle_filled(r.center(), 6.5, if on { TEXT() } else { METER[2] });
    resp
}

/// Caja común de los iconos: todos se dibujan en un cuadrado de 14 px centrado en el botón, para
/// que ninguno se vea más grande ni corrido que otro.
fn caja_icono(r: Rect) -> Rect {
    Rect::from_center_size(r.center(), vec2(14.0, 14.0))
}

pub fn draw_play(p: &Painter, r: Rect, c: Color32) {
    // Centrado óptico: el triángulo se corre un poco a la derecha.
    let b = caja_icono(r).translate(vec2(1.0, 0.0));
    p.add(egui::Shape::convex_polygon(vec![b.left_top() + vec2(1.0, 0.0), b.left_bottom() + vec2(1.0, 0.0), b.right_center()], c, Stroke::NONE));
}

pub fn draw_pause(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    for x in [b.left() + 2.5, b.right() - 2.5] {
        p.rect_filled(Rect::from_center_size(pos2(x, b.center().y), vec2(4.0, 14.0)), 1.0, c);
    }
}

pub fn draw_stop(p: &Painter, r: Rect, c: Color32) {
    p.rect_filled(caja_icono(r).shrink(1.0), rr(2.0), c);
}

pub fn draw_prev(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    p.rect_filled(Rect::from_min_size(b.left_top(), vec2(2.5, 14.0)), 1.0, c);
    p.add(egui::Shape::convex_polygon(vec![b.right_top(), b.right_bottom(), b.left_center() + vec2(3.0, 0.0)], c, Stroke::NONE));
}

pub fn draw_next(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    p.rect_filled(Rect::from_min_size(b.right_top() - vec2(2.5, 0.0), vec2(2.5, 14.0)), 1.0, c);
    p.add(egui::Shape::convex_polygon(vec![b.left_top(), b.left_bottom(), b.right_center() - vec2(3.0, 0.0)], c, Stroke::NONE));
}

/// Loop: rectángulo redondeado abierto con punta de flecha.
pub fn draw_loop(p: &Painter, r: Rect, c: Color32) {
    let s = Stroke::new(1.8, c);
    let b = Rect::from_center_size(r.center(), vec2(16.0, 10.0));
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

/// Cronómetro con un toque (contador de tempo).
pub fn draw_tap(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    let k = b.center() + vec2(0.0, 1.0);
    p.circle_stroke(k, 6.0, Stroke::new(1.7, c));
    p.line_segment([pos2(k.x, b.top() - 0.5), pos2(k.x, b.top() + 1.5)], Stroke::new(2.5, c));
    p.line_segment([k, k + vec2(3.0, -3.5)], Stroke::new(1.7, c));
    p.circle_filled(k, 1.4, c);
}

/// Metrónomo: cuerpo trapezoidal, péndulo y pesa.
pub fn draw_metronome(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    let s = Stroke::new(1.6, c);
    let (l, rt, t, bo, cx) = (b.left() + 1.0, b.right() - 1.0, b.top(), b.bottom(), b.center().x);
    p.add(egui::Shape::closed_line(vec![pos2(l, bo), pos2(rt, bo), pos2(cx + 2.5, t), pos2(cx - 2.5, t)], s));
    p.line_segment([pos2(cx, bo - 3.0), pos2(rt + 1.0, t + 2.0)], s);
    p.rect_filled(Rect::from_center_size(pos2(cx + 3.0, bo - 7.0), vec2(4.0, 3.0)), 1.0, c);
}

/// Puntero de selección (flecha).
pub fn draw_pointer(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    let (l, t, bo) = (b.left() + 2.0, b.top(), b.bottom());
    let pts = vec![pos2(l, t), pos2(l, bo - 2.0), pos2(l + 3.5, bo - 5.5), pos2(l + 6.5, bo), pos2(l + 8.5, bo - 1.0), pos2(l + 6.0, bo - 6.5), pos2(l + 10.5, bo - 7.0)];
    p.add(egui::Shape::closed_line(pts, Stroke::new(1.6, c)));
}

/// Lápiz en diagonal con su punta.
pub fn draw_pencil(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    let (l, t, rt, bo) = (b.left(), b.top(), b.right(), b.bottom());
    p.add(egui::Shape::convex_polygon(vec![pos2(l + 3.0, bo - 6.0), pos2(rt - 4.0, t), pos2(rt, t + 4.0), pos2(l + 6.0, bo - 3.0)], c, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(vec![pos2(l + 3.0, bo - 5.0), pos2(l + 5.0, bo - 3.0), pos2(l, bo)], c, Stroke::NONE));
}

/// Tijeras: dos anillos y dos hojas cruzadas.
pub fn draw_scissors(p: &Painter, r: Rect, c: Color32) {
    let b = caja_icono(r);
    let s = Stroke::new(1.6, c);
    let (a, d) = (pos2(b.left() + 3.0, b.bottom() - 3.0), pos2(b.right() - 3.0, b.bottom() - 3.0));
    p.circle_stroke(a, 2.8, s);
    p.circle_stroke(d, 2.8, s);
    p.line_segment([a + vec2(2.0, -2.0), pos2(b.right() - 1.0, b.top())], s);
    p.line_segment([d + vec2(-2.0, -2.0), pos2(b.left() + 1.0, b.top())], s);
}

/// Perilla de panorama: anillo de fondo, arco de color desde el centro, marcas L/C/R y disco con brillo.
pub fn pan_knob(ui: &mut egui::Ui, v: &mut f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(56.0, 48.0), Sense::click_and_drag());
    let antes = *v;
    if resp.dragged() {
        *v = (*v - resp.drag_delta().y / 120.0).clamp(-1.0, 1.0);
    }
    if resp.double_clicked() {
        *v = 0.0;
    }
    // Avisa el cambio: las cabeceras de pista y el inspector solo aplican el panorama si cambió.
    if *v != antes {
        resp.mark_changed();
    }
    let (c, r) = (rect.center() - vec2(0.0, 2.0), 16.0);
    let angle = |x: f32| (x * 135.0).to_radians();
    let at = |a: f32, rad: f32| c + vec2(a.sin(), -a.cos()) * rad;
    let arc = |a0: f32, a1: f32, rad: f32| -> Vec<egui::Pos2> { (0..=32).map(|i| at(a0 + (a1 - a0) * i as f32 / 32.0, rad)).collect() };
    let p = ui.painter();
    let hot = resp.hovered() || resp.dragged();
    p.add(egui::Shape::line(arc(angle(-1.0), angle(1.0), r + 4.0), Stroke::new(4.0, BG())));
    if v.abs() > 0.005 {
        p.add(egui::Shape::line(arc(0.0, angle(*v), r + 4.0), Stroke::new(4.0, ACCENT())));
    }
    for (x, label) in [(-1.0, "L"), (1.0, "R")] {
        p.text(at(angle(x), r + 10.0) + vec2(0.0, 4.0), Align2::CENTER_CENTER, tr(label), FontId::proportional(9.0), TEXT_DIM());
    }
    p.line_segment([at(0.0, r + 7.0), at(0.0, r + 10.0)], Stroke::new(1.5, TEXT_DIM()));
    perilla_3d(p, c, r, angle(*v), if hot { ACCENT() } else { TEXT() });
    resp
}

/// Perilla pequeña (cabeceras de pista): arrastre vertical (Shift: fino), doble clic = valor por defecto.
pub fn mini_knob(ui: &mut egui::Ui, v: &mut f32, min: f32, max: f32, default: f32, color: Color32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click_and_drag());
    let old = *v;
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        *v = (*v - resp.drag_delta().y * (max - min) / 150.0 * fine).clamp(min, max);
    }
    if resp.double_clicked() {
        *v = default;
    }
    if *v != old {
        resp.mark_changed();
    }
    let (c, r) = (rect.center(), 10.5);
    let angle = |x: f32| (-135.0 + 270.0 * (x - min) / (max - min)).to_radians();
    let at = |a: f32, rad: f32| c + vec2(a.sin(), -a.cos()) * rad;
    let arc = |a0: f32, a1: f32| -> Vec<egui::Pos2> { (0..=24).map(|i| at(a0 + (a1 - a0) * i as f32 / 24.0, r + 3.0)).collect() };
    let p = ui.painter();
    p.add(egui::Shape::line(arc(angle(min), angle(max)), Stroke::new(3.0, BG())));
    p.add(egui::Shape::line(arc(angle(default.clamp(min, max)), angle(*v)), Stroke::new(3.0, color)));
    perilla_3d(p, c, r, angle(*v), if resp.hovered() || resp.dragged() { color } else { TEXT() });
    resp
}

/// Perilla de consola: marcas alrededor, pista oscura, arco de color, botón con brillo y puntero.
/// Perilla de consola de cualquier tamaño (la grande central de Curves mide 120).
pub fn console_knob_size(ui: &mut egui::Ui, t: &mut f32, default: f32, color: Color32, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let k = size / 58.0;
    if resp.dragged() {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.2 } else { 1.0 };
        *t = (*t - resp.drag_delta().y / 180.0 * fine).clamp(0.0, 1.0);
    }
    if resp.double_clicked() {
        *t = default;
    }
    let (c, r) = (rect.center(), 19.0 * k);
    let angle = |x: f32| (-135.0 + 270.0 * x).to_radians();
    let at = |a: f32, rad: f32| c + vec2(a.sin(), -a.cos()) * rad;
    let arc = |a0: f32, a1: f32, rad: f32| -> Vec<egui::Pos2> { (0..=48).map(|i| at(a0 + (a1 - a0) * i as f32 / 48.0, rad)).collect() };
    let p = ui.painter();
    let hot = resp.hovered() || resp.dragged();
    if hot {
        p.circle_filled(c, r + 4.0 * k, color.gamma_multiply(0.14));
    }
    perilla_3d(p, c, r - k, angle(*t), color);
    for m in 0..=10 {
        let a = angle(m as f32 / 10.0);
        p.line_segment([at(a, r + 7.0 * k), at(a, r + 10.0 * k)], Stroke::new(if m % 5 == 0 { 1.6 } else { 1.0 } * k.sqrt(), TEXT_DIM()));
    }
    p.add(egui::Shape::line(arc(angle(0.0), angle(1.0), r + 3.5 * k), Stroke::new(3.0 * k, Color32::from_black_alpha(170))));
    p.add(egui::Shape::line(arc(angle(default), angle(*t), r + 3.5 * k), Stroke::new(3.0 * k, color)));
    resp.on_hover_text(tr("Arrastra arriba/abajo (Shift: ajuste fino) · doble clic: valor por defecto"))
}

/// Botón de perilla en relieve: sombra proyectada, faldón moleteado iluminado desde arriba, cuerpo
/// con degradé esférico y brillo, y el puntero del color del control. `a` es el ángulo del puntero.
pub fn perilla_3d(p: &Painter, c: Pos2, r: f32, a: f32, color: Color32) {
    use std::f32::consts::TAU;
    // Sombra proyectada hacia abajo.
    for k in 0..4 {
        p.circle_filled(c + vec2(0.0, r * 0.12 + k as f32 * 0.8), r + 1.0 - k as f32 * 0.4, Color32::from_black_alpha(34));
    }
    // Faldón moleteado: estrías más claras donde les da la luz (arriba a la izquierda).
    p.circle_filled(c, r, Color32::from_gray(20));
    let n = ((r * 2.2) as usize).clamp(20, 72);
    for i in 0..n {
        let ang = i as f32 / n as f32 * TAU;
        let d = vec2(ang.sin(), -ang.cos());
        let luz = (ang + 0.7).cos() * 0.5 + 0.5;
        p.line_segment([c + d * (r * 0.80), c + d * (r - 0.6)], Stroke::new(1.0, Color32::from_gray((34.0 + 70.0 * luz) as u8)));
    }
    // Cuerpo: círculos que se aclaran hacia el punto de luz dan el volumen de una esfera achatada.
    let cr = r * 0.78;
    let pasos = 12;
    for s in 0..pasos {
        let f = s as f32 / pasos as f32;
        p.circle_filled(c + vec2(-0.22, -0.30) * cr * f, cr * (1.0 - f * 0.82), Color32::from_gray((52.0 + 82.0 * f * f) as u8));
    }
    // Bisel: luz arriba y sombra abajo.
    let arco = |a0: f32, a1: f32| -> Vec<Pos2> { (0..=24).map(|i| a0 + (a1 - a0) * i as f32 / 24.0).map(|x| c + vec2(x.sin(), -x.cos()) * cr).collect() };
    p.add(egui::Shape::line(arco(-2.2, 0.9), Stroke::new(1.2, Color32::from_white_alpha(70))));
    p.add(egui::Shape::line(arco(1.4, 4.4), Stroke::new(1.2, Color32::from_black_alpha(140))));
    // Brillo especular.
    p.circle_filled(c + vec2(-0.30, -0.38) * cr, cr * 0.24, Color32::from_white_alpha(26));
    p.circle_filled(c + vec2(-0.30, -0.38) * cr, cr * 0.11, Color32::from_white_alpha(40));
    // Puntero grabado con su color y una sombra fina.
    let d = vec2(a.sin(), -a.cos());
    let ancho = (r * 0.11).max(1.8);
    p.line_segment([c + d * cr * 0.28 + vec2(0.6, 0.8), c + d * cr * 0.94 + vec2(0.6, 0.8)], Stroke::new(ancho, Color32::from_black_alpha(150)));
    p.line_segment([c + d * cr * 0.28, c + d * cr * 0.94], Stroke::new(ancho, color));
}

/// Chasis de un plugin: placa de metal cepillado con bisel, cuatro tornillos y sombra, detrás del
/// contenido (que va con margen para no tapar los tornillos).
pub fn chasis<R>(ui: &mut egui::Ui, acento: Color32, contenido: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let fondo = ui.painter().add(egui::Shape::Noop);
    let r = egui::Frame::new().inner_margin(egui::Margin { left: 24, right: 24, top: 16, bottom: 14 }).show(ui, contenido);
    let rect = r.response.rect;
    let radio = rr(12.0);
    let mut v = vec![];
    // Sombra y placa anodizada.
    for k in 1..=4 {
        v.push(egui::Shape::rect_filled(rect.translate(vec2(0.0, k as f32)).expand(k as f32 * 0.5), radio, Color32::from_black_alpha(30)));
    }
    v.push(egui::Shape::rect_filled(rect, radio, Color32::from_rgb(0x26, 0x26, 0x2B)));
    // Metal cepillado: líneas horizontales finas de brillo irregular (siempre igual para cada alto).
    let mut y = rect.top() + 3.0;
    let mut semilla = 0x9E37_79B9u32;
    while y < rect.bottom() - 3.0 {
        semilla ^= semilla << 13;
        semilla ^= semilla >> 17;
        semilla ^= semilla << 5;
        let a = 3 + (semilla % 7) as u8;
        v.push(egui::Shape::hline(rect.left() + radio * 0.6..=rect.right() - radio * 0.6, y, Stroke::new(1.0, Color32::from_white_alpha(a))));
        y += 2.0;
    }
    // Reflejo de luz en la parte de arriba.
    for k in 0..10 {
        let f = k as f32 / 10.0;
        v.push(egui::Shape::hline(rect.x_range().shrink(radio * 0.6), rect.top() + 2.0 + k as f32 * 3.0, Stroke::new(3.0, Color32::from_white_alpha((14.0 * (1.0 - f)) as u8))));
    }
    // Bisel: borde claro arriba, oscuro abajo, y una línea del color del plugin.
    v.push(egui::Shape::rect_stroke(rect, radio, Stroke::new(1.0, Color32::from_black_alpha(200)), egui::StrokeKind::Outside));
    v.push(egui::Shape::hline(rect.x_range().shrink(radio), rect.top() + 1.0, Stroke::new(1.0, Color32::from_white_alpha(60))));
    v.push(egui::Shape::hline(rect.x_range().shrink(radio), rect.bottom() - 1.0, Stroke::new(1.0, Color32::from_black_alpha(160))));
    v.push(egui::Shape::rect_filled(Rect::from_min_size(rect.left_top() + vec2(radio, 0.0), vec2(rect.width() - 2.0 * radio, 2.0)), 0.0, acento.gamma_multiply(0.8)));
    // Tornillos en las esquinas.
    for (k, esquina) in [rect.left_top() + vec2(11.0, 11.0), rect.right_top() + vec2(-11.0, 11.0), rect.left_bottom() + vec2(11.0, -11.0), rect.right_bottom() + vec2(-11.0, -11.0)].into_iter().enumerate() {
        v.push(egui::Shape::circle_filled(esquina + vec2(0.0, 1.0), 4.6, Color32::from_black_alpha(160)));
        for s in 0..5 {
            let f = s as f32 / 5.0;
            v.push(egui::Shape::circle_filled(esquina - vec2(0.8, 1.0) * f, 4.2 * (1.0 - f * 0.6), Color32::from_gray((95.0 + 90.0 * f) as u8)));
        }
        let ang = 0.5 + k as f32 * 1.1;
        let d = vec2(ang.cos(), ang.sin()) * 3.2;
        v.push(egui::Shape::line_segment([esquina - d, esquina + d], Stroke::new(1.3, Color32::from_gray(45))));
    }
    ui.painter().set(fondo, egui::Shape::Vec(v));
    r.inner
}

/// Chapa con el nombre del plugin: placa metálica clara con remaches y las letras grabadas.
pub fn chapa(ui: &mut egui::Ui, marca: &str, nombre: &str, acento: Color32) -> Response {
    let fuente = FontId::proportional(15.0);
    let g = ui.painter().layout_no_wrap(nombre.to_uppercase(), fuente.clone(), Color32::BLACK);
    let gm = ui.painter().layout_no_wrap(marca.to_uppercase(), FontId::proportional(8.5), Color32::BLACK);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x.max(gm.size().x) + 52.0, 34.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect.translate(vec2(0.0, 2.0)), rr(5.0), Color32::from_black_alpha(140));
    // Degradé vertical del aluminio.
    let bandas = 12;
    for k in 0..bandas {
        let f = k as f32 / bandas as f32;
        let gris = (206.0 - 70.0 * f + 22.0 * (f * 6.0).sin().abs() * (1.0 - f)) as u8;
        let alto = rect.height() / bandas as f32;
        let fila = Rect::from_min_size(pos2(rect.left(), rect.top() + k as f32 * alto), vec2(rect.width(), alto + 0.5));
        p.rect_filled(fila, if k == 0 || k == bandas - 1 { rr(5.0) } else { 0.0 }, Color32::from_gray(gris));
    }
    p.rect_stroke(rect, rr(5.0), Stroke::new(1.0, Color32::from_gray(70)), egui::StrokeKind::Inside);
    p.hline(rect.x_range().shrink(4.0), rect.top() + 1.5, Stroke::new(1.0, Color32::from_white_alpha(170)));
    // Remaches.
    for x in [rect.left() + 9.0, rect.right() - 9.0] {
        let c = pos2(x, rect.center().y);
        p.circle_filled(c + vec2(0.0, 0.8), 3.4, Color32::from_black_alpha(120));
        p.circle_filled(c, 3.0, Color32::from_gray(120));
        p.circle_filled(c - vec2(0.8, 0.8), 1.4, Color32::from_gray(230));
    }
    // Letras grabadas: luz debajo y tinta oscura encima; la marca, pequeña y del color del plugin.
    let centro = pos2(rect.center().x, rect.center().y + 4.0);
    p.galley_with_override_text_color(pos2(rect.center().x - gm.size().x / 2.0, rect.top() + 3.0), gm, acento.gamma_multiply(0.8).to_opaque());
    p.galley_with_override_text_color(centro - g.size() / 2.0 + vec2(0.0, 1.0), g.clone(), Color32::from_white_alpha(200));
    p.galley_with_override_text_color(centro - g.size() / 2.0, g, Color32::from_gray(36));
    resp
}

/// Botón de encendido con LED: verde encendido, apagado en bypass.
pub fn led(ui: &mut egui::Ui, on: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    let p = ui.painter();
    let c = rect.center();
    p.circle_filled(c + vec2(0.0, 1.0), 11.0, Color32::from_black_alpha(150));
    p.circle_filled(c, 10.5, Color32::from_gray(if resp.hovered() { 90 } else { 70 }));
    p.circle_filled(c, 8.0, Color32::from_gray(25));
    let verde = METER[0];
    if on {
        p.circle_filled(c, 9.5, verde.gamma_multiply(0.25));
        p.circle_filled(c, 6.0, verde);
    } else {
        p.circle_filled(c, 6.0, verde.gamma_multiply(0.15).to_opaque());
    }
    p.circle_filled(c - vec2(2.0, 2.2), 2.0, Color32::from_white_alpha(if on { 150 } else { 40 }));
    resp
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
        st.text_styles.insert(ts, egui::FontId::proportional(15.5));
    }
    st.spacing.button_padding = vec2(12.0, 6.0);
    st.spacing.item_spacing.y = 6.0;
    st.spacing.interact_size.y = 30.0;
}
