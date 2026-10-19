//! QUANTUM Partitura: editor de partituras propio (ventana aparte, que también se puede separar a
//! otra pantalla). Documento independiente de las pistas: pentagramas con su clave e instrumento,
//! escritura con el ratón o el teclado (letras A–G, figuras 3–7, puntillo, silencios), alteraciones,
//! claves, armadura y compás; transcripción en vivo de lo que se toca (MIDI o audio monofónico)
//! con detección del pulso; reproducción, impresión/PDF, MusicXML e intercambio con pistas MIDI.
use crate::partitura::{self as pt, Clave, Conf, Diseno, Parte, tiempo_a_x};
use crate::transcripcion::{self as tx, Golpe};
use crate::*;
use egui::{FontId, Key, Pos2, RichText, Sense, Stroke, pos2, vec2};
use project::partitura::{Anotacion, NotaP, Partitura, Pentagrama};

/// Figuras de la paleta: (duración en negras, glifo SMuFL de la nota, nombre, tecla).
const FIGURAS: [(f64, char, &str, Key); 5] = [
    (4.0, '\u{E1D2}', "Redonda", Key::Num7),
    (2.0, '\u{E1D3}', "Blanca", Key::Num6),
    (1.0, '\u{E1D5}', "Negra", Key::Num5),
    (0.5, '\u{E1D7}', "Corchea", Key::Num4),
    (0.25, '\u{E1D9}', "Semicorchea", Key::Num3),
];
/// Instrumentos: (nombre, transposición escrita, clave habitual).
const INSTRUMENTOS: [(&str, i32, Clave); 12] = [
    ("Piano", 0, Clave::Sol),
    ("Voz", 0, Clave::Sol),
    ("Violín", 0, Clave::Sol),
    ("Viola", 0, Clave::Alto),
    ("Violonchelo", 0, Clave::Fa),
    ("Flauta", 0, Clave::Sol),
    ("Clarinete en Sib", 2, Clave::Sol),
    ("Trompeta en Sib", 2, Clave::Sol),
    ("Saxo alto en Mib", 9, Clave::Sol),
    ("Saxo tenor en Sib", 14, Clave::Sol),
    ("Trompa en Fa", 7, Clave::Sol),
    // La clave de sol octava baja ya escribe una octava arriba del sonido.
    ("Guitarra (octava baja)", 0, Clave::Sol8vb),
];
const TONALIDADES: [&str; 15] = ["Dob / Labm", "Solb / Mibm", "Reb / Sibm", "Lab / Fam", "Mib / Dom", "Sib / Solm", "Fa / Rem", "Do / Lam", "Sol / Mim", "Re / Sim", "La / Fa#m", "Mi / Do#m", "Si / Sol#m", "Fa# / Re#m", "Do# / La#m"];
const COMPASES: [(u32, u32); 8] = [(2, 4), (3, 4), (4, 4), (5, 4), (6, 8), (7, 8), (9, 8), (12, 8)];

/// Evento de la reproducción: (segundos, encender, nota).
type EventoSonido = (f64, bool, u8);

/// Grabación en vivo en curso.
struct Grabacion {
    audio: bool,
    detectar: bool,
    escritos0: u64,
    /// Hora del inicio (las notas MIDI se fechan al llegar, con precisión de microsegundos).
    inicio: std::time::Instant,
    cuadros: Vec<tx::Cuadro>,
    mono: Vec<f32>,
    golpes: Vec<Golpe>,
    vista: Vec<NotaP>,
    tempo: Option<f32>,
    actualizado: std::time::Instant,
    detener_transporte: bool,
}

pub struct EditorPartitura {
    pub abierto: bool,
    pub doc: Partitura,
    deshacer: Vec<Partitura>,
    rehacer: Vec<Partitura>,
    pentagrama: usize,
    /// Nota seleccionada: (pentagrama, índice).
    sel: Option<(usize, usize)>,
    figura: f64,
    puntillo: bool,
    /// Posición de escritura con el teclado (en negras).
    cursor: f64,
    sp: f32,
    audio: bool,
    detectar: bool,
    grabacion: Option<Grabacion>,
    /// Reproducción: inicio, eventos (segundos, encender, nota) y el siguiente.
    sonando: Option<(std::time::Instant, Vec<EventoSonido>, usize)>,
    /// El ratón estuvo sobre el editor (los atajos del teclado van a la partitura).
    pub foco: bool,
    /// Herramienta de anotaciones: un clic en el papel pone un texto ahí.
    anotar: bool,
    /// Anotación que se está escribiendo y dónde se ve su cuadro.
    editando: Option<(usize, Pos2)>,
    /// Punto del último clic derecho (pentagrama, tiempo, altura) y la anotación que había ahí.
    punto: Option<(usize, f64, f32, Option<usize>, Pos2)>,
    /// Anotación que se arrastra con el ratón.
    arrastre: Option<usize>,
}

impl EditorPartitura {
    pub fn tiene_seleccion(&self) -> bool {
        self.sel.is_some()
    }
}

impl Default for EditorPartitura {
    fn default() -> Self {
        Self { abierto: false, doc: Partitura::default(), deshacer: vec![], rehacer: vec![], pentagrama: 0, sel: None, figura: 1.0, puntillo: false, cursor: 0.0, sp: 8.0, audio: false, detectar: true, grabacion: None, sonando: None, foco: false, anotar: false, editando: None, punto: None, arrastre: None }
    }
}

/// Paso diatónico de cada letra en semitonos (Do Re Mi Fa Sol La Si).
const NATURAL: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Letras en el orden de la armadura (sostenidos: Fa Do Sol Re La Mi Si).
const ORDEN_SOST: [i32; 7] = [3, 0, 4, 1, 5, 2, 6];

/// Alteración que pone la armadura a una letra.
fn alteracion_armadura(letra: i32, quintas: i32) -> i32 {
    let n = quintas.unsigned_abs() as usize;
    if quintas > 0 && ORDEN_SOST[..n.min(7)].contains(&letra) {
        1
    } else if quintas < 0 && ORDEN_SOST.iter().rev().take(n.min(7)).any(|&l| l == letra) {
        -1
    } else {
        0
    }
}

/// Altura escrita (MIDI) de una posición del pentagrama según la clave y la armadura.
fn altura_en(pos: i32, clave: Clave, quintas: i32) -> u8 {
    if clave == Clave::Percusion {
        return pt::BATERIA.iter().min_by_key(|b| (b.1 - pos).abs()).map_or(38, |b| b.0[0]);
    }
    let paso = pos + clave.paso_primera_linea();
    let (octava, letra) = (paso.div_euclid(7), paso.rem_euclid(7));
    ((octava + 1) * 12 + NATURAL[letra as usize] + alteracion_armadura(letra, quintas)).clamp(0, 127) as u8
}

impl App {
    fn doc_editar(&mut self) {
        let e = &mut self.partitura_app;
        e.deshacer.push(e.doc.clone());
        if e.deshacer.len() > 200 {
            e.deshacer.remove(0);
        }
        e.rehacer.clear();
        self.cambios = true;
    }

