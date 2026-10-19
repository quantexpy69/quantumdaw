//! Menús circulares («pie»): se abren bajo el puntero con el clic derecho sobre una región o una
//! nota, o manteniendo Q. Un disco central con el título y sectores alrededor; se elige señalando
//! (con Q, al soltar la tecla sobre un sector) o con clic.
use crate::*;
use egui::{Align2, FontId, Pos2, Shape, Stroke, vec2};

#[derive(Clone, Copy, PartialEq)]
pub enum Accion {
    // Regiones.
    Dividir,
    Duplicar,
    Normalizar,
    Invertir,
    QuitarSilencios,
    Transientes,
    Abrir,
    Eliminar,
    MasOpciones,
    // Notas.
    Cuantizar,
    OctavaArriba,
    OctavaAbajo,
    Arpegiar,
    Rasguear,
    GenerarAcordes,
    BorrarNotas,
    DuplicarNotas,
    // Partitura.
    PtFigura(u8),
    PtPuntillo,
    PtSilencio,
    PtSubir,
    PtBajar,
    PtBorrar,
    PtEscuchar,
    PtMenuClave,
    PtMenuArmadura,
    PtClave(project::partitura::Clave),
    PtArmadura(i8),
    PtAnotacion,
}

pub struct Pie {
    pub centro: Pos2,
    pub titulo: &'static str,
    pub items: Vec<(&'static str, Accion, bool)>,
    /// Abierto con la tecla Q: se elige al soltarla.
    pub con_tecla: bool,
    /// Región sobre la que se abrió (para las acciones de región).
    pub region: Option<(usize, usize)>,
    /// Abierto en QUANTUM Partitura (se dibuja en su ventana, que puede estar en otra pantalla).
    pub en_partitura: bool,
    /// Arrastrando el menú desde su disco central.
    pub arrastrando: bool,
}

const R_EXT: f32 = 178.0;
const R_INT: f32 = 62.0;

/// Pinta un icono simple para cada acción (líneas, sin depender de la fuente).
fn icono(p: &egui::Painter, c: Pos2, a: Accion, color: Color32) {
    let s = Stroke::new(1.6, color);
    match a {
        Accion::Dividir | Accion::Transientes => {
            p.line_segment([c + vec2(0.0, -7.0), c + vec2(0.0, 7.0)], s);
            p.line_segment([c + vec2(-6.0, -3.0), c + vec2(-6.0, 3.0)], s);
            p.line_segment([c + vec2(6.0, -3.0), c + vec2(6.0, 3.0)], s);
        }
        Accion::Duplicar | Accion::DuplicarNotas => {
            p.rect_stroke(egui::Rect::from_center_size(c + vec2(-2.0, -2.0), vec2(9.0, 9.0)), 1.5, s, egui::StrokeKind::Middle);
            p.rect_stroke(egui::Rect::from_center_size(c + vec2(3.0, 3.0), vec2(9.0, 9.0)), 1.5, s, egui::StrokeKind::Middle);
        }
        Accion::Normalizar => (0..5).for_each(|k| {
            let h = [4.0, 8.0, 6.0, 8.0, 4.0][k];
            p.line_segment([c + vec2(-8.0 + k as f32 * 4.0, -h), c + vec2(-8.0 + k as f32 * 4.0, h)], s);
        }),
        Accion::Invertir => {
            p.line_segment([c + vec2(6.0, -6.0), c + vec2(6.0, 6.0)], s);
            p.add(Shape::convex_polygon(vec![c + vec2(3.0, -6.0), c + vec2(3.0, 6.0), c + vec2(-6.0, 0.0)], Color32::TRANSPARENT, s));
        }
        Accion::QuitarSilencios => {
            p.line_segment([c + vec2(-8.0, 0.0), c + vec2(-3.0, 0.0)], s);
            p.line_segment([c + vec2(3.0, 0.0), c + vec2(8.0, 0.0)], s);
            p.line_segment([c + vec2(-3.0, -5.0), c + vec2(3.0, 5.0)], s);
        }
        Accion::Abrir => {
            p.rect_stroke(egui::Rect::from_center_size(c, vec2(14.0, 10.0)), 2.0, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-7.0, -2.0), c + vec2(7.0, -2.0)], s);
        }
        Accion::Eliminar | Accion::BorrarNotas => {
            p.rect_stroke(egui::Rect::from_center_size(c + vec2(0.0, 2.0), vec2(10.0, 11.0)), 1.5, s, egui::StrokeKind::Middle);
            p.line_segment([c + vec2(-7.0, -5.0), c + vec2(7.0, -5.0)], s);
        }
        Accion::MasOpciones => (0..3).for_each(|k| _ = p.circle_filled(c + vec2(-6.0 + k as f32 * 6.0, 0.0), 1.8, color)),
        Accion::Cuantizar => (0..3).for_each(|k| _ = p.line_segment([c + vec2(-7.0 + k as f32 * 7.0, -7.0), c + vec2(-7.0 + k as f32 * 7.0, 7.0)], s)),
        Accion::OctavaArriba | Accion::OctavaAbajo => {
            let d = if a == Accion::OctavaArriba { -1.0 } else { 1.0 };
            p.line_segment([c + vec2(0.0, 7.0 * -d), c + vec2(0.0, 7.0 * d)], s);
            p.line_segment([c + vec2(0.0, 7.0 * d), c + vec2(-5.0, 2.0 * d)], s);
            p.line_segment([c + vec2(0.0, 7.0 * d), c + vec2(5.0, 2.0 * d)], s);
        }
        Accion::Arpegiar | Accion::Rasguear => (0..3).for_each(|k| {
            let x = -6.0 + k as f32 * 5.0;
            p.line_segment([c + vec2(x, 5.0 - k as f32 * 5.0), c + vec2(x + 4.0, 5.0 - k as f32 * 5.0)], s);
        }),
        Accion::GenerarAcordes => (0..3).for_each(|k| _ = p.circle_filled(c + vec2(0.0, -6.0 + k as f32 * 6.0), 2.4, color)),
        // Partitura: los símbolos de la fuente musical.
        Accion::PtFigura(k) => glifo(p, c, ['\u{E1D2}', '\u{E1D3}', '\u{E1D5}', '\u{E1D7}', '\u{E1D9}'][k.min(4) as usize], color),
        Accion::PtPuntillo => _ = p.circle_filled(c, 2.8, color),
        Accion::PtSilencio => glifo(p, c, '\u{E4E5}', color),
        Accion::PtSubir | Accion::PtArmadura(1..) => glifo(p, c, '\u{E262}', color),
        Accion::PtBajar | Accion::PtArmadura(..=-1) => glifo(p, c, '\u{E260}', color),
        Accion::PtArmadura(_) => glifo(p, c, '\u{E261}', color),
        Accion::PtBorrar => icono(p, c, Accion::Eliminar, color),
        Accion::PtEscuchar => {
            p.add(Shape::convex_polygon(vec![c + vec2(-4.0, -6.0), c + vec2(6.0, 0.0), c + vec2(-4.0, 6.0)], color, Stroke::NONE));
        }
        Accion::PtMenuClave | Accion::PtClave(project::partitura::Clave::Sol | project::partitura::Clave::Sol8vb) => glifo(p, c, '\u{E050}', color),
        Accion::PtClave(project::partitura::Clave::Fa) => glifo(p, c, '\u{E062}', color),
        Accion::PtClave(project::partitura::Clave::Percusion) => glifo(p, c, '\u{E069}', color),
        Accion::PtClave(_) => glifo(p, c, '\u{E05C}', color),
        Accion::PtMenuArmadura => glifo(p, c, '\u{E262}', color),
        Accion::PtAnotacion => _ = p.text(c, egui::Align2::CENTER_CENTER, "T", FontId::proportional(17.0), color),
    }
}

