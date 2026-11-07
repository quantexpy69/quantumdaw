//! Abrir sesiones de otros programas: DAWproject (formato abierto que exportan Cubase, Studio One y
//! Bitwig), Studio One (.song) y, de Cubase (.cpr), Pro Tools (.ptx) y Logic (.logicx), el audio de la
//! sesión (sus formatos son cerrados: se trae cada archivo a su pista, al comienzo). Solo se lee: la
//! sesión original nunca se modifica.
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// Región a colocar: archivo de audio, inicio en la canción, desde dónde del archivo y cuánto dura (s).
#[derive(Debug, Clone)]
pub struct RegionSes {
    pub archivo: PathBuf,
    pub inicio: f64,
    pub desde: f64,
    pub largo: Option<f64>,
}

#[derive(Debug, Default)]
pub struct PistaSes {
    pub nombre: String,
    pub regiones: Vec<RegionSes>,
}

#[derive(Debug, Default)]
pub struct Sesion {
    pub nombre: String,
    pub programa: &'static str,
    pub bpm: Option<f32>,
    pub compas: Option<(u32, u32)>,
    pub pistas: Vec<PistaSes>,
    /// Lo que no se pudo traer tal cual (para avisar al usuario).
    pub avisos: Vec<String>,
    /// Carpeta temporal con el audio sacado de un archivo comprimido (se borra al terminar).
    pub temporal: Option<PathBuf>,
}

const AUDIO: [&str; 9] = ["wav", "wave", "aif", "aiff", "flac", "mp3", "ogg", "m4a", "caf"];
/// Carpetas de audio que no son material de las pistas (mezclas, congelados, fundidos).
const IGNORAR: [&str; 8] = ["bounced files", "bounces", "mixdown", "freeze", "fade files", "rendered", "backups", "previews"];

pub fn es_sesion(p: &Path) -> bool {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "dawproject" | "song" | "cpr" | "npr" | "ptx" | "ptf" | "pts" | "logicx" | "logic")
}

/// Lee la sesión `p` (archivo o carpeta-paquete de Logic).
pub fn leer(p: &Path) -> anyhow::Result<Sesion> {
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let nombre = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let mut s = match ext.as_str() {
        "dawproject" => dawproject(p)?,
        "song" => studio_one(p)?,
        "cpr" | "npr" => solo_audio(p, "Cubase")?,
        "ptx" | "ptf" | "pts" => solo_audio(p, "Pro Tools")?,
        "logicx" | "logic" => solo_audio(p, "Logic Pro")?,
        _ => anyhow::bail!("formato de sesión no reconocido: .{ext}"),
    };
    s.nombre = nombre;
    s.pistas.retain(|t| !t.regiones.is_empty());
    anyhow::ensure!(!s.pistas.is_empty(), "la sesión no tiene audio que se pueda traer");
    Ok(s)
}

// ---------------------------------------------------------------- ZIP (solo lectura)

/// Entradas de un ZIP: nombre → (método, tamaño comprimido, inicio de los datos).
fn zip_indice(datos: &[u8]) -> anyhow::Result<HashMap<String, (u16, usize, usize)>> {
    let le16 = |o: usize| u16::from_le_bytes([datos[o], datos[o + 1]]) as usize;
    let le32 = |o: usize| u32::from_le_bytes([datos[o], datos[o + 1], datos[o + 2], datos[o + 3]]) as usize;
    let fin = (0..datos.len().saturating_sub(21)).rev().take(70_000).find(|&o| datos[o..o + 4] == [0x50, 0x4B, 0x05, 0x06]).ok_or_else(|| anyhow::anyhow!("no es un archivo ZIP válido"))?;
    let (n, mut o) = (le16(fin + 10), le32(fin + 16));
    let mut out = HashMap::new();
    for _ in 0..n {
        anyhow::ensure!(o + 46 <= datos.len() && datos[o..o + 4] == [0x50, 0x4B, 0x01, 0x02], "índice del ZIP dañado");
        let (metodo, comp, ln, le, lc, local) = (le16(o + 10) as u16, le32(o + 20), le16(o + 28), le16(o + 30), le16(o + 32), le32(o + 42));
        let nombre = String::from_utf8_lossy(&datos[o + 46..o + 46 + ln]).replace('\\', "/");
        anyhow::ensure!(local + 30 <= datos.len(), "entrada del ZIP fuera del archivo");
        let ini = local + 30 + le16(local + 26) + le16(local + 28);
        out.insert(nombre, (metodo, comp, ini));
        o += 46 + ln + le + lc;
    }
    Ok(out)
}

