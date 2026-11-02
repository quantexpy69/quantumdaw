//! Plugins LV2 en la interfaz: la carátula dibujada de cada plugin, la sección de la biblioteca
//! (recomendados y todos por marca) y el menú para añadirlos desde los dispositivos y el mixer.
use crate::{
    library::{LibItem, grupo, recortar},
    widgets::rr,
    *,
};
use egui::{Align2, FontId, Painter, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};
use engine::lv2::{self, Categoria, Info};
use std::f32::consts::{PI, TAU};

/// Los plugins de cabecera: cinco efectos y cinco instrumentos que no se repiten entre sí.
pub const RECOMENDADOS: [(&str, &str); 10] = [
    ("http://lsp-plug.in/plugins/lv2/compressor_stereo", "Compresor profesional con sidechain, rodilla y medidores"),
    ("http://gareus.org/oss/lv2/fil4#stereo", "EQ paramétrico de 6 bandas con filtros de corte y analizador"),
    ("https://github.com/michaelwillis/dragonfly-reverb", "Reverb de sala de conciertos, natural y envolvente"),
    ("http://guitarix.sourceforge.net/plugins/gx_amp_stereo#GUITARIX_ST", "Amplificador de guitarra a válvulas con pantalla y tono"),
    ("http://calf.sourceforge.net/plugins/VintageDelay", "Delay de cinta con repeticiones sincronizadas al tempo"),
    ("http://zynaddsubfx.sourceforge.net", "Sintetizador completo: pads, bajos, leads y cientos de sonidos"),
    ("http://synthv1.sourceforge.net/lv2", "Sintetizador analógico de dos osciladores, filtros y efectos"),
    ("http://gareus.org/oss/lv2/b_synth", "Órgano Hammond de ruedas fónicas con Leslie"),
    ("http://gareus.org/oss/lv2/avldrums#BlackPearl", "Batería acústica real: bombo, caja, toms y platillos"),
    ("http://calf.sourceforge.net/plugins/Monosynth", "Sintetizador monofónico para bajos y solos"),
];

/// Descripción corta: la de los recomendados o su tipo y marca.
pub fn descripcion(i: &Info) -> String {
    RECOMENDADOS.iter().find(|r| r.0 == i.uri).map_or_else(|| format!("{} · {}", tr(i.categoria.nombre()), i.marca), |r| tr(r.1).to_string())
}

fn recomendados(catalogo: &[Arc<Info>], instrumentos: bool) -> Vec<Arc<Info>> {
    RECOMENDADOS.iter().filter_map(|r| catalogo.iter().find(|i| i.uri == r.0 && i.instrumento == instrumentos)).cloned().collect()
}

pub fn color(c: Categoria) -> Color32 {
    let (r, g, b) = match c {
        Categoria::Reverb => (0xB0, 0x8C, 0xF2),
        Categoria::Delay => (0x5B, 0x9C, 0xFF),
        Categoria::Dinamica => (0xFF, 0xB0, 0x20),
        Categoria::Eq => (0x2E, 0xD8, 0xC3),
        Categoria::Distorsion => (0xFF, 0x7A, 0x45),
        Categoria::Amplificador => (0xE8, 0x5A, 0x3C),
        Categoria::Modulacion => (0x4F, 0xC3, 0xF7),
        Categoria::Afinacion => (0x3D, 0xDC, 0x84),
        Categoria::Sintetizador => (0xF2, 0x8C, 0xD0),
        Categoria::Bateria => (0xFF, 0x6B, 0x3D),
        Categoria::Organo => (0xE0, 0xB0, 0x4C),
        Categoria::Sampler => (0x9D, 0x7C, 0xFF),
        Categoria::Analisis => (0x9C, 0xD6, 0x4A),
        Categoria::Utilidad => (0x8A, 0x9B, 0xB8),
    };
    Color32::from_rgb(r, g, b)
}

fn trazo(p: &Painter, pts: Vec<Pos2>, ancho: f32, c: Color32) {
    p.add(egui::Shape::line(pts, Stroke::new(ancho, c)));
}

