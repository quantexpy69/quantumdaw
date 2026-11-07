//! Ventana de plugins QUANTUM: instrumento y cadena de efectos, con visor, perillas y firma.
use crate::{
    widgets::{self, rr},
    *,
};
use egui::{Align2, FontId, Pos2, Rect, RichText, Sense, Stroke, pos2, vec2};
use engine::{AtomicF32, SYNTH_PARAMS, eq_response};

const SIGNATURE: &str = "QUANTUM";
/// Parámetros de encendido/apagado: se muestran como un botón, no como perilla.
const CONMUTADORES: [&str; 9] = ["Ping-pong", "Sincronía", "Escuchar lo quitado", "Escuchar eses", "Parar", "Mono", "Invertir izq.", "Invertir der.", "Cambiar canales"];
const DISCRETE: [&str; 19] = ["Detección", "Escuchar eses", "Tonalidad", "Nota manual", "Escala", "Modo", "Onda", "Ping-pong", "Sincronía", "División", "Escuchar lo quitado", "Forma", "Tipo", "Patrón", "Etapas", "Parar", "Mono", "Invertir izq.", "Invertir der."];
/// Ancho de cada dispositivo en la pestaña Dispositivos; Curves (visor y controles en fila) es ancho.
const ANCHO_DISPOSITIVO: f32 = 380.0;
const ANCHO_CURVES: f32 = 820.0;
/// En fila (Dispositivos) las tarjetas son compactas: perillas y visores más chicos, con los textos del
/// mismo tamaño para que se lean igual. Curves cabe entonces en menos ancho.
const ANCHO_CURVES_FILA: f32 = 660.0;
const PERILLA: f32 = 58.0;
const PERILLA_FILA: f32 = 42.0;

/// En fila, cada dispositivo ocupa una columna de ancho fijo con su propio desplazamiento vertical.
fn columna(ui: &mut egui::Ui, en_fila: bool, ancho: f32, add: impl FnOnce(&mut egui::Ui)) {
    if !en_fila {
        return add(ui);
    }
    ui.allocate_ui_with_layout(vec2(ancho, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_width(ancho);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, add);
    });
}

/// Color de acento de cada plugin (None = instrumento).
pub fn kind_color(kind: Option<FxKind>) -> Color32 {
    match kind {
        Some(FxKind::Eq) => Color32::from_rgb(0x2E, 0xD8, 0xC3),
        Some(FxKind::Compressor) => Color32::from_rgb(0xFF, 0xB0, 0x20),
        Some(FxKind::Delay) => Color32::from_rgb(0x5B, 0x9C, 0xFF),
        Some(FxKind::Reverb) => Color32::from_rgb(0xB0, 0x8C, 0xF2),
        Some(FxKind::Drive) => Color32::from_rgb(0xFF, 0x7A, 0x45),
        Some(FxKind::Tune) => Color32::from_rgb(0x3D, 0xDC, 0x84),
        Some(FxKind::Curves) => Color32::from_rgb(0xFF, 0x6B, 0x9A),
        Some(FxKind::Limiter) => Color32::from_rgb(0xFF, 0x4D, 0x4F),
        Some(FxKind::Gate) => Color32::from_rgb(0x9C, 0xD6, 0x4A),
        Some(FxKind::Chorus) => Color32::from_rgb(0x4F, 0xC3, 0xF7),
        Some(FxKind::DeEsser) => Color32::from_rgb(0xF2, 0xC9, 0x4C),
        Some(FxKind::AntiPop) => Color32::from_rgb(0x5C, 0xE1, 0xE6),
        Some(FxKind::VozAlFrente) => Color32::from_rgb(0xFF, 0x9E, 0x6B),
        Some(FxKind::Ancho | FxKind::AutoPan) => Color32::from_rgb(0x6E, 0xA8, 0xFF),
        Some(FxKind::FiltroDj | FxKind::AutoFiltro) => Color32::from_rgb(0x3D, 0xD6, 0xA5),
        Some(FxKind::PuertaRitmica) => Color32::from_rgb(0xFF, 0x8A, 0x3D),
        Some(FxKind::Phaser | FxKind::Flanger) => Color32::from_rgb(0xC3, 0x8B, 0xFF),
        Some(FxKind::LoFi) => Color32::from_rgb(0xE6, 0xC2, 0x5C),
        Some(FxKind::TapeStop) => Color32::from_rgb(0xFF, 0x6B, 0x6B),
        Some(FxKind::Utilidad | FxKind::Afinador) => Color32::from_rgb(0xA8, 0xB4, 0xC4),
        Some(FxKind::Transitorios) => Color32::from_rgb(0xFF, 0xC4, 0x3D),
        Some(FxKind::Desplazador) => Color32::from_rgb(0x7C, 0xE0, 0xFF),
        Some(FxKind::TapePro) => Color32::from_rgb(0xFF, 0xA8, 0x4A),
        Some(FxKind::SpacePro) => Color32::from_rgb(0x9D, 0x7C, 0xFF),
        Some(FxKind::VocalPro) => Color32::from_rgb(0xFF, 0x5C, 0xA8),
        Some(FxKind::MasterPro) => Color32::from_rgb(0x2E, 0xE6, 0xC8),
        Some(FxKind::Externo) => Color32::from_rgb(0x4F, 0xC3, 0xF7),
        None => Color32::from_rgb(0xF2, 0x8C, 0xD0),
    }
}

/// Qué hace cada efecto (vista previa en la biblioteca).
pub fn summary(k: FxKind) -> &'static str {
    match k {
        FxKind::Eq => "Ecualizador de 3 bandas con frecuencias ajustables: realza o atenúa graves, medios y agudos.",
        FxKind::Compressor => "Controla la dinámica bajando los picos sobre el umbral. Ataque, release, rodilla suave y mezcla paralela.",
        FxKind::Delay => "Repeticiones con feedback, ping-pong estéreo, filtros en las repeticiones y sincronía al tempo.",
        FxKind::Reverb => "Simula un espacio: tamaño, amortiguación, pre-delay, corte de graves y anchura estéreo.",
        FxKind::Drive => "Saturación analógica suave, dura o de válvula, con control de tono y salida.",
        FxKind::Tune => "Afinación de voz automática a la tonalidad y escala, o manual a una nota fija.",
        FxKind::Curves => "EQ de 7 bandas con analizador en vivo, detección automática y supresor de resonancias molestas, con presets por instrumento.",
        FxKind::Limiter => "Limitador con anticipación: sube el volumen sin pasar del techo. Ideal al final de la cadena del master.",
        FxKind::Gate => "Puerta de ruido: silencia el ruido de fondo entre frases o golpes, con retención y rango.",
        FxKind::Chorus => "Duplica la señal con pequeñas variaciones de tiempo para un sonido más ancho y envolvente.",
        FxKind::AntiPop => "Baja los golpes de aire de las P y B y el chasquido de las T y K, sutil y solo en el instante en que aparecen. La voz normal pasa intacta y un techo suave evita que el audio sature.",
        FxKind::VozAlFrente => "Quita el eco de la sala para que la voz suene cerca y al frente: compara cómo se apaga cada banda con cómo se apagaría la sala y baja solo la cola; la voz directa y las notas sostenidas pasan intactas. Con presencia para adelantarla aún más.",
        FxKind::Ancho => "Abre o cierra la imagen estéreo (de mono a súper ancho) y deja los graves en el centro para que nada se pierda en mono. Muestra la correlación entre los canales.",
        FxKind::AutoPan => "Mueve el sonido de un altavoz al otro al ritmo de la canción: seno, triángulo o cuadrada, con profundidad y fase.",
        FxKind::FiltroDj => "Una sola perilla: a la izquierda quita los agudos, a la derecha quita los graves y al centro deja la pista como está.",
        FxKind::AutoFiltro => "Filtro que se mueve solo: con un LFO a tempo, con la fuerza de la señal (envolvente) o con ambos. Paso bajo, alto o banda.",
        FxKind::PuertaRitmica => "Corta el sonido en ritmo con ocho patrones de 16 pasos, a tempo, con profundidad y suavizado para que no haga clic.",
        FxKind::Phaser => "Barrido clásico de fases: 4, 6 u 8 etapas con realimentación, abierto en estéreo.",
        FxKind::Flanger => "Retardo cortísimo que se mueve: el silbido metálico de los discos, con realimentación positiva o negativa.",
        FxKind::LoFi => "Menos bits y menos muestras: el color de los samplers viejos y los videojuegos, con un tono para suavizarlo.",
        FxKind::TapeStop => "El motor de la cinta se apaga y el sonido frena bajando de tono; al soltar vuelve a arrancar. Ideal automatizando «Parar».",
        FxKind::Utilidad => "Ganancia limpia en cualquier lugar de la cadena: panorama, anchura, mono, invertir la fase de cada lado y cambiar canales, con medidor de pico.",
        FxKind::Transitorios => "Modela el golpe y la cola: más o menos ataque y más o menos sostenido, sin umbral que ajustar.",
        FxKind::Desplazador => "Mueve todas las frecuencias la misma cantidad de hercios: desde un leve movimiento metálico hasta voces de robot.",
        FxKind::Afinador => "Muestra la nota que suena y cuántos cents está desafinada, en vivo. Deja pasar el audio sin tocarlo.",
        FxKind::DeEsser => "Baja las eses y sibilancias de las voces: detecta los agudos fuertes y reduce el volumen solo en ese instante.",
        FxKind::TapePro => "Cinta y válvula de estudio: saturación, golpe de cabeza, wow, flutter y siseo, con VU analógicos y tres velocidades de cinta.",
        FxKind::SpacePro => "Reverb de 8 líneas (sala, placa, catedral y shimmer) con eco sincronizado, ducking y congelar, en una sala 3D animada.",
        FxKind::VocalPro => "Cadena de voz completa: limpieza, de-esser, nivelador, dos compresores, tono, afinación y espacio, con un macro de carácter y presets.",
        FxKind::MasterPro => "Mastering en 3 bandas: compresión e imagen por banda, saturación, limitador y ganancia automática hacia Spotify, YouTube o Apple Music, con A/B al mismo volumen.",
        FxKind::Externo => "Plugin LV2 instalado en el sistema.",
    }
}