    /// Partes para el diseño: la música escrita (con la transposición de cada instrumento).
    fn partes_doc(doc: &Partitura) -> Vec<Parte> {
        doc.pentagramas
            .iter()
            .map(|p| Parte {
                nombre: p.nombre.clone(),
                abrev: p.nombre.chars().take(4).collect::<String>() + ".",
                clave: p.clave,
                llave: p.llave,
                notas: {
                    // Ordenadas por tiempo (el diseño agrupa en acordes las que empiezan juntas).
                    let mut v: Vec<_> = p.notas.iter().enumerate().map(|(k, n)| (n.inicio, n.dur, (n.altura as i32 + p.transposicion).clamp(0, 127) as u8, false, k)).collect();
                    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)));
                    v
                },
            })
            .collect()
    }

    fn conf_doc(doc: &Partitura) -> Conf {
        let compases = (doc.fin() / doc.negras_compas()).ceil() as usize + 1;
        Conf { num: doc.compas.0, den: doc.compas.1, quintas: doc.armadura, acordes: doc.acordes.clone(), compases_min: compases.max(4), anotaciones: doc.anotaciones.iter().map(|a| (a.pentagrama, a.inicio, a.alto, a.texto.clone())).collect() }
    }

    /// Escribe una nota (o silencio si `altura` es None) en el pentagrama desde `inicio`, sustituyendo
    /// lo que había en ese tramo; si ya hay una nota que empieza ahí con la misma duración, se suma
    /// al acorde.
    fn doc_escribir(&mut self, pi: usize, inicio: f64, dur: f64, altura: Option<u8>) {
        let Some(p) = self.partitura_app.doc.pentagramas.get_mut(pi) else { return };
        let fin = inicio + dur;
        if let Some(a) = altura
            && p.notas.iter().any(|n| (n.inicio - inicio).abs() < 1e-6 && (n.dur - dur).abs() < 1e-6 && n.altura != a)
            && !p.notas.iter().any(|n| (n.inicio - inicio).abs() < 1e-6 && n.altura == a)
            && self.ctx.input(|i| i.modifiers.shift)
        {
            p.notas.push(NotaP { inicio, dur, altura: a, alteracion: None });
        } else {
            p.notas.retain(|n| !(n.inicio >= inicio - 1e-6 && n.inicio < fin - 1e-6));
            for n in p.notas.iter_mut().filter(|n| n.inicio < inicio && n.inicio + n.dur > inicio) {
                n.dur = inicio - n.inicio;
            }
            if let Some(a) = altura {
                p.notas.push(NotaP { inicio, dur, altura: a, alteracion: None });
            }
        }
        p.notas.sort_by(|a, b| a.inicio.total_cmp(&b.inicio).then(a.altura.cmp(&b.altura)));
        self.partitura_app.sel = altura.and_then(|a| p.notas.iter().position(|n| (n.inicio - inicio).abs() < 1e-6 && n.altura == a)).map(|k| (pi, k));
        self.partitura_app.cursor = fin;
    }

    /// Agrega notas al pentagrama `pi`; si es un piano, las de Do central hacia arriba van a la mano
    /// derecha (clave de sol) y las demás a la izquierda (clave de fa).
    fn repartir(doc: &mut Partitura, pi: usize, notas: impl Iterator<Item = NotaP>) {
        let (g, n) = doc.grupo(pi);
        for nota in notas {
            let k = if n == 2 { if nota.altura >= 60 { g } else { g + 1 } } else { pi };
            doc.pentagramas[k].notas.push(nota);
        }
        for p in doc.pentagramas[g..g + n].iter_mut() {
            p.notas.sort_by(|a, b| a.inicio.total_cmp(&b.inicio).then(a.altura.cmp(&b.altura)));
        }
    }

    /// Final de lo escrito en el pentagrama (o el piano) de `pi`, redondeado al compás siguiente.
    fn fin_grupo(doc: &Partitura, pi: usize) -> f64 {
        let (g, n) = doc.grupo(pi);
        let compas = doc.negras_compas();
        let fin = doc.pentagramas[g..g + n].iter().flat_map(|p| p.notas.iter().map(|x| x.inicio + x.dur)).fold(0.0, f64::max);
        (fin / compas).ceil() * compas
    }

    /// Quita los pentagramas `a..b` (y sus anotaciones; las demás siguen a su pentagrama).
    fn quitar_pentagramas(&mut self, a: usize, b: usize) {
        let doc = &mut self.partitura_app.doc;
        doc.pentagramas.drain(a..b);
        doc.anotaciones.retain(|x| !(a..b).contains(&x.pentagrama));
        doc.anotaciones.iter_mut().filter(|x| x.pentagrama >= b).for_each(|x| x.pentagrama -= b - a);
        self.partitura_app.sel = None;
        self.partitura_app.editando = None;
    }

    /// Duración actual de la paleta (con puntillo).
    fn dur_paleta(&self) -> f64 {
        self.partitura_app.figura * if self.partitura_app.puntillo { 1.5 } else { 1.0 }
    }

    /// Pentagramas desde las pistas MIDI del proyecto.
    fn doc_desde_pistas(&mut self) {
        let a_negras = 4.0 / self.engine.unit.load(Relaxed).max(1) as f64;
        let mut pentagramas = vec![];
        for t in self.s.tracks.iter().filter(|t| t.midi()) {
            let nombre_min = t.name.to_lowercase();
            let bateria = t.instrument == "bateria" || t.instrument.ends_with(pianoroll::CAJA_RITMOS) || nombre_min.contains("bater") || nombre_min.contains("drum");
            let transp = pt::TRANSPOSITORES.iter().find(|x| x.0.iter().any(|k| nombre_min.contains(k))).map_or(0, |x| x.1);
            let mut notas = vec![];
            for c in &t.clips {
                let Some(ns) = c.notes() else { continue };
                for n in ns.iter() {
                    let ini = c.origin() + n.start as i64;
                    if ini < c.start as i64 || ini >= c.end() as i64 {
                        continue;
                    }
                    let q = |f: i64| (self.beats(f as f64) * a_negras * 4.0).round() / 4.0;
                    let (a, b) = (q(ini), q((ini + n.len as i64).min(c.end() as i64)));
                    notas.push(NotaP { inicio: a, dur: (b - a).max(0.25), altura: n.key, alteracion: None });
                }
            }
            if notas.is_empty() {
                continue;
            }
            notas.sort_by(|a, b| a.inicio.total_cmp(&b.inicio).then(a.altura.cmp(&b.altura)));
            let mut alturas: Vec<u8> = notas.iter().map(|n| n.altura).collect();
            alturas.sort();
            // Piano (por el nombre, o música que abarca los dos registros): sistema de dos pentagramas.
            let piano = !bateria && transp == 0 && (nombre_min.contains("piano") || nombre_min.contains("keys") || (alturas[0] < 55 && alturas[alturas.len() - 1] >= 67));
            if piano {
                let mut doc = Partitura { pentagramas: Pentagrama::piano(), ..Default::default() };
                doc.pentagramas[0].nombre = t.name.clone();
                doc.pentagramas[1].nombre = format!("{} ({})", t.name, tr("izq."));
                Self::repartir(&mut doc, 0, notas.into_iter());
                pentagramas.extend(doc.pentagramas);
                continue;
            }
            let clave = if bateria { Clave::Percusion } else if alturas[alturas.len() / 2] < 57 { Clave::Fa } else { Clave::Sol };
            pentagramas.push(Pentagrama { nombre: t.name.clone(), clave, transposicion: transp, notas, llave: false });
        }
        if pentagramas.is_empty() {
            self.status = tr("No hay pistas MIDI con notas para pasar a la partitura").into();
            return;
        }
        let acordes: Vec<(f64, String)> = self.s.chords.iter().map(|(a, _, n)| ((self.beats(*a as f64) * a_negras * 4.0).round() / 4.0, n.clone())).collect();
        let (compas, tempo, titulo) = ((self.engine.beats.load(Relaxed), self.engine.unit.load(Relaxed)), self.s.tempo.first().map_or(120.0, |t| t.1), self.meta.title.clone());
        self.doc_editar();
        let doc = &mut self.partitura_app.doc;
        doc.pentagramas = pentagramas;
        doc.anotaciones.clear();
        (doc.compas, doc.tempo, doc.acordes) = (compas, tempo, acordes);
        doc.armadura = pt::detectar_armadura(&Self::partes_doc(doc));
        if doc.titulo.is_empty() {
            doc.titulo = titulo;
        }
        self.partitura_app.sel = None;
    }

    /// Una pista MIDI por pentagrama con la música de la partitura (en el inicio del proyecto).
    fn doc_a_pistas(&mut self) {
        let doc = self.partitura_app.doc.clone();
        let de_negras = self.engine.unit.load(Relaxed).max(1) as f64 / 4.0;
        self.edit();
        for p in doc.pentagramas.iter().filter(|p| !p.notas.is_empty()) {
            self.add_track(format!("{} ({})", p.nombre, tr("partitura")), TrackKind::Midi);
            let i = self.s.tracks.len() - 1;
            let notas: Vec<Note> = p.notas.iter().map(|n| {
                let (a, b) = (self.frame_of(n.inicio * de_negras) as u64, self.frame_of((n.inicio + n.dur) * de_negras) as u64);
                Note::new(a, (b - a).max(1), n.altura, 100)
            }).collect();
            let fin = notas.iter().map(|n| n.start + n.len).max().unwrap_or(0);
            let compas = self.bar_frames().max(1);
            self.s.tracks[i].clips.push(Clip::new(Source::Midi(Arc::new(notas)), 0, fin.div_ceil(compas) * compas));
            if p.clave == Clave::Percusion {
                let t = &mut self.s.tracks[i];
                t.instrument = format!("synth:{}", pianoroll::CAJA_RITMOS);
                if let Some(s) = &t.synth {
                    s.kit.store(true, Relaxed);
                }
            }
        }
        self.dirty = true;
        self.status = tr("Partitura pasada a pistas MIDI").into();
    }

    /// Exporta a MusicXML (lo abren MuseScore, Sibelius, Finale, Dorico…).
    fn doc_musicxml(&mut self) -> anyhow::Result<PathBuf> {
        let doc = &self.partitura_app.doc;
        let div = 4; // divisiones por negra (semicorcheas)
        let compas = doc.negras_compas();
        let n_compases = ((doc.fin() / compas).ceil() as usize).max(1);
        let tipo = |d: f64| match d {
            d if d >= 4.0 - 1e-6 => ("whole", false),
            d if d >= 3.0 - 1e-6 => ("half", true),
            d if d >= 2.0 - 1e-6 => ("half", false),
            d if d >= 1.5 - 1e-6 => ("quarter", true),
            d if d >= 1.0 - 1e-6 => ("quarter", false),
            d if d >= 0.75 - 1e-6 => ("eighth", true),
            d if d >= 0.5 - 1e-6 => ("eighth", false),
            _ => ("16th", false),
        };
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE score-partwise PUBLIC \"-//Recordare//DTD MusicXML 4.0 Partwise//EN\" \"http://www.musicxml.org/dtds/partwise.dtd\">\n<score-partwise version=\"4.0\">\n");
        x += &format!("<work><work-title>{}</work-title></work><identification><creator type=\"composer\">{}</creator><encoding><software>Quantum DAW {}</software></encoding></identification>\n<part-list>", pt::html_escapar(&doc.titulo), pt::html_escapar(&doc.compositor), welcome::VERSION);
        for (k, p) in doc.pentagramas.iter().enumerate() {
            if p.llave && k + 1 < doc.pentagramas.len() {
                x += &format!("<part-group type=\"start\" number=\"{}\"><group-symbol>brace</group-symbol><group-barline>yes</group-barline></part-group>", k + 1);
            }
            x += &format!("<score-part id=\"P{}\"><part-name>{}</part-name></score-part>", k + 1, pt::html_escapar(&p.nombre));
            if k > 0 && doc.pentagramas[k - 1].llave {
                x += &format!("<part-group type=\"stop\" number=\"{k}\"/>");
            }
        }
        x += "</part-list>\n";
        // Anotaciones como textos (words) al principio de su compás.
        let compas_de = |a: &Anotacion| (a.inicio / compas).floor().max(0.0) as usize;
        const LETRAS: [&str; 7] = ["C", "D", "E", "F", "G", "A", "B"];
        let ortografia: [(usize, i32); 12] = if doc.armadura >= 0 { [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)] } else { [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)] };
        for (k, p) in doc.pentagramas.iter().enumerate() {
            x += &format!("<part id=\"P{}\">\n", k + 1);
            for m in 0..n_compases {
                x += &format!("<measure number=\"{}\">", m + 1);
                if m == 0 {
                    let (signo, linea, oct) = match p.clave {
                        Clave::Sol => ("G", 2, 0),
                        Clave::Sol8vb => ("G", 2, -1),
                        Clave::Fa => ("F", 4, 0),
                        Clave::Alto => ("C", 3, 0),
                        Clave::Tenor => ("C", 4, 0),
                        Clave::Percusion => ("percussion", 3, 0),
                    };
                    let transp = if p.transposicion != 0 { format!("<transpose><chromatic>{}</chromatic></transpose>", -p.transposicion) } else { String::new() };
                    x += &format!("<attributes><divisions>{div}</divisions><key><fifths>{}</fifths></key><time><beats>{}</beats><beat-type>{}</beat-type></time><clef><sign>{signo}</sign><line>{linea}</line>{}</clef>{transp}</attributes>", doc.armadura, doc.compas.0, doc.compas.1, if oct != 0 { format!("<clef-octave-change>{oct}</clef-octave-change>") } else { String::new() });
                    if k == 0 {
                        x += &format!("<direction placement=\"above\"><direction-type><metronome><beat-unit>quarter</beat-unit><per-minute>{:.0}</per-minute></metronome></direction-type><sound tempo=\"{:.0}\"/></direction>", doc.tempo, doc.tempo);
                    }
                }
                for an in doc.anotaciones.iter().filter(|an| an.pentagrama == k && compas_de(an) == m && !an.texto.is_empty()) {
                    let pos = ((an.inicio - m as f64 * compas) * div as f64).round();
                    x += &format!("<direction placement=\"{}\"><direction-type><words>{}</words></direction-type><offset>{pos}</offset></direction>", if an.alto >= -2.0 { "above" } else { "below" }, pt::html_escapar(&an.texto));
                }
                let (a, b) = (m as f64 * compas, (m + 1) as f64 * compas);
                let mut t = a;
                let mut notas: Vec<&NotaP> = p.notas.iter().filter(|n| n.inicio >= a - 1e-6 && n.inicio < b - 1e-6).collect();
                notas.sort_by(|x, y| x.inicio.total_cmp(&y.inicio));
                let mut i = 0;
                while i < notas.len() {
                    let ini = notas[i].inicio;
                    if ini > t + 1e-6 {
                        for f in pt_figuras(ini - t) {
                            let (ty, dot) = tipo(f);
                            x += &format!("<note><rest/><duration>{}</duration><type>{ty}</type>{}</note>", (f * div as f64).round(), if dot { "<dot/>" } else { "" });
                        }
                    }
                    let grupo: Vec<&NotaP> = notas[i..].iter().take_while(|n| (n.inicio - ini).abs() < 1e-6).copied().collect();
                    let dur = grupo.iter().map(|n| n.dur).fold(0.0, f64::max).min(b - ini);
                    let (ty, dot) = tipo(dur);
                    for (g, n) in grupo.iter().enumerate() {
                        let escrita = (n.altura as i32 + p.transposicion).clamp(0, 127);
                        let (l, alt) = ortografia[(escrita % 12) as usize];
                        let alter = if alt != 0 { format!("<alter>{alt}</alter>") } else { String::new() };
                        x += &format!("<note>{}<pitch><step>{}</step>{alter}<octave>{}</octave></pitch><duration>{}</duration><type>{ty}</type>{}</note>", if g > 0 { "<chord/>" } else { "" }, LETRAS[l], escrita / 12 - 1, (dur * div as f64).round(), if dot { "<dot/>" } else { "" });
                    }
                    t = ini + dur;
                    i += grupo.len();
                }
                if t < b - 1e-6 {
                    for f in pt_figuras(b - t) {
                        let (ty, dot) = tipo(f);
                        x += &format!("<note><rest/><duration>{}</duration><type>{ty}</type>{}</note>", (f * div as f64).round(), if dot { "<dot/>" } else { "" });
                    }
                }
                x += "</measure>\n";
            }
            x += "</part>\n";
        }
        x += "</score-partwise>\n";
        let titulo = if doc.titulo.is_empty() { tr("Partitura").to_string() } else { doc.titulo.clone() };
        let ruta = self.dir.join(project::RENDER_DIR).join(format!("{}.musicxml", safe(&titulo)));
        fs::create_dir_all(ruta.parent().unwrap_or(&self.dir))?;
        fs::write(&ruta, x)?;
        Ok(ruta)
    }

    /// Reproduce la partitura en la primera pista MIDI (o la detiene).
    fn doc_reproducir(&mut self) {
        if let Some((_, ev, _)) = self.partitura_app.sonando.take() {
            ev.iter().filter(|e| !e.1).for_each(|e| self.engine.send_midi([0x80, e.2, 0]));
            return;
        }
        let Some(i) = self.s.tracks.iter().position(|t| t.midi()) else {
            self.status = tr("Crea una pista MIDI para escuchar la partitura").into();
            return;
        };
        self.s.tracks[i].params.monitor.store(true, Relaxed);
        let seg = 60.0 / self.partitura_app.doc.tempo.max(20.0) as f64;
        let mut ev: Vec<(f64, bool, u8)> = self.partitura_app.doc.pentagramas.iter().flat_map(|p| p.notas.iter().flat_map(|n| [(n.inicio * seg, true, n.altura), ((n.inicio + n.dur) * seg - 0.01, false, n.altura)])).collect();
        ev.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        self.partitura_app.sonando = Some((std::time::Instant::now(), ev, 0));
    }

    fn tick_reproduccion(&mut self) {
        let Some((inicio, ev, sig)) = &mut self.partitura_app.sonando else { return };
        let t = inicio.elapsed().as_secs_f64();
        while *sig < ev.len() && ev[*sig].0 <= t {
            let (_, on, k) = ev[*sig];
            self.engine.send_midi(if on { [0x90, k, 96] } else { [0x80, k, 0] });
            *sig += 1;
        }
        if *sig >= ev.len() {
            self.partitura_app.sonando = None;
        } else {
            self.ctx.request_repaint();
        }
    }

    /// Empieza o termina la grabación en vivo.
    fn doc_grabar(&mut self) {
        if let Some(g) = self.partitura_app.grabacion.take() {
            if g.detener_transporte {
                self.stop();
            }
            self.engine.metronome.store(false, Relaxed);
            if g.vista.is_empty() {
                self.status = tr("No se detectaron notas").into();
                return;
            }
            self.doc_editar();
            let e = &mut self.partitura_app;
            let pi = e.pentagrama.min(e.doc.pentagramas.len() - 1);
            // Se agrega en el compás siguiente al final de lo escrito (en el piano, de las dos manos).
            let desde = Self::fin_grupo(&e.doc, pi);
            let vacio = desde == 0.0;
            let piano = e.doc.grupo(pi).1 == 2;
            Self::repartir(&mut e.doc, pi, g.vista.iter().map(|n| NotaP { inicio: n.inicio + desde, ..*n }));
            if vacio {
                // Clave según el registro (el piano ya tiene las suyas) y tempo detectado.
                let mut alturas: Vec<u8> = g.vista.iter().map(|n| n.altura).collect();
                alturas.sort();
                let p = &mut e.doc.pentagramas[pi];
                if p.clave != Clave::Percusion && !piano {
                    p.clave = if alturas[alturas.len() / 2] < 57 { Clave::Fa } else { Clave::Sol };
                }
                if let Some(t) = g.tempo.filter(|_| g.detectar) {
                    e.doc.tempo = t.round();
                }
                e.doc.armadura = pt::detectar_armadura(&Self::partes_doc(&e.doc));
            }
            self.status = format!("{} {}", g.vista.len(), tr("notas escritas en la partitura"));
            return;
        }
        let escritos0 = self.engine.captura.lock().map(|c| c.escritos).unwrap_or_default();
        let detectar = self.partitura_app.detectar;
        let mut detener = false;
        if !detectar {
            // Pulso fijo: metrónomo al tempo de la partitura desde el inicio del compás.
            let bpm = self.partitura_app.doc.tempo;
            self.set_tempo_here(bpm);
            self.engine.metronome.store(true, Relaxed);
            if !self.playing() {
                let compas = self.bar_frames().max(1);
                self.go(self.pos() / compas * compas);
                self.toggle_play();
                detener = true;
            }
        }
        if self.partitura_app.audio && self.engine.in_channels == 0 {
            self.status = tr("No hay entrada de audio: elígela en la configuración de audio").into();
        }
        self.partitura_app.grabacion = Some(Grabacion {
            audio: self.partitura_app.audio,
            detectar,
            escritos0,
            inicio: std::time::Instant::now(),
            cuadros: vec![],
            mono: vec![],
            golpes: vec![],
            vista: vec![],
            tempo: None,
            actualizado: std::time::Instant::now(),
            detener_transporte: detener,
        });
    }

    /// Transcribe lo tocado desde que empezó la grabación (cada 0,3 s).
    fn tick_grabacion(&mut self) {
        let sr = self.engine.sample_rate as f64;
        let tempo_doc = self.partitura_app.doc.tempo as f64;
        let Some(g) = &mut self.partitura_app.grabacion else { return };
        // Latencia mínima: el MIDI se transcribe en cada cuadro; el audio cada 60 ms (análisis por ventanas).
        self.ctx.request_repaint();
        if g.audio && g.actualizado.elapsed() < Duration::from_millis(60) {
            return;
        }
        g.actualizado = std::time::Instant::now();
        let Ok(c) = self.engine.captura.lock() else { return };
        if g.audio {
            // Audio nuevo (mezcla mono) y análisis solo de las ventanas nuevas.
            let nuevos = (c.escritos - g.escritos0) as usize - g.mono.len();
            let ch = c.canales.max(1);
            let datos = c.ultimos(nuevos);
            drop(c);
            g.mono.extend(datos.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32));
            let desde = g.cuadros.len() * tx::SALTO;
            g.cuadros.extend(tx::cuadros_audio(&g.mono, sr as f32, desde));
            g.golpes = tx::segmentar(&g.cuadros, sr as f32);
        } else {
            drop(c);
            // MIDI fechado al llegar: la duración de las notas que siguen sonando llega hasta ahora.
            let ahora = g.inicio.elapsed().as_secs_f64();
            let mut sueltas: std::collections::HashMap<u8, (f64, u8)> = Default::default();
            let mut golpes = vec![];
            for (t, e) in engine::llegadas_desde(g.inicio) {
                match (e[0] & 0xF0, e[2]) {
                    (0x90, v) if v > 0 => _ = sueltas.insert(e[1], (t, v)),
                    (0x80 | 0x90, _) => {
                        if let Some((a, v)) = sueltas.remove(&e[1]) {
                            golpes.push(Golpe { inicio: a, dur: t - a, altura: e[1], vel: v });
                        }
                    }
                    _ => {}
                }
            }
            golpes.extend(sueltas.into_iter().map(|(k, (a, v))| Golpe { inicio: a, dur: ahora - a, altura: k, vel: v }));
            golpes.sort_by(|a, b| a.inicio.total_cmp(&b.inicio));
            g.golpes = golpes;
        }
        let pulso = if g.detectar { tx::estimar_pulso(&g.golpes.iter().map(|x| x.inicio).collect::<Vec<_>>()) } else { Some((60.0 / tempo_doc, 0.0)) };
        if let Some((periodo, fase)) = pulso {
            g.tempo = Some((60.0 / periodo) as f32);
            // Con pulso fijo el tiempo 0 es el inicio de la grabación (el metrónomo arrancó ahí).
            g.vista = tx::cuantizar_con(&g.golpes, periodo, fase, !g.detectar);
        }
    }

    /// Acciones del menú circular de la partitura.
    pub fn pie_partitura(&mut self, a: pie::Accion) {
        use pie::Accion;
        match a {
            Accion::PtFigura(k) => self.partitura_app.figura = FIGURAS[(k as usize).min(4)].0,
            Accion::PtPuntillo => self.partitura_app.puntillo = !self.partitura_app.puntillo,
            Accion::PtSilencio => self.doc_silencio(),
            Accion::PtSubir => self.doc_mover_sel(1),
            Accion::PtBajar => self.doc_mover_sel(-1),
            Accion::PtBorrar => self.doc_borrar_sel(),
            Accion::PtEscuchar => self.doc_reproducir(),
            Accion::PtClave(c) => {
                self.doc_editar();
                let pi = self.partitura_app.pentagrama.min(self.partitura_app.doc.pentagramas.len() - 1);
                self.partitura_app.doc.pentagramas[pi].clave = c;
            }
            Accion::PtArmadura(q) => {
                self.doc_editar();
                self.partitura_app.doc.armadura = q as i32;
            }
            // Anotación en el punto del clic derecho (o editar la que había ahí).
            Accion::PtAnotacion => {
                match self.partitura_app.punto.take() {
                    Some((_, _, _, Some(a), q)) => self.partitura_app.editando = Some((a, q)),
                    Some((pi, t, alto, None, q)) => self.doc_anotar(pi, t, alto, q),
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// Ventana de QUANTUM Partitura.
    pub fn ventana_partitura(&mut self, ctx: &egui::Context) {
        self.tick_reproduccion();
        self.tick_grabacion();
        if !self.partitura_app.abierto {
            self.partitura_app.foco = false;
            return;
        }
        let abierta = self.ventana(ctx, "partitura-app", "QUANTUM Partitura".into(), [1200.0, 820.0], false, |app, ui| app.editor_partitura(ui));
        if !abierta {
            self.partitura_app.abierto = false;
        }
    }

    fn editor_partitura(&mut self, ui: &mut egui::Ui) {
        self.partitura_app.foco = ui.ui_contains_pointer() && !ui.ctx().egui_wants_keyboard_input();
        // Barra 1: documento.
        ui.horizontal(|ui| {
            let e = &mut self.partitura_app;
            ui.add(egui::TextEdit::singleline(&mut e.doc.titulo).hint_text(tr("Título")).desired_width(200.0).font(egui::TextStyle::Heading));
            ui.add(egui::TextEdit::singleline(&mut e.doc.compositor).hint_text(tr("Compositor")).desired_width(160.0));
            ui.separator();
            if ui.button(tr("Nueva")).clicked() {
                self.doc_editar();
                self.partitura_app.doc = Partitura::default();
                self.partitura_app.sel = None;
            }
            if ui.button(tr("Desde pistas MIDI")).on_hover_text(tr("Escribe en la partitura las pistas MIDI del proyecto")).clicked() {
                self.doc_desde_pistas();
            }
            if ui.button(tr("A pistas MIDI")).on_hover_text(tr("Crea una pista MIDI por pentagrama con esta música")).clicked() {
                self.doc_a_pistas();
            }
            if ui.button("MusicXML…").on_hover_text(tr("Para abrirla en MuseScore, Sibelius, Finale o Dorico")).clicked() {
                let r = self.doc_musicxml();
                self.report(r.as_ref().map_or(String::new(), |p| format!("{} {}", tr("MusicXML guardado en"), p.display())), r);
            }
            if ui.add(egui::Button::new(RichText::new(tr("Imprimir / PDF…")).color(BG()).strong()).fill(ACCENT())).clicked() {
                let doc = self.partitura_app.doc.clone();
                let titulo = if doc.titulo.is_empty() { tr("Partitura").to_string() } else { doc.titulo.clone() };
                let r = pt::pagina_html(&titulo, &doc.compositor, |ancho, sp| pt::disenar(&Self::partes_doc(&doc), &Self::conf_doc(&doc), ancho, sp)).and_then(|h| self.abrir_html_partitura(&titulo, h));
                self.report(tr("Partitura abierta en el navegador: imprímela o guárdala como PDF"), r);
            }
            ui.separator();
            if ui.add_enabled(!self.partitura_app.deshacer.is_empty(), egui::Button::new(tr("Deshacer"))).clicked() {
                self.doc_deshacer();
            }
            if ui.add_enabled(!self.partitura_app.rehacer.is_empty(), egui::Button::new(tr("Rehacer"))).clicked() {
                self.doc_rehacer();
            }
        });
        // Barra 2: compás, tonalidad, tempo y pentagrama elegido.
        ui.horizontal(|ui| {
            let mut cambio = None;
            let doc = &self.partitura_app.doc;
            let (mut compas, mut armadura, mut tempo) = (doc.compas, doc.armadura, doc.tempo);
            egui::ComboBox::from_id_salt("pt-compas").width(60.0).selected_text(format!("{}/{}", compas.0, compas.1)).show_ui(ui, |ui| {
                for c in COMPASES {
                    ui.selectable_value(&mut compas, c, format!("{}/{}", c.0, c.1));
                }
            });
            egui::ComboBox::from_id_salt("pt-armadura").width(120.0).selected_text(TONALIDADES[(armadura + 7) as usize]).show_ui(ui, |ui| {
                for (k, n) in TONALIDADES.iter().enumerate() {
                    ui.selectable_value(&mut armadura, k as i32 - 7, *n);
                }
            });
            ui.add(egui::DragValue::new(&mut tempo).range(20.0..=300.0).speed(0.5).suffix(" BPM"));
            if (compas, armadura, tempo) != (doc.compas, doc.armadura, doc.tempo) {
                cambio = Some((compas, armadura, tempo));
            }
            if let Some((c, a, t)) = cambio {
                self.doc_editar();
                let d = &mut self.partitura_app.doc;
                (d.compas, d.armadura, d.tempo) = (c, a, t);
            }
            ui.separator();
            let e = &mut self.partitura_app;
            e.pentagrama = e.pentagrama.min(e.doc.pentagramas.len().saturating_sub(1));
            egui::ComboBox::from_id_salt("pt-pentagrama").width(130.0).selected_text(e.doc.pentagramas.get(e.pentagrama).map_or(String::new(), |p| p.nombre.clone())).show_ui(ui, |ui| {
                for (k, p) in e.doc.pentagramas.iter().enumerate() {
                    ui.selectable_value(&mut e.pentagrama, k, &p.nombre);
                }
            });
            let pi = e.pentagrama;
            if let Some(p) = e.doc.pentagramas.get(pi).cloned() {
                let (mut clave, mut nombre, mut transp) = (p.clave, p.nombre.clone(), p.transposicion);
                ui.add(egui::TextEdit::singleline(&mut nombre).desired_width(110.0));
                egui::ComboBox::from_id_salt("pt-clave").width(140.0).selected_text(format!("{} {}", tr("Clave de"), tr(clave.nombre()))).show_ui(ui, |ui| {
                    for c in Clave::TODAS {
                        ui.selectable_value(&mut clave, c, tr(c.nombre()));
                    }
                });
                let (g, n_grupo) = e.doc.grupo(pi);
                let es_piano = n_grupo == 2;
                let instr = if es_piano { "Piano" } else { INSTRUMENTOS[1..].iter().find(|x| x.1 == transp && x.2 == clave).or(INSTRUMENTOS[1..].iter().find(|x| x.1 == transp)).map_or("—", |x| x.0) };
                let mut elegido = None;
                egui::ComboBox::from_id_salt("pt-instrumento").width(170.0).selected_text(tr(instr)).show_ui(ui, |ui| {
                    for (k, (n, t, c)) in INSTRUMENTOS.into_iter().enumerate() {
                        if ui.selectable_label(tr(instr) == tr(n), tr(n)).clicked() {
                            elegido = Some(k);
                            (transp, clave) = (t, c);
                            if nombre.starts_with(tr("Pentagrama")) || nombre.starts_with("Piano") || INSTRUMENTOS.iter().any(|x| tr(x.0).starts_with(nombre.as_str())) {
                                nombre = tr(n).split(" en ").next().unwrap_or(n).split(" (").next().unwrap_or(n).to_string();
                            }
                        }
                    }
                });
                match elegido {
                    // Piano: clave de sol arriba y de fa abajo, unidas por la llave (las notas graves pasan abajo).
                    Some(0) if !es_piano => {
                        self.doc_editar();
                        let doc = &mut self.partitura_app.doc;
                        let notas = std::mem::take(&mut doc.pentagramas[pi].notas);
                        let mut nuevo = Pentagrama::piano();
                        nuevo[0].nombre = nombre.clone();
                        nuevo[1].nombre = format!("{nombre} ({})", tr("izq."));
                        doc.pentagramas.splice(pi..=pi, nuevo);
                        doc.anotaciones.iter_mut().filter(|a| a.pentagrama > pi).for_each(|a| a.pentagrama += 1);
                        Self::repartir(doc, pi, notas.into_iter());
                        self.partitura_app.sel = None;
                    }
                    Some(0) => {}
                    // Otro instrumento en un piano: un solo pentagrama con la música de las dos manos.
                    Some(_) if es_piano => {
                        self.doc_editar();
                        let abajo = std::mem::take(&mut self.partitura_app.doc.pentagramas[g + 1].notas);
                        self.quitar_pentagramas(g + 1, g + 2);
                        let q = &mut self.partitura_app.doc.pentagramas[g];
                        q.llave = false;
                        q.notas.extend(abajo);
                        q.notas.sort_by(|a, b| a.inicio.total_cmp(&b.inicio).then(a.altura.cmp(&b.altura)));
                        (q.clave, q.nombre, q.transposicion) = (clave, nombre, transp);
                        self.partitura_app.pentagrama = g;
                    }
                    _ => {
                        if (clave, &nombre, transp) != (p.clave, &p.nombre, p.transposicion) {
                            self.doc_editar();
                            let q = &mut self.partitura_app.doc.pentagramas[pi];
                            (q.clave, q.nombre, q.transposicion) = (clave, nombre, transp);
                        }
                    }
                }
            }
            if ui.button(tr("+ Pentagrama")).clicked() {
                self.doc_editar();
                let n = self.partitura_app.doc.pentagramas.len() + 1;
                self.partitura_app.doc.pentagramas.push(Pentagrama { nombre: format!("{} {n}", tr("Pentagrama")), ..Default::default() });
                self.partitura_app.pentagrama = n - 1;
            }
            if ui.button(tr("+ Piano")).on_hover_text(tr("Agrega un sistema de piano: clave de sol y clave de fa unidas por una llave")).clicked() {
                self.doc_editar();
                let n = self.partitura_app.doc.pentagramas.len();
                self.partitura_app.doc.pentagramas.extend(Pentagrama::piano());
                self.partitura_app.pentagrama = n;
            }
            // Quitar: el pentagrama elegido (en el piano, los dos).
            let (g, n_grupo) = self.partitura_app.doc.grupo(self.partitura_app.pentagrama);
            if self.partitura_app.doc.pentagramas.len() > n_grupo && ui.button(tr("− Quitar")).clicked() {
                self.doc_editar();
                self.quitar_pentagramas(g, g + n_grupo);
                self.partitura_app.pentagrama = g.min(self.partitura_app.doc.pentagramas.len() - 1);
            }
        });
        // Barra 3: figuras, alteraciones, grabación y reproducción.
        ui.horizontal(|ui| {
            let musica = |t: f32| FontId::new(t, egui::FontFamily::Name("musica".into()));
            for (d, glifo, nombre, tecla) in FIGURAS {
                let on = (self.partitura_app.figura - d).abs() < 1e-6;
                let b = egui::Button::new(RichText::new(glifo.to_string()).font(musica(24.0)).color(if on { BG() } else { TEXT() })).fill(if on { ACCENT() } else { ELEVATED() }).min_size(vec2(34.0, 40.0));
                if ui.add(b).on_hover_text(format!("{} ({})", tr(nombre), tecla.name())).clicked() {
                    self.partitura_app.figura = d;
                }
            }
            let on = self.partitura_app.puntillo;
            if ui.add(egui::Button::new(RichText::new(tr("Puntillo")).color(if on { BG() } else { TEXT() })).fill(if on { ACCENT() } else { ELEVATED() })).on_hover_text(".").clicked() {
                self.partitura_app.puntillo = !on;
            }
            if ui.button(tr("Silencio")).on_hover_text(tr("Escribe un silencio en el cursor (0)")).clicked() {
                self.doc_silencio();
            }
            ui.separator();
            for (glifo, delta, ayuda) in [('\u{E262}', 1, "Subir un semitono (flecha arriba)"), ('\u{E260}', -1, "Bajar un semitono (flecha abajo)")] {
                if ui.add(egui::Button::new(RichText::new(glifo.to_string()).font(musica(22.0))).min_size(vec2(30.0, 40.0))).on_hover_text(tr(ayuda)).clicked() {
                    self.doc_mover_sel(delta);
                }
            }
            if ui.button(tr("Borrar")).on_hover_text(tr("Borra la nota elegida (Supr)")).clicked() {
                self.doc_borrar_sel();
            }
            ui.separator();
            let on = self.partitura_app.anotar;
            if ui.add(egui::Button::new(RichText::new(format!("T  {}", tr("Anotación"))).color(if on { BG() } else { TEXT() })).fill(if on { ACCENT() } else { ELEVATED() })).on_hover_text(tr("Con esta herramienta, un clic en la partitura pone un texto en ese punto (T). También con clic derecho → Anotación")).clicked() {
                self.partitura_app.anotar = !on;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Slider::new(&mut self.partitura_app.sp, 5.0..=14.0).show_value(false).text(tr("Tamaño")));
                let sonando = self.partitura_app.sonando.is_some();
                if ui.button(if sonando { tr("Detener") } else { tr("Escuchar") }).clicked() {
                    self.doc_reproducir();
                }
            });
        });
        // Barra 4: transcripción en vivo.
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr("Escribir tocando:")).color(TEXT_DIM()));
            let grabando = self.partitura_app.grabacion.is_some();
            ui.add_enabled_ui(!grabando, |ui| {
                ui.selectable_value(&mut self.partitura_app.audio, false, tr("Teclado MIDI"));
                ui.selectable_value(&mut self.partitura_app.audio, true, tr("Audio (una nota a la vez)"));
                egui::ComboBox::from_id_salt("pt-pulso").width(150.0).selected_text(if self.partitura_app.detectar { tr("Detectar el pulso") } else { tr("Pulso fijo (metrónomo)") }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.partitura_app.detectar, true, tr("Detectar el pulso"));
                    ui.selectable_value(&mut self.partitura_app.detectar, false, tr("Pulso fijo (metrónomo)"));
                });
            });
            let texto = if grabando { tr("Terminar") } else { tr("Grabar tocando") };
            if ui.add(egui::Button::new(RichText::new(texto).color(Color32::WHITE).strong()).fill(METER[2])).on_hover_text(tr("Toca y la partitura se escribe sola; al terminar se agrega al pentagrama elegido")).clicked() {
                self.doc_grabar();
            }
            if let Some(g) = &self.partitura_app.grabacion {
                let t = g.tempo.map_or("—".to_string(), |t| format!("{t:.0} BPM"));
                ui.label(RichText::new(format!("{} {} · {} {}", g.vista.len(), tr("notas"), tr("pulso"), t)).color(METER[2]));
            } else {
                ui.label(RichText::new(tr("Teclado: A–G notas, 3–7 figuras, «.» puntillo, 0 silencio, T anotación, flechas mueven, Supr borra")).size(11.0).color(TEXT_DIM()));
            }
        });
        self.doc_teclado(ui);
        // Papel con la partitura.
        let sp = self.partitura_app.sp;
        let mut doc_vista = self.partitura_app.doc.clone();
        if let Some(g) = &self.partitura_app.grabacion {
            // Mientras se graba se ve lo transcrito, en el pentagrama elegido.
            let pi = self.partitura_app.pentagrama.min(doc_vista.pentagramas.len() - 1);
            let desde = Self::fin_grupo(&doc_vista, pi);
            Self::repartir(&mut doc_vista, pi, g.vista.iter().map(|n| NotaP { inicio: n.inicio + desde, ..*n }));
        }
        egui::Frame::new().fill(Color32::from_rgb(0xFA, 0xF8, 0xF2)).corner_radius(widgets::rr(6.0)).inner_margin(sp * 1.5).show(ui, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                let ancho = ui.available_width().max(500.0);
                let diseno: Diseno = pt::disenar(&Self::partes_doc(&doc_vista), &Self::conf_doc(&doc_vista), ancho, sp);
                let (r, resp) = ui.allocate_exact_size(vec2(ancho, diseno.alto * diseno.sistemas.len() as f32), Sense::click_and_drag());
                let p = ui.painter_at(r);
                for (k, s) in diseno.sistemas.iter().enumerate() {
                    pt::pintar(&p, s, r.min.to_vec2() + vec2(0.0, k as f32 * diseno.alto), Color32::from_gray(20));
                }
                self.doc_interaccion(ui, &resp, r, &diseno, &doc_vista);
            });
        });
        self.doc_editor_anotacion(ui.ctx());
        if self.pie.as_ref().is_some_and(|p| p.en_partitura) {
            self.procesar_pie(ui.ctx());
        }
    }

    /// Pone una anotación en un punto (pentagrama, tiempo, altura) y abre su cuadro para escribir.
    fn doc_anotar(&mut self, pi: usize, inicio: f64, alto: f32, pantalla: Pos2) {
        self.doc_editar();
        let doc = &mut self.partitura_app.doc;
        doc.anotaciones.push(Anotacion { pentagrama: pi, inicio, alto, texto: String::new() });
        self.partitura_app.editando = Some((doc.anotaciones.len() - 1, pantalla));
    }

    /// Cuadro flotante para escribir la anotación elegida (Intro o «Listo» cierra; vacía se borra).
    fn doc_editor_anotacion(&mut self, ctx: &egui::Context) {
        let Some((k, pos)) = self.partitura_app.editando else { return };
        if k >= self.partitura_app.doc.anotaciones.len() {
            self.partitura_app.editando = None;
            return;
        }
        let (mut cerrar, mut borrar) = (false, false);
        egui::Area::new(egui::Id::new("pt-anotacion")).order(egui::Order::Foreground).fixed_pos(pos + vec2(0.0, 10.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new(tr("Anotación")).strong().color(ACCENT()));
                let t = &mut self.partitura_app.doc.anotaciones[k].texto;
                let r = ui.add(egui::TextEdit::singleline(t).hint_text(tr("Escribe el texto (p. ej. dolce, rit., D.C., digitación)…")).desired_width(280.0));
                if !r.has_focus() && !r.lost_focus() {
                    r.request_focus();
                }
                cerrar |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                ui.horizontal(|ui| {
                    for sugerencia in ["p", "mf", "f", "cresc.", "rit.", "a tempo", "D.C. al Fine"] {
                        if ui.small_button(sugerencia).clicked() {
                            self.partitura_app.doc.anotaciones[k].texto = sugerencia.to_string();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    cerrar |= ui.button(tr("Listo")).clicked();
                    borrar |= ui.button(RichText::new(tr("Borrar")).color(METER[2])).clicked();
                    ui.label(RichText::new(tr("Arrastra el texto en la partitura para moverlo")).size(11.0).color(TEXT_DIM()));
                });
            });
        });
        cerrar |= ctx.input(|i| i.key_pressed(Key::Escape));
        if borrar || (cerrar && self.partitura_app.doc.anotaciones[k].texto.trim().is_empty()) {
            self.partitura_app.doc.anotaciones.remove(k);
            cerrar = true;
        }
        if cerrar {
            self.partitura_app.editando = None;
        }
    }

    /// Clic en el papel: escribir en el lugar señalado o elegir una nota.
    fn doc_interaccion(&mut self, ui: &egui::Ui, resp: &egui::Response, r: egui::Rect, d: &Diseno, doc: &Partitura) {
        let sp = self.partitura_app.sp;
        let p = ui.painter_at(r);
        let compas = doc.negras_compas();
        // Selección resaltada y cursor de escritura.
        for (k, mapa) in d.mapas.iter().enumerate() {
            let dy = r.top() + k as f32 * d.alto;
            for &(c, pi, id) in &mapa.cabezas {
                if self.partitura_app.sel == Some((pi, id)) {
                    p.circle_stroke(pos2(r.left() + c.x, dy + c.y), sp * 1.0, Stroke::new(2.0, ACCENT()));
                }
            }
            if let Some(&(base, _)) = mapa.partes.get(self.partitura_app.pentagrama) {
                for (m, x0, x1, puntos) in &mapa.compases {
                    let rel = self.partitura_app.cursor - *m as f64 * compas;
                    if (0.0..compas).contains(&rel) {
                        let x = tiempo_a_x(puntos, *x0, *x1, compas, rel);
                        p.line_segment([pos2(r.left() + x - sp * 0.3, dy + base + sp), pos2(r.left() + x - sp * 0.3, dy + base - sp * 5.0)], Stroke::new(2.0, ACCENT().gamma_multiply(0.8)));
                    }
                }
            }
        }
        let Some(q) = resp.hover_pos().or(resp.interact_pointer_pos()) else { return };
        if resp.secondary_clicked() {
            self.partitura_app.punto = None;
        }
        let k = ((q.y - r.top()) / d.alto.max(1.0)).floor().max(0.0) as usize;
        let Some(mapa) = d.mapas.get(k) else { return };
        let local = Pos2::new(q.x - r.left(), q.y - r.top() - k as f32 * d.alto);
        // Anotación bajo el puntero: clic la edita, arrastrar la mueve.
        let anotacion = mapa.anotaciones.iter().find(|a| a.0.contains(local)).map(|a| a.1);
        if resp.drag_started() && let Some(a) = anotacion {
            self.doc_editar();
            self.partitura_app.arrastre = Some(a);
        }
        if !resp.dragged() {
            self.partitura_app.arrastre = None;
        }
        if anotacion.is_some() || self.partitura_app.arrastre.is_some() {
            ui.ctx().set_cursor_icon(if self.partitura_app.arrastre.is_some() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
        }
        if let Some(a) = anotacion
            && resp.clicked()
        {
            self.partitura_app.editando = Some((a, q));
            return;
        }
        // Una cabeza bajo el puntero: clic la elige.
        let cabeza = mapa.cabezas.iter().find(|c| (c.0 - local).length() < sp * 0.9).copied();
        // Pentagrama más cercano en vertical.
        let Some((pi, &(base, clave))) = mapa.partes.iter().enumerate().min_by(|a, b| (a.1.0 - sp * 2.0 - local.y).abs().total_cmp(&(b.1.0 - sp * 2.0 - local.y).abs())) else { return };
        // Clic en la cabecera del pentagrama: la clave o la armadura abren su menú para cambiarlas.
        let inicio_musica = mapa.compases.first().map_or(f32::MAX, |c| c.1);
        if local.x > sp * 13.0 && local.x < inicio_musica - sp && (base - sp * 5.0..base + sp).contains(&local.y) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                self.partitura_app.pentagrama = pi;
                let que = if local.x < sp * 17.0 { pie::Accion::PtMenuClave } else { pie::Accion::PtMenuArmadura };
                self.abrir_pie_partitura_sub(q, que);
            }
            return;
        }
        let Some((m, x0, x1, puntos)) = mapa.compases.iter().find(|c| local.x >= c.1 - sp && local.x <= c.2 + sp).cloned() else { return };
        // Punto exacto para anotar: tiempo (a la semicorchea) y altura con el texto centrado en el puntero.
        let t_anotacion = m as f64 * compas + (x_a_tiempo(&puntos, x0, x1, compas, local.x) * 4.0).round() / 4.0;
        let alto_anotacion = (base - sp * 4.0 - local.y) / sp - 0.5;
        if let Some(a) = self.partitura_app.arrastre {
            if let Some(an) = self.partitura_app.doc.anotaciones.get_mut(a) {
                (an.pentagrama, an.inicio, an.alto) = (pi, t_anotacion, alto_anotacion);
            }
            return;
        }
        if resp.secondary_clicked() {
            self.partitura_app.punto = Some((pi, t_anotacion, alto_anotacion, anotacion, q));
        }
        if self.partitura_app.anotar && anotacion.is_none() && self.partitura_app.grabacion.is_none() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            if resp.clicked() {
                self.doc_anotar(pi, t_anotacion, alto_anotacion, q);
            }
            if !resp.secondary_clicked() {
                return;
            }
        }
        let dur = self.dur_paleta();
        let reja = if dur >= 1.0 { 1.0 } else { dur.max(0.25) };
        let rel = ((x_a_tiempo(&puntos, x0, x1, compas, local.x) / reja).round() * reja).clamp(0.0, compas - reja.min(compas));
        let pos = ((base - local.y) / (sp / 2.0)).round() as i32;
        let quintas = doc.armadura;
        let transp = doc.pentagramas.get(pi).map_or(0, |p| p.transposicion);
        let escrita = altura_en(pos, clave, quintas);
        let sonido = (escrita as i32 - transp).clamp(0, 127) as u8;
        // Nota fantasma donde se escribiría.
        if cabeza.is_none() && anotacion.is_none() && !self.partitura_app.anotar && self.partitura_app.grabacion.is_none() {
            let x = tiempo_a_x(&puntos, x0, x1, compas, rel);
            let y = base - pos as f32 * sp / 2.0;
            p.circle_filled(pos2(r.left() + x + sp * 0.6, r.top() + k as f32 * d.alto + y), sp * 0.55, ACCENT().gamma_multiply(0.5));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if resp.clicked() && anotacion.is_none() && self.partitura_app.grabacion.is_none() {
            self.partitura_app.pentagrama = pi;
            match cabeza {
                Some((_, cpi, id)) if id != usize::MAX => {
                    self.partitura_app.sel = Some((cpi, id));
                    if let Some(n) = self.partitura_app.doc.pentagramas.get(cpi).and_then(|p| p.notas.get(id)) {
                        self.partitura_app.cursor = n.inicio;
                        self.engine.send_midi([0x90, n.altura, 90]);
                        self.engine.send_midi([0x80, n.altura, 0]);
                    }
                }
                _ => {
                    self.doc_editar();
                    self.doc_escribir(pi, m as f64 * compas + rel, dur, Some(sonido));
                    self.engine.send_midi([0x90, sonido, 90]);
                    self.engine.send_midi([0x80, sonido, 0]);
                }
            }
        }
        // Clic derecho: menú circular (sobre una nota, primero la elige).
        if resp.secondary_clicked() {
            if let Some((_, cpi, id)) = cabeza.filter(|c| c.2 != usize::MAX) {
                self.partitura_app.sel = Some((cpi, id));
            }
            self.partitura_app.pentagrama = pi;
            self.abrir_pie_partitura(q);
        }
    }

    fn doc_silencio(&mut self) {
        let (pi, c, d) = (self.partitura_app.pentagrama, self.partitura_app.cursor, self.dur_paleta());
        self.doc_editar();
        self.doc_escribir(pi, c, d, None);
    }

    fn doc_borrar_sel(&mut self) {
        let Some((pi, k)) = self.partitura_app.sel.take() else { return };
        if self.partitura_app.doc.pentagramas.get(pi).is_some_and(|p| k < p.notas.len()) {
            self.doc_editar();
            self.partitura_app.doc.pentagramas[pi].notas.remove(k);
        }
    }

    /// Mueve la nota elegida `delta` semitonos.
    fn doc_mover_sel(&mut self, delta: i32) {
        let Some((pi, k)) = self.partitura_app.sel else { return };
        if self.partitura_app.doc.pentagramas.get(pi).is_none_or(|p| k >= p.notas.len()) {
            return;
        }
        self.doc_editar();
        let n = &mut self.partitura_app.doc.pentagramas[pi].notas[k];
        n.altura = (n.altura as i32 + delta).clamp(0, 127) as u8;
        let a = n.altura;
        self.engine.send_midi([0x90, a, 90]);
        self.engine.send_midi([0x80, a, 0]);
    }

    fn doc_deshacer(&mut self) {
        let e = &mut self.partitura_app;
        if let Some(d) = e.deshacer.pop() {
            e.rehacer.push(std::mem::replace(&mut e.doc, d));
            e.sel = None;
        }
    }

    fn doc_rehacer(&mut self) {
        let e = &mut self.partitura_app;
        if let Some(d) = e.rehacer.pop() {
            e.deshacer.push(std::mem::replace(&mut e.doc, d));
            e.sel = None;
        }
    }

    /// Teclado (con el ratón sobre el editor): letras A–G escriben en el cursor, 3–7 figuras,
    /// «.» puntillo, 0 silencio, ↑↓ semitono (Ctrl: octava), ←→ mueven la selección, Supr borra,
    /// Ctrl+Z / Ctrl+Y deshacer y rehacer.
    fn doc_teclado(&mut self, ui: &egui::Ui) {
        if !self.partitura_app.foco {
            return;
        }
        let eventos = ui.input(|i| i.events.clone());
        for e in eventos {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = e else { continue };
            match key {
                Key::Z if modifiers.command => self.doc_deshacer(),
                Key::Y if modifiers.command => self.doc_rehacer(),
                Key::A | Key::B | Key::C | Key::D | Key::E | Key::F | Key::G if !modifiers.command => {
                    let letra = match key {
                        Key::C => 0,
                        Key::D => 1,
                        Key::E => 2,
                        Key::F => 3,
                        Key::G => 4,
                        Key::A => 5,
                        _ => 6,
                    };
                    let pi = self.partitura_app.pentagrama.min(self.partitura_app.doc.pentagramas.len() - 1);
                    let doc = &self.partitura_app.doc;
                    // La octava más cercana a la nota anterior (o al centro del pentagrama).
                    let anterior = doc.pentagramas[pi].notas.iter().rfind(|n| n.inicio < self.partitura_app.cursor).map_or(64, |n| n.altura as i32);
                    let base = NATURAL[letra as usize] + alteracion_armadura(letra, doc.armadura);
                    let altura = (-1..10).map(|o| (o + 1) * 12 + base).min_by_key(|a| (a - anterior).abs()).unwrap_or(60).clamp(0, 127) as u8;
                    let (c, d) = (self.partitura_app.cursor, self.dur_paleta());
                    self.doc_editar();
                    self.doc_escribir(pi, c, d, Some(altura));
                    self.engine.send_midi([0x90, altura, 90]);
                    self.engine.send_midi([0x80, altura, 0]);
                }
                Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 => {
                    if let Some(f) = FIGURAS.iter().find(|f| f.3 == key) {
                        self.partitura_app.figura = f.0;
                    }
                }
                Key::T if !modifiers.command => self.partitura_app.anotar = !self.partitura_app.anotar,
                Key::Period => self.partitura_app.puntillo = !self.partitura_app.puntillo,
                Key::Num0 => self.doc_silencio(),
                Key::ArrowUp => self.doc_mover_sel(if modifiers.command { 12 } else { 1 }),
                Key::ArrowDown => self.doc_mover_sel(if modifiers.command { -12 } else { -1 }),
                Key::ArrowLeft | Key::ArrowRight => {
                    let e = &mut self.partitura_app;
                    let derecha = key == Key::ArrowRight;
                    if let Some((pi, k)) = e.sel {
                        let n = e.doc.pentagramas[pi].notas.len();
                        let nuevo = if derecha { (k + 1).min(n.saturating_sub(1)) } else { k.saturating_sub(1) };
                        e.sel = Some((pi, nuevo));
                        e.cursor = e.doc.pentagramas[pi].notas.get(nuevo).map_or(e.cursor, |x| x.inicio);
                    } else {
                        let d = e.figura;
                        e.cursor = if derecha { e.cursor + d } else { (e.cursor - d).max(0.0) };
                    }
                }
                Key::Delete | Key::Backspace => self.doc_borrar_sel(),
                _ => {}
            }
        }
    }
}