fn zip_leer(datos: &[u8], e: (u16, usize, usize)) -> anyhow::Result<Vec<u8>> {
    let (metodo, comp, ini) = e;
    let crudo = datos.get(ini..ini + comp).ok_or_else(|| anyhow::anyhow!("entrada del ZIP incompleta"))?;
    match metodo {
        0 => Ok(crudo.to_vec()),
        8 => miniz_oxide::inflate::decompress_to_vec(crudo).map_err(|e| anyhow::anyhow!("no se pudo descomprimir: {e:?}")),
        m => anyhow::bail!("compresión ZIP no soportada ({m})"),
    }
}

/// Carpeta temporal para el audio sacado del ZIP.
fn temporal(nombre: &str) -> anyhow::Result<PathBuf> {
    let d = std::env::temp_dir().join(format!("quantum-sesion-{}-{}", std::process::id(), nombre.replace(['/', '\\'], "_")));
    fs::create_dir_all(&d)?;
    Ok(d)
}

// ---------------------------------------------------------------- DAWproject

fn num(n: roxmltree::Node, a: &str) -> Option<f64> {
    n.attribute(a).and_then(|v| v.trim().parse().ok())
}

fn dawproject(p: &Path) -> anyhow::Result<Sesion> {
    let datos = fs::read(p)?;
    let indice = zip_indice(&datos)?;
    let xml = String::from_utf8(zip_leer(&datos, *indice.get("project.xml").ok_or_else(|| anyhow::anyhow!("falta project.xml"))?)?)?;
    let doc = roxmltree::Document::parse(&xml)?;
    let raiz = doc.root_element();
    let mut s = Sesion { programa: "DAWproject", ..Default::default() };
    let app = raiz.descendants().find(|n| n.has_tag_name("Application")).and_then(|n| n.attribute("name")).unwrap_or("DAWproject").to_string();
    s.bpm = raiz.descendants().find(|n| n.has_tag_name("Tempo")).and_then(|n| num(n, "value")).map(|v| v as f32);
    s.compas = raiz.descendants().find(|n| n.has_tag_name("TimeSignature")).and_then(|n| Some((num(n, "numerator")? as u32, num(n, "denominator")? as u32)));
    let bpm = s.bpm.unwrap_or(120.0) as f64;
    // Pistas por id (las carpetas pueden anidarlas).
    let mut pistas: Vec<(String, PistaSes)> = raiz.descendants().filter(|n| n.has_tag_name("Track") && n.attribute("contentType").is_none_or(|c| c.contains("audio"))).map(|n| (n.attribute("id").unwrap_or_default().to_string(), PistaSes { nombre: n.attribute("name").unwrap_or("Audio").to_string(), regiones: vec![] })).collect();
    let tmp = temporal(&p.file_stem().unwrap_or_default().to_string_lossy())?;
    s.temporal = Some(tmp.clone());
    let mut extraidos: HashMap<String, PathBuf> = HashMap::new();
    let mut midi = 0;
    for lanes in raiz.descendants().filter(|n| n.has_tag_name("Lanes") && n.attribute("track").is_some()) {
        let id = lanes.attribute("track").unwrap_or_default();
        let Some(pista) = pistas.iter_mut().find(|t| t.0 == id) else { continue };
        // Unidad de tiempo: la del carril más cercano que la declare (beats por defecto).
        let unidad = lanes.ancestors().find_map(|a| a.attribute("timeUnit")).unwrap_or("beats");
        let a_seg = |v: f64| if unidad == "seconds" { v } else { v * 60.0 / bpm };
        for clip in lanes.descendants().filter(|n| n.has_tag_name("Clip")) {
            let Some(archivo) = clip.descendants().find(|n| n.has_tag_name("File")) else {
                midi += usize::from(clip.descendants().any(|n| n.has_tag_name("Notes")));
                continue;
            };
            let ruta = archivo.attribute("path").unwrap_or_default().replace('\\', "/");
            let externo = archivo.attribute("external") == Some("true");
            let destino = if externo {
                PathBuf::from(&ruta)
            } else if let Some(d) = extraidos.get(&ruta) {
                d.clone()
            } else {
                let Some(e) = indice.get(&ruta) else {
                    s.avisos.push(format!("falta el audio {ruta}"));
                    continue;
                };
                let d = tmp.join(Path::new(&ruta).file_name().unwrap_or_default());
                fs::write(&d, zip_leer(&datos, *e)?)?;
                extraidos.insert(ruta.clone(), d.clone());
                d
            };
            let inicio = a_seg(num(clip, "time").unwrap_or(0.0));
            let largo = num(clip, "duration").map(a_seg);
            // playStart está en la unidad del contenido: segundos si el clip es audio sin warps.
            let desde = num(clip, "playStart").unwrap_or(0.0);
            let contenido_seg = clip.descendants().find(|n| n.has_tag_name("Warps")).and_then(|w| w.attribute("contentTimeUnit")) == Some("seconds") || clip.descendants().all(|n| !n.has_tag_name("Warps"));
            let desde = if contenido_seg && unidad == "seconds" { desde } else { a_seg(desde) };
            pista.1.regiones.push(RegionSes { archivo: destino, inicio, desde, largo });
        }
    }
    if midi > 0 {
        s.avisos.push(format!("{midi} regiones MIDI no se trajeron"));
    }
    s.programa = if app.to_lowercase().contains("cubase") { "Cubase (DAWproject)" } else if app.to_lowercase().contains("studio one") { "Studio One (DAWproject)" } else { "DAWproject" };
    s.pistas = pistas.into_iter().map(|t| t.1).collect();
    Ok(s)
}