/// Vista previa de un efecto (visor con sus valores por defecto), para mostrar al pasar el ratón.
pub fn preview(ui: &mut egui::Ui, k: FxKind, sr: f32) {
    ui.set_width(300.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("QUANTUM")).size(10.0).strong().color(kind_color(Some(k))));
        ui.label(RichText::new(tr(k.name()).trim_start_matches("QUANTUM ")).size(15.0).strong());
    });
    let defaults: Vec<f32> = k.params().iter().map(|p| p.3).collect();
    display(ui, Some(k), &defaults, [0.0; 2], kind_color(Some(k)), sr, 96.0, None);
    ui.label(RichText::new(tr(summary(k))).size(12.5));
    ui.label(RichText::new(format!("{} {}", k.params().len(), tr("parámetros"))).size(11.0).color(TEXT_DIM()));
}

/// Texto del valor de un parámetro con su unidad (o su nombre, en los discretos).
pub(crate) fn value_text(kind: Option<FxKind>, name: &str, v: f32) -> String {
    if kind == Some(FxKind::Externo) {
        return engine::lv2::param(name).map_or_else(|| format!("{v:.2}"), |m| m.texto(v));
    }
    let pick = |names: &[&str]| tr(names[(v.round().max(0.0) as usize).min(names.len() - 1)]).to_string();
    // Las frecuencias bajas (velocidades de LFO) llevan decimales: 0.40 Hz, no «0 Hz».
    let hz = |v: f32| match v.abs() {
        a if a >= 1000.0 => format!("{:.1} kHz", v / 1000.0),
        a if a < 10.0 => format!("{v:.2} Hz"),
        a if a < 100.0 => format!("{v:.1} Hz"),
        _ => format!("{v:.0} Hz"),
    };
    match name {
        "Tonalidad" | "Nota manual" => pick(&NOTE_NAMES),
        "Escala" => pick(&["Cromática", "Mayor", "Menor"]),
        "Modo" if kind == Some(FxKind::Drive) => pick(&["Suave", "Duro", "Válvula", "Plegado", "Rectificado", "Cinta"]),
        "Forma" => pick(&["Seno", "Triángulo", "Cuadrada"]),
        "Detección" => pick(&["Pico", "RMS"]),
        "Tipo" => pick(&["Paso bajo", "Paso alto", "Paso banda"]),
        "Patrón" => format!("{} {}", tr("Patrón"), v.round() as i32 + 1),
        "Etapas" => pick(&["4", "6", "8"]),
        "Parar" | "Mono" | "Invertir izq." | "Invertir der." | "Cambiar canales" => pick(&["No", "Sí"]),
        "Bits" => format!("{:.0} bits", v.round()),
        "Reducción" => format!("÷{:.0}", v.round()),
        "Filtro" if v.abs() < 0.02 => tr("Sin filtro").to_string(),
        "Filtro" if v < 0.0 => format!("{} {:.0} %", tr("Graves"), -v * 100.0),
        "Filtro" => format!("{} {:.0} %", tr("Agudos"), v * 100.0),
        "Panorama" => match (v * 100.0).round() as i32 {
            0 => "C".into(),
            x if x < 0 => format!("L{}", -x),
            x => format!("R{x}"),
        },
        "Ataque" | "Sostenido" | "Envolvente" | "Realimentación" => format!("{:+.0} %", v * 100.0),
        "Modo" => pick(&["Automático", "Manual"]),
        "Onda" => pick(&["Sierra", "Cuadrada", "Triángulo", "Seno"]),
        "Ping-pong" | "Sincronía" | "Escuchar lo quitado" | "Escuchar eses" => pick(&["No", "Sí"]),
        "Fase" => format!("{:.0}°", v * 360.0),
        "División" => pick(&["1/1", "1/2", "1/4", "1/8", "1/16", "1/8 ·"]),
        "Volumen" => format!("{v:+.1} dB"),
        "Frecuencia" | "Corte" | "Tono" | "Frec. graves" | "Frec. medios" | "Frec. agudos" | "Corte graves" | "Corte agudos" | "Frec. eses" | "Cruce graves" | "Cruce agudos" | "Frec. plosivas" | "Frec. consonantes" => hz(v),
        "Q medios" => format!("Q {v:.2}"),
        "Desafinar" => format!("{v:.0} ct"),
        "Ratio" => format!("{v:.1}:1"),
        "Caída" | "Liberación" => format!("{v:.0} ms"),
        n if n.ends_with(" Hz") => hz(v),
        n if n.ends_with(" Q") => format!("Q {v:.2}"),
        n if n.ends_with("dB") => format!("{v:+.1} dB"),
        n if n.ends_with("ms") => format!("{v:.1} ms"),
        n if n.ends_with(" s") => format!("{v:.1} s"),
        _ => format!("{:.0} %", v * 100.0),
    }
}

/// Perillas en una cuadrícula que reparte el ancho: tantas columnas como entren, separadas por igual
/// y alineadas fila a fila (la última fila queda bajo las primeras columnas).
fn rejilla_perillas(ui: &mut egui::Ui, kind: Option<FxKind>, defs: &[(&str, f32, f32, f32)], params: &[AtomicF32], tam: f32) {
    // La celda deja lugar al valor y al nombre debajo de la perilla (ancho mínimo para los textos).
    let celda = (tam + 26.0).max(76.0);
    let ancho = ui.available_width();
    let columnas = (((ancho + 4.0) / (celda + 4.0)).floor() as usize).clamp(1, defs.len().max(1));
    let hueco = if columnas > 1 { ((ancho - celda * columnas as f32) / (columnas - 1) as f32).max(0.0) } else { 0.0 };
    for (f, fila) in defs.chunks(columnas).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = hueco;
            for (k, def) in fila.iter().enumerate() {
                ui.allocate_ui(vec2(celda, tam + 44.0), |ui| perilla(ui, kind, *def, &params[f * columnas + k], tam));
            }
        });
    }
}

