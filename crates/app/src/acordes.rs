//! Acordes para el piano roll: reconocer el acorde que forman unas notas, construir acordes con
//! inversiones, círculo de quintas, acordes de una tonalidad y progresiones automáticas.
use crate::*;
use egui::{Align2, FontId, Sense, Shape, Stroke, vec2};

/// Raíces con el mismo cifrado que la pista de acordes.
pub const RAICES: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/// Tipos de acorde: (sufijo del cifrado, nombre, intervalos en semitonos desde la raíz).
pub const TIPOS: [(&str, &str, &[u8]); 14] = [
    ("", "Mayor", &[0, 4, 7]),
    ("m", "Menor", &[0, 3, 7]),
    ("7", "Séptima", &[0, 4, 7, 10]),
    ("maj7", "Séptima mayor", &[0, 4, 7, 11]),
    ("m7", "Menor séptima", &[0, 3, 7, 10]),
    ("m7b5", "Semidisminuido", &[0, 3, 6, 10]),
    ("dim", "Disminuido", &[0, 3, 6]),
    ("dim7", "Disminuido séptima", &[0, 3, 6, 9]),
    ("aug", "Aumentado", &[0, 4, 8]),
    ("sus2", "Sus2", &[0, 2, 7]),
    ("sus4", "Sus4", &[0, 5, 7]),
    ("6", "Sexta", &[0, 4, 7, 9]),
    ("add9", "Add9", &[0, 4, 7, 14]),
    ("9", "Novena", &[0, 4, 7, 10, 14]),
];
const MAYOR: usize = 0;
const MENOR: usize = 1;
const DISM: usize = 6;

pub const INVERSIONES: [&str; 4] = ["Fundamental", "1.ª inversión", "2.ª inversión", "3.ª inversión"];

/// Ritmos para escribir una progresión.
pub const RITMOS: [&str; 3] = ["Bloque", "Negras", "Arpegio"];

/// Progresiones por grados de la escala (índices de `diatonicos`).
pub const PROGRESIONES: [(&str, &[usize]); 13] = [
    ("I – V – vi – IV (pop)", &[0, 4, 5, 3]),
    ("I – IV – V – I", &[0, 3, 4, 0]),
    ("vi – IV – I – V", &[5, 3, 0, 4]),
    ("I – vi – IV – V (años 50)", &[0, 5, 3, 4]),
    ("ii – V – I (jazz)", &[1, 4, 0, 0]),
    ("I – IV – vi – V", &[0, 3, 5, 4]),
    ("I – V – vi – iii – IV – I – IV – V (Canon)", &[0, 4, 5, 2, 3, 0, 3, 4]),
    ("i – VI – III – VII (épica, en menor)", &[0, 5, 2, 6]),
    ("Blues de 12 compases (I – IV – V)", &[0, 0, 0, 0, 3, 3, 0, 0, 4, 3, 0, 4]),
    ("i – VII – VI – VII (rock, en menor)", &[0, 6, 5, 6]),
    ("I – iii – IV – V (balada)", &[0, 2, 3, 4]),
    ("vi – ii – V – I (círculo)", &[5, 1, 4, 0]),
    ("I – IV – I – V (folk)", &[0, 3, 0, 4]),
];

/// Círculo de quintas: tonalidades mayores en el orden del círculo (C, G, D…).
const QUINTAS: [u8; 12] = [0, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10, 5];

pub fn nombre(raiz: u8, tipo: usize) -> String {
    format!("{}{}", RAICES[raiz as usize % 12], TIPOS[tipo].0)
}

/// Nombre de una tonalidad («C», «Am»).
pub fn tonalidad(tono: u8, menor: bool) -> String {
    format!("{}{}", RAICES[tono as usize % 12], if menor { "m" } else { "" })
}

/// Notas MIDI del acorde con la raíz en `base` y la inversión pedida (0 = fundamental).
pub fn notas(base: u8, tipo: usize, inversion: usize) -> Vec<u8> {
    let mut v: Vec<i32> = TIPOS[tipo].2.iter().map(|&i| base as i32 + i as i32).collect();
    for n in v.iter_mut().take(inversion.min(TIPOS[tipo].2.len() - 1)) {
        *n += 12;
    }
    v.sort();
    v.into_iter().filter(|n| (0..=127).contains(n)).map(|n| n as u8).collect()
}

/// Raíz en una octava central (entre Fa#3 y Fa4) para que los acordes queden cómodos.
pub fn base_central(raiz: u8) -> u8 {
    let pc = raiz % 12;
    if pc > 5 { 48 + pc } else { 60 + pc }
}

/// Acorde que forman estas notas («Am7», «C/E», «G5»), si se reconoce.
pub fn detectar(keys: &[u8]) -> Option<String> {
    let bajo = *keys.iter().min()? % 12;
    let mut pcs: Vec<u8> = keys.iter().map(|k| k % 12).collect();
    pcs.sort();
    pcs.dedup();
    if pcs.len() < 2 {
        return None;
    }
    let mut mejor: Option<(u8, usize)> = None;
    for raiz in 0..12u8 {
        for (t, (_, _, iv)) in TIPOS.iter().enumerate() {
            let mut set: Vec<u8> = iv.iter().map(|i| (raiz + i) % 12).collect();
            set.sort();
            set.dedup();
            // Entre lecturas equivalentes (C6 = Am7) gana la que tiene la raíz en el bajo.
            if set == pcs && (mejor.is_none() || raiz == bajo) && !mejor.is_some_and(|m| m.0 == bajo) {
                mejor = Some((raiz, t));
            }
        }
    }
    let Some((raiz, t)) = mejor else {
        // Quinta sola: «power chord».
        let otra = pcs.iter().find(|&&p| p != bajo)?;
        return (pcs.len() == 2 && (otra + 12 - bajo) % 12 == 7).then(|| format!("{}5", RAICES[bajo as usize]));
    };
    let mut s = nombre(raiz, t);
    if bajo != raiz {
        s += &format!("/{}", RAICES[bajo as usize]);
    }
    Some(s)
}

