//! Edición de audio por cortes (no destructiva) y grabación retrospectiva:
//! - Quitar silencios, cerrar huecos, cortar en los transientes y cuantizar audio a la rejilla.
//! - «Capturar lo último tocado»: recupera los últimos 30 s de la entrada y del MIDI tocado.
use crate::*;

/// Cortes sobre las regiones de audio seleccionadas.
#[derive(Clone, Copy)]
pub enum Corte {
    /// Quita los tramos por debajo de este nivel (dBFS) de al menos 150 ms.
    Silencios(f32),
    /// Junta las regiones seleccionadas de cada pista una detrás de otra.
    CerrarHuecos,
    /// Corta en cada ataque (sensibilidad 0–1).
    Transientes(f32),
    /// Corta en cada ataque y lleva cada trozo a la rejilla (1/16).
    Cuantizar(f32),
}

impl App {
    /// Aplica un corte a las regiones de audio seleccionadas (se puede deshacer).
    pub fn cortar_audio(&mut self, corte: Corte) -> anyhow::Result<usize> {
        let sr = self.engine.sample_rate as f32;
        let rejilla = self.engine.beat_frames() / 4.0;
        let seleccion = self.s.tracks.iter().any(|t| t.clips.iter().any(|c| c.selected && c.buf().is_some()));
        anyhow::ensure!(seleccion, "selecciona una o más regiones de audio");
        self.edit();
        let mut piezas_total = 0;
        for t in self.s.tracks.iter_mut() {
            if let Corte::CerrarHuecos = corte {
                let mut idx: Vec<usize> = (0..t.clips.len()).filter(|&j| t.clips[j].selected).collect();
                idx.sort_by_key(|&j| t.clips[j].start);
                for w in 1..idx.len() {
                    t.clips[idx[w]].start = t.clips[idx[w - 1]].end();
                }
                piezas_total += idx.len();
                continue;
            }
            let mut nuevas = vec![];
            for c in std::mem::take(&mut t.clips) {
                let Some(buf) = c.buf().filter(|_| c.selected).cloned() else {
                    nuevas.push(c);
                    continue;
                };
                let (ini, fin) = (c.offset as usize, (c.offset + c.len) as usize);
                let tramo = &buf.frames[ini.min(buf.frames.len())..fin.min(buf.frames.len())];
                let abs = |k: usize| c.start + k as u64;
                let mut piezas: Vec<Clip> = match corte {
                    Corte::Silencios(db) => {
                        // Lo que queda entre silencios, con 10 ms de margen y fundidos cortos.
                        let margen = (sr * 0.01) as usize;
                        let mut sonido = vec![];
                        let mut desde = 0;
                        for (a, b) in engine::find_silences(tramo, sr, db, 150.0) {
                            if a > desde {
                                sonido.push((desde, (a + margen).min(tramo.len())));
                            }
                            desde = b.saturating_sub(margen);
                        }
                        if desde < tramo.len() {
                            sonido.push((desde, tramo.len()));
                        }
                        sonido
                            .into_iter()
                            .filter_map(|(a, b)| {
                                let mut p = c.trim(abs(a), abs(b))?;
                                (p.fade_in, p.fade_out) = (p.fade_in.max((sr * 0.005) as u64), p.fade_out.max((sr * 0.005) as u64));
                                Some(p)
                            })
                            .collect()
                    }
                    Corte::Transientes(sens) | Corte::Cuantizar(sens) => {
                        let mut cortes: Vec<usize> = engine::find_onsets(tramo, sr, sens).into_iter().filter(|&k| k > 0).collect();
                        cortes.insert(0, 0);
                        cortes.push(tramo.len());
                        cortes.windows(2).filter_map(|w| c.trim(abs(w[0]), abs(w[1]))).collect()
                    }
                    Corte::CerrarHuecos => unreachable!(),
                };
                if let Corte::Cuantizar(_) = corte {
                    // Cada trozo empieza en la división de 1/16 más cercana; si se pisa con el siguiente,
                    // se acorta.
                    for p in piezas.iter_mut() {
                        p.start = ((p.start as f64 / rejilla).round() * rejilla) as u64;
                    }
                    piezas.sort_by_key(|p| p.start);
                    for k in 0..piezas.len().saturating_sub(1) {
                        let siguiente = piezas[k + 1].start;
                        if piezas[k].end() > siguiente {
                            piezas[k].len = siguiente.saturating_sub(piezas[k].start).max(1);
                        }
                    }
                }
                piezas_total += piezas.len();
                nuevas.extend(piezas.into_iter().map(|mut p| {
                    p.selected = true;
                    p
                }));
            }
            t.clips = nuevas;
        }
        self.dirty = true;
        Ok(piezas_total)
    }

