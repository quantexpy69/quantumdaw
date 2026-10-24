//! Kits de batería propios: el usuario elige una carpeta con sus muestras, QUANTUM reconoce cada pieza
//! por el nombre de la carpeta o del archivo (kick, snare, hi hat, crash, floor tom…) y arma el kit,
//! que queda como un instrumento más. Las muestras no se copian ni se distribuyen: el kit apunta a
//! ellas en la computadora del usuario.
use crate::{widgets::rr, *};
use egui::{RichText, vec2};
use std::sync::mpsc;

/// Piezas del kit: (nombre, nota General MIDI, palabras que la reconocen en la ruta). El orden
/// importa: las más específicas primero.
const PIEZAS: [(&str, u8, &[&str]); 19] = [
    ("Bombo", 36, &["kick", "bombo", "bassdrum", "bass drum"]),
    ("Caja", 38, &["snare", "caja"]),
    ("Aro", 37, &["rim", "stick", "aro", "sidestick", "cross"]),
    ("Palmas", 39, &["clap", "palma"]),
    ("Hi-hat cerrado", 42, &["hi hat", "hihat", "hi-hat", "hats", "closed", "cerrado"]),
    ("Hi-hat de pie", 44, &["pedal", "foot"]),
    ("Hi-hat abierto", 46, &["open", "abierto"]),
    ("Tom de piso", 43, &["floor", "piso", "low tom"]),
    ("Tom medio", 47, &["mid tom", "midtom", "tom medio"]),
    ("Tom alto", 50, &["high tom", "hi tom", "hitom", "tom alto"]),
    ("Crash", 49, &["crash"]),
    ("Ride", 51, &["ride"]),
    ("Campana del ride", 53, &["ride bell", "bell ride", "campana"]),
    ("Splash", 55, &["splash"]),
    ("Cencerro", 56, &["cowbell", "cencerro"]),
    ("Pandereta", 54, &["tambourine", "pandereta"]),
    ("Shaker", 70, &["shaker", "maraca"]),
    ("Cascabeles", 83, &["sleigh", "jingle", "cascabel"]),
    ("Bloque", 76, &["block", "bloque", "woodblock"]),
];
/// Las tres piezas del hi-hat comparten muestras (cerrado = la más corta, abierto = la más larga).
const HIHAT: [u8; 3] = [42, 44, 46];
const EXTENSIONES: [&str; 6] = ["wav", "flac", "ogg", "mp3", "aif", "aiff"];

pub struct KitEditor {
    nombre: String,
    carpeta: Option<PathBuf>,
    archivos: Vec<PathBuf>,
    /// Pieza reconocida de cada archivo (índice en PIEZAS).
    pieza_de: Vec<Option<usize>>,
    /// Muestra elegida para cada pieza (índice en `archivos`).
    eleccion: [Option<usize>; PIEZAS.len()],
    error: String,
    escucha: Option<mpsc::Receiver<anyhow::Result<Vec<[f32; 2]>>>>,
}

impl Default for KitEditor {
    fn default() -> Self {
        Self { nombre: String::new(), carpeta: None, archivos: vec![], pieza_de: vec![], eleccion: [None; PIEZAS.len()], error: String::new(), escucha: None }
    }
}

/// Orden natural: «RD_K_2» antes que «RD_K_10».
fn clave_natural(p: &Path) -> Vec<(String, u64)> {
    let s = p.to_string_lossy().to_lowercase();
    let mut v = vec![];
    let (mut texto, mut num) = (String::new(), String::new());
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else {
            if !num.is_empty() {
                v.push((std::mem::take(&mut texto), num.parse().unwrap_or(0)));
                num.clear();
            }
            texto.push(c);
        }
    }
    v.push((texto, num.parse().unwrap_or(0)));
    v
}

fn reconocer(ruta: &str) -> Option<usize> {
    let r = ruta.to_lowercase().replace(['_', '-'], " ");
    PIEZAS.iter().position(|(_, _, palabras)| palabras.iter().any(|p| r.contains(p)))
}