/// Curva `f(t)` (t y resultado en 0..1) dentro de `r`.
fn curva(p: &Painter, r: Rect, ancho: f32, c: Color32, f: impl Fn(f32) -> f32) {
    trazo(p, (0..=60).map(|k| k as f32 / 60.0).map(|t| pos2(r.left() + t * r.width(), r.bottom() - f(t).clamp(0.0, 1.0) * r.height())).collect(), ancho, c);
}

/// Dibujo del tipo de plugin en `r` (reverb, EQ, amplificador, teclado…).
pub fn ilustracion(p: &Painter, r: Rect, cat: Categoria, c: Color32) {
    let w = (r.height() / 40.0).clamp(1.2, 2.4);
    let tenue = c.gamma_multiply(0.35);
    match cat {
        Categoria::Reverb => {
            for k in 0..14 {
                let t = k as f32 / 13.0;
                let x = r.left() + t * r.width();
                let h = (-t * 3.2).exp() * (0.75 + 0.25 * ((k * 7 % 5) as f32 / 4.0));
                p.vline(x, r.bottom() - h * r.height()..=r.bottom(), Stroke::new(w, if k == 0 { c } else { tenue }));
            }
            curva(p, r, w, c, |t| (-t * 3.2).exp());
        }
        Categoria::Delay => {
            for k in 0..6 {
                let x = r.left() + (k as f32 + 0.5) / 6.0 * r.width();
                let h = 0.95 * 0.68f32.powi(k);
                p.vline(x, r.bottom() - h * r.height()..=r.bottom(), Stroke::new(w * 2.2, if k == 0 { c } else { c.gamma_multiply(0.85 - k as f32 * 0.1) }));
            }
        }
        Categoria::Dinamica => {
            p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.0, tenue));
            curva(p, r, w, c, |t| if t < 0.55 { t } else { 0.55 + (t - 0.55) * 0.25 });
            let gr = Rect::from_min_max(pos2(r.right() - r.width() * 0.08, r.top()), pos2(r.right(), r.top() + r.height() * 0.45));
            p.rect_filled(gr, rr(2.0), METER[1]);
        }
        Categoria::Eq => {
            p.hline(r.x_range(), r.center().y, Stroke::new(1.0, tenue));
            let f = |t: f32| 0.5 + 0.3 * (-((t - 0.25) * 9.0).powi(2)).exp() - 0.22 * (-((t - 0.62) * 11.0).powi(2)).exp() + 0.18 / (1.0 + (-(t - 0.9) * 25.0).exp());
            curva(p, r, w, c, f);
            for t in [0.25, 0.62, 0.9] {
                p.circle_filled(pos2(r.left() + t * r.width(), r.bottom() - f(t) * r.height()), w * 2.2, c);
            }
        }
        Categoria::Distorsion => {
            curva(p, r, 1.0, tenue, |t| 0.5 + 0.45 * (t * TAU * 1.5).sin());
            curva(p, r, w, c, |t| 0.5 + 0.45 * (3.0 * (t * TAU * 1.5).sin()).tanh());
        }
        Categoria::Amplificador => {
            // Cabezal con perillas y la rejilla del parlante debajo.
            let cab = Rect::from_min_max(r.min, pos2(r.right(), r.top() + r.height() * 0.32));
            p.rect(cab, rr(3.0), c.gamma_multiply(0.25), Stroke::new(w * 0.8, c), StrokeKind::Inside);
            for k in 0..5 {
                p.circle_filled(pos2(cab.left() + (k as f32 + 0.8) / 5.6 * cab.width(), cab.center().y), cab.height() * 0.2, c);
            }
            let rej = Rect::from_min_max(pos2(r.left(), cab.bottom() + r.height() * 0.06), r.max);
            p.rect_stroke(rej, rr(3.0), Stroke::new(w * 0.8, c), StrokeKind::Inside);
            p.circle_stroke(rej.center(), rej.height() * 0.36, Stroke::new(w, tenue));
            p.circle_filled(rej.center(), rej.height() * 0.12, tenue);
        }
        Categoria::Modulacion => {
            curva(p, r, w, c, |t| 0.5 + 0.38 * (t * TAU * 2.0).sin());
            curva(p, r, w, tenue, |t| 0.5 + 0.38 * (t * TAU * 2.0 + PI * 0.6).sin());
        }
        Categoria::Afinacion => {
            let centro = pos2(r.center().x, r.bottom());
            let radio = r.height().min(r.width() / 2.0) * 0.95;
            trazo(p, (0..=30).map(|k| PI + k as f32 / 30.0 * PI).map(|a| centro + vec2(a.cos(), a.sin()) * radio).collect(), w, tenue);
            p.line_segment([centro, centro + vec2(0.18, -1.0).normalized() * radio * 0.9], Stroke::new(w * 1.3, c));
            p.circle_filled(centro, w * 2.5, c);
        }
        Categoria::Sintetizador | Categoria::Organo | Categoria::Sampler => {
            // Arriba la onda (o los tiradores del órgano), abajo un teclado.
            let arriba = Rect::from_min_max(r.min, pos2(r.right(), r.top() + r.height() * 0.48));
            match cat {
                Categoria::Organo => {
                    for k in 0..9 {
                        let x = arriba.left() + (k as f32 + 0.5) / 9.0 * arriba.width();
                        let h = [0.9, 0.5, 0.8, 0.3, 0.6, 0.2, 0.7, 0.4, 0.5][k];
                        p.vline(x, arriba.top()..=arriba.top() + h * arriba.height(), Stroke::new(w * 2.0, if k < 2 { Color32::from_rgb(0x8B, 0x4A, 0x2B) } else { c }));
                    }
                }
                Categoria::Sampler => {
                    for k in 0..24 {
                        let x = arriba.left() + (k as f32 + 0.5) / 24.0 * arriba.width();
                        let h = (0.2 + 0.8 * ((k as f32 * 1.7).sin() * 0.5 + 0.5)) * (1.0 - k as f32 / 30.0);
                        p.vline(x, arriba.center().y - h * arriba.height() * 0.5..=arriba.center().y + h * arriba.height() * 0.5, Stroke::new(w, c));
                    }
                }
                _ => curva(p, arriba, w, c, |t| (t * 3.0).fract()),
            }
            let teclas = Rect::from_min_max(pos2(r.left(), r.top() + r.height() * 0.58), r.max);
            let n = 10;
            let ancho = teclas.width() / n as f32;
            for k in 0..n {
                let t = Rect::from_min_size(pos2(teclas.left() + k as f32 * ancho, teclas.top()), vec2(ancho - 1.0, teclas.height()));
                p.rect_filled(t, rr(1.5), Color32::from_gray(0xE8));
            }
            for k in [0, 1, 3, 4, 5, 7, 8] {
                let x = teclas.left() + (k as f32 + 1.0) * ancho - ancho * 0.3;
                p.rect_filled(Rect::from_min_size(pos2(x, teclas.top()), vec2(ancho * 0.6, teclas.height() * 0.6)), rr(1.0), Color32::from_gray(0x18));
            }
        }
        Categoria::Bateria => {
            // Bombo de frente, caja y platillo.
            let bombo = pos2(r.center().x, r.bottom() - r.height() * 0.38);
            p.circle_stroke(bombo, r.height() * 0.36, Stroke::new(w * 1.3, c));
            p.circle_filled(bombo, r.height() * 0.12, tenue);
            let caja = Rect::from_center_size(pos2(r.left() + r.width() * 0.16, r.top() + r.height() * 0.42), vec2(r.width() * 0.22, r.height() * 0.24));
            p.rect_stroke(caja, rr(2.0), Stroke::new(w, c), StrokeKind::Inside);
            let plato = pos2(r.right() - r.width() * 0.16, r.top() + r.height() * 0.18);
            p.line_segment([plato - vec2(r.width() * 0.16, -r.height() * 0.04), plato + vec2(r.width() * 0.16, r.height() * 0.04)], Stroke::new(w * 1.4, METER[1]));
            p.vline(plato.x, plato.y..=r.bottom(), Stroke::new(w * 0.7, tenue));
        }
        Categoria::Analisis => {
            for k in 0..16 {
                let x = r.left() + (k as f32 + 0.5) / 16.0 * r.width();
                let h = 0.15 + 0.8 * (1.0 - (k as f32 / 16.0 - 0.25).abs() * 1.3).max(0.0) * (0.7 + 0.3 * (k as f32 * 2.3).sin().abs());
                p.vline(x, r.bottom() - h * r.height()..=r.bottom(), Stroke::new(w * 1.8, c));
            }
        }
        Categoria::Utilidad => {
            for k in 0..4 {
                let x = r.left() + (k as f32 + 0.5) / 4.0 * r.width();
                p.vline(x, r.y_range(), Stroke::new(w, tenue));
                let y = r.top() + [0.3, 0.65, 0.45, 0.2][k] * r.height();
                p.rect_filled(Rect::from_center_size(pos2(x, y), vec2(r.width() * 0.12, r.height() * 0.1)), rr(2.0), c);
            }
        }
    }
}

