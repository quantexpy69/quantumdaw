//! Librerías de Decent Sampler (`.dspreset`): se traducen a regiones SFZ para el sampler propio, así
//! suenan sin el plugin. Se toman los grupos activos (`enabled` distinto de "false") con sus muestras;
//! los atributos se heredan de `<groups>` a `<group>` y a `<sample>` (el volumen y la afinación se
//! suman entre niveles, como en Decent Sampler). Quedan fuera la interfaz, los efectos, las
//! modulaciones y las muestras de soltar tecla (`trigger="release"`).
use std::collections::HashMap;

type Region = HashMap<String, String>;

/// Volumen de Decent Sampler en dB: "-3dB" o amplitud lineal ("0.5"; 0 = silencio).
fn volumen_db(v: &str) -> Option<f32> {
    let v = v.trim();
    match v.to_ascii_lowercase().strip_suffix("db") {
        Some(db) => db.trim().parse().ok(),
        None => v.parse::<f32>().ok().map(|a| if a <= 0.0 { -144.0 } else { 20.0 * a.log10() }),
    }
}

fn activo(n: &roxmltree::Node) -> bool {
    !n.attribute("enabled").is_some_and(|e| e.eq_ignore_ascii_case("false"))
}

/// Regiones SFZ (claves del SFZ) de un `.dspreset`.
pub fn regiones(xml: &str) -> anyhow::Result<Vec<Region>> {
    let doc = roxmltree::Document::parse(xml)?;
    let raiz = doc.root_element();
    anyhow::ensure!(raiz.has_tag_name("DecentSampler"), "no es un preset de Decent Sampler");
    let mut regiones = vec![];
    for grupos in raiz.children().filter(|n| n.has_tag_name("groups")) {
        for grupo in grupos.children().filter(|n| n.has_tag_name("group") && activo(n)) {
            for muestra in grupo.children().filter(|n| n.has_tag_name("sample") && activo(n)) {
                let niveles = [grupos, grupo, muestra];
                // Atributos heredados: el nivel más específico gana.
                let mut a: HashMap<&str, &str> = HashMap::new();
                niveles.iter().for_each(|n| n.attributes().for_each(|x| _ = a.insert(x.name(), x.value())));
                if a.get("trigger").is_some_and(|t| !t.eq_ignore_ascii_case("attack")) {
                    continue;
                }
                let Some(ruta) = a.get("path") else { continue };
                let mut r = Region::new();
                let mut poner = |k: &str, v: String| _ = r.insert(k.to_string(), v);
                poner("sample", ruta.replace('\\', "/"));
                for (ds, sfz) in [("rootNote", "pitch_keycenter"), ("loNote", "lokey"), ("hiNote", "hikey"), ("loVel", "lovel"), ("hiVel", "hivel"), ("pan", "pan"), ("start", "offset"), ("end", "end")] {
                    if let Some(v) = a.get(ds) {
                        poner(sfz, v.to_string());
                    }
                }
                // Volumen y afinación se suman entre niveles.
                let suma = |f: &dyn Fn(&roxmltree::Node) -> Option<f32>| niveles.iter().filter_map(f).sum::<f32>();
                let db = suma(&|n| n.attribute("volume").and_then(volumen_db));
                if db != 0.0 {
                    poner("volume", db.to_string());
                }
                let semitonos = suma(&|n| n.attribute("tuning").and_then(|v| v.parse().ok())) + suma(&|n| n.attribute("globalTuning").and_then(|v| v.parse().ok()));
                if semitonos != 0.0 {
                    poner("tune", ((semitonos * 100.0 * 100.0).round() / 100.0).to_string());
                }
                let num = |k: &str| a.get(k).and_then(|v| v.parse::<f32>().ok());
                if !a.get("ampEnvEnabled").is_some_and(|v| v.eq_ignore_ascii_case("false")) {
                    if let Some(v) = num("attack") {
                        poner("ampeg_attack", v.to_string());
                    }
                    if let Some(v) = num("release") {
                        poner("ampeg_release", v.to_string());
                    }
                    // Sin sostenido, la nota se apaga sola en el tiempo de caída.
                    if let (Some(d), Some(s)) = (num("decay"), num("sustain"))
                        && s < 0.01
                    {
                        poner("ampeg_decay", d.to_string());
                    }
                }
                if let Some(v) = num("ampVelTrack") {
                    poner("amp_veltrack", (v * 100.0).round().to_string());
                }
                if a.get("loopEnabled").is_some_and(|v| v.eq_ignore_ascii_case("true"))
                    && let (Some(ls), Some(le)) = (a.get("loopStart"), a.get("loopEnd"))
                {
                    poner("loop_mode", "loop_continuous".into());
                    poner("loop_start", ls.to_string());
                    poner("loop_end", le.to_string());
                }
                // Variaciones (round robin): la posición elige una franja del azar.
                let largo = num("seqLength").unwrap_or(0.0);
                let modo = a.get("seqMode").map_or("round_robin", |m| m);
                if largo > 1.0
                    && !modo.eq_ignore_ascii_case("always")
                    && let Some(p) = num("seqPosition")
                {
                    poner("lorand", ((p - 1.0) / largo).clamp(0.0, 1.0).to_string());
                    poner("hirand", (p / largo).clamp(0.0, 1.0).to_string());
                }
                regiones.push(r);
            }
        }
    }
    anyhow::ensure!(!regiones.is_empty(), "el preset no tiene muestras activas");
    Ok(regiones)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn traduce_grupos_activos_herencia_y_variaciones() {
        let xml = r#"<?xml version="1.0"?>
<DecentSampler presetFormatVersion="1">
  <ui width="800" height="600"><label text="x"/></ui>
  <groups volume="-6dB" attack="0.5" release="1.2" globalTuning="1">
    <group name="apagado" enabled="false"><sample path="Samples/no.wav" rootNote="60"/></group>
    <group name="vita" seqMode="random" seqLength="2" volume="0.5" ampVelTrack="0.3">
      <sample path="Samples\a 1.wav" rootNote="60" loNote="0" hiNote="64" seqPosition="1" start="100" tuning="-0.5"/>
      <sample path="Samples/a2.wav" rootNote="60" loNote="0" hiNote="64" seqPosition="2" loopEnabled="true" loopStart="10" loopEnd="900"/>
      <sample path="Samples/rel.wav" rootNote="60" trigger="release"/>
    </group>
  </groups>
  <effects><effect type="reverb"/></effects>
</DecentSampler>"#;
        let r = regiones(xml).unwrap();
        assert_eq!(r.len(), 2, "sin el grupo apagado ni la muestra de soltar");
        let a = &r[0];
        assert_eq!(a["sample"], "Samples/a 1.wav");
        assert_eq!((a["pitch_keycenter"].as_str(), a["lokey"].as_str(), a["hikey"].as_str()), ("60", "0", "64"));
        // -6 dB del conjunto + 0,5 lineal (≈ -6 dB) del grupo.
        assert!((a["volume"].parse::<f32>().unwrap() + 12.02).abs() < 0.05, "{}", a["volume"]);
        // +1 semitono global − 0,5 de la muestra = +50 cents.
        assert_eq!(a["tune"], "50");
        assert_eq!((a["ampeg_attack"].as_str(), a["ampeg_release"].as_str(), a["amp_veltrack"].as_str()), ("0.5", "1.2", "30"));
        assert_eq!((a["lorand"].as_str(), a["hirand"].as_str(), a["offset"].as_str()), ("0", "0.5", "100"));
        let b = &r[1];
        assert_eq!((b["lorand"].as_str(), b["hirand"].as_str()), ("0.5", "1"));
        assert_eq!((b["loop_mode"].as_str(), b["loop_start"].as_str(), b["loop_end"].as_str()), ("loop_continuous", "10", "900"));
        assert!(regiones("<DecentSampler><groups/></DecentSampler>").is_err());
        assert!(regiones("<otra/>").is_err());
    }

    #[test]
    fn volumen_en_db_o_lineal() {
        assert_eq!(volumen_db("-3dB"), Some(-3.0));
        assert_eq!(volumen_db("1.0"), Some(0.0));
        assert!((volumen_db("0.5").unwrap() + 6.02).abs() < 0.01);
        assert_eq!(volumen_db("0"), Some(-144.0));
    }
}
