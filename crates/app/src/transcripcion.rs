//! Transcripción de lo que se toca: detección del pulso (tempo y fase) a partir de los ataques,
//! cuantización a figuras (redonda a semicorchea, con puntillo) y detección de alturas desde audio
//! monofónico (voz, guitarra, vientos) con YIN.
use project::partitura::NotaP;

/// Nota tocada: inicio y duración en segundos, altura MIDI y velocidad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Golpe {
    pub inicio: f64,
    pub dur: f64,
    pub altura: u8,
    pub vel: u8,
}

/// Pulso más probable de unos ataques (segundos): (duración de un tiempo en segundos, fase: momento
/// de un tiempo). Prefiere tempos cercanos a 100 BPM para decidir entre el doble y la mitad.
pub fn estimar_pulso(ataques: &[f64]) -> Option<(f64, f64)> {
    // Un acorde cuenta como un solo ataque.
    let mut unicos: Vec<f64> = ataques.to_vec();
    unicos.sort_by(|a, b| a.total_cmp(b));
    unicos.dedup_by(|b, a| *b - *a < 0.05);
    let ataques = &unicos[..];
    if ataques.len() < 3 {
        return None;
    }
    let mut mejor: Option<(f64, f64, f64)> = None;
    let mut bpm = 50.0;
    while bpm <= 210.0 {
        let p = 60.0 / bpm;
        let (mut re, mut im) = (0.0, 0.0);
        for &t in ataques {
            let a = std::f64::consts::TAU * t / p;
            (re, im) = (re + a.cos(), im + a.sin());
        }
        let coherencia = (re * re + im * im).sqrt() / ataques.len() as f64;
        let octavas = (bpm / 100.0f64).log2();
        let puntaje = coherencia * (-0.5 * (octavas / 0.8).powi(2)).exp();
        if mejor.is_none_or(|m| puntaje > m.0) {
            let fase = (im.atan2(re) / std::f64::consts::TAU).rem_euclid(1.0) * p;
            mejor = Some((puntaje, p, fase));
        }
        bpm += 0.5;
    }
    mejor.map(|m| (m.1, m.2))
}

/// Figuras escribibles (en negras), de mayor a menor.
const FIGURAS: [f64; 8] = [4.0, 3.0, 2.0, 1.5, 1.0, 0.75, 0.5, 0.25];

/// Pasa lo tocado a notas de partitura con el pulso dado: inicios en la semicorchea más cercana y
/// duraciones a la figura más cercana que no pise el ataque siguiente. El primer tiempo con un
/// ataque es el tiempo 0 (se puede desplazar con `desplazar`).
#[cfg(test)]
pub fn cuantizar(golpes: &[Golpe], periodo: f64, fase: f64) -> Vec<NotaP> {
    cuantizar_con(golpes, periodo, fase, false)
}

/// Como `cuantizar`; con `origen_fijo` el tiempo 0 es el momento 0 (pulso fijo con metrónomo).
pub fn cuantizar_con(golpes: &[Golpe], periodo: f64, fase: f64, origen_fijo: bool) -> Vec<NotaP> {
    if golpes.is_empty() || periodo <= 0.0 {
        return vec![];
    }
    // Acordes: las notas que se tocan juntas (menos de 50 ms) empiezan en el mismo momento.
    let mut golpes = golpes.to_vec();
    golpes.sort_by(|a, b| a.inicio.total_cmp(&b.inicio));
    let mut ancla = f64::MIN;
    for g in golpes.iter_mut() {
        if g.inicio - ancla < 0.05 {
            g.inicio = ancla;
        } else {
            ancla = g.inicio;
        }
    }
    let golpes = &golpes[..];
    let a_tiempos = |t: f64| (t - fase) / periodo;
    // El tiempo más cercano al primer ataque es el tiempo 0 (sin silencios de más al principio).
    let origen = if origen_fijo { 0.0 } else { a_tiempos(golpes.iter().map(|g| g.inicio).fold(f64::MAX, f64::min)).round() };
    let q = |x: f64| (x * 4.0).round() / 4.0;
    let mut notas: Vec<NotaP> = golpes
        .iter()
        .map(|g| {
            let inicio = q(a_tiempos(g.inicio) - origen).max(0.0);
            NotaP { inicio, dur: (g.dur / periodo).max(0.2), altura: g.altura, alteracion: None }
        })
        .collect();
    notas.sort_by(|a, b| a.inicio.total_cmp(&b.inicio).then(a.altura.cmp(&b.altura)));
    notas.dedup_by(|a, b| a.inicio == b.inicio && a.altura == b.altura);
    // Duración: la figura más cercana sin pasar del siguiente ataque.
    let inicios: Vec<f64> = notas.iter().map(|n| n.inicio).collect();
    for n in notas.iter_mut() {
        let siguiente = inicios.iter().copied().find(|&i| i > n.inicio + 1e-9).unwrap_or(f64::MAX);
        let tope = (siguiente - n.inicio).max(0.25);
        // Legato: si la nota llena la mayor parte del espacio hasta el ataque siguiente, lo ocupa entero
        // (se lee como una negra, no como una corchea con puntillo y un silencio).
        if siguiente < f64::MAX && n.dur >= tope * 0.6 {
            n.dur = tope;
        }
        // La figura más cercana, prefiriendo las simples: las de puntillo solo si encajan claramente.
        let costo = |f: f64| (f - n.dur).abs() + if [3.0, 1.5, 0.75].contains(&f) { 0.2 } else { 0.0 };
        n.dur = FIGURAS.iter().copied().filter(|&f| f <= tope + 1e-9).min_by(|a, b| costo(*a).total_cmp(&costo(*b))).unwrap_or(0.25);
    }
    notas
}