/// Un símbolo de la fuente musical centrado en `c`.
fn glifo(p: &egui::Painter, c: Pos2, g: char, color: Color32) {
    // Línea base algo abajo del centro: las cabezas quedan centradas y las plicas suben.
    crate::partitura::texto_base(p, c + vec2(0.0, 6.0), &g.to_string(), FontId::new(20.0, egui::FontFamily::Name("musica".into())), color, 1);
}

/// Dibuja el menú y devuelve la acción elegida y si hay que cerrarlo.
fn dibujar(ctx: &egui::Context, pie: &mut Pie) -> (Option<Accion>, bool) {
    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("pie")));
    let (puntero, clic, soltada_q, esc, clic_der) = ctx.input(|i| (i.pointer.hover_pos(), i.pointer.primary_clicked(), i.key_released(egui::Key::Q), i.key_pressed(egui::Key::Escape), i.pointer.secondary_clicked()));
    // Arrastrar el disco central mueve el menú (para ver lo que hay debajo).
    let (pulsado, abajo, delta) = ctx.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.delta()));
    if pulsado && puntero.is_some_and(|q| (q - pie.centro).length() < R_INT) {
        pie.arrastrando = true;
    }
    if pie.arrastrando {
        if abajo {
            pie.centro += delta;
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else {
            pie.arrastrando = false;
        }
        ctx.request_repaint();
    } else if puntero.is_some_and(|q| (q - pie.centro).length() < R_INT) {
        ctx.set_cursor_icon(egui::CursorIcon::Grab);
    }
    let n = pie.items.len().max(1);
    let paso = std::f32::consts::TAU / n as f32;
    let angulo = |k: f32| -std::f32::consts::FRAC_PI_2 - paso / 2.0 + k * paso;
    let punto = |a: f32, r: f32| pie.centro + vec2(a.cos(), a.sin()) * r;
    let sobre = puntero.and_then(|q| {
        let d = q - pie.centro;
        (d.length() > R_INT && d.length() < R_EXT + 30.0).then(|| (((d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2 + paso / 2.0).rem_euclid(std::f32::consts::TAU)) / paso) as usize % n)
    });
    p.circle_filled(pie.centro, R_EXT + 4.0, Color32::from_black_alpha(90));
    for (k, (texto, accion, activo)) in pie.items.iter().enumerate() {
        let (a0, a1) = (angulo(k as f32), angulo(k as f32 + 1.0));
        let encima = sobre == Some(k) && *activo;
        let fondo = if encima { ELEVATED().lerp_to_gamma(Color32::WHITE, 0.10) } else { PANEL() };
        // Sector como una sola malla (sin costuras entre trozos).
        let mut malla = egui::Mesh::default();
        for s in 0..=16 {
            let b = a0 + (a1 - a0) * s as f32 / 16.0;
            malla.colored_vertex(punto(b, R_INT), fondo);
            malla.colored_vertex(punto(b, R_EXT), fondo);
            if s > 0 {
                let v = malla.vertices.len() as u32;
                malla.add_triangle(v - 4, v - 3, v - 1);
                malla.add_triangle(v - 4, v - 1, v - 2);
            }
        }
        p.add(malla);
        p.line_segment([punto(a0, R_INT), punto(a0, R_EXT)], Stroke::new(1.5, BG()));
        let medio = (a0 + a1) / 2.0;
        let color = if !*activo { TEXT_DIM().gamma_multiply(0.5) } else if encima { TEXT() } else { TEXT().gamma_multiply(0.9) };
        // Icono arriba y texto debajo, apilados en el centro del sector.
        let c = punto(medio, (R_INT + R_EXT) / 2.0 + 4.0);
        let ci = c + vec2(0.0, -12.0);
        p.circle_filled(ci, 15.0, if encima { ACCENT().gamma_multiply(0.35) } else { ELEVATED() });
        icono(&p, ci, *accion, if encima { ACCENT() } else { color });
        p.text(c + vec2(0.0, 16.0), Align2::CENTER_CENTER, tr(texto), FontId::proportional(11.5), color);
    }
    p.circle_stroke(pie.centro, R_EXT, Stroke::new(1.0, BORDER()));
    p.circle_filled(pie.centro, R_INT, BG());
    p.circle_stroke(pie.centro, R_INT, Stroke::new(1.0, BORDER()));
    p.text(pie.centro + vec2(0.0, -8.0), Align2::CENTER_CENTER, tr(pie.titulo), FontId::proportional(12.5), TEXT());
    p.text(pie.centro + vec2(0.0, 10.0), Align2::CENTER_CENTER, tr("Señala para elegir"), FontId::proportional(10.5), TEXT_DIM());
    p.text(pie.centro + vec2(0.0, 26.0), Align2::CENTER_CENTER, tr("arrastra para mover"), FontId::proportional(9.0), TEXT_DIM().gamma_multiply(0.7));
    let elegida = sobre.and_then(|k| pie.items.get(k)).filter(|i| i.2).map(|i| i.1);
    if pie.arrastrando || (clic && puntero.is_some_and(|q| (q - pie.centro).length() < R_INT)) {
        return (None, false);
    }
    match () {
        _ if esc => (None, true),
        _ if pie.con_tecla && soltada_q => (elegida, true),
        _ if clic || clic_der => (elegida, elegida.is_some() || puntero.is_some_and(|q| (q - pie.centro).length() > R_EXT + 30.0)),
        _ => (None, false),
    }
}