/// Divide una duración en figuras escribibles.
fn pt_figuras(mut d: f64) -> Vec<f64> {
    let mut v = vec![];
    for f in [4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.25] {
        while d + 1e-6 >= f {
            v.push(f);
            d -= f;
        }
    }
    v
}

/// Tiempo dentro del compás de una x (inversa de `tiempo_a_x`).
fn x_a_tiempo(puntos: &[(f64, f32)], x0: f32, x1: f32, compas: f64, x: f32) -> f64 {
    let mut v: Vec<(f64, f32)> = vec![(0.0, x0)];
    v.extend(puntos.iter().copied().filter(|p| p.0 > 1e-9));
    v.push((compas, x1));
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    for w in v.windows(2) {
        if x <= w[1].1 {
            let f = ((x - w[0].1) / (w[1].1 - w[0].1).max(1e-3)).clamp(0.0, 1.0) as f64;
            return w[0].0 + (w[1].0 - w[0].0) * f;
        }
    }
    compas
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn alturas_por_clave_y_armadura() {
        // Clave de sol: primera línea Mi4 (64); en Sol mayor el Fa es sostenido.
        assert_eq!(altura_en(0, Clave::Sol, 0), 64);
        assert_eq!(altura_en(-2, Clave::Sol, 0), 60);
        assert_eq!(altura_en(8, Clave::Sol, 1), 78);
        // Clave de fa: primera línea Sol2 (43); clave de do en 3.ª: tercera línea Do4.
        assert_eq!(altura_en(0, Clave::Fa, 0), 43);
        assert_eq!(altura_en(4, Clave::Alto, 0), 60);
        // Fa mayor: el Si es bemol.
        assert_eq!(altura_en(4, Clave::Sol, -1), 70);
    }

    #[test]
    fn piano_reparte_las_manos() {
        let mut doc = Partitura::default();
        assert_eq!((doc.grupo(0), doc.grupo(1)), ((0, 2), (0, 2)));
        let nota = |altura| NotaP { inicio: 0.0, dur: 1.0, altura, alteracion: None };
        App::repartir(&mut doc, 1, [nota(60), nota(64), nota(48), nota(55)].into_iter());
        assert_eq!(doc.pentagramas[0].notas.iter().map(|n| n.altura).collect::<Vec<_>>(), [60, 64]);
        assert_eq!(doc.pentagramas[1].notas.iter().map(|n| n.altura).collect::<Vec<_>>(), [48, 55]);
        assert_eq!((doc.pentagramas[0].clave, doc.pentagramas[1].clave), (Clave::Sol, Clave::Fa));
    }

    #[test]
    fn mapa_tiempo_x() {
        let puntos = [(0.0, 10.0), (1.0, 30.0), (2.0, 50.0)];
        assert!((tiempo_a_x(&puntos, 10.0, 90.0, 4.0, 1.0) - 30.0).abs() < 1e-3);
        assert!((x_a_tiempo(&puntos, 10.0, 90.0, 4.0, 40.0) - 1.5).abs() < 1e-6);
    }
}
