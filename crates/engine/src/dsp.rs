//! Procesos offline sobre audio: time-stretch (WSOLA) y cambio de tono.
use crate::io::resample_by;

pub fn peak(frames: &[[f32; 2]]) -> f32 {
    frames.iter().fold(0f32, |m, s| m.max(s[0].abs()).max(s[1].abs()))
}

/// Estira el audio por `factor` (> 1 más largo) conservando el tono, con WSOLA: cada ventana se
/// toma de la posición (dentro de una tolerancia) que mejor continúa la forma de onda anterior.
pub fn stretch(input: &[[f32; 2]], factor: f64) -> Vec<[f32; 2]> {
    const N: usize = 2048;
    const HS: usize = N / 2;
    const TOL: usize = 512;
    let out_len = (input.len() as f64 * factor) as usize;
    if input.len() < N * 2 || (factor - 1.0).abs() < 1e-4 {
        return resample_by(input, 1.0 / factor);
    }
    let win: Vec<f32> = (0..N).map(|i| (std::f32::consts::PI * i as f32 / N as f32).sin().powi(2)).collect();
    let mono = |i: usize| input.get(i).map_or(0.0, |s| s[0] + s[1]);
    let corr = |a: usize, b: usize| (0..HS).step_by(4).map(|j| mono(a + j) * mono(b + j)).sum::<f32>();
    let (mut out, mut norm) = (vec![[0f32; 2]; out_len + N], vec![0f32; out_len + N]);
    let (ha, last) = (HS as f64 / factor, input.len() - N);
    let mut prev = 0;
    for k in 0..=out_len / HS {
        let nominal = ((k as f64 * ha) as usize).min(last);
        let best = match k {
            0 => 0,
            _ => (nominal.saturating_sub(TOL)..=(nominal + TOL).min(last)).step_by(8).map(|c| (c, corr(prev + HS, c))).max_by(|a, b| a.1.total_cmp(&b.1)).map_or(nominal, |b| b.0),
        };
        for (j, w) in win.iter().enumerate() {
            let (s, o) = (input[best + j], k * HS + j);
            out[o] = [out[o][0] + s[0] * w, out[o][1] + s[1] * w];
            norm[o] += w;
        }
        prev = best;
    }
    out.truncate(out_len);
    out.iter_mut().zip(norm).filter(|(_, n)| *n > 1e-3).for_each(|(o, n)| *o = [o[0] / n, o[1] / n]);
    out
}

/// Transpone `semitones` sin cambiar la duración: estira y vuelve a remuestrear.
pub fn pitch_shift(input: &[[f32; 2]], semitones: f32) -> Vec<[f32; 2]> {
    let ratio = 2f64.powf(semitones as f64 / 12.0);
    let mut out = resample_by(&stretch(input, ratio), ratio);
    out.resize(input.len(), [0.0; 2]);
    out
}

/// Estima el tempo (BPM, 70–180) por autocorrelación de la envolvente de ataques.
pub fn detect_bpm(frames: &[[f32; 2]], sr: f32) -> Option<f32> {
    const HOP: usize = 256;
    let energy: Vec<f32> = frames.chunks(HOP).map(|c| (c.iter().map(|s| s[0] * s[0] + s[1] * s[1]).sum::<f32>() / c.len() as f32 + 1e-9).ln()).collect();
    let onset: Vec<f32> = energy.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    let rate = sr / HOP as f32;
    let lag = |bpm: f32| (rate * 60.0 / bpm).round() as usize;
    let (lo, hi) = (lag(180.0), lag(70.0));
    if onset.len() < hi * 4 {
        return None;
    }
    let ac = |l: usize| onset.iter().zip(&onset[l..]).map(|(a, b)| a * b).sum::<f32>() / (onset.len() - l) as f32;
    // Preferencia perceptual (log-normal centrada en 120 BPM) para decidir entre octavas (70 o 140).
    let score = |l: usize| {
        let octaves = (rate * 60.0 / l as f32 / 120.0).log2();
        ac(l) * (-0.5 * octaves * octaves).exp()
    };
    let best = (lo..=hi).max_by(|&a, &b| score(a).total_cmp(&score(b)))?;
    // Interpolación parabólica alrededor del máximo para afinar el tempo.
    let (y0, y1, y2) = (score(best - 1), score(best), score(best + 1));
    let d = 0.5 * (y0 - y2) / (y0 - 2.0 * y1 + y2).min(-1e-9);
    Some(rate * 60.0 / (best as f32 + d.clamp(-0.5, 0.5)))
}

/// Picos: el máximo de cada ventana de 50 ms que supera `thresh` (lineal), como posiciones en frames.
pub fn find_peaks(frames: &[[f32; 2]], sr: f32, thresh: f32) -> Vec<usize> {
    let win = (sr * 0.05) as usize;
    frames
        .chunks(win.max(1))
        .enumerate()
        .filter_map(|(k, c)| {
            let (i, v) = c.iter().enumerate().map(|(i, s)| (i, s[0].abs().max(s[1].abs()))).max_by(|a, b| a.1.total_cmp(&b.1))?;
            (v >= thresh).then_some(k * win + i)
        })
        .collect()
}

/// Tramos de silencio (frames inicio–fin) por debajo de `thresh_db` durante al menos `min_ms`.
pub fn find_silences(frames: &[[f32; 2]], sr: f32, thresh_db: f32, min_ms: f32) -> Vec<(usize, usize)> {
    let win = (sr * 0.01) as usize;
    let thresh = 10f32.powf(thresh_db / 20.0);
    let (mut out, mut start) = (vec![], None);
    for (k, c) in frames.chunks(win.max(1)).enumerate() {
        let quiet = c.iter().all(|s| s[0].abs().max(s[1].abs()) < thresh);
        match (quiet, start) {
            (true, None) => start = Some(k * win),
            (false, Some(a)) => {
                out.push((a, k * win));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(a) = start {
        out.push((a, frames.len()));
    }
    out.retain(|(a, b)| (b - a) as f32 >= sr * min_ms / 1000.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bpm_of_click_track() {
        let sr = 48_000.0;
        for bpm in [96.0, 120.0, 140.0] {
            let beat = (sr * 60.0 / bpm) as usize;
            let frames: Vec<[f32; 2]> = (0..beat * 24).map(|i| if i % beat < 400 { [0.8, 0.8] } else { [0.0; 2] }).collect();
            let found = detect_bpm(&frames, sr).unwrap();
            assert!((found - bpm).abs() < 1.5, "{bpm} → {found}");
            assert!(find_peaks(&frames, sr, 0.5).len() >= 24);
            assert!(find_silences(&frames, sr, -60.0, 100.0).len() >= 20);
        }
    }
}