impl App {
    /// Menú circular de una región (clic derecho sobre ella o Q encima).
    pub fn abrir_pie_region(&mut self, centro: Pos2, i: usize, j: usize, con_tecla: bool) {
        let audio = self.s.tracks[i].clips[j].buf().is_some();
        let midi = self.s.tracks[i].midi();
        self.pie = Some(Pie {
            centro,
            titulo: if midi { "REGIÓN MIDI" } else { "REGIÓN DE AUDIO" },
            items: vec![
                ("Abrir", Accion::Abrir, audio || midi),
                ("Dividir", Accion::Dividir, true),
                ("Duplicar", Accion::Duplicar, true),
                ("Normalizar", Accion::Normalizar, audio),
                ("Transientes", Accion::Transientes, audio),
                ("Silencios", Accion::QuitarSilencios, audio),
                ("Invertir", Accion::Invertir, audio),
                ("Eliminar", Accion::Eliminar, true),
                ("Más opciones", Accion::MasOpciones, true),
            ],
            con_tecla,
            region: Some((i, j)),
            en_partitura: false,
            arrastrando: false,
        });
    }

    /// Menú circular de las notas del piano roll.
    pub fn abrir_pie_notas(&mut self, centro: Pos2) {
        let hay = self.roll.has_selection();
        self.pie = Some(Pie {
            centro,
            titulo: "NOTAS",
            items: vec![
                ("Cuantizar", Accion::Cuantizar, true),
                ("Octava arriba", Accion::OctavaArriba, hay),
                ("Arpegiar", Accion::Arpegiar, hay),
                ("Generar acordes", Accion::GenerarAcordes, true),
                ("Borrar", Accion::BorrarNotas, hay),
                ("Duplicar", Accion::DuplicarNotas, hay),
                ("Rasguear", Accion::Rasguear, hay),
                ("Octava abajo", Accion::OctavaAbajo, hay),
            ],
            con_tecla: false,
            region: None,
            en_partitura: false,
            arrastrando: false,
        });
    }