/// Frecuencia fundamental de una ventana con YIN (None si no hay tono claro).
fn yin(x: &[f32], sr: f32) -> Option<f32> {
    let (min_lag, max_lag) = ((sr / 1100.0) as usize, (sr / 60.0) as usize);
    let w = x.len() / 2;
    if max_lag >= w {
        return None;
    }
    let mut d = vec![0f32; max_lag + 1];
    for (tau, dt) in d.iter_mut().enumerate().skip(1) {
        *dt = (0..w).map(|i| (x[i] - x[i + tau]).powi(2)).sum();
    }
    // Diferencia normalizada acumulada y primer mínimo bajo el umbral.
    let mut suma = 0.0;
    let mut cmnd = vec![1f32; max_lag + 1];
    for tau in 1..=max_lag {
        suma += d[tau];
        cmnd[tau] = if suma > 0.0 { d[tau] * tau as f32 / suma } else { 1.0 };
    }
    let mut tau = min_lag.max(2);
    while tau < max_lag {
        if cmnd[tau] < 0.15 {
            while tau + 1 < max_lag && cmnd[tau + 1] < cmnd[tau] {
                tau += 1;
            }
            // Interpolación parabólica.
            let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
            let ajuste = 0.5 * (a - c) / (a - 2.0 * b + c).max(1e-9);
            return Some(sr / (tau as f32 + ajuste.clamp(-1.0, 1.0)));
        }
        tau += 1;
    }
    None
}

/// Ventanas de análisis de audio: (altura MIDI si hay tono, nivel RMS).
pub type Cuadro = (Option<u8>, f32);
pub const VENTANA: usize = 2048;
pub const SALTO: usize = 512;

/// Analiza las ventanas que empiezan desde la muestra `desde` (para ir de a poco mientras se graba).
pub fn cuadros_audio(x: &[f32], sr: f32, desde: usize) -> Vec<Cuadro> {
    let mut cuadros = vec![];
    let mut i = desde;
    while i + VENTANA <= x.len() {
        let v = &x[i..i + VENTANA];
        let rms = (v.iter().map(|s| s * s).sum::<f32>() / VENTANA as f32).sqrt();
        let altura = if rms > 10f32.powf(-45.0 / 20.0) { yin(v, sr).map(|f| (69.0 + 12.0 * (f / 440.0).log2()).round().clamp(0.0, 127.0) as u8) } else { None };
        cuadros.push((altura, rms));
        i += SALTO;
    }
    cuadros
}

