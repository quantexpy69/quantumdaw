//! Instrumentos reales gratuitos y libres (FreePats y Versilian VSCO-2 / VCSL): catálogo,
//! descarga bajo demanda a ~/.local/share/quantum-daw/instruments y generación del mapeo SFZ
//! para las bibliotecas que solo traen muestras sueltas.
use crate::*;
use std::{
    process::Command,
    sync::{Mutex, mpsc},
};

pub enum Src {
    /// Archivo .7z o .tar.xz con un .sfz dentro.
    Archive(&'static str),
    /// Carpetas de muestras de un repositorio de GitHub (el .sfz se genera desde los nombres).
    Samples(&'static str, &'static [&'static str]),
}

pub struct Entry {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub mb: u32,
    pub license: &'static str,
    pub credit: &'static str,
    pub src: Src,
}

const VSCO: &str = "sgossner/VSCO-2-CE";
const VCSL: &str = "sgossner/VCSL";

pub const CATALOG: [Entry; 13] = [
    Entry {
        id: "guitarra-nylon",
        name: "Guitarra española (nylon)",
        family: "Guitarras",
        mb: 5,
        license: "CC0",
        credit: "FreePats · Spanish Classical Guitar",
        src: Src::Archive("https://freepats.zenvoid.org/Guitar/SpanishClassicalGuitar/SpanishClassicalGuitar-SFZ+FLAC-20190618.7z"),
    },
    Entry {
        id: "guitarra-acero",
        name: "Guitarra acústica (acero)",
        family: "Guitarras",
        mb: 3,
        license: "GPL-3.0 con excepción para música",
        credit: "FreePats · FSS Steel String Guitar",
        src: Src::Archive("https://freepats.zenvoid.org/Guitar/FSS-SteelStringGuitar/FSS-SteelStringGuitar-small-SFZ-20200521.tar.xz"),
    },
    Entry {
        id: "guitarra-electrica",
        name: "Guitarra eléctrica limpia",
        family: "Guitarras",
        mb: 3,
        license: "CC0",
        credit: "FreePats · Electric Guitar FSBS",
        src: Src::Archive("https://github.com/freepats/electric-guitar-FSBS-clean/releases/download/2026-08-07/EGuitarFSBS-clean-bridge-small-SFZ+FLAC-20260807.7z"),
    },
    Entry {
        id: "bajo-dedos",
        name: "Bajo eléctrico (dedos)",
        family: "Bajos",
        mb: 3,
        license: "CC0",
        credit: "FreePats · Finger Bass YR",
        src: Src::Archive("https://github.com/freepats/electric-bass-YR/releases/download/2019-09-30/FingerBassYR-SFZ+FLAC-20190930.7z"),
    },
    Entry {
        id: "bajo-pua",
        name: "Bajo eléctrico (púa)",
        family: "Bajos",
        mb: 3,
        license: "CC0",
        credit: "FreePats · Picked Bass YR",
        src: Src::Archive("https://github.com/freepats/electric-bass-YR/releases/download/2019-09-30/PickedBassYR-SFZ+FLAC-20190930.7z"),
    },
    Entry { id: "contrabajo", name: "Contrabajo", family: "Bajos", mb: 25, license: "CC0", credit: "Versilian Studios · VSCO-2 CE", src: Src::Samples(VSCO, &["Strings/Solo Contrabass/SusVib"]) },
    Entry {
        id: "bateria",
        name: "Batería acústica",
        family: "Baterías y percusión",
        mb: 160,
        license: "CC-BY 4.0",
        credit: "Lars Muldjord · MuldjordKit (FreePats)",
        src: Src::Archive("https://github.com/freepats/muldjordkit/releases/download/2020-10-18/MuldjordKit-SFZ+FLAC-20201018.7z"),
    },
    Entry {
        id: "percusion",
        name: "Percusión latina (bongós, congas, cajón…)",
        family: "Baterías y percusión",
        mb: 20,
        license: "CC0",
        credit: "Versilian Studios · VCSL",
        src: Src::Samples(
            VCSL,
            &[
                "Membranophones/Struck Membranophones/Bongos",
                "Membranophones/Struck Membranophones/Conga",
                "Idiophones/Struck Idiophones/Cajon",
                "Idiophones/Struck Idiophones/Claves",
                "Idiophones/Struck Idiophones/Cowbells",
                "Idiophones/Struck Idiophones/Shaker, Small",
                "Idiophones/Struck Idiophones/Tambourine 1",
                "Idiophones/Struck Idiophones/Guiro",
                "Idiophones/Struck Idiophones/Claps",
                "Idiophones/Struck Idiophones/Agogo Bells",
            ],
        ),
    },
    Entry { id: "violin", name: "Violín", family: "Cuerdas", mb: 40, license: "CC0", credit: "Versilian Studios · VSCO-2 CE", src: Src::Samples(VSCO, &["Strings/Solo Violin/Arco Vib"]) },
    Entry { id: "viola", name: "Violas (sección)", family: "Cuerdas", mb: 36, license: "CC0", credit: "Versilian Studios · VSCO-2 CE", src: Src::Samples(VSCO, &["Strings/Viola Section/susvib"]) },
    Entry {
        id: "violonchelo",
        name: "Violonchelos (sección)",
        family: "Cuerdas",
        mb: 36,
        license: "CC0",
        credit: "Versilian Studios · VSCO-2 CE",
        src: Src::Samples(VSCO, &["Strings/Cello Section/susvib"]),
    },
    Entry {
        id: "violines-pizz",
        name: "Violines pizzicato",
        family: "Cuerdas",
        mb: 5,
        license: "CC0",
        credit: "Versilian Studios · VSCO-2 CE",
        src: Src::Samples(VSCO, &["Strings/Violin Section/Pizz"]),
    },
    Entry { id: "piano", name: "Piano vertical", family: "Teclados", mb: 80, license: "CC0", credit: "Versilian Studios · VSCO-2 CE", src: Src::Samples(VSCO, &["Keys/Upright Nr1"]) },
];

pub fn find(id: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn dir(id: &str) -> PathBuf {
    home().join(".local/share/quantum-daw/instruments").join(id)
}

pub fn installed(id: &str) -> bool {
    dir(id).join(".listo").exists()
}

/// Primer .sfz dentro de la carpeta del instrumento.
pub fn sfz_path(id: &str) -> Option<PathBuf> {
    fn walk(p: &Path) -> Option<PathBuf> {
        let mut entries: Vec<PathBuf> = fs::read_dir(p).ok()?.flatten().map(|e| e.path()).collect();
        entries.sort();
        entries.iter().find(|p| p.extension().is_some_and(|e| e == "sfz")).cloned().or_else(|| entries.iter().filter(|p| p.is_dir()).find_map(|d| walk(d)))
    }
    walk(&dir(id))
}

/// Traduce los nombres de las piezas de batería para el piano roll.
pub fn spanish(name: &str) -> String {
    [
        ("Kick drum", "Bombo"),
        ("Snare rest", "Caja (aro)"),
        ("Snare", "Caja"),
        ("Hi-hat closed", "Hi-hat cerrado"),
        ("Hi-hat open", "Hi-hat abierto"),
        ("bell", "campana"),
        (" left", " izq."),
        (" right", " der."),
    ]
    .iter()
    .fold(name.to_string(), |s, (a, b)| s.replace(a, b))
}

fn run(cmd: &mut Command) -> anyhow::Result<()> {
    let out = cmd.output()?;
    anyhow::ensure!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("falló la descarga"));
    Ok(())
}

fn url_path(p: &str) -> String {
    p.replace('%', "%25").replace(' ', "%20").replace('#', "%23").replace(',', "%2C").replace('(', "%28").replace(')', "%29")
}

/// Descarga e instala un instrumento en segundo plano; `status` informa del progreso a la interfaz.
pub fn install(e: &'static Entry, status: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let set = |s: String| _ = status.lock().map(|mut st| *st = s);
        let target = dir(e.id);
        let result = (|| -> anyhow::Result<()> {
            fs::create_dir_all(&target)?;
            match &e.src {
                Src::Archive(url) => {
                    set(format!("Descargando {} MB…", e.mb));
                    let pkg = target.join(if url.ends_with(".7z") { "paquete.7z" } else { "paquete.tar.xz" });
                    run(Command::new("curl").args(["-sfL", "--retry", "2", "-o"]).arg(&pkg).arg(url))?;
                    set("Descomprimiendo…".into());
                    if url.ends_with(".7z") {
                        run(Command::new("7z").args(["x", "-y"]).arg(format!("-o{}", target.display())).arg(&pkg))?;
                    } else {
                        run(Command::new("tar").arg("-xJf").arg(&pkg).arg("-C").arg(&target))?;
                    }
                    fs::remove_file(pkg)?;
                }
                Src::Samples(repo, folders) => {
                    set("Buscando muestras…".into());
                    let tree = Command::new("curl").args(["-sfL", &format!("https://api.github.com/repos/{repo}/git/trees/master?recursive=1")]).output()?;
                    let tree: serde_json::Value = serde_json::from_slice(&tree.stdout)?;
                    let paths: Vec<String> =
                        tree["tree"].as_array().into_iter().flatten().filter_map(|t| t["path"].as_str()).filter(|p| p.to_lowercase().ends_with(".wav")).map(String::from).collect();
                    let mut files = vec![];
                    for folder in folders.iter() {
                        let names: Vec<&String> = paths.iter().filter(|p| p.starts_with(&format!("{folder}/")) && !p[folder.len() + 1..].contains('/')).collect();
                        files.extend(names.into_iter().filter(|p| keep(file_name(p))).map(|p| (folder.to_string(), p.clone())));
                    }
                    anyhow::ensure!(!files.is_empty(), "no se encontraron muestras");
                    fs::create_dir_all(target.join("samples"))?;
                    for (k, (_, path)) in files.iter().enumerate() {
                        set(format!("Descargando muestras {}/{}…", k + 1, files.len()));
                        let dst = target.join("samples").join(file_name(path));
                        if !dst.exists() {
                            run(Command::new("curl").args(["-sfL", "--retry", "2", "-o"]).arg(&dst).arg(format!("https://raw.githubusercontent.com/{repo}/master/{}", url_path(path))))?;
                        }
                    }
                    fs::write(target.join(format!("{}.sfz", e.id)), generate_sfz(&files))?;
                }
            }
            fs::write(target.join(".listo"), e.credit)?;
            Ok(())
        })();
        match result {
            Ok(()) => set("Listo".into()),
            Err(err) => set(format!("Error: {err}")),
        }
    });
}

fn file_name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// Partes del nombre: nota (si la hay), capa de velocidad y número de variación (round robin).
fn tokens(name: &str) -> (Option<u8>, u32, u32, String) {
    let stem = name.trim_end_matches(".wav").trim_end_matches(".WAV");
    let (mut note, mut vel, mut rr, mut art) = (None, 0, 1, vec![]);
    let parts: Vec<&str> = stem.split('_').collect();
    for (i, t) in parts.iter().enumerate() {
        let lower = t.to_lowercase();
        let dyn_rank = ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"].iter().position(|d| *d == lower);
        if let Some(n) = parse_note(t) {
            note = Some(n);
        } else if let Some(r) = dyn_rank {
            vel = r as u32;
        } else if let Some(v) = lower.strip_prefix("vl").or(lower.strip_prefix('v')).and_then(|v| v.parse().ok()) {
            vel = v;
        } else if let Some(r) = lower.strip_prefix("rr").and_then(|v| v.parse().ok()) {
            rr = r;
        } else if i == parts.len() - 1 && lower.parse::<u32>().is_ok() {
            rr = lower.parse().unwrap_or(1);
        } else if !matches!(lower.as_str(), "mid" | "sum" | "close" | "room" | "far") {
            art.push(*t);
        }
    }
    (note, vel, rr, art.join(" "))
}

fn parse_note(t: &str) -> Option<u8> {
    let mut c = t.chars();
    let base: i32 = [9, 11, 0, 2, 4, 5, 7]["ABCDEFG".find(c.next()?)?];
    let rest: String = c.collect();
    let (acc, oct) = match rest.chars().next()? {
        '#' => (1, &rest[1..]),
        'b' if rest.len() > 1 => (-1, &rest[1..]),
        _ => (0, rest.as_str()),
    };
    let n = (oct.parse::<i32>().ok()? + 1) * 12 + base + acc;
    (0..=127).contains(&n).then_some(n as u8)
}

/// Solo la primera variación de cada muestra (menos descarga y memoria).
fn keep(name: &str) -> bool {
    tokens(name).2 == 1
}

/// Mapeo SFZ desde los nombres: con nota → zonas cromáticas entre notas vecinas; sin nota →
/// una tecla por articulación desde Do3 (percusión). Las capas de velocidad se reparten.
fn generate_sfz(files: &[(String, String)]) -> String {
    let mut out = String::from("// Mapeo generado por Quantum DAW\n<global>\n ampeg_release=0.5\n");
    let parsed: Vec<(Option<u8>, u32, String, String)> = files
        .iter()
        .map(|(folder, p)| {
            let (n, v, _, art) = tokens(file_name(p));
            (n, v, format!("{} {art}", file_name(folder)), file_name(p).to_string())
        })
        .collect();
    let layers = |items: &mut Vec<(u32, String)>| {
        items.sort();
        let n = items.len().max(1);
        items.iter().enumerate().map(|(k, (_, f))| (k * 128 / n, (k + 1) * 128 / n - 1, f.clone())).collect::<Vec<_>>()
    };
    if parsed.iter().any(|p| p.0.is_some()) {
        let mut notes: Vec<u8> = parsed.iter().filter_map(|p| p.0).collect();
        notes.sort();
        notes.dedup();
        for (k, &n) in notes.iter().enumerate() {
            let lo = if k == 0 { n.saturating_sub(12) } else { (notes[k - 1] + n) / 2 + 1 };
            let hi = notes.get(k + 1).map_or((n + 12).min(127), |&m| (n + m) / 2);
            let mut items: Vec<(u32, String)> = parsed.iter().filter(|p| p.0 == Some(n)).map(|p| (p.1, p.3.clone())).collect();
            for (lv, hv, f) in layers(&mut items) {
                out += &format!("<region> lokey={lo} hikey={hi} pitch_keycenter={n} lovel={lv} hivel={hv} sample=samples/{f}\n");
            }
        }
    } else {
        let mut arts: Vec<String> = parsed.iter().map(|p| p.2.clone()).collect();
        arts.sort();
        arts.dedup();
        out += " amp_veltrack=60\n";
        for (k, art) in arts.iter().enumerate() {
            let key = (48 + k).min(127);
            out += &format!("// {art}\n<group> key={key}\n");
            let mut items: Vec<(u32, String)> = parsed.iter().filter(|p| &p.2 == art).map(|p| (p.1, p.3.clone())).collect();
            for (lv, hv, f) in layers(&mut items) {
                out += &format!("<region> lovel={lv} hivel={hv} sample=samples/{f}\n");
            }
        }
    }
    out
}

/// Carga el sampler de un instrumento instalado en segundo plano.
pub fn load(id: &str, rate: u32) -> mpsc::Receiver<Result<Arc<engine::Sampler>, String>> {
    let (tx, rx) = mpsc::channel();
    let id = id.to_string();
    std::thread::spawn(move || {
        let r = sfz_path(&id).ok_or_else(|| "instrumento no instalado".to_string()).and_then(|p| engine::Sampler::load(&p, rate).map(Arc::new).map_err(|e| e.to_string()));
        let _ = tx.send(r);
    });
    rx
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_to_regions() {
        assert_eq!(super::parse_note("A#0"), Some(22));
        assert_eq!(super::parse_note("C4"), Some(60));
        assert_eq!(super::tokens("BKCtbss_SusVib_A#0_v1_rr1.wav").0, Some(22));
        assert!(!super::keep("VlnEns_Pizz_A2_v1_rr2.wav"));
        assert!(super::keep("susvib_A2_v3_1.wav"));
        let sfz = super::generate_sfz(&[
            ("Strings/X".into(), "Strings/X/a_C4_v1_1.wav".into()),
            ("Strings/X".into(), "Strings/X/a_C4_v3_1.wav".into()),
            ("Strings/X".into(), "Strings/X/a_E4_v1_1.wav".into()),
        ]);
        assert!(sfz.contains("lokey=48 hikey=62 pitch_keycenter=60 lovel=0 hivel=63"));
        let kit = super::generate_sfz(&[("P/Bongos".into(), "P/Bongos/BongoH_Hit1_v1_rr1_Mid.wav".into())]);
        assert!(kit.contains("// Bongos BongoH Hit1\n<group> key=48"));
    }
}
