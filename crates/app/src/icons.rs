//! Siluetas SVG de instrumentos para identificar cada pista (se tiñen con el color de la pista).
use crate::*;

pub const ICONS: [(&str, &str); 12] = [
    ("audio", "Audio"),
    ("microfono", "Voz / micrófono"),
    ("guitarra", "Guitarra"),
    ("bajo", "Bajo"),
    ("bateria", "Batería"),
    ("percusion", "Percusión"),
    ("teclado", "Piano / teclado"),
    ("sintetizador", "Sintetizador"),
    ("violin", "Cuerdas"),
    ("midi", "MIDI"),
    ("metronomo", "Clic"),
    ("video", "Video"),
];

pub fn image(id: &str) -> egui::Image<'static> {
    egui::Image::new(match id {
        "microfono" => egui::include_image!("../../../assets/icons/microfono.svg"),
        "guitarra" => egui::include_image!("../../../assets/icons/guitarra.svg"),
        "bajo" => egui::include_image!("../../../assets/icons/bajo.svg"),
        "bateria" => egui::include_image!("../../../assets/icons/bateria.svg"),
        "percusion" => egui::include_image!("../../../assets/icons/percusion.svg"),
        "teclado" => egui::include_image!("../../../assets/icons/teclado.svg"),
        "sintetizador" => egui::include_image!("../../../assets/icons/sintetizador.svg"),
        "violin" => egui::include_image!("../../../assets/icons/violin.svg"),
        "midi" => egui::include_image!("../../../assets/icons/midi.svg"),
        "metronomo" => egui::include_image!("../../../assets/icons/metronomo.svg"),
        "video" => egui::include_image!("../../../assets/icons/video.svg"),
        _ => egui::include_image!("../../../assets/icons/audio.svg"),
    })
}

/// Icono de la pista: el elegido o uno deducido del nombre, el instrumento y el tipo.
pub fn of(t: &Track) -> &'static str {
    if let Some((id, _)) = ICONS.iter().find(|(id, _)| *id == t.icon) {
        return id;
    }
    let text = format!("{} {}", t.name, t.instrument).to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
    match t.kind {
        TrackKind::Video => "video",
        TrackKind::Click => "metronomo",
        _ if has(&["bater", "drum", "kick", "bombo", "snare", "caja", "hat", "tom"]) => "bateria",
        _ if has(&["percu", "conga", "bongo", "cajón", "shaker", "pandero"]) => "percusion",
        _ if has(&["bajo", "bass"]) => "bajo",
        _ if has(&["guit"]) => "guitarra",
        _ if has(&["voz", "vocal", "coro", "mic", "voice"]) => "microfono",
        _ if has(&["piano", "tecla", "key", "órgano", "organo", "rhodes"]) => "teclado",
        _ if has(&["viol", "chelo", "cello", "cuerd", "string", "contrabajo"]) => "violin",
        _ if has(&["synth", "pad", "lead", "sinte"]) => "sintetizador",
        TrackKind::Midi => "midi",
        _ => "audio",
    }
}

/// Icono de la pista que, al hacer clic, abre la paleta de iconos para elegir otro.
pub fn picker(ui: &mut egui::Ui, t: &mut Track, size: f32) {
    let resp = ui.add(image(of(t)).tint(t.color).fit_to_exact_size(egui::vec2(size, size)).sense(egui::Sense::click())).on_hover_text(tr("Icono de la pista · clic para cambiarlo"));
    egui::Popup::from_toggle_button_response(&resp).show(|ui| {
        ui.label(egui::RichText::new(tr("Icono de la pista")).strong());
        egui::Grid::new(("icon-grid", t.id)).num_columns(4).spacing([8.0, 8.0]).show(ui, |ui| {
            for (k, (id, label)) in ICONS.iter().enumerate() {
                let on = of(t) == *id;
                // El elegido: silueta en color de acento sobre fondo oscuro con anillo (no relleno sólido).
                let img = image(id).tint(if on { ACCENT } else { TEXT }).fit_to_exact_size(egui::vec2(28.0, 28.0));
                let fill = if on { ACCENT.gamma_multiply(0.15) } else { Color32::TRANSPARENT };
                let b = ui.add(egui::Button::image(img).fill(fill).stroke(egui::Stroke::new(if on { 2.0 } else { 0.0 }, ACCENT))).on_hover_text(tr(label));
                if b.clicked() {
                    t.icon = id.to_string();
                }
                if k % 4 == 3 {
                    ui.end_row();
                }
            }
        });
        if ui.button(tr("Automático (según el nombre)")).clicked() {
            t.icon.clear();
        }
    });
}
