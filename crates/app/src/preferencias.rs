//! Preferencias del menú principal: audio, MIDI, comportamiento, guardado, atajos, interfaz, plugins,
//! espacios de trabajo, acerca de y tienda. Todas las páginas comparten el mismo diseño: título con su
//! descripción y el buscador a la derecha, tarjetas del mismo ancho y filas con la etiqueta en una
//! columna fija y el control alineado a su derecha.
use crate::{
    welcome::Welcome,
    widgets::{self, rr},
    *,
};
use egui::{Align2, FontId, Rect, RichText, Sense, Stroke, StrokeKind, pos2, vec2};

/// Ancho máximo de las tarjetas, columna de las etiquetas y alto de cada fila.
pub const ANCHO: f32 = 880.0;
const ETIQUETA: f32 = 210.0;
const ALTO: f32 = 32.0;

/// Secciones del menú principal: (pestaña, nombre, descripción bajo el título).
pub const SECCIONES: [(u8, &str, &str); 14] = [
    (0, "Inicio", ""),
    (3, "Nueva canción", "Nombre, formato, tempo y plantilla de la canción"),
    (2, "Abrir", "Proyectos y canciones de la carpeta de proyectos"),
    (1, "Datos de la canción", "Título, artista y derechos de la canción abierta"),
    (10, "Audio", "Dispositivos de entrada y salida, frecuencia, búfer y metrónomo"),
    (11, "MIDI", "Teclados y controladores MIDI que tocan las pistas"),
    (12, "Comportamiento", "Proyectos, qué se muestra al iniciar, pistas y biblioteca"),
    (13, "Guardado", "Autoguardado, copias de seguridad y registros"),
    (14, "Atajos", "Haz clic en un atajo y pulsa la nueva combinación"),
    (5, "Interfaz", "Tema de color, tamaño, idioma y forma de la interfaz"),
    (15, "Plugins", "Carpetas donde se buscan los plugins VST3, LV2 y CLAP"),
    (6, "Espacios de trabajo", "Cada forma de trabajar con los paneles que necesita a mano"),
    (4, "Acerca de", "Versión, sistema, licencias y enlaces"),
    (16, "Tienda", "Instrumentos, efectos y sonidos de quantumdaw.com"),
];

/// Opciones que encuentra el buscador: (pestaña, opción).
const INDICE: [(u8, &str); 40] = [
    (10, "Dispositivo de salida"),
    (10, "Dispositivo de entrada"),
    (10, "Mostrar salidas HDMI / DisplayPort"),
    (10, "Frecuencia de muestreo"),
    (10, "Tamaño de búfer"),
    (10, "Latencia"),
    (10, "Resolución de grabación"),
    (10, "Estado del motor de audio"),
    (10, "Volumen del clic"),
    (10, "Acento del primer tiempo"),
    (11, "Entrada MIDI"),
    (11, "Teclados y controladores MIDI"),
    (12, "Carpeta de proyectos"),
    (12, "Creado por"),
    (12, "Mostrar el panel al iniciar"),
    (12, "Botón de instrumento y efectos en las pistas"),
    (12, "Trabajar con proxys"),
    (12, "Previsualizar al hacer clic"),
    (13, "Autoguardado"),
    (13, "Intervalo de autoguardado"),
    (13, "Copias de seguridad"),
    (13, "Carpeta de registros"),
    (13, "Enviar informe de error"),
    (14, "Atajos de teclado"),
    (14, "Rueda del ratón"),
    (14, "Restaurar atajos"),
    (5, "Tema de color"),
    (5, "Tamaño de la interfaz"),
    (5, "Idioma"),
    (5, "Interfaz redondeada"),
    (15, "Carpetas de plugins"),
    (15, "Volver a buscar plugins"),
    (15, "Plugins ocultos"),
    (6, "Espacios de trabajo"),
    (4, "Versión"),
    (4, "Sistema y tarjeta gráfica"),
    (4, "Licencias QUANTUM Pro"),
    (16, "Tienda"),
    (3, "Plantilla de la canción"),
    (1, "Copyright"),
];

/// Configuración de audio y MIDI en edición: se aplica (y se reinicia el motor) con un botón.
pub struct Borrador {
    pub cfg: AudioConfig,
    pub bits: u16,
    pub midi: String,
    pub dispositivos: Vec<DeviceInfo>,
    pub puertos: Vec<String>,
}

/// Carpeta de plugins revisada: (ruta, formato, cantidad encontrada; None si la carpeta no existe).
pub type Carpeta = (PathBuf, &'static str, Option<usize>);

pub fn seccion(k: u8) -> (&'static str, &'static str) {
    SECCIONES.iter().find(|s| s.0 == k).map_or(("", ""), |s| (s.1, s.2))
}

/// Título y descripción de la página con el buscador de preferencias a la derecha y una línea debajo.
pub fn cabecera(ui: &mut egui::Ui, k: u8, buscar: &mut String) {
    let (titulo, descripcion) = seccion(k);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(RichText::new(tr(titulo)).size(22.0).strong().color(TEXT()));
            ui.label(RichText::new(tr(descripcion)).size(12.5).color(TEXT_DIM()));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(44.0);
            buscador(ui, buscar);
        });
    });
    ui.add_space(4.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, BORDER()));
    ui.add_space(8.0);
}