    /// Dibuja el menú abierto y ejecuta la acción elegida.
    pub fn ventana_pie(&mut self, ctx: &egui::Context) {
        if let Some(pos) = self.menu_region {
            // «Más opciones»: el menú clásico de la región junto al puntero.
            let r = egui::Area::new(egui::Id::new("menu-region")).fixed_pos(pos).order(egui::Order::Foreground).show(ctx, |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    widgets::menu_style(ui);
                    self.region_menu(ui);
                });
            });
            if ctx.input(|i| i.pointer.any_click()) && !r.response.contains_pointer() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.menu_region = None;
            }
        }
        if self.pie.as_ref().is_some_and(|p| !p.en_partitura) {
            self.procesar_pie(ctx);
        }
    }

    /// Dibuja el menú abierto en el contexto (ventana) actual y ejecuta la acción elegida.
    pub fn procesar_pie(&mut self, ctx: &egui::Context) {
        let Some(pie) = &mut self.pie else { return };
        let (accion, cerrar) = dibujar(ctx, pie);
        let region = pie.region;
        let centro = pie.centro;
        if cerrar {
            self.pie = None;
        }
        let Some(a) = accion else { return };
        let corre = |app: &mut Self, op: RegionOp| {
            let r = app.region_op(op);
            app.report("Listo", r);
        };
        match a {
            Accion::Abrir => {
                if let Some((i, j)) = region {
                    if self.s.tracks[i].midi() { self.open_roll(i, j) } else { self.open_audio_editor(i, j) }
                }
            }
            Accion::Dividir => self.split(self.pos()),
            Accion::Duplicar => corre(self, RegionOp::Duplicate),
            Accion::Normalizar => corre(self, RegionOp::Normalize),
            Accion::Invertir => corre(self, RegionOp::Reverse),
            Accion::QuitarSilencios | Accion::Transientes => {
                let c = if a == Accion::Transientes { edicion::Corte::Transientes(0.5) } else { edicion::Corte::Silencios(-40.0) };
                let r = self.cortar_audio(c);
                self.report(r.as_ref().map_or(String::new(), |n| format!("{n} {}", tr("región(es) resultantes"))), r);
            }
            Accion::Eliminar => self.delete(),
            Accion::MasOpciones => self.menu_region = Some(centro),
            Accion::GenerarAcordes => self.generador.abierto = true,
            Accion::PtFigura(_) | Accion::PtPuntillo | Accion::PtSilencio | Accion::PtSubir | Accion::PtBajar | Accion::PtBorrar | Accion::PtEscuchar | Accion::PtClave(_) | Accion::PtArmadura(_) | Accion::PtAnotacion => self.pie_partitura(a),
            Accion::PtMenuClave | Accion::PtMenuArmadura => self.abrir_pie_partitura_sub(centro, a),
            notas => self.pie_notas(notas),
        }
    }
}

