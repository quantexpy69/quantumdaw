//! Archivos MIDI estándar (.mid, formatos 0 y 1): lectura de notas y tempo, y escritura de pistas.
//! Los tiempos se manejan en segundos para no depender del tempo del proyecto.

/// Nota leída o por escribir: (inicio en segundos, duración en segundos, nota, velocidad).
pub type NotaSeg = (f64, f64, u8, u8);
/// Nota en ticks: (inicio, duración, nota, velocidad).
type NotaTick = (u64, u64, u8, u8);

/// Pista de un archivo MIDI: nombre y notas.
pub struct PistaMidi {
    pub nombre: String,
    pub notas: Vec<NotaSeg>,
}

fn vlq(d: &[u8], i: &mut usize) -> anyhow::Result<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        let b = *d.get(*i).ok_or_else(|| anyhow::anyhow!("archivo MIDI cortado"))?;
        *i += 1;
        v = (v << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    anyhow::bail!("número de longitud variable inválido")
}

fn escribir_vlq(out: &mut Vec<u8>, mut v: u32) {
    let mut bytes = vec![(v & 0x7F) as u8];
    v >>= 7;
    while v > 0 {
        bytes.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
    out.extend(bytes.iter().rev());
}

/// Lee un archivo MIDI: pistas con notas (en segundos, siguiendo los cambios de tempo) y el primer tempo.
pub fn leer(d: &[u8]) -> anyhow::Result<(Vec<PistaMidi>, f64)> {
    anyhow::ensure!(d.len() >= 14 && &d[0..4] == b"MThd", "no es un archivo MIDI");
    let division = u16::from_be_bytes([d[12], d[13]]);
    anyhow::ensure!(division & 0x8000 == 0, "los archivos MIDI con tiempo SMPTE no están soportados");
    let ppq = division.max(1) as f64;
    let mut i = 8 + u32::from_be_bytes([d[4], d[5], d[6], d[7]]) as usize;
    // Primero se leen todos los eventos en ticks; el tempo puede estar en cualquier pista (normalmente la 0).
    let mut tempos: Vec<(u64, f64)> = vec![]; // (tick, microsegundos por negra)
    let mut crudas: Vec<(String, Vec<NotaTick>)> = vec![];
    while i + 8 <= d.len() {
        let largo = u32::from_be_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
        let (ini, fin) = (i + 8, (i + 8 + largo).min(d.len()));
        if &d[i..i + 4] != b"MTrk" {
            i = fin;
            continue;
        }
        let (mut j, mut tick, mut estado) = (ini, 0u64, 0u8);
        let mut nombre = String::new();
        let mut abiertas: Vec<(u8, u8, u64, u8)> = vec![]; // (canal, nota, inicio, velocidad)
        let mut notas = vec![];
        while j < fin {
            tick += vlq(d, &mut j)? as u64;
            let mut b = *d.get(j).ok_or_else(|| anyhow::anyhow!("archivo MIDI cortado"))?;
            if b & 0x80 != 0 {
                j += 1;
                if b < 0xF0 {
                    estado = b;
                }
            } else {
                b = estado; // estado continuo (running status)
            }
            match b {
                0xFF => {
                    let tipo = d.get(j).copied().unwrap_or(0);
                    j += 1;
                    let n = vlq(d, &mut j)? as usize;
                    let datos = d.get(j..(j + n).min(fin)).unwrap_or(&[]);
                    match tipo {
                        0x51 if n == 3 => tempos.push((tick, ((datos[0] as u32) << 16 | (datos[1] as u32) << 8 | datos[2] as u32) as f64)),
                        0x03 => nombre = String::from_utf8_lossy(datos).trim().to_string(),
                        _ => {}
                    }
                    j += n;
                }
                0xF0 | 0xF7 => {
                    let n = vlq(d, &mut j)? as usize;
                    j += n;
                }
                _ => {
                    let datos = if matches!(b & 0xF0, 0xC0 | 0xD0) { 1 } else { 2 };
                    let (k, v) = (d.get(j).copied().unwrap_or(0), if datos == 2 { d.get(j + 1).copied().unwrap_or(0) } else { 0 });
                    j += datos;
                    let canal = b & 0x0F;
                    match b & 0xF0 {
                        0x90 if v > 0 => abiertas.push((canal, k, tick, v)),
                        0x80 | 0x90 => {
                            if let Some(p) = abiertas.iter().position(|a| a.0 == canal && a.1 == k) {
                                let (_, key, ini, vel) = abiertas.remove(p);
                                notas.push((ini, tick.saturating_sub(ini).max(1), key, vel));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if !notas.is_empty() {
            crudas.push((nombre, notas));
        }
        i = fin;
    }
    tempos.sort_by_key(|t| t.0);
    if tempos.first().is_none_or(|t| t.0 > 0) {
        tempos.insert(0, (0, 500_000.0));
    }
    // Ticks → segundos siguiendo el mapa de tempo.
    let seg = |t: u64| {
        let mut s = 0.0;
        for (k, &(desde, us)) in tempos.iter().enumerate() {
            let hasta = tempos.get(k + 1).map_or(u64::MAX, |x| x.0);
            if t <= desde {
                break;
            }
            s += (t.min(hasta) - desde) as f64 / ppq * us / 1e6;
        }
        s
    };
    let pistas = crudas
        .into_iter()
        .enumerate()
        .map(|(k, (nombre, notas))| PistaMidi {
            nombre: if nombre.is_empty() { format!("MIDI {}", k + 1) } else { nombre },
            notas: notas.into_iter().map(|(t, l, key, vel)| (seg(t), seg(t + l) - seg(t), key, vel)).collect(),
        })
        .collect();
    Ok((pistas, 60e6 / tempos[0].1))
}

/// Escribe un archivo MIDI de formato 1 (480 ticks por negra) con un tempo fijo.
pub fn escribir(pistas: &[PistaMidi], bpm: f64) -> Vec<u8> {
    const PPQ: u16 = 480;
    let ticks = |s: f64| (s * bpm / 60.0 * PPQ as f64).round().max(0.0) as u64;
    let mut out = b"MThd".to_vec();
    out.extend(6u32.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend((pistas.len() as u16 + 1).to_be_bytes());
    out.extend(PPQ.to_be_bytes());
    let pista = |out: &mut Vec<u8>, cuerpo: Vec<u8>| {
        out.extend(b"MTrk");
        out.extend((cuerpo.len() as u32).to_be_bytes());
        out.extend(cuerpo);
    };
    // Pista 0: tempo.
    let us = (60e6 / bpm).round() as u32;
    let mut t0 = vec![0, 0xFF, 0x51, 3, (us >> 16) as u8, (us >> 8) as u8, us as u8];
    t0.extend([0, 0xFF, 0x2F, 0]);
    pista(&mut out, t0);
    for (k, p) in pistas.iter().enumerate() {
        let canal = (k % 16) as u8;
        let mut ev: Vec<(u64, u8, u8, u8)> = vec![]; // (tick, estado, nota, velocidad); las notas off van antes
        for &(ini, len, key, vel) in &p.notas {
            ev.push((ticks(ini), 0x90 | canal, key, vel.max(1)));
            ev.push((ticks(ini + len).max(ticks(ini) + 1), 0x80 | canal, key, 0));
        }
        ev.sort_by_key(|e| (e.0, e.1 & 0xF0 == 0x90));
        let mut c = vec![0, 0xFF, 0x03];
        escribir_vlq(&mut c, p.nombre.len() as u32);
        c.extend(p.nombre.as_bytes());
        let mut antes = 0;
        for (t, est, key, vel) in ev {
            escribir_vlq(&mut c, (t - antes) as u32);
            c.extend([est, key, vel]);
            antes = t;
        }
        c.extend([0, 0xFF, 0x2F, 0]);
        pista(&mut out, c);
    }
    out
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn ida_y_vuelta() {
        let p = PistaMidi { nombre: "Piano".into(), notas: vec![(0.0, 0.5, 60, 100), (0.5, 0.25, 64, 90), (1.0, 2.0, 67, 80)] };
        let bytes = escribir(&[p], 120.0);
        let (pistas, bpm) = leer(&bytes).unwrap();
        assert_eq!((pistas.len(), bpm.round()), (1, 120.0));
        assert_eq!(pistas[0].nombre, "Piano");
        let n = &pistas[0].notas;
        assert_eq!(n.iter().map(|x| (x.2, x.3)).collect::<Vec<_>>(), vec![(60, 100), (64, 90), (67, 80)]);
        assert!((n[2].0 - 1.0).abs() < 1e-3 && (n[2].1 - 2.0).abs() < 1e-3);
        assert!(leer(b"no es midi").is_err());
    }
}