/// Carátula del plugin: placa con su color, marca, nombre, el dibujo de su tipo y, si es grande,
/// sus primeras perillas en su valor de fábrica.
pub fn caratula(p: &Painter, r: Rect, i: &Info) {
    let c = color(i.categoria);
    let grande = r.height() >= 90.0;
    p.rect(r, rr(8.0), BG(), Stroke::new(1.0, c.gamma_multiply(0.6)), StrokeKind::Inside);
    p.rect_filled(r.shrink(1.0), rr(7.0), c.gamma_multiply(0.10));
    // Franja superior con la marca.
    let franja = Rect::from_min_size(r.min + vec2(1.0, 1.0), vec2(r.width() - 2.0, if grande { 18.0 } else { 3.0 }));
    p.rect_filled(franja, egui::CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, c.gamma_multiply(if grande { 0.35 } else { 0.9 }));
    if !grande {
        let dibujo = Rect::from_min_max(r.min + vec2(6.0, 8.0), r.max - vec2(6.0, 5.0));
        ilustracion(p, dibujo, i.categoria, c);
        return;
    }
    p.text(franja.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, i.marca.to_uppercase(), FontId::proportional(9.5), c);
    p.text(franja.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, tr(i.categoria.nombre()).to_uppercase(), FontId::proportional(9.0), TEXT_DIM());
    let cuerpo = Rect::from_min_max(pos2(r.left() + 10.0, franja.bottom() + 8.0), r.max - vec2(10.0, 8.0));
    let dibujo = Rect::from_min_max(pos2(cuerpo.right() - cuerpo.width() * 0.42, cuerpo.top()), pos2(cuerpo.right(), cuerpo.bottom() - if r.height() > 120.0 { 26.0 } else { 0.0 }));
    ilustracion(p, dibujo.shrink(2.0), i.categoria, c);
    let ancho_txt = dibujo.left() - cuerpo.left() - 8.0;
    p.text(cuerpo.left_top(), Align2::LEFT_TOP, recortar(p, &i.nombre, 15.0, ancho_txt), FontId::proportional(15.0), TEXT());
    let n = i.defs.len();
    let texto = if i.instrumento { format!("{} · {n} {}", tr("Instrumento"), tr("controles")) } else { format!("{n} {}", tr("controles")) };
    p.text(cuerpo.left_top() + vec2(0.0, 20.0), Align2::LEFT_TOP, texto, FontId::proportional(10.5), TEXT_DIM());
    // Perillas de muestra (las primeras del plugin) en su valor de fábrica.
    if r.height() > 120.0 {
        let radio = 9.0;
        let y = r.bottom() - 8.0 - radio;
        for (k, d) in i.defs.iter().take(((cuerpo.width() / 30.0) as usize).min(8)).enumerate() {
            let centro = pos2(cuerpo.left() + radio + k as f32 * 30.0, y);
            let t = ((d.3 - d.1) / (d.2 - d.1)).clamp(0.0, 1.0);
            let a = PI * 0.75 + t * PI * 1.5;
            p.circle_filled(centro, radio, ELEVATED());
            p.circle_stroke(centro, radio, Stroke::new(1.0, BORDER()));
            p.line_segment([centro, centro + vec2(a.cos(), a.sin()) * radio * 0.8], Stroke::new(2.0, c));
        }
    }
}