/// Campo de búsqueda con lupa: subrayado de acento mientras se escribe.
fn buscador(ui: &mut egui::Ui, texto: &mut String) {
    let (r, _) = ui.allocate_exact_size(vec2(320.0, 34.0), Sense::hover());
    let id = egui::Id::new("buscar-preferencias");
    let activo = ui.memory(|m| m.has_focus(id));
    let p = ui.painter();
    p.rect(r, rr(9.0), BG(), Stroke::new(1.0, if activo { ACCENT() } else { BORDER() }), StrokeKind::Inside);
    let lupa = pos2(r.left() + 18.0, r.center().y - 1.0);
    p.circle_stroke(lupa, 5.5, Stroke::new(1.6, TEXT_DIM()));
    p.line_segment([lupa + vec2(4.0, 4.0), lupa + vec2(8.5, 8.5)], Stroke::new(1.8, TEXT_DIM()));
    let campo = Rect::from_min_max(pos2(r.left() + 34.0, r.top() + 2.0), pos2(r.right() - 10.0, r.bottom() - 2.0));
    ui.put(campo, egui::TextEdit::singleline(texto).id(id).frame(egui::Frame::NONE).hint_text(tr("Buscar en las preferencias…")).vertical_align(egui::Align::Center).font(FontId::proportional(13.5)));
}

/// Texto en minúsculas y sin tildes, para que «bufer» encuentre «búfer».
pub fn sin_tildes(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Opciones que coinciden con la búsqueda; devuelve la sección elegida.
pub fn resultados(ui: &mut egui::Ui, buscar: &str) -> Option<u8> {
    let q = sin_tildes(buscar.trim());
    let mut elegida = None;
    let hallados: Vec<_> = INDICE.iter().filter(|(k, o)| sin_tildes(tr(o)).contains(&q) || sin_tildes(tr(seccion(*k).0)).contains(&q)).collect();
    tarjeta(ui, "RESULTADOS", |ui| {
        if hallados.is_empty() {
            ui.label(RichText::new(tr("Ninguna opción coincide con la búsqueda.")).color(TEXT_DIM()));
        }
        for (k, opcion) in hallados {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ALTO + 4.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(r, rr(8.0), BORDER());
            }
            let p = ui.painter();
            p.text(r.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, tr(seccion(*k).0), FontId::proportional(12.5), TEXT_DIM());
            p.text(r.left_center() + vec2(ETIQUETA, 0.0), Align2::LEFT_CENTER, tr(opcion), FontId::proportional(13.5), TEXT());
            p.text(r.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, "›", FontId::proportional(18.0), if resp.hovered() { ACCENT() } else { TEXT_DIM() });
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                elegida = Some(*k);
            }
        }
    });
    elegida
}

/// Tarjeta de una sección de la página: fondo elevado, título en versalitas y su contenido.
pub fn tarjeta(ui: &mut egui::Ui, titulo: &str, add: impl FnOnce(&mut egui::Ui)) {
    let ancho = ui.available_width().min(ANCHO);
    egui::Frame::new().fill(ELEVATED()).stroke(Stroke::new(1.0, BORDER())).corner_radius(rr(12.0)).inner_margin(egui::Margin { left: 18, right: 18, top: 14, bottom: 16 }).show(ui, |ui| {
        ui.set_width(ancho - 36.0);
        ui.spacing_mut().item_spacing = vec2(10.0, 8.0);
        ui.label(RichText::new(tr(titulo)).size(11.0).strong().color(TEXT_DIM()));
        ui.add_space(2.0);
        add(ui);
    });
    ui.add_space(14.0);
}

/// Fila de opción: etiqueta en la columna fija y el control ocupando el resto del ancho.
pub fn fila<R>(ui: &mut egui::Ui, etiqueta: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(ETIQUETA, ALTO), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, tr(etiqueta), FontId::proportional(13.5), TEXT());
        ui.allocate_ui_with_layout(vec2(ui.available_width(), ALTO), egui::Layout::left_to_right(egui::Align::Center), add).inner
    })
    .inner
}

/// Explicación bajo una opción, alineada con la columna de los controles.
pub fn nota(ui: &mut egui::Ui, texto: &str) {
    ui.horizontal(|ui| {
        ui.add_space(ETIQUETA + ui.spacing().item_spacing.x);
        ui.add(egui::Label::new(RichText::new(tr(texto)).size(11.5).color(TEXT_DIM())).wrap());
    });
}

/// Texto explicativo a lo ancho de la tarjeta.
pub fn parrafo(ui: &mut egui::Ui, texto: &str) {
    ui.add(egui::Label::new(RichText::new(tr(texto)).size(12.0).color(TEXT_DIM())).wrap());
}

/// Lista desplegable que ocupa todo el ancho del control.
pub fn desplegable(ui: &mut egui::Ui, id: &str, texto: impl Into<egui::WidgetText>, add: impl FnOnce(&mut egui::Ui)) {
    egui::ComboBox::from_id_salt(id).width(ui.available_width()).height(320.0).selected_text(texto).show_ui(ui, add);
}