/// Armaduras del submenú: (quintas, nombre).
const ARMADURAS: [(i8, &str); 12] = [(-5, "Reb / Sibm"), (-4, "Lab / Fam"), (-3, "Mib / Dom"), (-2, "Sib / Solm"), (-1, "Fa / Rem"), (0, "Do / Lam"), (1, "Sol / Mim"), (2, "Re / Sim"), (3, "La / Fa#m"), (4, "Mi / Do#m"), (5, "Si / Sol#m"), (6, "Fa# / Re#m")];

impl App {
    /// Menú circular de QUANTUM Partitura (clic derecho en el papel).
    pub fn abrir_pie_partitura(&mut self, centro: Pos2) {
        let hay = self.partitura_app.tiene_seleccion();
        self.pie = Some(Pie {
            centro,
            titulo: "PARTITURA",
            items: vec![
                ("Negra", Accion::PtFigura(2), true),
                ("Corchea", Accion::PtFigura(3), true),
                ("Semicorchea", Accion::PtFigura(4), true),
                ("Puntillo", Accion::PtPuntillo, true),
                ("Silencio", Accion::PtSilencio, true),
                ("Subir", Accion::PtSubir, hay),
                ("Bajar", Accion::PtBajar, hay),
                ("Borrar", Accion::PtBorrar, hay),
                ("Escuchar", Accion::PtEscuchar, true),
                ("Anotación", Accion::PtAnotacion, true),
                ("Clave", Accion::PtMenuClave, true),
                ("Armadura", Accion::PtMenuArmadura, true),
                ("Blanca", Accion::PtFigura(1), true),
            ],
            con_tecla: false,
            region: None,
            en_partitura: true,
            arrastrando: false,
        });
    }

    /// Submenús circulares de clave y armadura.
    pub fn abrir_pie_partitura_sub(&mut self, centro: Pos2, a: Accion) {
        let items = if a == Accion::PtMenuClave {
            project::partitura::Clave::TODAS.iter().map(|&c| (c.nombre(), Accion::PtClave(c), true)).collect()
        } else {
            ARMADURAS.iter().map(|&(q, n)| (n, Accion::PtArmadura(q), true)).collect()
        };
        self.pie = Some(Pie { centro, titulo: if a == Accion::PtMenuClave { "CLAVE" } else { "ARMADURA" }, items, con_tecla: false, region: None, en_partitura: true, arrastrando: false });
    }
}