impl KitEditor {
    /// Lee la carpeta (con sus subcarpetas) y propone un sonido para cada pieza.
    fn cargar(&mut self, carpeta: PathBuf) {
        fn recorrer(d: &Path, v: &mut Vec<PathBuf>) {
            for e in fs::read_dir(d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    // Los temas de demostración no son sonidos sueltos.
                    if !p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().contains("demo")) {
                        recorrer(&p, v);
                    }
                } else if p.extension().is_some_and(|x| EXTENSIONES.contains(&x.to_string_lossy().to_lowercase().as_str())) {
                    v.push(p);
                }
            }
        }
        let mut archivos = vec![];
        recorrer(&carpeta, &mut archivos);
        archivos.sort_by_key(|p| clave_natural(p));
        self.pieza_de = archivos.iter().map(|p| reconocer(&p.strip_prefix(&carpeta).unwrap_or(p).to_string_lossy())).collect();
        if self.nombre.is_empty() {
            self.nombre = carpeta.file_name().map_or_else(|| tr("Mi kit").to_string(), |n| n.to_string_lossy().to_string());
        }
        self.archivos = archivos;
        self.carpeta = Some(carpeta);
        self.eleccion = [None; PIEZAS.len()];
        // La primera de cada pieza, prefiriendo las menos anidadas (las «Processed» van después).
        let profundidad = |p: &Path| p.components().count();
        for k in 0..PIEZAS.len() {
            self.eleccion[k] = self.candidatos(k).into_iter().min_by_key(|&i| profundidad(&self.archivos[i]));
        }
        // Hi-hat: cerrado el más corto, de pie el siguiente y abierto el más largo.
        let mut hats: Vec<usize> = self.candidatos(4);
        hats.sort_by_key(|&i| fs::metadata(&self.archivos[i]).map_or(0, |m| m.len()));
        if !hats.is_empty() {
            self.eleccion[4] = Some(hats[0]);
            self.eleccion[5] = Some(hats[1.min(hats.len() - 1)]);
            self.eleccion[6] = Some(hats[hats.len() - 1]);
        }
        self.error = if self.archivos.is_empty() { tr("No hay audios en esa carpeta (WAV, FLAC, OGG, MP3 o AIFF).").into() } else { String::new() };
    }

    /// Muestras reconocidas para la pieza `k` (las tres del hi-hat comparten las suyas).
    fn candidatos(&self, k: usize) -> Vec<usize> {
        let hat = HIHAT.contains(&PIEZAS[k].1);
        (0..self.archivos.len())
            .filter(|&i| match self.pieza_de[i] {
                Some(p) if hat => HIHAT.contains(&PIEZAS[p].1),
                Some(p) => p == k,
                None => false,
            })
            .collect()
    }

    fn nombre_archivo(&self, i: usize) -> String {
        let p = &self.archivos[i];
        let rel = self.carpeta.as_ref().and_then(|c| p.strip_prefix(c).ok()).unwrap_or(p);
        rel.to_string_lossy().to_string()
    }
}

impl App {
    #[cfg_attr(not(feature = "pro"), allow(dead_code))]
    /// Abre el creador con un kit ya guardado: su carpeta, su nombre y los sonidos que tenía.
    pub fn editar_kit(&mut self, id: &str) {
        let Ok(datos) = fs::read_to_string(instruments::dir(id).join("kit.json")).map(|s| serde_json::from_str::<serde_json::Value>(&s).unwrap_or_default()) else { return };
        let mut k = KitEditor { nombre: datos["nombre"].as_str().unwrap_or_default().to_string(), ..Default::default() };
        if let Some(c) = datos["carpeta"].as_str() {
            k.cargar(PathBuf::from(c));
        }
        for p in datos["piezas"].as_array().into_iter().flatten() {
            let (Some(nota), Some(m)) = (p["nota"].as_u64(), p["muestra"].as_str()) else { continue };
            if let (Some(pi), Some(ai)) = (PIEZAS.iter().position(|x| x.1 as u64 == nota), k.archivos.iter().position(|a| a == Path::new(m))) {
                k.eleccion[pi] = Some(ai);
            }
        }
        self.kit_editor = Some(k);
        self.enfocar_ventana = Some("kits");
    }

    pub fn abrir_kits(&mut self) {
        if self.kit_editor.is_none() {
            self.kit_editor = Some(KitEditor::default());
        }
        self.enfocar_ventana = Some("kits");
    }