/// Perilla de consola: escala logarítmica en rangos amplios (frecuencias, tiempos), color del plugin.
pub fn param_knob(ui: &mut egui::Ui, kind: Option<FxKind>, def: (&str, f32, f32, f32), a: &AtomicF32) {
    perilla(ui, kind, def, a, 58.0);
}

/// Valor de un parámetro para la posición `t` (0..1) de su perilla: escala logarítmica en rangos
/// amplios y redondeo en los discretos. Lo usan la perilla y los controles MIDI asignados.
pub fn valor_de_perilla((name, min, max, _): (&str, f32, f32, f32), t: f32) -> f32 {
    let v = if min > 0.0 && max / min > 10.0 { min * (max / min).powf(t) } else { min + t * (max - min) };
    let discreto = DISCRETE.contains(&name) || engine::lv2::param(name).is_some_and(|m| m.entero || m.conmutador);
    if discreto { v.round() } else { v }
}

/// Nombre de un parámetro sin la unidad final (dB, ms, Hz, s).
fn sin_unidad(n: &str) -> String {
    [" dB", " ms", " Hz", " s"].iter().find_map(|u| n.strip_suffix(u)).unwrap_or(n).to_string()
}

/// Botón de encendido para los parámetros Sí/No, del mismo alto que una perilla con su texto.
fn conmutador(ui: &mut egui::Ui, kind: Option<FxKind>, name: &str, a: &AtomicF32, size: f32) {
    let color = kind_color(kind);
    ui.allocate_ui(vec2(size + 26.0, size + 44.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let tam = vec2(size + 6.0, (size * 0.55).max(22.0));
            let resto = (size + 14.0 - tam.y).max(0.0);
            ui.add_space(resto * 0.6);
            let on = a.get() >= 0.5;
            let (r, resp) = ui.allocate_exact_size(tam, Sense::click());
            let p = ui.painter();
            let fondo = if on { color.gamma_multiply(0.85) } else if resp.hovered() { ELEVATED() } else { BG() };
            p.rect(r, rr(4.0), fondo, Stroke::new(1.0, if on { color } else { BORDER() }), egui::StrokeKind::Inside);
            p.circle_filled(r.left_center() + vec2(10.0, 0.0), 3.5, if on { Color32::WHITE } else { TEXT_DIM() });
            p.text(r.center() + vec2(5.0, 0.0), Align2::CENTER_CENTER, tr(if on { "Sí" } else { "No" }), FontId::proportional(12.0), if on { Color32::WHITE } else { TEXT_DIM() });
            if resp.clicked() {
                a.set(if on { 0.0 } else { 1.0 });
            }
            crate::controlador::menu_perilla(&resp, a);
            ui.add_space(resto * 0.4);
            let nombre = sin_unidad(tr(name));
            let ancho = ui.painter().layout_no_wrap(nombre.clone(), egui::FontId::proportional(10.5), TEXT()).size().x;
            let t = if ancho > ui.available_width() { (10.5 * ui.available_width() / ancho).max(8.5) } else { 10.5 };
            ui.add(egui::Label::new(RichText::new(nombre).size(t).color(TEXT())).extend());
        });
    });
}

/// Opciones con nombre (Modo, Forma, Escala, División…): un menú desplegable con el valor elegido, del
/// mismo tamaño que una perilla con su texto.
fn selector(ui: &mut egui::Ui, kind: Option<FxKind>, (name, min, max, _): (&str, f32, f32, f32), a: &AtomicF32, size: f32) {
    let color = kind_color(kind);
    ui.allocate_ui(vec2(size + 26.0, size + 44.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            // Mismo alto que perilla + valor: los nombres quedan en la misma línea que los de las perillas.
            let resto = (size + 14.0 - 24.0).max(0.0);
            ui.add_space(resto * 0.6);
            let actual = a.get().round();
            let texto = RichText::new(value_text(kind, name, actual)).size(11.5).strong().color(color);
            let id = egui::Id::new(("selector", name, a as *const AtomicF32 as usize));
            let r = egui::ComboBox::from_id_salt(id).width(size + 14.0).selected_text(texto).show_ui(ui, |ui| {
                widgets::menu_style(ui);
                let mut k = min.round();
                while k <= max.round() {
                    if ui.selectable_label(actual == k, value_text(kind, name, k)).clicked() {
                        a.set(k);
                    }
                    k += 1.0;
                }
            });
            crate::controlador::menu_perilla(&r.response, a);
            ui.add_space(resto * 0.4);
            let nombre = sin_unidad(tr(name));
            let ancho = ui.painter().layout_no_wrap(nombre.clone(), egui::FontId::proportional(10.5), TEXT()).size().x;
            let t = if ancho > ui.available_width() { (10.5 * ui.available_width() / ancho).max(8.5) } else { 10.5 };
            ui.add(egui::Label::new(RichText::new(nombre).size(t).color(TEXT())).extend());
        });
    });
}

/// Perilla de un parámetro con su valor y su nombre, del tamaño indicado.
pub fn perilla(ui: &mut egui::Ui, kind: Option<FxKind>, (name, min, max, def): (&str, f32, f32, f32), a: &AtomicF32, size: f32) {
    if CONMUTADORES.contains(&name) && min == 0.0 && max == 1.0 {
        return conmutador(ui, kind, name, a, size);
    }
    // Opciones con nombre: menú. (Los LV2 externos siguen con su perilla.)
    if DISCRETE.contains(&name) && kind != Some(FxKind::Externo) && max - min <= 12.0 {
        return selector(ui, kind, (name, min, max, def), a, size);
    }
    let log = min > 0.0 && max / min > 10.0;
    let to_t = |v: f32| {
        if log { (v / min).ln() / (max / min).ln() } else { (v - min) / (max - min) }
    };
    let color = kind_color(kind);
    ui.allocate_ui(vec2(size + 26.0, size + 44.0), |ui| {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            let mut t = to_t(a.get());
            let r = widgets::console_knob_size(ui, &mut t, to_t(def), color, size);
            if r.dragged() || r.double_clicked() {
                a.set(valor_de_perilla((name, min, max, def), t));
            }
            crate::controlador::menu_perilla(&r, a);
            ui.label(RichText::new(value_text(kind, name, a.get())).monospace().size(11.0).strong().color(color));
            // Nombre en una sola línea: si no entra en la celda, la letra se achica un poco (no se corta).
            // La unidad ya está en el valor: el nombre va sin ella («Corte», no «Corte Hz»).
            let nombre = sin_unidad(tr(name));
            let ancho = ui.painter().layout_no_wrap(nombre.clone(), egui::FontId::proportional(10.5), TEXT()).size().x;
            let tam = if ancho > ui.available_width() { (10.5 * ui.available_width() / ancho).max(8.5) } else { 10.5 };
            ui.add(egui::Label::new(RichText::new(nombre).size(tam).color(TEXT())).extend());
        });
    });
}

/// Altura de un nivel en dB (-48 abajo, 0 arriba).
fn y_db(r: Rect, db: f32) -> f32 {
    r.bottom() - ((db + 48.0) / 48.0).clamp(0.0, 1.0) * r.height()
}

