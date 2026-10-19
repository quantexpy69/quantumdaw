//! Herramientas creativas del piano roll: bloqueo de escala, arpegiador, rasgueo (strum) y generador
//! de melodías. Son funciones puras sobre notas; el piano roll las aplica con deshacer.
use engine::Note;

/// Escalas: (nombre, intervalos desde la tónica).
pub const ESCALAS: [(&str, &[u8]); 24] = [
    ("Mayor (jónica)", &[0, 2, 4, 5, 7, 9, 11]),
    ("Menor natural (eólica)", &[0, 2, 3, 5, 7, 8, 10]),
    ("Menor armónica", &[0, 2, 3, 5, 7, 8, 11]),
    ("Menor melódica", &[0, 2, 3, 5, 7, 9, 11]),
    ("Dórica", &[0, 2, 3, 5, 7, 9, 10]),
    ("Frigia", &[0, 1, 3, 5, 7, 8, 10]),
    ("Lidia", &[0, 2, 4, 6, 7, 9, 11]),
    ("Mixolidia", &[0, 2, 4, 5, 7, 9, 10]),
    ("Locria", &[0, 1, 3, 5, 6, 8, 10]),
    ("Pentatónica mayor", &[0, 2, 4, 7, 9]),
    ("Pentatónica menor", &[0, 3, 5, 7, 10]),
    ("Blues", &[0, 3, 5, 6, 7, 10]),
    ("Blues mayor", &[0, 2, 3, 4, 7, 9]),
    ("Frigia dominante (flamenca)", &[0, 1, 4, 5, 7, 8, 10]),
    ("Húngara menor", &[0, 2, 3, 6, 7, 8, 11]),
    ("Doble armónica (árabe)", &[0, 1, 4, 5, 7, 8, 11]),
    ("Hirajoshi", &[0, 2, 3, 7, 8]),
    ("In sen", &[0, 1, 5, 7, 10]),
    ("Tonos enteros", &[0, 2, 4, 6, 8, 10]),
    ("Disminuida (tono-semitono)", &[0, 2, 3, 5, 6, 8, 9, 11]),
    ("Bebop dominante", &[0, 2, 4, 5, 7, 9, 10, 11]),
    ("Lidia dominante", &[0, 2, 4, 6, 7, 9, 10]),
    ("Alterada", &[0, 1, 3, 4, 6, 8, 10]),
    ("Cromática", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
];

/// ¿La nota pertenece a la escala con esa tónica?
pub fn en_escala(key: u8, tonica: u8, escala: usize) -> bool {
    ESCALAS[escala].1.contains(&((key + 12 - tonica % 12) % 12))
}

/// Lleva la nota a la más cercana de la escala (en empate, hacia abajo).
pub fn ajustar(key: u8, tonica: u8, escala: usize) -> u8 {
    (0..12i32)
        .flat_map(|d| [key as i32 - d, key as i32 + d])
        .find(|&k| (0..=127).contains(&k) && en_escala(k as u8, tonica, escala))
        .unwrap_or(key as i32) as u8
}

pub const PATRONES_ARPEGIO: [&str; 4] = ["Arriba", "Abajo", "Arriba y abajo", "Aleatorio"];

/// Generador pseudoaleatorio pequeño (xorshift) para melodías y arpegios aleatorios reproducibles.
pub struct Azar(u64);

impl Azar {
    pub fn new(semilla: u64) -> Self {
        Self(semilla | 1)
    }
    pub fn sig(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    pub fn hasta(&mut self, n: usize) -> usize {
        (self.sig() % n.max(1) as u64) as usize
    }
}

/// Agrupa las notas que empiezan juntas (acordes): índices por grupo, en orden de inicio.
fn grupos(notas: &[Note], tolerancia: u64) -> Vec<Vec<usize>> {
    let mut idx: Vec<usize> = (0..notas.len()).collect();
    idx.sort_by_key(|&k| (notas[k].start, notas[k].key));
    let mut out: Vec<Vec<usize>> = vec![];
    for k in idx {
        match out.last_mut() {
            Some(g) if notas[k].start.abs_diff(notas[g[0]].start) <= tolerancia => g.push(k),
            _ => out.push(vec![k]),
        }
    }
    out
}

/// Convierte cada acorde en un arpegio que dura lo mismo que el acorde, con notas de `paso` frames.
pub fn arpegiar(notas: &[Note], paso: u64, patron: usize, semilla: u64) -> Vec<Note> {
    let mut azar = Azar::new(semilla);
    let mut out = vec![];
    for g in grupos(notas, paso / 4) {
        let mut teclas: Vec<Note> = g.iter().map(|&k| notas[k]).collect();
        teclas.sort_by_key(|n| n.key);
        let (inicio, fin) = (teclas[0].start, teclas.iter().map(|n| n.start + n.len).max().unwrap_or(0));
        let orden: Vec<Note> = match patron {
            1 => teclas.iter().rev().copied().collect(),
            2 if teclas.len() > 2 => teclas.iter().chain(teclas[1..teclas.len() - 1].iter().rev()).copied().collect(),
            _ => teclas.clone(),
        };
        let mut t = inicio;
        let mut k = 0;
        while t < fin && paso > 0 {
            let n = if patron == 3 { teclas[azar.hasta(teclas.len())] } else { orden[k % orden.len()] };
            out.push(Note { start: t, len: paso.min(fin - t), ..n });
            (t, k) = (t + paso, k + 1);
        }
    }
    out
}

/// Rasgueo: retrasa cada nota del acorde `separacion` frames respecto de la anterior (hacia arriba o
/// hacia abajo), manteniendo el final de las notas.
pub fn rasguear(notas: &[Note], separacion: u64, hacia_abajo: bool) -> Vec<Note> {
    let mut out = notas.to_vec();
    for g in grupos(notas, separacion.max(1)) {
        let mut orden = g.clone();
        orden.sort_by_key(|&k| notas[k].key);
        if hacia_abajo {
            orden.reverse();
        }
        for (paso, &k) in orden.iter().enumerate() {
            let n = &mut out[k];
            let fin = n.start + n.len;
            let retraso = (separacion * paso as u64).min(n.len.saturating_sub(1));
            (n.start, n.len) = (n.start + retraso, fin - (n.start + retraso));
        }
    }
    out
}

pub const DENSIDADES: [&str; 3] = ["Tranquila", "Media", "Movida"];

/// Melodía en la escala: paseo por grados con saltos cortos, notas largas en los tiempos fuertes
/// y final en la tónica. `paso` = duración de la rejilla, `compas` = frames por compás.
/// `escala` = (tónica, índice en `ESCALAS`).
pub fn melodia(inicio: u64, compases: u32, compas: u64, paso: u64, (tonica, escala): (u8, usize), densidad: usize, semilla: u64) -> Vec<Note> {
    let mut azar = Azar::new(semilla);
    let grados = ESCALAS[escala].1;
    // Grados de la escala en dos octavas a partir de Do4/la tónica central.
    let base = 60 + tonica % 12;
    let notas: Vec<u8> = (-1i32..2).flat_map(|o| grados.iter().map(move |&g| (base as i32 + o * 12 + g as i32) as u8)).collect();
    let centro = notas.iter().position(|&n| n == base).unwrap_or(0) as i32;
    let mut pos = centro;
    let fin = inicio + compas * compases as u64;
    let mut t = inicio;
    let mut out = vec![];
    while t < fin && paso > 0 {
        // Duración: más larga con densidad baja.
        let opciones: &[u64] = match densidad {
            0 => &[2, 4, 4, 8],
            1 => &[1, 2, 2, 4],
            _ => &[1, 1, 2, 2],
        };
        let len = (paso * opciones[azar.hasta(opciones.len())]).min(fin - t);
        // Silencios ocasionales (menos en la densidad alta).
        let silencio = azar.hasta(10) < [3, 2, 1][densidad.min(2)];
        if !silencio || t == inicio {
            let salto = [-2, -1, -1, 0, 1, 1, 2, -3, 3][azar.hasta(9)];
            pos = (pos + salto).clamp(centro - 4, centro + 7);
            out.push(Note::new(t, len, notas[pos as usize], 80 + azar.hasta(30) as u8));
        }
        t += len;
    }
    // Termina en la tónica.
    if let Some(ultima) = out.last_mut() {
        ultima.key = base;
    }
    out
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn escala_y_herramientas() {
        // Do mayor: Do# se ajusta a Do, Fa# a Fa.
        assert_eq!(ajustar(61, 0, 0), 60);
        assert_eq!(ajustar(66, 0, 0), 65);
        assert!(en_escala(69, 9, 1)); // La en La menor
        let acorde = [Note::new(0, 960, 60, 100), Note::new(0, 960, 64, 100), Note::new(0, 960, 67, 100)];
        let arp = arpegiar(&acorde, 240, 0, 1);
        assert_eq!(arp.iter().map(|n| n.key).collect::<Vec<_>>(), vec![60, 64, 67, 60]);
        let r = rasguear(&acorde, 30, false);
        assert_eq!(r.iter().map(|n| (n.start, n.start + n.len)).collect::<Vec<_>>(), vec![(0, 960), (30, 960), (60, 960)]);
        let m = melodia(0, 2, 1920, 240, (0, 0), 1, 7);
        assert!(!m.is_empty() && m.iter().all(|n| en_escala(n.key, 0, 0) && n.start + n.len <= 3840));
        assert_eq!(m.last().unwrap().key, 60);
    }
}
