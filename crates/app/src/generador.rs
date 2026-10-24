//! Generador de progresiones de acordes por estilo: elige estilo, tonalidad, largo, velocidad de
//! acordes, color (séptimas, novenas, sus) y variación; propone una progresión con sus grados, se
//! escucha y se escribe en la pista de acordes (y si se quiere, como notas en una pista MIDI).
use crate::acordes::{self, RAICES, TIPOS};
use crate::creativo::Azar;
use crate::*;
use egui::{RichText, vec2};

// Tipos de `acordes::TIPOS`.
const MAY: usize = 0;
const MEN: usize = 1;
const DOM7: usize = 2;
const MAJ7: usize = 3;
const MEN7: usize = 4;
const DISM: usize = 6;
const SUS2: usize = 9;
const SUS4: usize = 10;
const ADD9: usize = 12;

/// Grado: semitonos sobre la tónica, tipo de acorde y cifrado romano.
type Grado = (u8, usize, &'static str);

// Grados frecuentes (en mayor y en menor, con acordes prestados).
const I: Grado = (0, MAY, "I");
const II: Grado = (2, MEN, "ii");
const III: Grado = (4, MEN, "iii");
const IV: Grado = (5, MAY, "IV");
const V: Grado = (7, MAY, "V");
const VI: Grado = (9, MEN, "vi");
const BVII: Grado = (10, MAY, "bVII");
const BVI: Grado = (8, MAY, "bVI");
const BIII: Grado = (3, MAY, "bIII");
const BII: Grado = (1, MAY, "bII");
const IVM: Grado = (5, MEN, "iv");
const IM: Grado = (0, MEN, "i");
const VM: Grado = (7, MEN, "v");
const II7: Grado = (2, MEN7, "ii7");
const III7: Grado = (4, MEN7, "iii7");
const VI7: Grado = (9, MEN7, "vi7");
const V7: Grado = (7, DOM7, "V7");
const IMAJ7: Grado = (0, MAJ7, "Imaj7");
const IVMAJ7: Grado = (5, MAJ7, "IVmaj7");
const I7: Grado = (0, DOM7, "I7");
const IV7: Grado = (5, DOM7, "IV7");
const VIIDIM: Grado = (11, DISM, "vii°");

/// Estilo: (nombre, descripción, ¿menor?, acordes por compás, progresiones).
type Estilo = (&'static str, &'static str, bool, u32, &'static [&'static [Grado]]);
/// Acordes de la vista previa: (segundos desde el inicio, duración, notas).
type Sonido = Vec<(f64, f64, Vec<u8>)>;