/// Historial de unos 4 segundos: entrada (relleno gris), salida (línea del color del plugin), reducción
/// de ganancia (barras ámbar desde arriba, 1 dB = 2 % del alto) y una línea de umbral opcional.
fn historial(p: &egui::Painter, r: Rect, fx: &engine::Fx, color: Color32, fuerza: f32, umbral: Option<f32>) {
    let v = &fx.vivo;
    let (ent, sal, red) = (v.serie(&v.entrada), v.serie(&v.salida), v.serie(&v.reduccion));
    let n = ent.len().max(2);
    let x = |k: usize| r.left() + k as f32 / (n - 1) as f32 * r.width();
    let db = |g: f32| 20.0 * g.max(1e-6).log10();
    if fuerza >= 1.0 {
        for d in [-12.0, -24.0, -36.0] {
            p.hline(r.x_range(), y_db(r, d), Stroke::new(1.0, PANEL()));
            p.text(pos2(r.left() + 2.0, y_db(r, d)), Align2::LEFT_BOTTOM, format!("{d:.0}"), FontId::monospace(8.5), TEXT_DIM().gamma_multiply(0.7));
        }
    }
    let base = r.bottom();
    let gris = Color32::from_gray(120).gamma_multiply(0.35 * fuerza);
    for (k, e) in ent.iter().enumerate() {
        let y = y_db(r, db(*e));
        if y < base {
            p.vline(x(k), y..=base, Stroke::new((r.width() / n as f32).max(1.0), gris));
        }
    }
    let linea: Vec<Pos2> = (0..n).map(|k| pos2(x(k), y_db(r, db(sal[k])))).collect();
    p.add(egui::Shape::line(linea, Stroke::new(1.5, color.gamma_multiply(fuerza))));
    if fuerza >= 1.0 {
        for (k, g) in red.iter().enumerate() {
            if *g > 0.05 {
                p.vline(x(k), r.top()..=r.top() + (g / 50.0).min(1.0) * r.height(), Stroke::new((r.width() / n as f32).max(1.0), METER[1].gamma_multiply(0.75)));
            }
        }
    }
    if let Some(u) = umbral {
        let y = y_db(r, u);
        let mut xx = r.left();
        while xx < r.right() {
            p.hline(xx..=(xx + 5.0).min(r.right()), y, Stroke::new(1.0, TEXT()));
            xx += 9.0;
        }
    }
}

/// Medidores verticales finos de entrada (gris) y salida (verde, ámbar y rojo).
fn medidores_es(p: &egui::Painter, r: Rect, fx: &engine::Fx) {
    let w = (r.width() - 2.0) / 2.0;
    for (k, nivel) in fx.vivo.nivel.iter().enumerate() {
        let b = Rect::from_min_size(pos2(r.left() + k as f32 * (w + 2.0), r.top()), vec2(w, r.height()));
        p.rect_filled(b, 1.0, PANEL());
        let d = 20.0 * nivel.get().max(1e-6).log10();
        let c = if k == 0 { Color32::from_gray(140) } else { METER[(d > -12.0) as usize + (d > -3.0) as usize] };
        p.rect_filled(Rect::from_min_max(pos2(b.left(), y_db(b, d)), b.max), 1.0, c);
    }
}

/// Curva de transferencia del compresor (entrada → salida, -48 a 0 dB) con el punto del nivel actual.
fn transferencia(p: &egui::Painter, r: Rect, prm: &[f32], nivel: f32, color: Color32) {
    p.rect_stroke(r, rr(3.0), Stroke::new(1.0, PANEL()), egui::StrokeKind::Inside);
    p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.0, PANEL()));
    let (umbral, ratio, ganancia, rodilla) = (prm[0], prm[1].max(1.0), prm[2], prm[5].max(0.01));
    let sal = |x: f32| {
        let o = x - umbral;
        let red = match o {
            o if o <= -rodilla / 2.0 => 0.0,
            o if o < rodilla / 2.0 => (1.0 - 1.0 / ratio) * (o + rodilla / 2.0).powi(2) / (2.0 * rodilla),
            o => (1.0 - 1.0 / ratio) * o,
        };
        x - red + ganancia
    };
    let a = |db: f32| ((db + 48.0) / 48.0).clamp(0.0, 1.0);
    let pt = |x: f32| pos2(r.left() + a(x) * r.width(), r.bottom() - a(sal(x)) * r.height());
    let pts: Vec<Pos2> = (0..=48).map(|k| pt(-48.0 + k as f32)).collect();
    p.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
    let xu = r.left() + a(umbral) * r.width();
    p.vline(xu, r.y_range(), Stroke::new(1.0, TEXT_DIM().gamma_multiply(0.5)));
    let d = 20.0 * nivel.max(1e-6).log10();
    if d > -48.0 {
        p.circle_filled(pt(d.min(0.0)), 4.0, Color32::WHITE);
    }
}