/// Interruptor deslizante con «Sí» / «No» al lado; devuelve si cambió.
pub fn interruptor(ui: &mut egui::Ui, valor: &mut bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *valor = !*valor;
    }
    let t = ui.ctx().animate_bool(resp.id, *valor);
    let p = ui.painter();
    let fondo = if *valor { VERDE_PLAY } else { BORDER() };
    p.rect(r, 11.0, fondo, Stroke::new(1.0, if resp.hovered() { TEXT_DIM() } else { Color32::TRANSPARENT }), StrokeKind::Inside);
    p.circle_filled(pos2(r.left() + 11.0 + t * 18.0, r.center().y), 8.0, if *valor { BG() } else { TEXT() });
    ui.label(RichText::new(tr(if *valor { "Sí" } else { "No" })).size(12.5).color(TEXT_DIM()));
    resp.clicked()
}

/// Dato de solo lectura en una píldora: «etiqueta  valor».
pub fn dato(ui: &mut egui::Ui, etiqueta: &str, valor: &str) {
    egui::Frame::new().fill(BG()).corner_radius(rr(7.0)).inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(RichText::new(tr(etiqueta)).size(12.0).color(TEXT_DIM()));
        ui.label(RichText::new(valor).size(12.0).strong().color(TEXT()));
    });
}

/// Estado con un punto de color («En marcha», «Detenido»).
fn estado(ui: &mut egui::Ui, ok: bool, texto: &str) {
    let color = if ok { VERDE_PLAY } else { METER[2] };
    egui::Frame::new().fill(color.gamma_multiply(0.16)).corner_radius(rr(11.0)).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
        let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, color);
        ui.label(RichText::new(tr(texto)).size(12.0).strong().color(color));
    });
}

/// Barra deslizante a lo ancho con el valor en el centro; devuelve si cambió.
pub fn barra(ui: &mut egui::Ui, valor: &mut f32, rango: std::ops::RangeInclusive<f32>, texto: String) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
    let (a, b) = (*rango.start(), *rango.end());
    let antes = *valor;
    if (resp.dragged() || resp.clicked())
        && let Some(pt) = resp.interact_pointer_pos()
    {
        *valor = (a + (pt.x - r.left()) / r.width() * (b - a)).clamp(a, b);
    }
    let t = ((*valor - a) / (b - a)).clamp(0.0, 1.0);
    let p = ui.painter();
    p.rect(r, rr(7.0), BG(), Stroke::new(1.0, if resp.hovered() || resp.dragged() { ACCENT().gamma_multiply(0.6) } else { BORDER() }), StrokeKind::Inside);
    p.rect_filled(Rect::from_min_max(r.min, pos2(r.left() + t * r.width(), r.bottom())), rr(7.0), ACCENT().gamma_multiply(0.28));
    let x = (r.left() + t * r.width()).clamp(r.left() + 3.0, r.right() - 3.0);
    p.rect_filled(Rect::from_center_size(pos2(x, r.center().y), vec2(4.0, 18.0)), 2.0, ACCENT());
    // El valor va del lado contrario al tirador, para que nunca quede tapado.
    if t > 0.5 {
        p.text(r.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, texto, FontId::proportional(12.5), TEXT());
    } else {
        p.text(r.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, texto, FontId::proportional(12.5), TEXT());
    }
    (*valor - antes).abs() > f32::EPSILON
}

/// Botón principal (acento) y secundario, con la misma altura que las filas.
pub fn boton_principal(ui: &mut egui::Ui, texto: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(tr(texto)).size(13.5).strong().color(BG())).fill(ACCENT()).min_size(vec2(180.0, ALTO)).corner_radius(rr(8.0)))
}

pub fn boton(ui: &mut egui::Ui, texto: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(tr(texto)).size(13.0)).min_size(vec2(0.0, ALTO)).corner_radius(rr(8.0)))
}

/// Fila del ancho de la tarjeta con su contenido alineado a la derecha (botones de aplicar o guardar).
/// No ocupa más alto que una fila: un `right_to_left` suelto se estiraría hasta el final de la página.
pub fn a_la_derecha<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), ALTO), egui::Layout::right_to_left(egui::Align::Center), add).inner
}

/// Ruta de solo lectura con aspecto de campo, recortada si no entra.
pub fn ruta(ui: &mut egui::Ui, ancho: f32, texto: &str) {
    let (r, resp) = ui.allocate_exact_size(vec2(ancho, ALTO - 2.0), Sense::hover());
    ui.painter().rect(r, rr(7.0), BG(), Stroke::new(1.0, BORDER()), StrokeKind::Inside);
    ui.painter().with_clip_rect(r.shrink(4.0)).text(r.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, texto, FontId::proportional(12.5), TEXT());
    resp.on_hover_text(texto);
}

/// Etiquetas de los dispositivos y formatos de audio.
fn etiqueta_dispositivo(dispositivos: &[DeviceInfo], id: &str) -> String {
    match id {
        "" => tr("Predeterminado del sistema").to_string(),
        "-" => tr("Ninguna").to_string(),
        id => dispositivos.iter().find(|d| d.id == id).map_or_else(|| id.into(), |d| format!("{} · {}", d.kind, d.name)),
    }
}

fn sistema_o(v: u32, unidad: &str) -> String {
    if v == 0 { tr("Predeterminado del sistema").to_string() } else { format!("{v} {unidad}") }
}