pub const ESTILOS: [Estilo; 15] = [
    ("Cinematic", "Crescendos épicos en menor, acordes prestados y suspensiones (tráiler, banda sonora)", true, 1, &[&[IM, BVI, BIII, BVII], &[IM, BIII, BVII, IVM], &[IM, BVI, IVM, V], &[BVI, BVII, IM, IM], &[IM, VM, BVI, BVII]]),
    ("Ambient", "Acordes abiertos que respiran, sin tensión, con novenas y sus", false, 1, &[&[I, IV, VI, IV], &[I, III, IV, I], &[IV, I, V, VI]]),
    ("Indie Pop", "Pop luminoso con vueltas al relativo menor", false, 1, &[&[VI, IV, I, V], &[I, V, VI, IV], &[IV, VI, I, V]]),
    ("Emo", "Melancolía con fuerza: menores y cuartos grados que empujan", false, 1, &[&[VI, IV, I, V], &[I, III, VI, IV], &[IV, V, III, VI]]),
    ("Pop", "Las progresiones de los grandes éxitos", false, 1, &[&[I, V, VI, IV], &[I, VI, IV, V], &[VI, IV, I, V]]),
    ("Lo-fi / Neo Soul", "Séptimas y novenas suaves, giros ii–V con calidez", false, 1, &[&[II7, V7, IMAJ7, VI7], &[IVMAJ7, III7, VI7, II7], &[IMAJ7, VI7, II7, V7]]),
    ("Jazz", "ii–V–I, círculos de quintas y dominantes", false, 2, &[&[II7, V7, IMAJ7, IMAJ7], &[IMAJ7, VI7, II7, V7], &[III7, VI7, II7, V7], &[IMAJ7, VIIDIM, III7, VI7]]),
    ("R&B / Gospel", "Cuartos grados con séptima y menor prestado", false, 1, &[&[IVMAJ7, III7, VI7, II7], &[I, I7, IV, IVM], &[II7, III7, IVMAJ7, V7]]),
    ("Rock", "Quintas y el bVII mixolidio de siempre", false, 1, &[&[I, BVII, IV, I], &[I, IV, V, IV], &[I, BIII, IV, I]]),
    ("Synthwave", "Menor ochentero con bVI y bVII", true, 1, &[&[IM, BVI, BIII, BVII], &[IM, BVII, BVI, BVII], &[IM, IVM, BVI, BVII]]),
    ("EDM / Trance", "Bucles en menor que suben hacia el drop", true, 1, &[&[IM, BVI, BIII, BVII], &[BVI, BVII, IM, IM], &[IM, BVII, BVI, V]]),
    ("Dark / Horror", "Semitono napolitano y dominantes que inquietan", true, 1, &[&[IM, BII, IM, BII], &[IM, IVM, BII, V], &[IM, BVI, BII, V]]),
    ("Folk / Country", "Mayores sencillos y cálidos", false, 1, &[&[I, IV, I, V], &[I, V, IV, I], &[I, VI, IV, V]]),
    ("Blues", "Doce compases con séptimas de dominante", false, 1, &[&[I7, IV7, I7, I7, IV7, IV7, I7, I7, V7, IV7, I7, V7]]),
    ("Heroic / Game", "Fanfarria: bVI y bVII que levantan hacia la tónica", false, 1, &[&[I, BVI, BVII, I], &[I, V, VI, IV], &[IV, V, I, I]]),
];

/// Estado de la ventana del generador.
pub struct Generador {
    pub abierto: bool,
    estilo: usize,
    tono: u8,
    /// 0 = la del estilo, 1 = mayor, 2 = menor.
    modo: u8,
    compases: u32,
    /// 0 más lento, 1 el del estilo, 2 más rápido.
    velocidad: u8,
    desde_compas: u32,
    color: f32,
    variacion: f32,
    bajo: bool,
    escribir_notas: bool,
    /// 0 completo (con bajo), 1 tríada.
    voz: u8,
    semilla: u64,
    /// Progresión propuesta: (raíz, tipo, cifrado romano) y tonalidad resultante.
    resultado: Vec<(u8, usize, String)>,
    tonalidad: (u8, bool),
    /// Vista previa: inicio y acordes (segundos desde el inicio, duración, notas).
    sonando: Option<(std::time::Instant, Sonido, usize)>,
}

impl Default for Generador {
    fn default() -> Self {
        let mut g = Self { abierto: false, estilo: 0, tono: 9, modo: 0, compases: 8, velocidad: 1, desde_compas: 1, color: 0.5, variacion: 0.35, bajo: true, escribir_notas: true, voz: 0, semilla: 0x51ED, resultado: vec![], tonalidad: (9, true), sonando: None };
        g.generar();
        g
    }
}

/// Color por familia de acorde (como en la pista de acordes).
fn color_familia(tipo: usize) -> Color32 {
    match TIPOS[tipo].0 {
        "m" | "m7" | "m7b5" => Color32::from_rgb(0x6D, 0x4F, 0xB0),
        "7" | "9" => Color32::from_rgb(0xC8, 0x7A, 0x2A),
        "sus2" | "sus4" => Color32::from_rgb(0x1F, 0x8E, 0x7C),
        "dim" | "dim7" | "aug" => Color32::from_rgb(0xB0, 0x44, 0x4A),
        _ => Color32::from_rgb(0x2F, 0x6E, 0xB8),
    }
}