/// Visor de cada plugin: curvas de respuesta, medidores o la nota detectada.
#[allow(clippy::too_many_arguments)]
fn display(ui: &mut egui::Ui, kind: Option<FxKind>, p: &[f32], readout: [f32; 2], color: Color32, sr: f32, alto: f32, fx: Option<&engine::Fx>) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), alto), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, rr(8.0), BG());
    let r = rect.shrink(8.0);
    // En vivo: la interfaz se redibuja sola mientras el visor está a la vista.
    if fx.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
    }
    let dinamica = matches!(kind, Some(FxKind::Compressor | FxKind::Limiter | FxKind::Gate | FxKind::DeEsser));
    // Historial de la señal detrás del dibujo (entrada gris, salida del color del plugin), salvo en los
    // de dinámica, que lo dibujan con su umbral y su reducción, y en los que tienen visor propio.
    let propio = matches!(kind, Some(FxKind::Tune | FxKind::Afinador | FxKind::AntiPop | FxKind::VozAlFrente | FxKind::PuertaRitmica | FxKind::Utilidad | FxKind::Externo) | None);
    // Los de eje de frecuencia o de forma (EQ, filtro DJ, saturación, imagen, desplazador) no llevan
    // historial en el tiempo detrás: medidores de entrada y salida a la derecha.
    let por_frecuencia = matches!(kind, Some(FxKind::Eq | FxKind::FiltroDj | FxKind::Drive | FxKind::Ancho | FxKind::Desplazador));
    let r = match fx {
        Some(fx) if por_frecuencia => {
            medidores_es(&painter, Rect::from_min_max(pos2(r.right() - 14.0, r.top()), r.max), fx);
            Rect::from_min_max(r.min, pos2(r.right() - 20.0, r.bottom()))
        }
        Some(fx) if !dinamica && !propio => {
            historial(&painter, r, fx, color, 0.35, None);
            r
        }
        _ => r,
    };
    let line = |pts: Vec<Pos2>| painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
    let curve = |f: &dyn Fn(f32) -> f32| line((0..=120).map(|k| k as f32 / 120.0).map(|t| pos2(r.left() + t * r.width(), r.bottom() - f(t).clamp(0.0, 1.0) * r.height())).collect());
    // Textos del visor sobre una placa del color del fondo: se leen aunque pase una curva por debajo.
    let label = |pos: Pos2, align: Align2, text: String, size: f32, c: Color32| {
        let g = painter.layout_no_wrap(text, FontId::proportional(size), c);
        let caja = align.anchor_size(pos, g.size()).expand2(vec2(4.0, 1.0));
        painter.rect_filled(caja, rr(3.0), BG());
        painter.galley(caja.min + vec2(4.0, 1.0), g, c);
        caja
    };
    painter.hline(r.x_range(), r.center().y, Stroke::new(1.0, PANEL()));
    match kind {
        Some(FxKind::Eq) => {
            curve(&|t| 0.5 + eq_response(p, 20.0 * 1000f32.powf(t), sr) / 36.0);
            for (f, text) in [(100.0, "100"), (1000.0, "1k"), (10000.0, "10k")] {
                let x = r.left() + (f / 20.0f32).log10() / 3.0 * r.width();
                painter.vline(x, r.y_range(), Stroke::new(1.0, PANEL()));
                label(pos2(x + 3.0, r.bottom()), Align2::LEFT_BOTTOM, text.into(), 9.0, TEXT_DIM());
            }
        }
        // Vista previa de Curves (en la biblioteca): curva del EQ; el visor completo está en la ventana del plugin.
        Some(FxKind::Curves) => {
            curve(&|t| 0.5 + engine::curves_response(p, 20.0 * 1000f32.powf(t), sr) / 48.0);
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, tr("espectro, nodos y resonancias en el plugin").into(), 10.0, TEXT_DIM());
        }
        Some(FxKind::Compressor) => {
            // Izquierda: historial con el umbral y la reducción; derecha: curva de transferencia con el
            // punto del nivel actual sobre ella.
            let lado = r.height();
            let curva = Rect::from_min_size(pos2(r.right() - lado, r.top()), vec2(lado, lado));
            let hist = Rect::from_min_max(r.min, pos2(curva.left() - 8.0, r.bottom()));
            match fx {
                Some(fx) => historial(&painter, hist, fx, color, 1.0, Some(p[0])),
                None => _ = painter.hline(hist.x_range(), y_db(hist, p[0]), Stroke::new(1.0, color.gamma_multiply(0.6))),
            }
            transferencia(&painter, curva, p, fx.map_or(0.0, |f| f.vivo.nivel[0].get()), color);
            label(pos2(hist.right(), hist.top()), Align2::RIGHT_TOP, format!("GR -{:.1} dB", readout[0].abs()), 10.5, METER[1]);
        }
        Some(FxKind::Delay) => {
            for k in 0..10 {
                let x = r.left() + (k as f32 * p[0] / 2000.0) * r.width();
                let h = if k == 0 { 1.0 } else { p[2] * p[1].powi(k - 1) };
                if x < r.right() {
                    painter.vline(x, r.bottom() - h * r.height()..=r.bottom(), Stroke::new(3.0, color));
                }
            }
        }
        // ANTIPOP: dos medidores horizontales con lo que está bajando ahora (P/B graves, T/K agudos).
        Some(FxKind::AntiPop) => {
            label(pos2(r.right() - 4.0, r.top() + 2.0), Align2::RIGHT_TOP, format!("{} {:.1} dB", tr("Techo"), p[5]), 10.5, TEXT_DIM());
            let alto = ((r.height() - 18.0) / 2.0).clamp(8.0, 22.0);
            for (k, (nombre, maximo)) in [("P · B", 18.0), ("T · K", 10.0)].into_iter().enumerate() {
                let y = r.top() + 16.0 + k as f32 * (alto + 4.0);
                let barra = Rect::from_min_max(pos2(r.left() + 46.0, y), pos2(r.right() - 64.0, y + alto));
                painter.rect_filled(barra, rr(3.0), Color32::from_white_alpha(10));
                let f = (readout[k] / maximo).clamp(0.0, 1.0);
                painter.rect_filled(Rect::from_min_max(barra.min, pos2(barra.left() + f * barra.width(), barra.bottom())), rr(3.0), color);
                label(pos2(r.left() + 6.0, barra.center().y), Align2::LEFT_CENTER, nombre.into(), 11.5, TEXT());
                label(pos2(r.right() - 6.0, barra.center().y), Align2::RIGHT_CENTER, if readout[k] < 0.05 { "0.0 dB".into() } else { format!("-{:.1} dB", readout[k]) }, 11.0, color);
            }
        }
        // Dinámica con lectura de reducción de ganancia.
        Some(k @ (FxKind::Limiter | FxKind::Gate | FxKind::DeEsser)) => {
            // Historial con la línea que importa (techo del limitador, umbral de la puerta) y la reducción.
            let linea = match k {
                FxKind::Limiter => Some(p[0]),
                FxKind::Gate => Some(p[0]),
                _ => None,
            };
            match fx {
                Some(fx) => historial(&painter, r, fx, color, 1.0, linea),
                None => {
                    if let Some(l) = linea {
                        painter.hline(r.x_range(), y_db(r, l), Stroke::new(1.0, color.gamma_multiply(0.6)));
                    }
                }
            }
            let texto = match (k, fx) {
                // La puerta: abierta o cerrada según su reducción (sin señal está cerrada, no «-60 dB»).
                (FxKind::Gate, Some(f)) if f.vivo.nivel[0].get() < 1e-4 => tr("Sin señal").to_string(),
                (FxKind::Gate, _) if readout[0] < 1.0 => tr("Abierta").to_string(),
                (FxKind::Gate, _) => format!("{} · -{:.0} dB", tr("Cerrada"), readout[0]),
                _ => format!("GR -{:.1} dB", readout[0].abs()),
            };
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, texto, 10.5, METER[1]);
        }
        Some(FxKind::Chorus) => {
            for ch in 0..2 {
                curve(&|t| 0.5 + 0.35 * p[1] / 15.0 * (std::f32::consts::TAU * (t * 2.0 + ch as f32 * 0.25 * p[3])).sin());
            }
        }
        Some(FxKind::Reverb) => {
            // Caída de la cola en dB (recta hasta -60 dB en el RT60) y el tiempo de reverberación.
            let rt = 0.4 * 20f32.powf(p[0].clamp(0.0, 1.0));
            let total = (rt * 1.3).max(1.0);
            let x = |t: f32| r.left() + t / total * r.width();
            let y = |db: f32| r.top() + 12.0 + (-db / 60.0).clamp(0.0, 1.0) * (r.height() - 14.0);
            let pre = p[3] / 1000.0;
            let mut pts = vec![pos2(x(0.0), y(-60.0)), pos2(x(pre), y(-60.0)), pos2(x(pre), y(-6.0))];
            pts.push(pos2(x(pre + rt), y(-60.0)));
            pts.push(pos2(r.right(), y(-60.0)));
            painter.add(egui::Shape::line(pts, Stroke::new(2.0, color)));
            // Los agudos se apagan antes cuanto más amortiguación.
            let rt_agudos = rt * (1.0 - 0.75 * p[1]);
            painter.line_segment([pos2(x(pre), y(-6.0)), pos2(x(pre + rt_agudos), y(-60.0))], Stroke::new(1.0, color.gamma_multiply(0.45)));
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, format!("RT60 {rt:.1} s"), 11.0, color);
        }
        Some(FxKind::Drive) => {
            let g = 10f32.powf(p[0] / 20.0);
            curve(&|t| {
                let x = t * 2.0 - 1.0;
                (x + ((x * g).tanh() / g.sqrt() - x) * p[1] + 1.0) / 2.0
            });
        }
        Some(FxKind::Tune) => {
            let name = |n: f32| format!("{}{}", NOTE_NAMES[(n.round() as i32).rem_euclid(12) as usize], (n.round() as i32) / 12 - 1);
            if readout[0] > 0.0 {
                let cents = ((readout[0] - readout[1]) * 100.0).round();
                label(pos2(r.center().x - 60.0, r.center().y), Align2::CENTER_CENTER, name(readout[0]), 30.0, TEXT_DIM());
                // Flecha dibujada (la fuente no siempre trae «→»).
                let c = r.center();
                painter.line_segment([c - vec2(12.0, 0.0), c + vec2(10.0, 0.0)], Stroke::new(2.0, TEXT_DIM()));
                painter.add(egui::Shape::convex_polygon(vec![c + vec2(14.0, 0.0), c + vec2(6.0, -5.0), c + vec2(6.0, 5.0)], TEXT_DIM(), Stroke::NONE));
                label(pos2(r.center().x + 60.0, r.center().y), Align2::CENTER_CENTER, name(readout[1]), 30.0, color);
                label(pos2(r.center().x, r.bottom()), Align2::CENTER_BOTTOM, format!("{cents:+} cents corregidos"), 11.0, TEXT_DIM());
            } else {
                label(r.center(), Align2::CENTER_CENTER, "Canta o toca una nota para ver la corrección".into(), 13.0, TEXT_DIM());
            }
        }
        Some(FxKind::VozAlFrente) => {
            // Caída de la sala en dB a lo largo del tiempo (recta gris hasta -60 dB en el RT60) y lo que
            // queda con el plugin (color): los primeros 50 ms (la voz directa) intactos y luego la cola más
            // baja. Escala: 0 a -60 dB arriba-abajo; 0 a 1,5 × RT60 de izquierda a derecha.
            let rt = p[1].max(0.2);
            let y = |db: f32| r.top() + 14.0 + (-db / 60.0).clamp(0.0, 1.0) * (r.height() - 18.0);
            let x = |t: f32| r.left() + t / (1.5 * rt) * r.width();
            for db in [-20.0, -40.0] {
                painter.hline(r.x_range(), y(db), Stroke::new(1.0, PANEL()));
                label(pos2(r.left(), y(db)), Align2::LEFT_CENTER, format!("{db:.0}"), 9.0, TEXT_DIM());
            }
            let sala = |t: f32| -60.0 * t / rt;
            let pts = |f: &dyn Fn(f32) -> f32| -> Vec<Pos2> { (0..=90).map(|i| i as f32 / 90.0 * 1.5 * rt).map(|t| pos2(x(t), y(f(t)))).collect() };
            painter.add(egui::Shape::line(pts(&sala), Stroke::new(1.5, TEXT_DIM())));
            let quitar = 30.0 * p[0];
            painter.add(egui::Shape::line(pts(&|t: f32| sala(t) - quitar * ((t - 0.05) / 0.1).clamp(0.0, 1.0)), Stroke::new(2.0, color)));
            painter.vline(x(0.05), r.y_range(), Stroke::new(1.0, color.gamma_multiply(0.35)));
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, format!("{} -{:.1} dB", tr("Eco quitado"), readout[0].max(0.0)), 11.0, color);
            label(pos2(x(0.05) + 4.0, r.top()), Align2::LEFT_TOP, format!("{} · {} {:.1} s", tr("Voz directa"), tr("Sala"), rt), 10.0, TEXT_DIM());
        }
        Some(FxKind::Ancho) => {
            // Lissajous esquemático: una elipse tan ancha como la imagen, y la correlación abajo.
            let w = (p[0] / 2.0).clamp(0.02, 1.0);
            let c = r.center();
            let pts: Vec<Pos2> = (0..=64).map(|k| k as f32 / 64.0 * std::f32::consts::TAU).map(|a| c + vec2(a.cos() * r.width() * 0.3 * w, a.sin() * r.height() * 0.42)).collect();
            line(pts);
            painter.vline(c.x, r.y_range(), Stroke::new(1.0, PANEL()));
            let corr = readout[0];
            let barra = Rect::from_min_max(pos2(r.left() + 30.0, r.bottom() - 6.0), pos2(r.right() - 30.0, r.bottom() - 2.0));
            painter.rect_filled(barra, rr(2.0), PANEL());
            let x = barra.center().x + corr.clamp(-1.0, 1.0) * barra.width() / 2.0;
            painter.rect_filled(Rect::from_center_size(pos2(x, barra.center().y), vec2(4.0, 10.0)), 1.0, if corr < 0.0 { METER[2] } else { color });
            label(pos2(r.left(), r.bottom()), Align2::LEFT_BOTTOM, "-1".into(), 9.0, TEXT_DIM());
            label(pos2(r.right(), r.bottom()), Align2::RIGHT_BOTTOM, "+1".into(), 9.0, TEXT_DIM());
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, format!("{} {corr:+.2}", tr("Correlación")), 10.5, if corr < 0.0 { METER[2] } else { TEXT_DIM() });
        }
        Some(FxKind::AutoPan) => {
            let forma = p[2].round() as u8;
            curve(&|t| {
                let t = (t * 2.0 + p[3]).fract();
                let v = match forma {
                    1 => 1.0 - 4.0 * (t - 0.5).abs(),
                    2 => if t < 0.5 { 1.0 } else { -1.0 },
                    _ => (std::f32::consts::TAU * t).sin(),
                };
                0.5 + 0.45 * v * p[1]
            });
            let x = r.center().x + readout[0] * r.width() / 2.0;
            painter.circle_filled(pos2(x, r.top() + 6.0), 4.0, color);
            label(pos2(r.left(), r.top()), Align2::LEFT_TOP, "L".into(), 10.0, TEXT_DIM());
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, "R".into(), 10.0, TEXT_DIM());
        }
        Some(FxKind::FiltroDj) => {
            let v = p[0];
            let q = 1.0 + p[1] * 0.25;
            curve(&|t| {
                let f = 20.0 * 1000f32.powf(t);
                let resp = if v < -0.02 {
                    let fc = 20000.0 * 0.01f32.powf(-v);
                    1.0 / (1.0 + (f / fc).powi(4)).sqrt() * if (f / fc - 1.0).abs() < 0.3 { q } else { 1.0 }
                } else if v > 0.02 {
                    let fc = 20.0 * 500f32.powf(v);
                    1.0 / (1.0 + (fc / f).powi(4)).sqrt() * if (f / fc - 1.0).abs() < 0.3 { q } else { 1.0 }
                } else {
                    1.0
                };
                resp * 0.75
            });
            label(pos2(r.center().x, r.top()), Align2::CENTER_TOP, value_text(kind, "Filtro", v), 11.0, color);
        }
        Some(FxKind::AutoFiltro) => {
            curve(&|t| 0.5 + 0.4 * p[4] * (std::f32::consts::TAU * t * 2.0).sin());
            let corte = if readout[0] > 0.0 { readout[0] } else { p[1] };
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, format!("{} {}", tr("Corte"), value_text(kind, "Corte Hz", corte)), 11.0, color);
        }
        Some(FxKind::PuertaRitmica) => {
            let patron = engine::fx_nuevos::PATRONES[(p[0].round().max(0.0) as usize).min(7)];
            let w = r.width() / 16.0;
            for k in 0..16 {
                let on = patron >> (15 - k) & 1 == 1;
                let celda = Rect::from_min_size(pos2(r.left() + k as f32 * w, r.top() + 4.0), vec2(w - 2.0, r.height() - 8.0));
                let actual = readout[0].round() as usize == k;
                let fondo = if on { color.gamma_multiply(if actual { 1.0 } else { 0.6 }) } else { PANEL().gamma_multiply(if actual { 1.6 } else { 1.0 }) };
                painter.rect_filled(celda, rr(2.0), fondo);
            }
        }
        Some(FxKind::Phaser | FxKind::Flanger) => {
            for ch in 0..2 {
                curve(&|t| 0.5 + 0.38 * p[1].min(1.0) * (std::f32::consts::TAU * (t * 2.0 + ch as f32 * 0.25)).sin());
            }
        }
        Some(FxKind::LoFi) => {
            let niveles = 2f32.powf(p[0].round() - 1.0);
            let salto = p[1].max(1.0) / 64.0;
            curve(&|t| {
                let t = (t / salto).floor() * salto;
                0.5 + 0.4 * ((std::f32::consts::TAU * t * 1.5).sin() * niveles).round() / niveles
            });
        }
        Some(FxKind::TapeStop) => {
            curve(&|t| 0.08 + 0.84 * (1.0 - t.powf(1.5 * (p[1] / 800.0).max(0.2))).max(0.0));
            let vel = readout[0];
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, format!("{} {:.0} %", tr("Velocidad"), vel * 100.0), 11.0, if p[0] >= 0.5 { METER[2] } else { TEXT_DIM() });
        }
        Some(FxKind::Utilidad) => {
            let db = readout[0];
            let barra = Rect::from_min_max(pos2(r.left() + 6.0, r.center().y - 6.0), pos2(r.right() - 70.0, r.center().y + 6.0));
            painter.rect_filled(barra, rr(2.0), PANEL());
            let f = ((db + 60.0) / 66.0).clamp(0.0, 1.0);
            painter.rect_filled(Rect::from_min_max(barra.min, pos2(barra.left() + f * barra.width(), barra.bottom())), rr(2.0), METER[(db > -12.0) as usize + (db > -3.0) as usize]);
            label(pos2(r.right(), r.center().y), Align2::RIGHT_CENTER, if db < -100.0 { "-∞ dB".into() } else { format!("{db:+.1} dB") }, 12.0, TEXT());
            label(pos2(r.left(), r.top()), Align2::LEFT_TOP, value_text(kind, "Ganancia dB", p[0]), 11.0, color);
        }
        Some(FxKind::Transitorios) => {
            curve(&|t| {
                let golpe = (-t * 30.0).exp() * (1.0 + p[0] * 0.8);
                let cola = (-t * 4.0).exp() * 0.4 * (1.0 + p[1] * 0.8);
                (golpe.max(cola)).min(1.0) * 0.9
            });
        }
        Some(FxKind::Desplazador) => {
            let x = r.center().x + (p[0] / 2000.0) * r.width() * 0.45;
            painter.vline(r.center().x, r.y_range(), Stroke::new(1.0, TEXT_DIM()));
            painter.line_segment([pos2(r.center().x, r.center().y), pos2(x, r.center().y)], Stroke::new(3.0, color));
            painter.circle_filled(pos2(x, r.center().y), 5.0, color);
            label(pos2(r.center().x, r.top()), Align2::CENTER_TOP, format!("{:+.0} Hz", p[0]), 12.0, color);
        }
        Some(FxKind::Afinador) => {
            let n = readout[0];
            if n > 0.0 {
                let nota = n.round();
                let cents = (n - nota) * 100.0;
                let nombre = format!("{}{}", NOTE_NAMES[(nota as i32).rem_euclid(12) as usize], nota as i32 / 12 - 1);
                let ok = cents.abs() < 5.0;
                label(pos2(r.center().x, r.top() + 4.0), Align2::CENTER_TOP, nombre, 26.0, if ok { METER[0] } else { TEXT() });
                let y = r.bottom() - 10.0;
                painter.hline(r.x_range().shrink(20.0), y, Stroke::new(1.0, PANEL()));
                painter.vline(r.center().x, y - 6.0..=y + 6.0, Stroke::new(1.0, TEXT_DIM()));
                let x = r.center().x + (cents / 50.0).clamp(-1.0, 1.0) * (r.width() / 2.0 - 20.0);
                painter.rect_filled(Rect::from_center_size(pos2(x, y), vec2(4.0, 14.0)), 1.0, if ok { METER[0] } else { METER[1] });
                label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, format!("{cents:+.0} ct"), 11.0, TEXT_DIM());
            } else {
                label(r.center(), Align2::CENTER_CENTER, tr("Toca o canta una nota").into(), 12.0, TEXT_DIM());
            }
        }
        Some(k @ (FxKind::TapePro | FxKind::SpacePro | FxKind::VocalPro | FxKind::MasterPro)) => {
            label(r.center() - vec2(0.0, 10.0), Align2::CENTER_CENTER, tr(k.name()).trim_start_matches("QUANTUM ").to_uppercase(), 22.0, color);
            let estado = if engine::pro::licenciado(k) { tr("Con licencia").to_string() } else { format!("{} · {} US$", tr("Modo demo"), k.pro().map_or(0, |p| p.2)) };
            label(r.center() + vec2(0.0, 18.0), Align2::CENTER_CENTER, estado, 11.0, TEXT_DIM());
        }
        // Los plugins LV2 muestran su carátula (ver `lv2ui::visor`).
        Some(FxKind::Externo) => {}
        None => {
            // Envolvente ADSR del sintetizador.
            let total = p[5] + p[6] + 400.0 + p[8];
            let x = |ms: f32| r.left() + ms / total * r.width();
            let (a, d, s) = (x(p[5]), x(p[5] + p[6]), x(p[5] + p[6] + 400.0));
            let y = |v: f32| r.bottom() - v * r.height();
            line(vec![pos2(r.left(), y(0.0)), pos2(a, y(1.0)), pos2(d, y(p[7])), pos2(s, y(p[7])), pos2(r.right(), y(0.0))]);
            label(pos2(r.right(), r.top()), Align2::RIGHT_TOP, value_text(None, "Onda", p[0]), 12.0, color);
        }
    }
}