/// Los siete acordes de la tonalidad: (raíz, tipo, grado en números romanos).
pub fn diatonicos(tono: u8, menor: bool) -> [(u8, usize, &'static str); 7] {
    let grados: [(u8, usize, &str); 7] = if menor {
        [(0, MENOR, "i"), (2, DISM, "ii°"), (3, MAYOR, "III"), (5, MENOR, "iv"), (7, MENOR, "v"), (8, MAYOR, "VI"), (10, MAYOR, "VII")]
    } else {
        [(0, MAYOR, "I"), (2, MENOR, "ii"), (4, MENOR, "iii"), (5, MAYOR, "IV"), (7, MAYOR, "V"), (9, MENOR, "vi"), (11, DISM, "vii°")]
    };
    grados.map(|(s, t, g)| ((tono + s) % 12, t, g))
}

/// Dibuja el círculo de quintas (mayores fuera, relativas menores dentro). Un clic en una casilla
/// elige esa tonalidad y devuelve su acorde.
pub fn circulo(ui: &mut egui::Ui, tono: &mut u8, menor: &mut bool) -> Option<(u8, usize)> {
    let lado = ui.available_width().min(320.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(lado, lado), Sense::click());
    let (c, r_ext, r_med, r_int) = (rect.center(), lado * 0.48, lado * 0.33, lado * 0.19);
    let p = ui.painter_at(rect);
    // Posición del tono elegido en el círculo (las menores van con su relativa mayor).
    let rel_mayor = if *menor { (*tono + 3) % 12 } else { *tono };
    let pos_tono = QUINTAS.iter().position(|&q| q == rel_mayor).unwrap_or(0);
    let angulo = |k: f32| (k * 30.0 - 90.0 - 15.0).to_radians();
    let punto = |a: f32, r: f32| c + vec2(a.cos(), a.sin()) * r;
    let mut clic = None;
    for (k, &mayor) in QUINTAS.iter().enumerate() {
        let distancia = (k as i32 - pos_tono as i32).rem_euclid(12).min((pos_tono as i32 - k as i32).rem_euclid(12));
        for (anillo, (r0, r1)) in [(r_med, r_ext), (r_int, r_med)].into_iter().enumerate() {
            let es_menor = anillo == 1;
            let raiz = if es_menor { (mayor + 9) % 12 } else { mayor };
            let elegido = raiz == *tono && es_menor == *menor;
            // Las casillas vecinas (IV, V y relativas) son los acordes de la tonalidad.
            let color = match (elegido, distancia) {
                (true, _) => ACCENT,
                (_, 0 | 1) => ACCENT.gamma_multiply(0.35),
                _ => Color32::from_white_alpha(if es_menor { 10 } else { 16 }),
            };
            // Sector de anillo en trozos convexos.
            let (a0, a1) = (angulo(k as f32), angulo(k as f32 + 1.0));
            for s in 0..6 {
                let (b0, b1) = (a0 + (a1 - a0) * s as f32 / 6.0, a0 + (a1 - a0) * (s + 1) as f32 / 6.0);
                p.add(Shape::convex_polygon(vec![punto(b0, r0), punto(b0, r1), punto(b1, r1), punto(b1, r0)], color, Stroke::NONE));
            }
            p.line_segment([punto(a0, r0), punto(a0, r1)], Stroke::new(1.5, PANEL));
            let txt = nombre(raiz, if es_menor { MENOR } else { MAYOR });
            let centro = punto((a0 + a1) / 2.0, (r0 + r1) / 2.0);
            p.text(centro, Align2::CENTER_CENTER, txt, FontId::proportional(if es_menor { 11.0 } else { 14.0 }), if elegido { BG } else { TEXT });
            if resp.clicked()
                && let Some(m) = resp.interact_pointer_pos()
            {
                let d = m - c;
                let a = (d.y.atan2(d.x).to_degrees() + 90.0 + 15.0).rem_euclid(360.0);
                if (a / 30.0) as usize == k && (r0..r1).contains(&d.length()) {
                    (*tono, *menor) = (raiz, es_menor);
                    clic = Some((raiz, if es_menor { MENOR } else { MAYOR }));
                }
            }
        }
    }
    p.circle_stroke(c, r_ext, Stroke::new(1.0, BORDER));
    p.text(c, Align2::CENTER_CENTER, tonalidad(*tono, *menor), FontId::proportional(18.0), ACCENT);
    clic
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn reconoce_acordes() {
        assert_eq!(detectar(&[60, 64, 67]).as_deref(), Some("C"));
        assert_eq!(detectar(&[57, 60, 64, 67]).as_deref(), Some("Am7"));
        assert_eq!(detectar(&[64, 67, 72]).as_deref(), Some("C/E"));
        assert_eq!(detectar(&[55, 59, 62, 65]).as_deref(), Some("G7"));
        assert_eq!(detectar(&[40, 47]).as_deref(), Some("E5"));
        assert_eq!(detectar(&[60]), None);
        assert_eq!(notas(60, 0, 1), vec![64, 67, 72]);
        assert_eq!(diatonicos(7, false)[4].0, 2); // V de Sol = Re
    }
}