impl Generador {
    /// Propone una progresión nueva con los ajustes actuales.
    fn generar(&mut self) {
        let mut azar = Azar::new(self.semilla);
        let (_, _, menor_estilo, por_compas, patrones) = ESTILOS[self.estilo];
        let menor = match self.modo {
            1 => false,
            2 => true,
            _ => menor_estilo,
        };
        // Si el modo pedido no es el del estilo, el patrón suena en la relativa.
        let raiz_patron = match (menor_estilo, menor) {
            (true, false) => (self.tono + 9) % 12,
            (false, true) => (self.tono + 3) % 12,
            _ => self.tono,
        };
        self.tonalidad = (self.tono, menor);
        let rate = match self.velocidad {
            0 => 0.5,
            2 => por_compas as f32 * 2.0,
            _ => por_compas as f32,
        };
        let n = ((self.compases as f32 * rate).round() as usize).clamp(2, 64);
        let patron = patrones[azar.hasta(patrones.len())];
        self.resultado = (0..n)
            .map(|k| {
                let (mut semis, mut tipo, mut romano) = patron[k % patron.len()];
                // Variación: a veces un acorde cercano (relativo, cuarto grado o dominante).
                if azar.hasta(1000) < (self.variacion * 350.0) as usize && k % patron.len() != 0 {
                    let alternativas: [Grado; 4] = if menor_estilo { [IM, BVI, IVM, BVII] } else { [VI, IV, II, V] };
                    (semis, tipo, romano) = alternativas[azar.hasta(4)];
                }
                // Color: tríadas que se vuelven séptimas, novenas o suspendidas.
                let mut cifrado = romano.to_string();
                if azar.hasta(1000) < (self.color * 700.0) as usize {
                    let nuevo = match tipo {
                        MAY => [ADD9, MAJ7, SUS2, SUS4][azar.hasta(4)],
                        MEN => [MEN7, MEN7, SUS2][azar.hasta(3)],
                        t => t,
                    };
                    if nuevo != tipo {
                        cifrado = format!("{romano}{}", TIPOS[nuevo].0);
                        tipo = nuevo;
                    }
                }
                ((raiz_patron + semis) % 12, tipo, cifrado)
            })
            .collect();
    }

    /// Notas de cada acorde (con inversiones para que el bajo camine por grados si se pidió).
    fn voces(&self) -> Vec<Vec<u8>> {
        let mut bajo_anterior: Option<u8> = None;
        self.resultado
            .iter()
            .map(|&(raiz, tipo, _)| {
                let base = acordes::base_central(raiz);
                let inversion = if self.bajo {
                    // La inversión cuyo bajo queda más cerca del bajo anterior.
                    (0..TIPOS[tipo].2.len().min(3)).min_by_key(|&inv| {
                        let n = acordes::notas(base, tipo, inv);
                        bajo_anterior.map_or(inv as i32, |b| (n[0] as i32 - b as i32).abs())
                    })
                } else {
                    Some(0)
                }
                .unwrap_or(0);
                let mut n = acordes::notas(base, tipo, inversion);
                bajo_anterior = n.first().copied();
                if self.voz == 0 {
                    // Completo: la fundamental una octava abajo como bajo.
                    n.insert(0, (base % 12) + 36);
                }
                n
            })
            .collect()
    }

    /// Cifrado completo de la tonalidad y los grados («E minor · i - bIII - bVII - iv»).
    fn resumen(&self) -> String {
        let grados: Vec<&str> = self.resultado.iter().take(8).map(|r| r.2.as_str()).collect();
        format!("{} {} · {}", RAICES[self.tonalidad.0 as usize], if self.tonalidad.1 { tr("menor") } else { tr("mayor") }, grados.join(" - "))
    }
}