// ---------------------------------------------------------------- Studio One

/// Ruta de un `url` de Studio One (file:///C:/… con %20) a algo que exista: la ruta tal cual o, si la
/// sesión se movió de equipo, el mismo nombre dentro de la carpeta de la canción.
fn resolver(url: &str, base: &Path) -> Option<PathBuf> {
    let mut s = url.trim_start_matches("file://").to_string();
    let mut bytes = vec![];
    let mut it = s.bytes();
    while let Some(b) = it.next() {
        if b == b'%' {
            let h: Vec<u8> = it.by_ref().take(2).collect();
            if let Ok(v) = u8::from_str_radix(&String::from_utf8_lossy(&h), 16) {
                bytes.push(v);
                continue;
            }
        }
        bytes.push(b);
    }
    s = String::from_utf8_lossy(&bytes).replace('\\', "/");
    let directo = PathBuf::from(&s);
    if directo.exists() {
        return Some(directo);
    }
    let nombre = s.rsplit('/').next()?.to_string();
    buscar_audio(base, 4).into_iter().find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(&nombre)))
}

fn studio_one(p: &Path) -> anyhow::Result<Sesion> {
    let datos = fs::read(p)?;
    let indice = zip_indice(&datos)?;
    let leer = |n: &str| -> anyhow::Result<String> { Ok(String::from_utf8_lossy(&zip_leer(&datos, *indice.get(n).ok_or_else(|| anyhow::anyhow!("falta {n}"))?)?).into_owned()) };
    let base = p.parent().unwrap_or(Path::new("."));
    let mut s = Sesion { programa: "Studio One", ..Default::default() };
    // Pool de medios: id del clip → archivo.
    let mut pool: HashMap<String, PathBuf> = HashMap::new();
    if let Ok(xml) = leer("Song/mediapool.xml") {
        let doc = roxmltree::Document::parse(&xml)?;
        for clip in doc.descendants().filter(|n| n.tag_name().name().ends_with("Clip")) {
            let Some(url) = clip.descendants().find_map(|n| n.attribute("url")) else { continue };
            let Some(ruta) = resolver(url, base) else {
                s.avisos.push(format!("no se encontró {}", url.rsplit('/').next().unwrap_or(url)));
                continue;
            };
            for id in clip.attributes().filter(|a| a.value().starts_with('{')).map(|a| a.value().to_string()) {
                pool.insert(id, ruta.clone());
            }
        }
    }
    let xml = leer("Song/song.xml")?;
    let doc = roxmltree::Document::parse(&xml)?;
    s.bpm = doc.descendants().find(|n| n.tag_name().name().contains("Tempo")).and_then(|n| num(n, "tempo")).map(|v| v as f32);
    let mut por_pista: Vec<PistaSes> = vec![];
    for ev in doc.descendants().filter(|n| n.tag_name().name() == "AudioEvent") {
        let Some(archivo) = ev.attributes().find_map(|a| pool.get(a.value())).cloned() else { continue };
        let nombre = ev.ancestors().find(|a| a.tag_name().name().ends_with("Track") && a.attribute("name").is_some()).and_then(|a| a.attribute("name")).unwrap_or("Audio").to_string();
        let r = RegionSes { archivo, inicio: num(ev, "start").unwrap_or(0.0), desde: num(ev, "offset").unwrap_or(0.0), largo: num(ev, "length") };
        match por_pista.iter_mut().find(|t| t.nombre == nombre) {
            Some(t) => t.regiones.push(r),
            None => por_pista.push(PistaSes { nombre, regiones: vec![r] }),
        }
    }
    if por_pista.is_empty() {
        // Versión que no se pudo leer: al menos el audio de la carpeta de la canción.
        let mut a = solo_audio(p, "Studio One")?;
        a.avisos.insert(0, "no se pudo leer la disposición de la canción: se trajo su audio".into());
        return Ok(a);
    }
    s.pistas = por_pista;
    Ok(s)
}