impl App {
    pub fn fx_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.fx_window.filter(|&i| i < self.s.tracks.len()) else {
            return;
        };
        let title = format!("QUANTUM · {}", self.s.tracks[i].name);
        let open = self.ventana(ctx, "fx", title, [860.0, 900.0], true, |app, ui| app.cadena(ui, i, false));
        if !open {
            self.fx_window = None;
        }
        // Los visores (reducción de ganancia, nota detectada) se actualizan en vivo.
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    /// Instrumento y cadena de efectos de la pista `i`: en columna (ventana de plugins) o en fila,
    /// un dispositivo junto al otro (pestaña Dispositivos del panel inferior).
    pub fn cadena(&mut self, ui: &mut egui::Ui, i: usize, en_fila: bool) {
        enum Act {
            Add(FxKind),
            Detectar(usize),
            Sincronizar,
            Remove(usize),
            Up(usize),
            AddLv2(Arc<engine::lv2::Info>),
            MyPlugins,
            Licencias,
            Drums,
        }
        let bateria = Self::drummer_disponible() && self.es_pista_bateria(i);
        let (mut act, sr) = (None, self.engine.sample_rate as f32);
        // En fila, todas las tarjetas toman el alto de la más alta (medido en el cuadro anterior), sin
        // pasar del alto del panel; en la ventana de plugins cada una tiene su alto natural.
        let alto_max = std::cell::Cell::new(0.0f32);
        let alto_fila = if en_fila { self.alto_dispositivos.min((ui.available_height() - 4.0).max(0.0)) } else { 0.0 };
        let mut cuerpo = |app: &Self, ui: &mut egui::Ui| {
            let t = &app.s.tracks[i];
            // Pistas que pueden ser clave del sidechain de Curves (todas menos esta).
            let pistas: Vec<(u64, String)> = app.s.tracks.iter().filter(|x| x.id != t.id).map(|x| (x.id, x.name.clone())).collect();
            let plugin = |ui: &mut egui::Ui,
                          name: &str,
                          kind: Option<FxKind>,
                          params: &[AtomicF32],
                          defs: &[(&str, f32, f32, f32)],
                          bypass: Option<&AtomicBool>,
                          readout: [f32; 2],
                          j: Option<usize>,
                          fx: Option<&engine::Fx>,
                          act: &mut Option<Act>| {
                let lv2 = fx.and_then(|f| f.lv2.as_ref());
                let accent = lv2.map_or(kind_color(kind), |i| crate::lv2ui::color(i.categoria));
                let firma = lv2.map_or(SIGNATURE.to_string(), |i| i.marca.to_uppercase());
                let ancho = match (kind == Some(FxKind::Curves), en_fila) {
                    (true, true) => ANCHO_CURVES_FILA,
                    (true, false) => ANCHO_CURVES,
                    _ => ANCHO_DISPOSITIVO,
                };
                let alto_visor = if en_fila { 86.0 } else { 110.0 };
                // Cabecera: encendido (bypass), nombre con la marca y, a la derecha, subir y quitar.
                let mut accion_cabecera = None;
                let cabecera = |ui: &mut egui::Ui| {
                    if let Some(b) = bypass {
                        let on = !b.load(Relaxed);
                        if widgets::encendido(ui, on).on_hover_text(if on { tr("Activo · clic para bypass") } else { tr("En bypass · clic para activar") }).clicked() {
                            b.store(on, Relaxed);
                        }
                    }
                    ui.label(RichText::new(tr(name).trim_start_matches("QUANTUM ")).size(13.5).strong().color(TEXT()));
                    ui.label(RichText::new(&firma).size(9.0).strong().color(accent));
                    if let Some(j) = j {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::boton_cabecera(ui, widgets::draw_quitar, true).on_hover_text(tr("Quitar")).clicked() {
                                accion_cabecera = Some(Act::Remove(j));
                            }
                            if widgets::boton_cabecera(ui, widgets::draw_subir, j > 0).on_hover_text(tr("Subir en la cadena")).clicked() {
                                accion_cabecera = Some(Act::Up(j));
                            }
                        });
                    }
                };
                columna(ui, en_fila, ancho, |ui| {
                    let (_, natural) = widgets::dispositivo(ui, accent, alto_fila, cabecera, |ui| {
                    ui.set_width(ui.available_width());
                    let values: Vec<f32> = params.iter().map(|a| a.get()).collect();
                    match (kind, fx, j) {
                        // Plugins Pro: su interfaz propia (con todos sus controles).
                        (Some(k), Some(fx), Some(_)) if k.pro().is_some() => {
                            if let Some(licencias::PedidoPro::Licencias) = licencias::panel_pro(ui, fx) {
                                *act = Some(Act::Licencias);
                            }
                        }
                        // Curves: presets, detección y visor interactivo.
                        (Some(FxKind::Curves), Some(fx), Some(j)) => {
                            let (mut detectar, mut sincronizar) = (false, false);
                            curves::barra(ui, fx, &pistas, &mut detectar, &mut sincronizar);
                            if detectar {
                                *act = Some(Act::Detectar(j));
                            }
                            if sincronizar {
                                *act = Some(Act::Sincronizar);
                            }
                            curves::visor(ui, fx, sr, accent, if en_fila { 130.0 } else { 240.0 });
                        }
                        // Compresor y puerta: también pueden escuchar a otra pista (sidechain).
                        (Some(k @ (FxKind::Compressor | FxKind::Gate)), Some(fx), Some(_)) => {
                            let mut sincronizar = false;
                            let ayuda = if k == FxKind::Compressor { tr("La pista elegida baja esta pista al sonar (por ejemplo, el bombo agacha el bajo)") } else { tr("La puerta se abre cuando suena la pista elegida (por ejemplo, un golpe de batería)") };
                            curves::selector_sidechain(ui, fx, &pistas, &mut sincronizar, ayuda);
                            if sincronizar {
                                *act = Some(Act::Sincronizar);
                            }
                            display(ui, kind, &values, readout, accent, sr, alto_visor, Some(fx));
                        }
                        (Some(FxKind::Externo), _, _) if let Some(i) = lv2 => crate::lv2ui::visor(ui, i, alto_visor + 20.0),
                        _ => display(ui, kind, &values, readout, accent, sr, alto_visor, fx),
                    }
                    // Curves muestra solo los controles esenciales; el resto en «Avanzado».
                    let todos = match (kind, fx) {
                        (Some(FxKind::Curves), Some(fx)) => curves::controles(ui, fx, accent, en_fila),
                        (Some(k), Some(_)) if k.pro().is_some() => false,
                        _ => true,
                    };
                    let tam = if en_fila { PERILLA_FILA } else { PERILLA };
                    // Los plugins LV2 pueden tener decenas de controles: los primeros a la vista y el resto plegado.
                    const VISIBLES: usize = 12;
                    if todos && lv2.is_some() && defs.len() > VISIBLES {
                        ui.add_space(4.0);
                        rejilla_perillas(ui, kind, &defs[..VISIBLES], &params[..VISIBLES], tam);
                        let id = fx.map_or(0, |f| f as *const engine::Fx as usize);
                        egui::CollapsingHeader::new(RichText::new(format!("{} ({})", tr("Más controles"), defs.len() - VISIBLES)).size(11.5)).id_salt(("lv2-mas", id)).show(ui, |ui| {
                            rejilla_perillas(ui, kind, &defs[VISIBLES..], &params[VISIBLES..], tam);
                        });
                    } else if todos {
                        ui.add_space(4.0);
                        rejilla_perillas(ui, kind, defs, params, tam);
                    }
                    });
                    alto_max.set(alto_max.get().max(natural));
                });
                if accion_cabecera.is_some() {
                    *act = accion_cabecera;
                }
                ui.add_space(6.0);
            };
            if let Some(p) = &t.plugin {
                // Instrumento LV2: su carátula y sus controles, como un efecto más.
                if !en_fila {
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM()));
                }
                plugin(ui, p.nombre(), Some(FxKind::Externo), &p.params, p.defs(), Some(&p.bypass), [0.0; 2], None, Some(p), &mut act);
            } else if let Some(s) = &t.sampler {
                // Instrumento real (sampler): volumen y créditos de la biblioteca.
                columna(ui, en_fila, ANCHO_DISPOSITIVO, |ui| {
                if !en_fila {
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM()));
                }
                let e = instruments::find(&t.instrument);
                let nombre = if instruments::es_kit(&t.instrument) { instruments::nombre(&t.instrument) } else { e.map_or(s.name.clone(), |e| e.name.to_string()) };
                let mut abrir_drums = false;
                let cabecera = |ui: &mut egui::Ui| {
                    ui.label(RichText::new(&nombre).size(13.5).strong().color(TEXT()));
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(9.0).strong().color(ACCENT()));
                    if bateria {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let b = egui::Button::new(RichText::new(tr("Abrir Q Drums")).strong().color(BG())).fill(Color32::from_rgb(0xFF, 0x6B, 0x3D));
                            abrir_drums = ui.add(b).on_hover_text(tr("Ritmos, kit y piezas de la batería")).clicked();
                        });
                    }
                };
                let (_, natural) = widgets::dispositivo(ui, ACCENT(), alto_fila, cabecera, |ui| {
                    ui.set_width(ui.available_width());
                    let mut db = widgets::db(s.gain.get());
                    if ui.add(egui::Slider::new(&mut db, -30.0..=12.0).suffix(" dB").text(tr("Volumen"))).changed() {
                        s.gain.set(10f32.powf(db / 20.0));
                    }
                    if let Some(e) = e {
                        ui.label(RichText::new(format!("Muestras: {} · licencia {}", e.credit, e.license)).size(10.0).color(TEXT_DIM()));
                    }
                });
                alto_max.set(alto_max.get().max(natural));
                if abrir_drums {
                    act = Some(Act::Drums);
                }
                ui.add_space(6.0);
                });
            } else if let Some(s) = &t.synth {
                if !en_fila {
                    ui.label(RichText::new(tr("INSTRUMENTO")).size(10.0).color(TEXT_DIM()));
                }
                plugin(ui, "Synth", None, &s.params, &SYNTH_PARAMS, None, [0.0; 2], None, None, &mut act);
            }
            if !en_fila {
                ui.label(RichText::new(tr("EFECTOS DE AUDIO")).size(10.0).color(TEXT_DIM()));
            }
            for (j, fx) in t.fx.iter().enumerate() {
                let readout = [fx.readout[0].get(), fx.readout[1].get()];
                plugin(ui, fx.nombre(), Some(fx.kind), &fx.params, fx.defs(), Some(&fx.bypass), readout, Some(j), Some(fx), &mut act);
            }
            // Al final de la cadena: añadir un efecto (en fila, como una ranura vacía más angosta).
            let mut anadir = |ui: &mut egui::Ui| {
                if t.fx.is_empty() {
                    ui.label(RichText::new(tr("Sin efectos. Añade uno abajo o arrástralo desde la Biblioteca.")).color(TEXT_DIM()));
                }
                ui.menu_button(tr("+ Añadir efecto"), |ui| {
                    for k in FxKind::disponibles() {
                        if ui.button(k.name()).clicked() {
                            act = Some(Act::Add(k));
                        }
                    }
                    if let Some(info) = App::menu_efectos_lv2(ui) {
                        act = Some(Act::AddLv2(info));
                    }
                    ui.separator();
                    if ui.button(tr("Todos los plugins…")).on_hover_text(tr("Plugins LV2 instalados, en la biblioteca")).clicked() {
                        act = Some(Act::MyPlugins);
                    }
                });
            };
            if en_fila {
                // Ranura vacía al final de la fila: marco del alto del panel con el botón en el centro.
                let alto = if alto_fila > 0.0 { alto_fila } else { (ui.available_height() - 8.0).max(120.0) };
                ui.allocate_ui_with_layout(vec2(220.0, alto), egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.set_width(220.0);
                    ui.painter().rect_stroke(egui::Rect::from_min_size(ui.max_rect().min, vec2(220.0, alto)), widgets::rr(12.0), egui::Stroke::new(1.0, BORDER()), egui::StrokeKind::Inside);
                    ui.add_space((alto / 2.0 - 40.0).max(8.0));
                    ui.vertical_centered(|ui| {
                        ui.set_max_width(190.0);
                        anadir(ui);
                    });
                });
            } else {
                anadir(ui);
            }
        };
        if en_fila {
            egui::ScrollArea::horizontal().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_top(|ui| cuerpo(self, ui));
            });
        } else {
            cuerpo(self, ui);
        }
        if en_fila && (alto_max.get() - self.alto_dispositivos).abs() > 0.5 {
            self.alto_dispositivos = alto_max.get();
            ui.ctx().request_repaint();
        }
        match act {
            Some(Act::Add(k)) => self.add_fx(i, k),
            Some(Act::AddLv2(info)) => self.add_fx_lv2(i, &info),
            Some(Act::Detectar(j)) => self.status = self.curves_detectar(i, j),
            Some(Act::Sincronizar) => self.dirty = true,
            Some(Act::MyPlugins) => (self.show_lib, self.lib_tab) = (true, library::PLUGINS_TAB),
            Some(Act::Licencias) => self.licencias_abierta = true,
            Some(Act::Drums) => self.abrir_drummer(Some(i)),
            Some(Act::Remove(j)) => {
                self.edit();
                self.s.tracks[i].fx.remove(j);
            }
            Some(Act::Up(j)) => {
                self.edit();
                self.s.tracks[i].fx.swap(j, j - 1);
            }
            None => {}
        }
    }
}