impl App {
    /// Ventana «Generar progresión de acordes».
    pub fn ventana_generador(&mut self, ctx: &egui::Context) {
        self.tick_vista_previa();
        if !self.generador.abierto {
            return;
        }
        let mut abierto = true;
        let (mut aplicar, mut cancelar, mut escuchar) = (false, false, false);
        let pista_notas = self.roll.target.map(|(i, _)| i).or_else(|| self.s.tracks.iter().position(|t| t.selected && t.midi()));
        let nombre_pista = pista_notas.map(|i| self.s.tracks[i].name.clone());
        egui::Window::new(RichText::new(tr("Generar progresión de acordes")).strong()).id(egui::Id::new("generador")).open(&mut abierto).default_width(760.0).resizable(false).show(ctx, |ui| {
            let g = &mut self.generador;
            let mut cambio = false;
            let boton = |ui: &mut egui::Ui, on: bool, texto: &str, ancho: f32| -> bool {
                let b = egui::Button::new(RichText::new(texto).size(12.5).color(if on { BG() } else { TEXT() })).fill(if on { ACCENT() } else { ELEVATED() }).min_size(vec2(ancho, 30.0)).corner_radius(widgets::rr(6.0));
                ui.add(b).clicked()
            };
            ui.label(RichText::new(tr("Estilo")).size(11.5).color(TEXT_DIM()));
            for fila in ESTILOS.chunks(5).enumerate() {
                ui.horizontal(|ui| {
                    for (k, e) in fila.1.iter().enumerate() {
                        let idx = fila.0 * 5 + k;
                        if boton(ui, g.estilo == idx, tr(e.0), 138.0) {
                            g.estilo = idx;
                            cambio = true;
                        }
                    }
                });
            }
            ui.label(RichText::new(tr(ESTILOS[g.estilo].1)).size(12.0).color(TEXT_DIM()));
            ui.add_space(4.0);
            ui.label(RichText::new(tr("Tonalidad")).size(11.5).color(TEXT_DIM()));
            ui.horizontal(|ui| {
                for (k, r) in RAICES.iter().enumerate() {
                    if boton(ui, g.tono == k as u8, r, 38.0) {
                        g.tono = k as u8;
                        cambio = true;
                    }
                }
                ui.add_space(10.0);
                for (k, m) in ["Cualquiera", "Mayor", "Menor"].iter().enumerate() {
                    if boton(ui, g.modo == k as u8, tr(m), 72.0) {
                        g.modo = k as u8;
                        cambio = true;
                    }
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Largo (compases)")).size(11.5).color(TEXT_DIM()));
                    ui.horizontal(|ui| {
                        for c in [4, 8, 16, 32] {
                            if boton(ui, g.compases == c, &c.to_string(), 36.0) {
                                g.compases = c;
                                cambio = true;
                            }
                        }
                    });
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Velocidad de acordes")).size(11.5).color(TEXT_DIM()));
                    ui.horizontal(|ui| {
                        for (k, v) in ["Más lento", "Estilo", "Más rápido"].iter().enumerate() {
                            if boton(ui, g.velocidad == k as u8, tr(v), 78.0) {
                                g.velocidad = k as u8;
                                cambio = true;
                            }
                        }
                    });
                });
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(tr("Empieza en el compás")).size(11.5).color(TEXT_DIM()));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut g.desde_compas).range(1..=999));
                        if ui.button("−").clicked() {
                            g.desde_compas = g.desde_compas.saturating_sub(1).max(1);
                        }
                        if ui.button("+").clicked() {
                            g.desde_compas += 1;
                        }
                    });
                });
            });
            ui.horizontal(|ui| {
                cambio |= ui.add(egui::Slider::new(&mut g.color, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).text(tr("Color"))).changed();
                ui.add_space(14.0);
                cambio |= ui.add(egui::Slider::new(&mut g.variacion, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0} %", v * 100.0)).text(tr("Variación"))).changed();
            });
            ui.checkbox(&mut g.bajo, tr("Bajo caminante (inversiones)"));
            if cambio {
                g.generar();
            }
            ui.separator();
            ui.label(RichText::new(g.resumen()).size(13.0).strong());
            // Tarjetas de los acordes (hasta 8 por fila) con su grado.
            for fila in g.resultado.chunks(8) {
                ui.horizontal(|ui| {
                    for &(raiz, tipo, ref romano) in fila {
                        let (r, _) = ui.allocate_exact_size(vec2(88.0, 52.0), egui::Sense::hover());
                        let c = color_familia(tipo);
                        ui.painter().rect_filled(r, widgets::rr(6.0), c.gamma_multiply(0.75));
                        ui.painter().rect_filled(egui::Rect::from_min_size(r.min, vec2(3.0, r.height())), 1.0, c);
                        ui.painter().text(r.left_top() + vec2(9.0, 15.0), egui::Align2::LEFT_CENTER, acordes::nombre(raiz, tipo), egui::FontId::proportional(14.0), Color32::WHITE);
                        ui.painter().text(r.left_top() + vec2(9.0, 36.0), egui::Align2::LEFT_CENTER, romano, egui::FontId::proportional(11.0), Color32::from_white_alpha(200));
                    }
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(tr("Otra")).min_size(vec2(110.0, 30.0))).clicked() {
                    g.semilla = g.semilla.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                    g.generar();
                }
                if ui.add(egui::Button::new(if g.sonando.is_some() { tr("Detener") } else { tr("Escuchar") }).min_size(vec2(110.0, 30.0))).clicked() {
                    escuchar = true;
                }
                ui.add_space(10.0);
                match &nombre_pista {
                    Some(n) => {
                        ui.checkbox(&mut g.escribir_notas, format!("{} «{n}»", tr("Escribir también las notas en")));
                        egui::ComboBox::from_id_salt("gen-voz").width(90.0).selected_text(if g.voz == 0 { tr("Completo") } else { tr("Tríada") }).show_ui(ui, |ui| {
                            ui.selectable_value(&mut g.voz, 0, tr("Completo"));
                            ui.selectable_value(&mut g.voz, 1, tr("Tríada"));
                        });
                    }
                    None => _ = ui.label(RichText::new(tr("Selecciona una pista MIDI para escribir también las notas.")).size(11.5).color(TEXT_DIM())),
                }
            });
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                aplicar = ui.add(egui::Button::new(RichText::new(tr("Aplicar")).strong().color(BG())).fill(ACCENT()).min_size(vec2(110.0, 32.0))).clicked();
                cancelar = ui.add(egui::Button::new(tr("Cancelar")).min_size(vec2(100.0, 32.0))).clicked();
            });
        });
        if escuchar {
            self.alternar_vista_previa(pista_notas);
        }
        if aplicar {
            self.aplicar_progresion(pista_notas);
        }
        if cancelar || aplicar || !abierto {
            self.generador.abierto = false;
            self.detener_vista_previa();
        }
    }

    /// Escribe la progresión en la pista de acordes (y como notas si se pidió).
    fn aplicar_progresion(&mut self, pista: Option<usize>) {
        let g = &self.generador;
        let tiempos = self.engine.beats.load(Relaxed).max(1) as f64;
        let n = g.resultado.len().max(1) as f64;
        let largo_tiempos = g.compases as f64 * tiempos / n;
        let inicio = (g.desde_compas.max(1) - 1) as f64 * tiempos;
        let (resultado, voces, escribir) = (g.resultado.clone(), g.voces(), g.escribir_notas && pista.is_some());
        self.edit();
        let tramos: Vec<(u64, u64)> = (0..resultado.len()).map(|k| (self.frame_of(inicio + k as f64 * largo_tiempos) as u64, self.frame_of(inicio + (k + 1) as f64 * largo_tiempos) as u64)).collect();
        let (a, b) = (tramos.first().map_or(0, |t| t.0), tramos.last().map_or(0, |t| t.1));
        self.s.chords.retain(|c| c.1 <= a || c.0 >= b);
        self.s.chords.extend(resultado.iter().zip(&tramos).map(|(&(raiz, tipo, _), &(x, y))| (x, y, acordes::nombre(raiz, tipo))));
        self.s.chords.sort_by_key(|c| c.0);
        self.show_chords = true;
        if escribir && let Some(i) = pista {
            // En la región abierta del piano roll; si no, en una región nueva que cubra la progresión.
            if self.roll.target.is_none_or(|(ti, _)| ti != i) {
                self.s.tracks[i].clips.push(Clip::new(Source::Midi(Arc::new(vec![])), a, b - a));
                let j = self.s.tracks[i].clips.len() - 1;
                self.roll.open(i, j);
            }
            self.insertar_acordes(tramos.iter().zip(voces).map(|(&(x, y), notas)| (x, y.saturating_sub(x), notas)).collect());
        }
        self.dirty = true;
        self.status = format!("{}: {}", tr("Progresión aplicada"), self.generador.resumen());
    }

    fn alternar_vista_previa(&mut self, pista: Option<usize>) {
        if self.generador.sonando.is_some() {
            self.detener_vista_previa();
            return;
        }
        // Las notas suenan en la pista MIDI elegida (se activa su monitoreo).
        if let Some(i) = pista {
            self.escuchar_en(i);
        }
        let seg_compas = self.engine.beat_frames() * self.engine.beats.load(Relaxed).max(1) as f64 / self.sr();
        let g = &self.generador;
        let dur = g.compases as f64 * seg_compas / g.resultado.len().max(1) as f64;
        let acordes: Vec<(f64, f64, Vec<u8>)> = g.voces().into_iter().enumerate().map(|(k, n)| (k as f64 * dur, dur * 0.95, n)).collect();
        self.generador.sonando = Some((std::time::Instant::now(), acordes, 0));
    }

    fn detener_vista_previa(&mut self) {
        if let Some((_, acordes, _)) = self.generador.sonando.take() {
            for (_, _, notas) in acordes {
                notas.iter().for_each(|&k| self.engine.send_midi([0x80, k, 0]));
            }
        }
    }

    /// Avanza la vista previa: enciende y apaga los acordes a su tiempo.
    fn tick_vista_previa(&mut self) {
        let Some((inicio, acordes, siguiente)) = &mut self.generador.sonando else { return };
        let t = inicio.elapsed().as_secs_f64();
        let engine = self.engine.clone();
        while *siguiente < acordes.len() && acordes[*siguiente].0 <= t {
            if *siguiente > 0 {
                acordes[*siguiente - 1].2.iter().for_each(|&k| engine.send_midi([0x80, k, 0]));
            }
            acordes[*siguiente].2.iter().for_each(|&k| engine.send_midi([0x90, k, 90]));
            *siguiente += 1;
        }
        let fin = acordes.last().map_or(0.0, |a| a.0 + a.1);
        if t >= fin {
            self.detener_vista_previa();
        } else {
            self.ctx.request_repaint_after(Duration::from_millis(15));
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn genera_en_la_tonalidad() {
        let mut g = Generador { color: 0.0, variacion: 0.0, ..Default::default() };
        // Pop en Do mayor, 4 compases: I-V-vi-IV o variantes con Do como I.
        (g.estilo, g.tono, g.modo, g.compases) = (4, 0, 1, 4);
        g.generar();
        assert_eq!(g.resultado.len(), 4);
        assert!(g.resultado.iter().all(|r| [0, 2, 4, 5, 7, 9].contains(&r.0)), "{:?}", g.resultado);
        // Pop pedido en La menor: suena en la relativa (Do mayor), los mismos acordes.
        g.modo = 2;
        (g.tono, g.semilla) = (9, 1);
        g.generar();
        assert!(g.resultado.iter().all(|r| [0, 2, 4, 5, 7, 9].contains(&r.0)));
        // Bajo caminante: cada acorde tiene notas y empieza por el bajo.
        assert!(g.voces().iter().all(|v| v.len() >= 3));
    }
}