const FRECUENCIAS: [u32; 6] = [0, 44_100, 48_000, 88_200, 96_000, 192_000];
const BUFERES: [u32; 7] = [0, 64, 128, 256, 512, 1024, 2048];

impl App {
    /// Aplica dispositivos, formato y entrada MIDI: se reinicia el motor de audio con el proyecto abierto.
    pub fn aplicar_audio(&mut self, cfg: AudioConfig, bits: u16, midi: String) {
        self.config.midi_port = midi;
        let mut p = self.to_project();
        (p.output_device, p.input_device, p.sample_rate, p.buffer, p.rec_bits) = (cfg.output, cfg.input, cfg.rate, cfg.buffer, bits);
        let r = self.reload(self.dir.clone(), p);
        if r.is_err() || self.engine.input_error.is_none() {
            self.report("Motor de audio reiniciado", r);
        }
        self.config.save();
    }

    fn borrador<'a>(&self, w: &'a mut Welcome) -> &'a mut Borrador {
        w.borrador.get_or_insert_with(|| Borrador { cfg: self.audio.clone(), bits: self.rec_bits, midi: self.config.midi_port.clone(), dispositivos: engine::audio_devices(), puertos: engine::midi_ports() })
    }

    pub fn pagina_audio(&mut self, ui: &mut egui::Ui, w: &mut Welcome) {
        let hdmi = self.config.show_hdmi;
        let actual = (self.audio.clone(), self.rec_bits);
        let b = self.borrador(w);
        let mut aplicar = false;
        tarjeta(ui, "DISPOSITIVO", |ui| {
            let visibles = |entrada: bool| b.dispositivos.iter().filter(move |d| d.input == entrada && (hdmi || d.kind != "HDMI")).cloned().collect::<Vec<_>>();
            let salidas = visibles(false);
            fila(ui, "Dispositivo de salida", |ui| {
                desplegable(ui, "pref-salida", etiqueta_dispositivo(&b.dispositivos, &b.cfg.output), |ui| {
                    ui.selectable_value(&mut b.cfg.output, String::new(), tr("Predeterminado del sistema"));
                    for d in &salidas {
                        ui.selectable_value(&mut b.cfg.output, d.id.clone(), format!("{} · {}", d.kind, d.name));
                    }
                });
            });
            let entradas = visibles(true);
            fila(ui, "Dispositivo de entrada", |ui| {
                desplegable(ui, "pref-entrada", etiqueta_dispositivo(&b.dispositivos, &b.cfg.input), |ui| {
                    ui.selectable_value(&mut b.cfg.input, String::new(), tr("Predeterminado del sistema"));
                    ui.selectable_value(&mut b.cfg.input, "-".into(), tr("Ninguna"));
                    for d in &entradas {
                        ui.selectable_value(&mut b.cfg.input, d.id.clone(), format!("{} · {}", d.kind, d.name));
                    }
                });
            });
            fila(ui, "", |ui| {
                if boton(ui, "Actualizar dispositivos").clicked() {
                    b.dispositivos = engine::audio_devices();
                }
                let mut ver = hdmi;
                if ui.checkbox(&mut ver, tr("Mostrar salidas HDMI / DisplayPort")).changed() {
                    self.config.show_hdmi = ver;
                    self.config.save();
                }
            });
            fila(ui, "Frecuencia de muestreo", |ui| {
                desplegable(ui, "pref-frecuencia", sistema_o(b.cfg.rate, "Hz"), |ui| {
                    FRECUENCIAS.iter().for_each(|&f| _ = ui.selectable_value(&mut b.cfg.rate, f, sistema_o(f, "Hz")));
                });
            });
            fila(ui, "Tamaño de búfer", |ui| {
                desplegable(ui, "pref-bufer", sistema_o(b.cfg.buffer, tr("muestras")), |ui| {
                    BUFERES.iter().for_each(|&f| _ = ui.selectable_value(&mut b.cfg.buffer, f, sistema_o(f, tr("muestras"))));
                });
            });
            fila(ui, "", |ui| {
                let sr = if b.cfg.rate == 0 { self.engine.sample_rate.max(1) } else { b.cfg.rate } as f32;
                let latencia = if b.cfg.buffer == 0 { tr("la del sistema").to_string() } else { format!("{:.2} ms", b.cfg.buffer as f32 / sr * 1000.0) };
                dato(ui, "Latencia del búfer", &latencia);
            });
            nota(ui, "Los búferes grandes dan más tiempo al procesamiento; los pequeños responden más rápido al tocar y grabar.");
            fila(ui, "Resolución de grabación", |ui| {
                let texto = |b: u16| if b == 32 { "32 bits float".to_string() } else { format!("{b} bits") };
                desplegable(ui, "pref-bits", texto(b.bits), |ui| {
                    [16, 24, 32].iter().for_each(|&f| _ = ui.selectable_value(&mut b.bits, f, texto(f)));
                });
            });
            ui.add_space(4.0);
            crate::preferencias::a_la_derecha(ui, |ui| {
                let cambios = (b.cfg.clone(), b.bits) != actual;
                aplicar = ui.add_enabled(cambios, egui::Button::new(RichText::new(tr("Aplicar y reiniciar audio")).size(13.5).strong().color(BG())).fill(VERDE_PLAY).min_size(vec2(220.0, ALTO)).corner_radius(rr(8.0))).clicked();
                if cambios {
                    ui.label(RichText::new(tr("Hay cambios sin aplicar")).size(12.0).color(METER[1]));
                }
            });
        });
        if aplicar {
            let (cfg, bits, midi) = (b.cfg.clone(), b.bits, b.midi.clone());
            self.aplicar_audio(cfg, bits, midi);
            w.borrador = None;
        }
        tarjeta(ui, "ESTADO", |ui| {
            ui.horizontal(|ui| {
                let en_marcha = !self.streams.is_empty();
                estado(ui, en_marcha, if en_marcha { "En marcha" } else { "Detenido" });
                let buf = match self.audio.buffer {
                    0 => tr("búfer del sistema").to_string(),
                    n => format!("{n} {} ({:.1} ms)", tr("muestras"), n as f64 / self.sr() * 1000.0),
                };
                ui.label(RichText::new(format!("{:.1} kHz  ·  {buf}", self.engine.sample_rate as f32 / 1000.0)).size(12.5).color(TEXT()));
            });
            let salida = etiqueta_dispositivo(&self.devices, &self.audio.output);
            fila(ui, "Salida", |ui| ui.label(RichText::new(salida).color(TEXT())));
            let entrada = match (&self.engine.input_error, self.engine.in_channels) {
                (Some(e), _) => format!("{}: {e}", tr("No se pudo abrir")),
                (None, 0) => tr("Sin entrada (solo reproducción). Elige un dispositivo arriba para grabar.").to_string(),
                (None, n) => format!("{} · {n} {}", etiqueta_dispositivo(&self.devices, &self.audio.input), tr("canales")),
            };
            fila(ui, "Entrada", |ui| ui.add(egui::Label::new(RichText::new(entrada).color(TEXT())).truncate()));
        });
        tarjeta(ui, "METRÓNOMO", |ui| {
            let mut cambio = fila(ui, "Volumen del clic", |ui| {
                let t = format!("{:+.1} dB", self.config.clic_nivel);
                barra(ui, &mut self.config.clic_nivel, -30.0..=6.0, t)
            });
            cambio |= fila(ui, "Acento del primer tiempo", |ui| {
                let t = format!("{:+.1} dB", self.config.clic_acento);
                barra(ui, &mut self.config.clic_acento, -12.0..=12.0, t)
            });
            if cambio {
                self.engine.clic_nivel.set(self.config.clic_nivel);
                self.engine.clic_acento.set(self.config.clic_acento);
                self.config.save();
            }
        });
    }

    pub fn pagina_midi(&mut self, ui: &mut egui::Ui, w: &mut Welcome) {
        let actual = self.config.midi_port.clone();
        let b = self.borrador(w);
        let mut aplicar = false;
        tarjeta(ui, "ENTRADA MIDI", |ui| {
            parrafo(ui, "Los teclados y controladores conectados tocan la pista armada (o la seleccionada) y graban en ella.");
            let texto = |m: &str| match m {
                "" => tr("Todos los dispositivos").to_string(),
                "-" => tr("Ninguna").to_string(),
                m => m.to_string(),
            };
            fila(ui, "Entrada MIDI", |ui| {
                desplegable(ui, "pref-midi", texto(&b.midi), |ui| {
                    ui.selectable_value(&mut b.midi, String::new(), tr("Todos los dispositivos"));
                    ui.selectable_value(&mut b.midi, "-".into(), tr("Ninguna"));
                    for p in &b.puertos {
                        ui.selectable_value(&mut b.midi, p.clone(), p);
                    }
                });
            });
            ui.add_space(4.0);
            crate::preferencias::a_la_derecha(ui, |ui| {
                let cambios = b.midi != actual;
                aplicar = ui.add_enabled(cambios, egui::Button::new(RichText::new(tr("Aplicar")).size(13.5).strong().color(BG())).fill(VERDE_PLAY).min_size(vec2(180.0, ALTO)).corner_radius(rr(8.0))).clicked();
            });
        });
        if aplicar {
            let (cfg, bits, midi) = (self.audio.clone(), self.rec_bits, b.midi.clone());
            self.aplicar_audio(cfg, bits, midi);
            w.borrador = None;
        }
        let b = self.borrador(w);
        tarjeta(ui, "DISPOSITIVOS DETECTADOS", |ui| {
            if b.puertos.is_empty() {
                parrafo(ui, "No hay dispositivos MIDI conectados.");
            }
            for p in &b.puertos {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(10.0, ALTO - 8.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.0, VERDE_PLAY);
                    ui.label(RichText::new(p).color(TEXT()));
                });
            }
            ui.add_space(4.0);
            if boton(ui, "Actualizar lista").clicked() {
                b.puertos = engine::midi_ports();
            }
        });
    }

    pub fn pagina_comportamiento(&mut self, ui: &mut egui::Ui) {
        tarjeta(ui, "PROYECTOS", |ui| {
            let carpeta = projects_dir();
            fila(ui, "Carpeta de proyectos", |ui| {
                ruta(ui, ui.available_width() - 130.0, &carpeta.display().to_string());
                if ui.add(egui::Button::new(tr("Abrir carpeta")).min_size(vec2(120.0, ALTO - 2.0)).corner_radius(rr(8.0))).clicked() {
                    informes::abrir_carpeta(&carpeta);
                }
            });
            nota(ui, "Donde se crean las canciones nuevas, cada una en su carpeta con su audio y sus copias.");
            fila(ui, "Creado por", |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.config.author).hint_text(user_name()).desired_width(ui.available_width()).min_size(vec2(0.0, ALTO - 4.0)).margin(egui::Margin::symmetric(10, 4)).vertical_align(egui::Align::Center));
                if r.lost_focus() {
                    self.config.save();
                }
            });
            nota(ui, "Se guarda en los datos de cada canción nueva y en el saludo del panel de inicio.");
        });
        tarjeta(ui, "INICIO", |ui| {
            let mut mostrar = !self.config.hide_welcome;
            if fila(ui, "Mostrar el panel al iniciar", |ui| interruptor(ui, &mut mostrar)) {
                self.config.hide_welcome = !mostrar;
                self.config.save();
            }
            nota(ui, "Al abrir el programa se carga el último proyecto; este panel queda encima para elegir otro.");
        });
        tarjeta(ui, "PISTAS", |ui| {
            let mut boton_fx = !self.config.ocultar_plugins_pista;
            if fila(ui, "Botón de instrumento y efectos", |ui| interruptor(ui, &mut boton_fx)) {
                self.config.ocultar_plugins_pista = !boton_fx;
                self.config.save();
            }
            nota(ui, "El botón FX en la cabecera de cada pista abre su instrumento y sus efectos.");
            let mut proxies = self.config.proxies;
            if fila(ui, "Trabajar con proxys", |ui| interruptor(ui, &mut proxies)) {
                self.cambiar_proxies(proxies);
            }
            nota(ui, "Las pistas con instrumento o efectos suenan desde un render previo guardado en Proxies/: ahorra procesador en proyectos grandes.");
        });
        tarjeta(ui, "BIBLIOTECA", |ui| {
            let mut al_clic = !self.config.preview_manual;
            if fila(ui, "Previsualizar al hacer clic", |ui| interruptor(ui, &mut al_clic)) {
                self.config.preview_manual = !al_clic;
                self.config.save();
            }
            nota(ui, "Al hacer clic en un audio de la biblioteca se escucha al momento; si no, solo con el botón ▶.");
        });
    }

    pub fn pagina_guardado(&mut self, ui: &mut egui::Ui) {
        tarjeta(ui, "AUTOGUARDADO", |ui| {
            let mut activo = !self.config.autoguardado_off;
            if fila(ui, "Autoguardado", |ui| interruptor(ui, &mut activo)) {
                self.config.autoguardado_off = !activo;
                self.config.save();
            }
            nota(ui, "Guarda el trabajo sin guardar en Backups/autoguardado.qproj (no toca el proyecto) para recuperarlo tras un cierre inesperado. Nunca mientras se graba.");
            ui.add_enabled_ui(activo, |ui| {
                fila(ui, "Intervalo de autoguardado", |ui| {
                    let actual = self.config.autoguardado_min.max(1);
                    let texto = |m: u32| if m == 1 { tr("Cada minuto").to_string() } else { format!("{} {m} {}", tr("Cada"), tr("minutos")) };
                    desplegable(ui, "pref-autoguardado", texto(actual), |ui| {
                        for m in [1, 2, 5, 10] {
                            if ui.selectable_label(actual == m, texto(m)).clicked() {
                                self.config.autoguardado_min = m;
                                self.config.save();
                            }
                        }
                    });
                });
            });
        });
        tarjeta(ui, "COPIAS DE SEGURIDAD", |ui| {
            parrafo(ui, "Cada vez que guardas, la versión anterior queda en la carpeta Backups/ de la canción con su fecha y hora; se conservan las 20 últimas.");
            let copias = self.dir.join(project::BACKUP_DIR);
            fila(ui, "Copias de esta canción", |ui| {
                ruta(ui, ui.available_width() - 130.0, &copias.display().to_string());
                if ui.add_enabled(copias.is_dir(), egui::Button::new(tr("Abrir carpeta")).min_size(vec2(120.0, ALTO - 2.0)).corner_radius(rr(8.0))).clicked() {
                    informes::abrir_carpeta(&copias);
                }
            });
        });
        tarjeta(ui, "REGISTROS E INFORMES", |ui| {
            let carpeta = informes::carpeta();
            fila(ui, "Carpeta de registros", |ui| {
                ruta(ui, ui.available_width() - 130.0, &carpeta.display().to_string());
                if ui.add(egui::Button::new(tr("Abrir carpeta")).min_size(vec2(120.0, ALTO - 2.0)).corner_radius(rr(8.0))).clicked() {
                    informes::abrir_carpeta(&carpeta);
                }
            });
            nota(ui, "Si el programa se cierra por un fallo o deja de responder, aquí queda un informe con lo que estaba pasando.");
            fila(ui, "", |ui| {
                if boton(ui, "Enviar informe de error…").clicked() {
                    informes::guardar("error", tr("Informe enviado por el usuario desde el menú"), "");
                    self.informe = informes::pendiente();
                }
            });
        });
    }

    pub fn pagina_atajos(&mut self, ui: &mut egui::Ui, w: &mut Welcome) {
        self.keys_editor(ui, &mut w.captura, true);
    }

    pub fn pagina_interfaz(&mut self, ui: &mut egui::Ui) {
        tarjeta(ui, "TEMA DE COLOR", |ui| {
            // Cuatro por fila, todas del mismo tamaño.
            let columnas = 4.0;
            let espacio = 12.0;
            let tam = vec2(((ui.available_width() - espacio * (columnas - 1.0)) / columnas).floor(), 92.0);
            for fila_temas in TEMAS.iter().enumerate().collect::<Vec<_>>().chunks(4) {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = espacio;
                    for &(k, (nombre, c)) in fila_temas {
                        let on = self.config.tema == k;
                        let (r, resp) = ui.allocate_exact_size(tam, Sense::click());
                        let p = ui.painter();
                        p.rect(r, rr(10.0), c[1], Stroke::new(if on { 2.0 } else { 1.0 }, if on { c[6] } else if resp.hovered() { c[4] } else { c[3] }), StrokeKind::Inside);
                        // Miniatura: barra superior, pistas y cursor de reproducción con los colores del tema.
                        let m = Rect::from_min_size(r.min + vec2(10.0, 10.0), vec2(r.width() - 20.0, 44.0));
                        p.rect_filled(m, rr(6.0), c[0]);
                        for i in 0..3 {
                            let y = m.top() + 8.0 + i as f32 * 12.0;
                            p.rect_filled(Rect::from_min_size(pos2(m.left() + 6.0, y), vec2(14.0, 8.0)), 2.0, c[3]);
                            p.rect_filled(Rect::from_min_size(pos2(m.left() + 26.0 + i as f32 * 14.0, y), vec2(m.width() * 0.45, 8.0)), 2.0, c[6].gamma_multiply(0.55 + i as f32 * 0.15));
                        }
                        p.vline(m.left() + m.width() * 0.62, m.y_range(), Stroke::new(1.5, c[6]));
                        p.text(pos2(r.left() + 12.0, r.bottom() - 16.0), Align2::LEFT_CENTER, tr(nombre), FontId::proportional(13.0), c[4]);
                        if on {
                            let m = pos2(r.right() - 16.0, r.bottom() - 16.0);
                            p.circle_filled(m, 8.0, c[6]);
                            let s = Stroke::new(1.8, c[0]);
                            p.line_segment([m + vec2(-3.5, 0.0), m + vec2(-1.0, 2.8)], s);
                            p.line_segment([m + vec2(-1.0, 2.8), m + vec2(3.8, -2.8)], s);
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.config.tema = k;
                            TEMA.store(k, Relaxed);
                            theme(ui.ctx());
                            self.config.save();
                        }
                    }
                });
            }
        });
        tarjeta(ui, "TAMAÑO Y FORMA", |ui| {
            fila(ui, "Tamaño de la interfaz", |ui| {
                let actual = ui.ctx().zoom_factor();
                for (texto, z) in [("90 %", 0.9), ("100 %", 1.0), ("115 %", 1.15), ("130 %", 1.3), ("150 %", 1.5)] {
                    if ui.add(egui::Button::selectable((actual - z).abs() < 0.01, RichText::new(texto).size(13.0)).min_size(vec2(64.0, ALTO - 4.0))).clicked() {
                        ui.ctx().set_zoom_factor(z);
                        self.config.ui_scale = z;
                        self.config.save();
                    }
                }
            });
            let mut redondeada = widgets::ROUND.load(Relaxed);
            if fila(ui, "Interfaz redondeada", |ui| interruptor(ui, &mut redondeada)) {
                widgets::ROUND.store(redondeada, Relaxed);
                self.config.square = !redondeada;
                self.config.save();
                theme(ui.ctx());
            }
            nota(ui, "Esquinas redondeadas en botones, paneles y regiones; sin ella, todo es recto.");
        });
        tarjeta(ui, "IDIOMA", |ui| {
            fila(ui, "Idioma", |ui| {
                desplegable(ui, "pref-idioma", i18n::LANGS[(self.config.lang as usize).min(i18n::LANGS.len() - 1)], |ui| {
                    for (k, nombre) in i18n::LANGS.iter().enumerate() {
                        if ui.selectable_label(self.config.lang == k as u8, *nombre).clicked() {
                            self.config.lang = k as u8;
                            i18n::set(k as u8);
                            self.config.save();
                        }
                    }
                });
            });
            nota(ui, "Cambia al momento en toda la interfaz.");
        });
    }

    pub fn pagina_plugins(&mut self, ui: &mut egui::Ui, w: &mut Welcome) {
        let carpetas = w.carpetas.get_or_insert_with(library::carpetas_plugins).clone();
        let mut reescanear = false;
        tarjeta(ui, "CARPETAS DE PLUGINS", |ui| {
            parrafo(ui, "Se buscan plugins en las carpetas estándar de cada formato. Los que encuentra aparecen en la pestaña Plugins de la Biblioteca.");
            ui.add_space(2.0);
            for (dir, formato, n) in &carpetas {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), ALTO), Sense::hover());
                let p = ui.painter();
                p.rect_filled(r, rr(7.0), BG());
                p.circle_filled(r.left_center() + vec2(14.0, 0.0), 4.0, if n.is_some_and(|n| n > 0) { VERDE_PLAY } else if n.is_some() { METER[1] } else { BORDER() });
                p.text(r.left_center() + vec2(28.0, 0.0), Align2::LEFT_CENTER, dir.display().to_string(), FontId::proportional(12.5), TEXT());
                let (texto, color) = match n {
                    Some(n) => (format!("{n} {formato}"), if *n > 0 { TEXT() } else { TEXT_DIM() }),
                    None => (tr("no existe").to_string(), TEXT_DIM()),
                };
                p.text(r.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, texto, FontId::proportional(12.5), color);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for f in ["VST3", "LV2", "CLAP"] {
                    dato(ui, &format!("{} {f}", tr("Plugins")), &self.plugins.iter().filter(|p| p.1 == f).count().to_string());
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    reescanear = boton_principal(ui, "Volver a buscar plugins").clicked();
                });
            });
        });
        if reescanear {
            self.plugins = library::scan_plugins();
            w.carpetas = None;
        }
        tarjeta(ui, "PLUGINS OCULTOS", |ui| {
            let n = self.config.hidden_plugins.len();
            if n == 0 {
                parrafo(ui, "No hay plugins ocultos. Para ocultar uno, haz clic derecho sobre él en la Biblioteca.");
            } else {
                fila(ui, "Ocultos en la biblioteca", |ui| {
                    dato(ui, "Plugins", &n.to_string());
                    if boton(ui, "Mostrar todos").clicked() {
                        self.config.hidden_plugins.clear();
                        self.config.save();
                    }
                });
            }
        });
    }

    pub fn pagina_espacios(&mut self, ui: &mut egui::Ui, cerrar: &mut bool) {
        tarjeta(ui, "ESPACIOS DE TRABAJO", |ui| {
            for (k, (nombre, desc, ..)) in welcome::ESPACIOS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
                let p = ui.painter();
                p.rect(r, rr(9.0), if resp.hovered() { BORDER() } else { BG() }, Stroke::new(1.0, if resp.hovered() { ACCENT().gamma_multiply(0.6) } else { BORDER() }), StrokeKind::Inside);
                p.text(r.left_center() + vec2(16.0, -9.0), Align2::LEFT_CENTER, tr(nombre), FontId::proportional(14.0), TEXT());
                p.text(r.left_center() + vec2(16.0, 10.0), Align2::LEFT_CENTER, tr(desc), FontId::proportional(11.5), TEXT_DIM());
                p.text(r.right_center() - vec2(16.0, 0.0), Align2::RIGHT_CENTER, tr("Aplicar"), FontId::proportional(12.5), if resp.hovered() { ACCENT() } else { TEXT_DIM() });
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.aplicar_espacio(k);
                    *cerrar = true;
                }
            }
        });
    }

    pub fn pagina_acerca(&mut self, ui: &mut egui::Ui, action: &mut Option<Box<dyn FnOnce(&mut App)>>) {
        tarjeta(ui, "QUANTUM DAW", |ui| {
            ui.label(RichText::new(format!("QUANTUM {}", welcome::VERSION)).size(18.0).strong().color(TEXT()));
            parrafo(ui, "Estación de audio digital escrita en Rust: graba, edita, arregla y mezcla con instrumentos y efectos propios.");
            ui.label(RichText::new(tr("Desarrollado por QUANTEX")).size(13.0).strong().color(ACCENT()));
            ui.horizontal(|ui| {
                for site in ["www.quantumdaw.com", "www.quantex.com.py"] {
                    ui.hyperlink_to(RichText::new(site).size(13.0).color(ACCENT()), format!("https://{site}"));
                }
            });
        });
        tarjeta(ui, "COMPILACIÓN Y SISTEMA", |ui| {
            fila(ui, "Versión", |ui| ui.label(RichText::new(welcome::VERSION).color(TEXT())));
            fila(ui, "Sistema", |ui| ui.add(egui::Label::new(RichText::new(informes::sistema()).color(TEXT())).truncate()));
            let gpu = informes::dato("GPU").unwrap_or_else(|| "—".into());
            fila(ui, "Tarjeta gráfica", |ui| ui.add(egui::Label::new(RichText::new(gpu).color(TEXT())).truncate()));
            let audio = format!("{:.1} kHz · {}", self.engine.sample_rate as f32 / 1000.0, etiqueta_dispositivo(&self.devices, &self.audio.output));
            fila(ui, "Audio", |ui| ui.add(egui::Label::new(RichText::new(audio).color(TEXT())).truncate()));
        });
        tarjeta(ui, "LICENCIAS", |ui| {
            parrafo(ui, "Código disponible bajo la Business Source License 1.1: úsalo para hacer y vender tu música; los plugins QUANTUM Pro tienen su propia licencia.");
            ui.horizontal(|ui| {
                if boton(ui, "Licencias QUANTUM Pro…").clicked() {
                    *action = Some(Box::new(|app: &mut Self| app.licencias_abierta = true));
                }
                if boton(ui, "Abrir la demo").clicked() {
                    *action = Some(Box::new(|app: &mut Self| {
                        let dir = projects_dir().join("Demo");
                        let r = Project::open_or_demo(&dir).and_then(|p| app.reload(dir, p));
                        app.report("Demo abierta", r);
                    }));
                }
            });
        });
    }
}