    /// Grabación retrospectiva: pone en las pistas armadas lo último que se tocó (MIDI y audio de la
    /// entrada) aunque no se estuviera grabando. La última frase: desde el último hueco de 4 s.
    pub fn capturar(&mut self) {
        let (sr, ich) = (self.engine.sample_rate as u64, self.engine.in_channels);
        let armadas: Vec<bool> = self.s.tracks.iter().filter(|t| t.params.arm.load(Relaxed)).map(|t| t.midi()).collect();
        if armadas.is_empty() {
            self.status = tr("Arma una pista (botón R) para capturar lo último que tocaste").into();
            return;
        }
        let (midi, audio, sonando) = {
            let Ok(c) = self.engine.captura.lock() else { return };
            (c.midi.iter().copied().collect::<Vec<_>>(), if armadas.iter().any(|m| !m) { c.ultimos(sr as usize * 30) } else { vec![] }, self.playing())
        };
        let ahora = self.pos();
        // MIDI: la última frase (después del último hueco de más de 4 s entre notas).
        let notas_on: Vec<usize> = (0..midi.len()).filter(|&k| midi[k].3[0] & 0xF0 == 0x90 && midi[k].3[2] > 0).collect();
        let desde = notas_on.windows(2).filter(|w| midi[w[1]].0 - midi[w[0]].0 > 4 * sr).map(|w| w[1]).next_back().or(notas_on.first().copied());
        let mut eventos: Vec<(u64, MidiEvent)> = vec![];
        if let Some(d) = desde {
            let primero = midi[d];
            // Si sonaba el transporte se respetan las posiciones de la canción; si no, la frase va en el cursor.
            eventos = midi[d..].iter().map(|&(t, pos, playing, e)| (if playing && primero.2 { pos } else { ahora + (t - primero.0) }, e)).collect();
            // Teclados que mandan cada tecla dos veces: se quitan las notas repetidas a menos de 30 ms.
            let ventana = sr * 3 / 100;
            let mut ultimo: std::collections::HashMap<(u8, u8), u64> = Default::default();
            eventos.retain(|&(t, e)| {
                let tipo = if e[0] & 0xF0 == 0x90 && e[2] > 0 { 1 } else if matches!(e[0] & 0xF0, 0x80 | 0x90) { 0 } else { return true };
                let repetida = ultimo.get(&(tipo, e[1])).is_some_and(|&u| t.saturating_sub(u) < ventana);
                ultimo.insert((tipo, e[1]), t);
                !repetida
            });
        }
        // Audio: los últimos 30 s sin el silencio del principio y del final.
        let ich_n = ich.max(1);
        let frames: Vec<[f32; 2]> = audio.chunks_exact(ich_n).map(|c| [c[0], c[(1).min(ich_n - 1)]]).collect();
        let suena = |s: &[f32; 2]| s[0].abs().max(s[1].abs()) > 10f32.powf(-50.0 / 20.0);
        let (a, b) = (frames.iter().position(suena), frames.iter().rposition(suena));
        let mut datos = vec![];
        let mut inicio_audio = None;
        if let (Some(a), Some(b)) = (a, b) {
            let margen = (sr / 20) as usize;
            let (a, b) = (a.saturating_sub(margen), (b + margen).min(frames.len()));
            datos = audio[a * ich_n..b * ich_n].to_vec();
            // Lo último del anillo corresponde a «ahora»: con el transporte sonando, el audio va donde se tocó.
            let atras = (frames.len() - a) as u64;
            inicio_audio = Some(if sonando { ahora.saturating_sub(atras) } else { ahora });
        }
        if eventos.is_empty() && datos.is_empty() {
            self.status = tr("No hay nada tocado en los últimos 30 segundos").into();
            return;
        }
        let inicio = inicio_audio.into_iter().chain(eventos.iter().map(|e| e.0)).min().unwrap_or(ahora);
        // Si la grabación de audio arranca después que el MIDI, se rellena con silencio.
        if let Some(ia) = inicio_audio {
            let mut relleno = vec![0.0; (ia - inicio) as usize * ich_n];
            relleno.extend(datos);
            datos = relleno;
        }
        let fin = eventos.iter().map(|e| e.0).max().unwrap_or(inicio).max(inicio + (datos.len() / ich_n) as u64) + sr / 10;
        self.edit();
        self.colocar_grabacion(inicio, fin, datos, eventos);
        self.status = tr("Capturado lo último que tocaste").into();
    }
}