// ---------------------------------------------------------------- Solo el audio

fn buscar_audio(dir: &Path, prof: usize) -> Vec<PathBuf> {
    let mut out = vec![];
    let Ok(rd) = fs::read_dir(dir) else { return out };
    let mut entradas: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    entradas.sort();
    for p in entradas {
        let nombre = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
        if p.is_dir() {
            if prof > 0 && !IGNORAR.contains(&nombre.as_str()) && !nombre.starts_with('.') {
                out.extend(buscar_audio(&p, prof - 1));
            }
        } else if p.extension().is_some_and(|e| AUDIO.contains(&e.to_string_lossy().to_lowercase().as_str())) && !nombre.starts_with("._") {
            out.push(p);
        }
    }
    out
}

/// Formatos cerrados: cada archivo de audio de la sesión en su pista, al comienzo de la canción.
fn solo_audio(p: &Path, programa: &'static str) -> anyhow::Result<Sesion> {
    // Logic guarda un paquete (carpeta); los demás, un archivo junto a sus carpetas de audio.
    let base = if p.is_dir() { p.to_path_buf() } else { p.parent().unwrap_or(Path::new(".")).to_path_buf() };
    let archivos = buscar_audio(&base, 4);
    let mut s = Sesion { programa, ..Default::default() };
    s.avisos.push(format!("{programa} guarda la sesión en un formato cerrado: se trajo cada audio en su pista, al comienzo. Para conservar las posiciones, exporta desde {programa} como DAWproject, o los audios desde el comienzo («stems»)."));
    s.pistas = archivos.into_iter().map(|a| PistaSes { nombre: a.file_stem().unwrap_or_default().to_string_lossy().to_string(), regiones: vec![RegionSes { archivo: a, inicio: 0.0, desde: 0.0, largo: None }] }).collect();
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ZIP mínimo sin compresión con las entradas dadas.
    fn zip(entradas: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut out, mut central) = (vec![], vec![]);
        for (nombre, datos) in entradas {
            let off = out.len() as u32;
            let n = nombre.len() as u16;
            out.extend([0x50, 0x4B, 0x03, 0x04, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            out.extend((datos.len() as u32).to_le_bytes());
            out.extend((datos.len() as u32).to_le_bytes());
            out.extend(n.to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(nombre.as_bytes());
            out.extend(*datos);
            central.extend([0x50, 0x4B, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            central.extend((datos.len() as u32).to_le_bytes());
            central.extend((datos.len() as u32).to_le_bytes());
            central.extend(n.to_le_bytes());
            central.extend([0u8; 12]);
            central.extend(off.to_le_bytes());
            central.extend(nombre.as_bytes());
        }
        let ini = out.len() as u32;
        let tam = central.len() as u32;
        out.extend(&central);
        out.extend([0x50, 0x4B, 0x05, 0x06, 0, 0, 0, 0]);
        out.extend((entradas.len() as u16).to_le_bytes());
        out.extend((entradas.len() as u16).to_le_bytes());
        out.extend(tam.to_le_bytes());
        out.extend(ini.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    #[test]
    fn lee_un_dawproject() {
        let xml = br#"<?xml version="1.0"?><Project version="1.0"><Application name="Cubase" version="14"/>
            <Transport><Tempo unit="bpm" value="120"/><TimeSignature numerator="3" denominator="4"/></Transport>
            <Structure><Track contentType="audio" id="t1" name="Voz"/><Track contentType="notes" id="t2" name="Piano"/></Structure>
            <Arrangement><Lanes timeUnit="beats"><Lanes track="t1"><Clips>
              <Clip time="4" duration="8" playStart="2"><Audio channels="2" sampleRate="44100"><File path="audio/voz.wav"/></Audio></Clip>
            </Clips></Lanes></Lanes></Arrangement></Project>"#;
        let dir = std::env::temp_dir().join(format!("qd-test-dp-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("Tema.dawproject");
        fs::write(&f, zip(&[("project.xml", xml), ("audio/voz.wav", b"RIFF")])).unwrap();
        let s = leer(&f).unwrap();
        assert_eq!(s.nombre, "Tema");
        assert_eq!(s.programa, "Cubase (DAWproject)");
        assert_eq!(s.compas, Some((3, 4)));
        assert_eq!(s.pistas.len(), 1);
        let r = &s.pistas[0].regiones[0];
        assert_eq!(s.pistas[0].nombre, "Voz");
        assert!((r.inicio - 2.0).abs() < 1e-9 && (r.largo.unwrap() - 4.0).abs() < 1e-9 && (r.desde - 1.0).abs() < 1e-9);
        assert!(r.archivo.exists());
        let _ = fs::remove_dir_all(dir);
        if let Some(t) = s.temporal {
            let _ = fs::remove_dir_all(t);
        }
    }

    #[test]
    fn lee_un_studio_one() {
        let pool = br#"<MediaPool><Attributes x:id="rootFolder" xmlns:x="x"><AudioClip mediaID="{AB-1}"><Url x:id="path" url="file:///C:/Users/x/Media/Guitarra%201.wav"/></AudioClip></Attributes></MediaPool>"#;
        let song = br#"<Song xmlns:x="x"><Attributes x:id="Root"><MediaTrack name="Guitarra" mediaType="Audio"><List x:id="Events"><AudioEvent name="g" start="1.5" length="3" offset="0.25" clipID="{AB-1}"/></List></MediaTrack></Attributes></Song>"#;
        let dir = std::env::temp_dir().join(format!("qd-test-s1-{}", std::process::id()));
        fs::create_dir_all(dir.join("Media")).unwrap();
        fs::write(dir.join("Media/Guitarra 1.wav"), b"RIFF").unwrap();
        let f = dir.join("Cancion.song");
        fs::write(&f, zip(&[("Song/mediapool.xml", pool), ("Song/song.xml", song)])).unwrap();
        let s = leer(&f).unwrap();
        assert_eq!(s.pistas.len(), 1);
        assert_eq!(s.pistas[0].nombre, "Guitarra");
        let r = &s.pistas[0].regiones[0];
        assert!((r.inicio - 1.5).abs() < 1e-9 && (r.desde - 0.25).abs() < 1e-9);
        assert!(r.archivo.ends_with("Media/Guitarra 1.wav"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn formato_cerrado_trae_el_audio() {
        let dir = std::env::temp_dir().join(format!("qd-test-pt-{}", std::process::id()));
        fs::create_dir_all(dir.join("Audio Files")).unwrap();
        fs::create_dir_all(dir.join("Bounced Files")).unwrap();
        fs::write(dir.join("Audio Files/Bajo_01.wav"), b"RIFF").unwrap();
        fs::write(dir.join("Bounced Files/mezcla.wav"), b"RIFF").unwrap();
        fs::write(dir.join("Tema.ptx"), b"\x03").unwrap();
        let s = leer(&dir.join("Tema.ptx")).unwrap();
        assert_eq!(s.pistas.len(), 1);
        assert_eq!(s.pistas[0].nombre, "Bajo_01");
        assert!(!s.avisos.is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}

impl crate::App {
    /// Abre una sesión de otro programa como canción nueva en la carpeta de proyectos: copia su audio
    /// a «Audio Files» (en WAV), crea sus pistas y coloca cada región en su lugar.
    pub fn abrir_sesion_externa(&mut self, p: &Path) -> anyhow::Result<()> {
        use crate::*;
        anyhow::ensure!(!self.recording(), "{}", tr("Para la grabación antes de abrir otra sesión"));
        let s = leer(p)?;
        let base = projects_dir().join(safe(&s.nombre).replace('_', " ").trim());
        let mut dir = base.clone();
        let mut n = 2;
        while dir.exists() {
            (dir, n) = (PathBuf::from(format!("{} {n}", base.display())), n + 1);
        }
        let ahora = now_text();
        let mut meta = project::SongMeta { title: s.nombre.clone(), ..Default::default() };
        (meta.created_at, meta.saved_at) = (ahora.clone(), ahora);
        let proyecto = Project { sample_rate: self.engine.sample_rate, rec_bits: self.rec_bits, bpm: s.bpm.unwrap_or(120.0), signature: s.compas.unwrap_or((4, 4)), meta, ..Default::default() };
        fs::create_dir_all(&dir)?;
        proyecto.save(&dir)?;
        self.reload(dir, proyecto)?;
        let sr = self.engine.sample_rate as f64;
        let mut cache: HashMap<PathBuf, Arc<engine::AudioBuf>> = HashMap::new();
        let (mut regiones, mut fallas) = (0, vec![]);
        for pista in &s.pistas {
            self.add_track(pista.nombre.clone(), TrackKind::AudioStereo);
            let i = self.s.tracks.len() - 1;
            for r in &pista.regiones {
                let buf = match cache.get(&r.archivo) {
                    Some(b) => b.clone(),
                    None => match self.traer_audio(&r.archivo) {
                        Ok(b) => {
                            cache.insert(r.archivo.clone(), b.clone());
                            b
                        }
                        Err(e) => {
                            fallas.push(format!("{}: {e}", r.archivo.file_name().unwrap_or_default().to_string_lossy()));
                            continue;
                        }
                    },
                };
                let total = buf.frames.len() as u64;
                let desde = ((r.desde.max(0.0) * sr) as u64).min(total.saturating_sub(1));
                let largo = r.largo.map_or(total - desde, |l| ((l.max(0.0) * sr) as u64).min(total - desde));
                let mut c = engine::Clip::audio(buf, (r.inicio.max(0.0) * sr) as u64);
                (c.offset, c.len) = (desde, largo.max(1));
                self.s.tracks[i].clips.push(c);
                regiones += 1;
            }
        }
        if let Some(t) = &s.temporal {
            let _ = fs::remove_dir_all(t);
        }
        self.save();
        let mut avisos = s.avisos.clone();
        avisos.extend(fallas);
        self.status = format!("{} «{}» ({}): {} {}, {regiones} {}", tr("Sesión abierta"), s.nombre, s.programa, s.pistas.len(), tr("pistas"), tr("regiones"));
        if !avisos.is_empty() {
            self.aviso_sesion = Some(avisos.join("\n"));
        }
        Ok(())
    }
}