    /// Escucha una muestra (sin pasar por la biblioteca).
    fn escuchar_muestra(&mut self, ruta: PathBuf) {
        let (tx, rx) = mpsc::channel();
        let rate = self.engine.sample_rate;
        std::thread::spawn(move || _ = tx.send(engine::decode(&ruta, rate)));
        if let Some(k) = &mut self.kit_editor {
            k.escucha = Some(rx);
        }
    }

    pub fn ventana_kits(&mut self, ctx: &egui::Context) {
        let Some(k) = &mut self.kit_editor else { return };
        if let Some(Ok(r)) = k.escucha.as_ref().map(|rx| rx.try_recv()) {
            k.escucha = None;
            if let Ok(frames) = r {
                self.engine.preview_pos.store(0, Relaxed);
                self.engine.preview_loop.store(false, Relaxed);
                self.engine.preview.store(Some(Arc::new(frames)));
            }
        }
        let abierta = self.ventana(ctx, "kits", tr("Crear kit de batería").to_string(), [760.0, 720.0], true, |app, ui| app.panel_kits(ui));
        if !abierta {
            self.kit_editor = None;
        }
    }

    fn panel_kits(&mut self, ui: &mut egui::Ui) {
        let Some(mut k) = self.kit_editor.take() else { return };
        ui.label(RichText::new(tr("Elige una carpeta con tus muestras de batería: QUANTUM reconoce cada pieza por el nombre de la carpeta o del archivo. Las muestras no se copian: el kit las usa desde donde están.")).color(TEXT_DIM()));
        ui.add_space(6.0);
        let mut pedir_carpeta = false;
        ui.horizontal(|ui| {
            if ui.button(tr("Elegir carpeta…")).clicked() {
                pedir_carpeta = true;
            }
            ui.label(RichText::new(k.carpeta.as_ref().map_or_else(|| tr("ninguna carpeta elegida").to_string(), |c| c.display().to_string())).size(11.5));
        });
        if pedir_carpeta && let Some(c) = rfd::FileDialog::new().set_title(tr("Carpeta con las muestras de batería")).set_directory(home()).pick_folder() {
            k.cargar(c);
        }
        if !k.archivos.is_empty() {
            ui.horizontal(|ui| {
                ui.label(tr("Nombre del kit"));
                ui.add(egui::TextEdit::singleline(&mut k.nombre).desired_width(260.0));
                ui.label(RichText::new(format!("{} {}", k.archivos.len(), tr("muestras encontradas"))).size(11.0).color(TEXT_DIM()));
            });
            ui.add_space(6.0);
            let mut escuchar = None;
            egui::Grid::new("kit-piezas").striped(true).spacing(vec2(10.0, 4.0)).show(ui, |ui| {
                for (p, (nombre, nota, _)) in PIEZAS.iter().enumerate() {
                    ui.label(RichText::new(tr(nombre)).strong());
                    ui.label(RichText::new(format!("{} {nota}", tr("nota"))).size(10.5).color(TEXT_DIM()));
                    let actual = k.eleccion[p].map_or_else(|| tr("(sin sonido)").to_string(), |i| k.nombre_archivo(i));
                    let candidatos = k.candidatos(p);
                    egui::ComboBox::from_id_salt(("kit-p", p)).width(330.0).selected_text(actual).height(420.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut k.eleccion[p], None, tr("(sin sonido)"));
                        if !candidatos.is_empty() {
                            ui.label(RichText::new(tr("RECONOCIDAS")).size(10.0).color(ACCENT()));
                        }
                        for &i in &candidatos {
                            let n = k.nombre_archivo(i);
                            ui.selectable_value(&mut k.eleccion[p], Some(i), n);
                        }
                        ui.label(RichText::new(tr("TODAS")).size(10.0).color(TEXT_DIM()));
                        for i in (0..k.archivos.len()).filter(|i| !candidatos.contains(i)) {
                            let n = k.nombre_archivo(i);
                            ui.selectable_value(&mut k.eleccion[p], Some(i), n);
                        }
                    });
                    if ui.add_enabled(k.eleccion[p].is_some(), egui::Button::new("▶")).on_hover_text(tr("Escuchar")).clicked() {
                        escuchar = k.eleccion[p].map(|i| k.archivos[i].clone());
                    }
                    ui.end_row();
                }
            });
            if let Some(r) = escuchar {
                self.kit_editor = Some(k);
                self.escuchar_muestra(r);
                k = self.kit_editor.take().unwrap_or_default();
            }
            ui.add_space(10.0);
            if !k.error.is_empty() {
                ui.label(RichText::new(&k.error).color(METER[2]));
            }
            let pista = self.s.tracks.iter().position(|t| t.selected && t.midi());
            ui.horizontal(|ui| {
                let guardar = |usar: bool, k: &mut KitEditor, app: &mut App| {
                    let piezas: Vec<(&str, u8, PathBuf)> = PIEZAS.iter().enumerate().filter_map(|(p, (n, nota, _))| Some((*n, *nota, k.archivos.get(k.eleccion[p]?)?.clone()))).collect();
                    let carpeta = k.carpeta.clone().unwrap_or_default();
                    match instruments::guardar_kit(k.nombre.trim(), &carpeta, &piezas) {
                        Ok(id) => {
                            // Las pistas que ya usaban este kit lo vuelven a cargar con los cambios.
                            let usan: Vec<usize> = (0..app.s.tracks.len()).filter(|&j| app.s.tracks[j].instrument == id).collect();
                            if let Some(sid) = instruments::id_estatico(&id) {
                                usan.into_iter().for_each(|j| app.set_instrument(j, sid));
                            }
                            if let Some(id) = instruments::id_estatico(&id) {
                                let i = match pista.filter(|_| usar) {
                                    Some(i) => Some(i),
                                    None if usar => {
                                        app.add_track(k.nombre.trim().to_string(), TrackKind::Midi);
                                        Some(app.s.tracks.len() - 1)
                                    }
                                    None => None,
                                };
                                if let Some(i) = i {
                                    app.set_instrument(i, id);
                                }
                            }
                            app.status = format!("{} «{}»", tr("Kit guardado:"), k.nombre.trim());
                            k.error.clear();
                        }
                        Err(e) => k.error = e.to_string(),
                    }
                };
                let b = egui::Button::new(RichText::new(if pista.is_some() { tr("Guardar y usar en la pista elegida") } else { tr("Guardar y crear una pista") }).strong().color(BG())).fill(ACCENT()).min_size(vec2(0.0, 32.0)).corner_radius(rr(8.0));
                if ui.add(b).clicked() {
                    guardar(true, &mut k, self);
                }
                if ui.button(tr("Solo guardar")).clicked() {
                    guardar(false, &mut k, self);
                }
            });
            ui.label(RichText::new(tr("El kit queda en «Mis kits» (biblioteca y selector de instrumento) y en QUANTUM Drummer. Si mueves la carpeta de las muestras, vuelve a crearlo.")).size(11.0).color(TEXT_DIM()));
        } else if !k.error.is_empty() {
            ui.label(RichText::new(&k.error).color(METER[2]));
        }
        self.kit_editor = Some(k);
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn reconoce_piezas() {
        let p = |r: &str| reconocer(r).map(|k| PIEZAS[k].1);
        assert_eq!(p("Kick/RD_K_1.wav"), Some(36));
        assert_eq!(p("Snare/Processed/RD_S_P_10.wav"), Some(38));
        assert_eq!(p("Cymbals/Hi Hat/RD_C_HH_10.wav"), Some(42));
        assert_eq!(p("Toms/Floor Tom/RD_T_FT_1.wav"), Some(43));
        assert_eq!(p("Toms/High Tom/RD_T_HT_1.wav"), Some(50));
        assert_eq!(p("Toms/Mid Tom/RD_T_MT_1.wav"), Some(47));
        assert_eq!(p("Percussion/Stick Hits/RD_P_SH_1.wav"), Some(37));
        assert_eq!(p("Percussion/Egg Shaker/x.wav"), Some(70));
        assert_eq!(p("Percussion/Blast Block/x.wav"), Some(76));
        assert_eq!(p("Cymbals/Splash/x.wav"), Some(55));
        assert_eq!(p("Percussion/Cowbell/RD_P_C_1.wav"), Some(56));
        assert_eq!(p("Percussion/Sleigh Bells/RD_P_SB_1.wav"), Some(83));
        let mut v = [PathBuf::from("K/RD_K_10.wav"), PathBuf::from("K/RD_K_2.wav")];
        v.sort_by_key(|p| clave_natural(p));
        assert_eq!(v[0], PathBuf::from("K/RD_K_2.wav"));
    }
}