/// Notas a partir de las ventanas: una nota nueva cuando cambia la altura (confirmada en la ventana
/// siguiente) o hay un ataque (el nivel sube 6 dB); mínimo 60 ms.
pub fn segmentar(cuadros: &[Cuadro], sr: f32) -> Vec<Golpe> {
    let seg = |k: usize| k as f64 * SALTO as f64 / sr as f64;
    let mut golpes: Vec<Golpe> = vec![];
    let mut actual: Option<(usize, u8, f32)> = None;
    let cerrar = |a: Option<(usize, u8, f32)>, fin: usize, golpes: &mut Vec<Golpe>| {
        if let Some((ini, altura, pico)) = a {
            let dur = seg(fin) - seg(ini);
            if dur >= 0.06 {
                golpes.push(Golpe { inicio: seg(ini), dur, altura, vel: (pico.sqrt() * 400.0).clamp(30.0, 127.0) as u8 });
            }
        }
    };
    for k in 0..cuadros.len() {
        let (altura, rms) = cuadros[k];
        let ataque = k > 0 && rms > cuadros[k - 1].1 * 2.0 && cuadros[k - 1].1 > 0.0;
        match (actual, altura) {
            (Some((_, a, _)), Some(h)) if a == h && !ataque => {
                if let Some(c) = actual.as_mut() {
                    c.2 = c.2.max(rms);
                }
            }
            (_, Some(h)) => {
                // Una sola ventana distinta no corta la nota (vibrato, ruido).
                let confirma = cuadros.get(k + 1).is_some_and(|c| c.0 == Some(h));
                if actual.is_none() || confirma || ataque {
                    cerrar(actual, k, &mut golpes);
                    actual = Some((k, h, rms));
                }
            }
            (Some(_), None) => {
                cerrar(actual, k, &mut golpes);
                actual = None;
            }
            (None, None) => {}
        }
    }
    cerrar(actual, cuadros.len(), &mut golpes);
    golpes
}

/// Notas de un audio monofónico (voz, guitarra, vientos).
#[cfg(test)]
pub fn notas_de_audio(x: &[f32], sr: f32) -> Vec<Golpe> {
    segmentar(&cuadros_audio(x, sr, 0), sr)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn pulso_y_figuras() {
        // 120 BPM con imprecisión humana: negra, negra, dos corcheas, blanca.
        let tiempos = [0.0, 1.0, 2.0, 2.5, 3.0, 5.0, 6.0];
        let azar = [0.012, -0.008, 0.015, -0.01, 0.006, -0.012, 0.009];
        let golpes: Vec<Golpe> = tiempos.iter().zip(azar).map(|(t, e)| Golpe { inicio: 1.3 + t * 0.5 + e, dur: 0.4, altura: 60, vel: 100 }).collect();
        let ataques: Vec<f64> = golpes.iter().map(|g| g.inicio).collect();
        let (periodo, fase) = estimar_pulso(&ataques).unwrap();
        assert!((60.0 / periodo - 120.0).abs() < 3.0, "tempo {}", 60.0 / periodo);
        let notas = cuantizar(&golpes, periodo, fase);
        let inicios: Vec<f64> = notas.iter().map(|n| n.inicio).collect();
        assert_eq!(inicios, vec![0.0, 1.0, 2.0, 2.5, 3.0, 5.0, 6.0]);
        let duraciones: Vec<f64> = notas.iter().map(|n| n.dur).collect();
        assert_eq!(&duraciones[..5], &[1.0, 1.0, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn acordes_tocados() {
        // Dos acordes (Do mayor y Fa mayor) a 100 BPM, con las notas de cada uno algo desparejas.
        let tiempo = 0.6;
        let mut golpes = vec![];
        for (k, acorde) in [[60, 64, 67], [65, 69, 72], [60, 64, 67], [65, 69, 72]].iter().enumerate() {
            for (j, &a) in acorde.iter().enumerate() {
                golpes.push(Golpe { inicio: 0.4 + k as f64 * 2.0 * tiempo + j as f64 * 0.015, dur: 1.9 * tiempo, altura: a, vel: 90 });
            }
        }
        let ataques: Vec<f64> = golpes.iter().map(|g| g.inicio).collect();
        let (periodo, fase) = estimar_pulso(&ataques).unwrap();
        let notas = cuantizar(&golpes, periodo, fase);
        let mut inicios: Vec<f64> = notas.iter().map(|n| n.inicio).collect();
        inicios.dedup();
        assert_eq!(inicios.len(), 4, "cuatro acordes: {notas:?}");
        assert!(notas.iter().filter(|n| n.inicio == inicios[1]).map(|n| n.altura).eq([65, 69, 72]));
    }

    #[test]
    fn alturas_de_un_audio() {
        // La4 medio segundo, silencio, Do5 medio segundo.
        let sr = 44_100.0;
        let tono = |f: f32, n: usize| (0..n).map(move |i| (std::f32::consts::TAU * f * i as f32 / sr).sin() * 0.4);
        let x: Vec<f32> = tono(440.0, 22_050).chain(std::iter::repeat_n(0.0, 8_000)).chain(tono(523.25, 22_050)).collect();
        let notas = notas_de_audio(&x, sr);
        assert_eq!(notas.iter().map(|n| n.altura).collect::<Vec<_>>(), vec![69, 72], "{notas:?}");
        assert!((notas[0].dur - 0.5).abs() < 0.08);
    }
}