/// Vista previa al pasar el ratón: carátula grande y descripción.
pub fn vista_previa(ui: &mut egui::Ui, i: &Info) {
    ui.set_width(320.0);
    let (r, _) = ui.allocate_exact_size(vec2(320.0, 150.0), Sense::hover());
    caratula(ui.painter(), r, i);
    ui.label(RichText::new(descripcion(i)).size(12.5));
    let ayuda = if i.instrumento { "Clic: tocar en la pista MIDI elegida (o en una nueva) · arrastrar: sobre una pista MIDI" } else { "Clic: añadir a la pista elegida · arrastrar: a una pista o canal" };
    ui.label(RichText::new(tr(ayuda)).size(11.0).color(TEXT_DIM()));
}

impl App {
    /// Sección «Plugins» de la biblioteca: recomendados en tarjetas y el resto por marca.
    pub(crate) fn plugins_section(&mut self, ui: &mut egui::Ui) {
        let Some(catalogo) = lv2::listo() else {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(tr("Buscando los plugins instalados…")).color(TEXT_DIM()));
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        let (efectos, instrumentos) = (catalogo.iter().filter(|i| !i.instrumento).count(), catalogo.iter().filter(|i| i.instrumento).count());
        let nombres = [tr("Recomendados").to_string(), format!("{} {efectos}", tr("Efectos")), format!("{} {instrumentos}", tr("Instrumentos"))];
        self.chips(ui, &nombres);
        if catalogo.is_empty() {
            ui.label(RichText::new(tr("No se encontraron plugins LV2. Instala por ejemplo LSP, Calf o ZynAddSubFX y vuelve a abrir el programa.")).color(TEXT_DIM()));
            return;
        }
        match self.lib_filtro {
            0 => {
                for (titulo, instr) in [("EFECTOS", false), ("INSTRUMENTOS", true)] {
                    let lista = recomendados(catalogo, instr);
                    if !lista.is_empty() {
                        grupo(ui, titulo, lista.len());
                        self.tarjetas_plugins(ui, &lista);
                    }
                }
                ui.label(RichText::new(tr("Los demás plugins instalados están en Efectos e Instrumentos, agrupados por marca.")).size(11.0).color(TEXT_DIM()));
            }
            f => {
                let lista: Vec<Arc<Info>> = catalogo.iter().filter(|i| i.instrumento == (f == 2)).cloned().collect();
                self.plugins_por_marca(ui, &lista);
            }
        }
    }

