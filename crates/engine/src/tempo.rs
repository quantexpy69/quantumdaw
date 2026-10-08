//! Mapa de tempo: tramos (frame inicial, BPM). Convierte entre frames y tiempos (beats) para la
//! rejilla, el ajuste, el metrónomo y el contador de compases. El primer tramo empieza en 0.

/// Duración de un tiempo en frames (`unit` = denominador del compás: 4 negra, 8 corchea).
pub fn beat_len(sr: f64, bpm: f32, unit: u32) -> f64 {
    sr * 60.0 / bpm.max(1.0) as f64 * 4.0 / unit.max(1) as f64
}

pub fn bpm_at(map: &[(u64, f32)], f: f64) -> f32 {
    map.iter().rev().find(|s| s.0 as f64 <= f).or(map.first()).map_or(120.0, |s| s.1)
}

/// Tiempos transcurridos desde el inicio hasta el frame `f`.
pub fn beats_at(map: &[(u64, f32)], sr: f64, unit: u32, f: f64) -> f64 {
    let mut beats = 0.0;
    for (k, &(s, bpm)) in map.iter().enumerate() {
        let end = map.get(k + 1).map_or(f64::INFINITY, |n| n.0 as f64);
        if f < end {
            return beats + (f - s as f64) / beat_len(sr, bpm, unit);
        }
        beats += (end - s as f64) / beat_len(sr, bpm, unit);
    }
    f / beat_len(sr, 120.0, unit)
}

/// Frame donde cae el tiempo `b` (inversa de `beats_at`).
pub fn frame_at_beat(map: &[(u64, f32)], sr: f64, unit: u32, b: f64) -> f64 {
    let mut beats = 0.0;
    for (k, &(s, bpm)) in map.iter().enumerate() {
        let len = beat_len(sr, bpm, unit);
        let seg = map.get(k + 1).map_or(f64::INFINITY, |n| (n.0 - s) as f64 / len);
        if b < beats + seg {
            return s as f64 + (b - beats) * len;
        }
        beats += seg;
    }
    b * beat_len(sr, 120.0, unit)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tempo_map_roundtrip() {
        let map = [(0, 120.0), (96_000, 60.0)];
        assert!((beats_at(&map, 48_000.0, 4, 96_000.0) - 4.0).abs() < 1e-9);
        assert!((beats_at(&map, 48_000.0, 4, 144_000.0) - 5.0).abs() < 1e-9);
        assert!((frame_at_beat(&map, 48_000.0, 4, 5.0) - 144_000.0).abs() < 1e-6);
        assert_eq!(bpm_at(&map, 100_000.0), 60.0);
    }
}