    /// Plugins agrupados por marca, cada grupo plegable.
    pub(crate) fn plugins_por_marca(&mut self, ui: &mut egui::Ui, lista: &[Arc<Info>]) {
        let mut marcas: Vec<&str> = vec![];
        for i in lista {
            if !marcas.contains(&i.marca.as_str()) {
                marcas.push(&i.marca);
            }
        }
        for m in marcas {
            let grupo: Vec<Arc<Info>> = lista.iter().filter(|i| i.marca == m).cloned().collect();
            egui::CollapsingHeader::new(RichText::new(format!("{m}  {}", grupo.len())).strong()).id_salt(("marca", m, grupo[0].instrumento)).show(ui, |ui| self.filas_plugins(ui, &grupo));
        }
    }

    /// Tarjetas grandes con la carátula (dos por fila si entran).
    fn tarjetas_plugins(&mut self, ui: &mut egui::Ui, lista: &[Arc<Info>]) {
        let cols = ((ui.available_width() + 6.0) / 220.0).floor().max(1.0) as usize;
        let w = (ui.available_width() - 6.0 * (cols as f32 - 1.0)) / cols as f32;
        for fila in lista.chunks(cols) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for info in fila {
                    let (r0, resp) = ui.allocate_exact_size(vec2(w, 110.0), Sense::click_and_drag());
                    let hov = resp.hovered() || resp.dragged();
                    let r = if hov { r0.translate(vec2(0.0, -1.0)) } else { r0 };
                    ui.painter().rect_filled(r.translate(vec2(0.0, 3.0)), rr(8.0), Color32::from_black_alpha(if hov { 60 } else { 30 }));
                    caratula(ui.painter(), r, info);
                    self.interaccion_plugin(ui, resp, info);
                }
            });
        }
    }

    /// Filas compactas con una carátula pequeña.
    fn filas_plugins(&mut self, ui: &mut egui::Ui, lista: &[Arc<Info>]) {
        for info in lista {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click_and_drag());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(r, rr(6.0), ELEVATED());
            }
            caratula(p, Rect::from_min_size(r.min + vec2(4.0, 3.0), vec2(52.0, 34.0)), info);
            let x = r.left() + 64.0;
            p.text(pos2(x, r.top() + 13.0), Align2::LEFT_CENTER, recortar(p, &info.nombre, 13.0, r.right() - x - 8.0), FontId::proportional(13.0), TEXT());
            p.text(pos2(x, r.top() + 29.0), Align2::LEFT_CENTER, tr(info.categoria.nombre()), FontId::proportional(10.5), color(info.categoria));
            self.interaccion_plugin(ui, resp, info);
        }
    }

    /// Clic, arrastre y vista previa de un plugin de la biblioteca.
    fn interaccion_plugin(&mut self, ui: &mut egui::Ui, resp: egui::Response, info: &Arc<Info>) {
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp.dnd_set_drag_payload(LibItem::Plugin(info.clone()));
        if resp.clicked() {
            self.clic_plugin(info);
        }
        resp.on_hover_ui(|ui| vista_previa(ui, info));
    }

    /// Clic en un plugin: un efecto va a la pista elegida; un instrumento a la pista MIDI elegida
    /// o, si no hay, a una pista MIDI nueva con su nombre.
    pub(crate) fn clic_plugin(&mut self, info: &Arc<Info>) {
        if info.instrumento {
            let i = match self.s.tracks.iter().position(|t| t.selected && t.midi()) {
                Some(i) => i,
                None => {
                    self.add_track(info.nombre.clone(), TrackKind::Midi);
                    self.s.tracks.len() - 1
                }
            };
            self.set_instrumento_lv2(i, info);
        } else {
            let i = match self.pista_destino_fx() {
                Some(i) => i,
                None => {
                    self.add_track(tr("Audio").to_string(), TrackKind::AudioStereo);
                    self.s.tracks.len() - 1
                }
            };
            self.add_fx_lv2(i, info);
        }
    }

    /// Submenú con los efectos LV2 recomendados (para «+ efecto»); devuelve el elegido.
    pub(crate) fn menu_efectos_lv2(ui: &mut egui::Ui) -> Option<Arc<Info>> {
        let mut elegido = None;
        let lista = lv2::listo().map(|c| recomendados(c, false)).unwrap_or_default();
        if lista.is_empty() {
            return None;
        }
        ui.separator();
        ui.label(RichText::new(tr("PLUGINS")).size(10.0).color(TEXT_DIM()));
        for info in lista {
            let r = ui.button(&info.nombre).on_hover_ui(|ui| vista_previa(ui, &info));
            if r.clicked() {
                elegido = Some(info);
            }
        }
        elegido
    }
}

/// Carátula en el visor de un dispositivo (efecto o instrumento LV2).
pub fn visor(ui: &mut egui::Ui, i: &Info, alto: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), alto.max(60.0)), Sense::hover());
    caratula(ui.painter(), r, i);
}
