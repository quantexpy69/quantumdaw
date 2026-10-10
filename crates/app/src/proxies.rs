//! Proxys y conversión de MIDI a audio. Un proxy es el render de una pista antes del fader
//! (regiones + instrumento + efectos) guardado en Proxies/; con «Trabajar con proxys» activo, esas
//! pistas suenan desde el proxy y el procesador no calcula su instrumento ni sus efectos en vivo.
//! Si la pista cambia, su firma deja de coincidir y vuelve a sonar en vivo hasta actualizar el proxy.
use crate::*;
use project::PROXY_DIR;
use std::hash::{DefaultHasher, Hash, Hasher};

/// Cola tras la última región (reverb, delay, release del instrumento).
const COLA_SEGUNDOS: u64 = 3;

/// Firma del contenido de la pista que el proxy representa (regiones, instrumento, efectos y frecuencia).
pub fn firma(t: &Track, rate: u32) -> u64 {
    let mut h = DefaultHasher::new();
    (rate, t.kind as u8, &t.instrument).hash(&mut h);
    for c in &t.clips {
        (c.start, c.offset, c.len, c.fade_in, c.fade_out, c.gain.to_bits()).hash(&mut h);
        match &c.src {
            Source::Audio(b) => b.file.hash(&mut h),
            Source::Midi(notes) => notes.iter().for_each(|n| (n.start, n.len, n.key, n.vel).hash(&mut h)),
        }
    }
    for fx in &t.fx {
        (fx.kind as u8, fx.bypass.load(Relaxed)).hash(&mut h);
        fx.params.iter().for_each(|p| p.get().to_bits().hash(&mut h));
    }
    if let Some(s) = &t.synth {
        s.params.iter().for_each(|p| p.get().to_bits().hash(&mut h));
    }
    h.finish()
}

/// Pistas que vale la pena pasar a proxy: con instrumento (MIDI) o con efectos, y con regiones.
pub fn pesada(t: &Track) -> bool {
    !t.clips.is_empty() && (t.midi() || !t.fx.is_empty()) && t.kind != TrackKind::Video
}

impl App {
    /// Final del material de la pista más la cola de efectos.
    fn fin_con_cola(&self, i: usize) -> u64 {
        self.s.tracks[i].clips.iter().map(|c| c.end()).max().unwrap_or(0) + COLA_SEGUNDOS * self.engine.sample_rate as u64
    }

    /// Render previo al fader de la pista `i` (con todo procesado en vivo).
    fn render_pista(&mut self, i: usize) -> Vec<[f32; 2]> {
        let fin = self.fin_con_cola(i);
        let params = self.s.tracks[i].params.clone();
        self.engine.bounce_prefader(0, fin, &params).chunks(2).map(|c| [c[0], c[1]]).collect()
    }

    /// Crea o actualiza los proxys de las pistas pesadas que cambiaron; devuelve cuántos se generaron.
    pub fn actualizar_proxies(&mut self) -> anyhow::Result<usize> {
        self.stop();
        (self.render_vivo, self.dirty) = (true, true);
        self.sync();
        let rate = self.engine.sample_rate;
        fs::create_dir_all(self.dir.join(PROXY_DIR))?;
        let mut hechos = 0;
        for i in 0..self.s.tracks.len() {
            let t = &self.s.tracks[i];
            let f = firma(t, rate);
            if !pesada(t) || t.proxy.as_ref().is_some_and(|p| p.1 == f) {
                continue;
            }
            let frames = self.render_pista(i);
            let file = format!("{PROXY_DIR}/{}-{}.wav", safe(&self.s.tracks[i].name), stamp());
            let samples: Vec<f32> = frames.iter().flatten().copied().collect();
            project::write_wav(&self.dir.join(&file), rate, 2, 32, &samples)?;
            if let Some((viejo, _)) = self.s.tracks[i].proxy.replace((AudioBuf::new(file, frames), f)) {
                let _ = fs::remove_file(self.dir.join(&viejo.file));
            }
            hechos += 1;
        }
        (self.render_vivo, self.dirty) = (false, true);
        Ok(hechos)
    }

    /// Activa o desactiva el trabajo con proxys (al activarlo se generan los que falten).
    pub fn cambiar_proxies(&mut self, activar: bool) {
        self.config.proxies = activar;
        self.config.save();
        self.dirty = true;
        if activar {
            let r = self.actualizar_proxies();
            self.status = match r {
                Ok(n) => format!("{} · {n} {}", tr("Trabajando con proxys"), tr("proxys generados")),
                Err(e) => format!("{}: {e}", tr("No se pudieron generar los proxys")),
            };
        } else {
            self.status = tr("Proxys desactivados: todo se procesa en vivo").into();
        }
    }

    /// ¿La pista suena ahora desde su proxy?
    pub fn usa_proxy(&self, i: usize) -> bool {
        let t = &self.s.tracks[i];
        self.config.proxies && t.proxy.as_ref().is_some_and(|p| p.1 == firma(t, self.engine.sample_rate))
    }

    /// Convierte una pista MIDI en audio: nueva pista estéreo con el render del instrumento y sus efectos,
    /// justo debajo, con el mismo color, volumen y panorama; la pista MIDI queda silenciada.
    pub fn midi_a_audio(&mut self, i: usize) -> anyhow::Result<()> {
        anyhow::ensure!(self.s.tracks[i].midi(), "la pista no es MIDI");
        anyhow::ensure!(!self.s.tracks[i].clips.is_empty(), "la pista no tiene regiones MIDI");
        self.stop();
        (self.render_vivo, self.dirty) = (true, true);
        self.sync();
        let frames = self.render_pista(i);
        self.render_vivo = false;
        let nombre = self.s.tracks[i].name.clone();
        let buf = self.write_audio(&format!("{nombre}.wav"), "audio", frames)?;
        self.add_track(format!("{nombre} ({})", tr("audio")), TrackKind::AudioStereo);
        let mut nueva = self.s.tracks.pop().unwrap();
        let origen = &self.s.tracks[i];
        nueva.color = origen.color;
        nueva.params.gain.set(origen.params.gain.get());
        nueva.params.pan.set(origen.params.pan.get());
        nueva.clips.push(Clip::audio(buf, 0));
        origen.params.mute.store(true, Relaxed);
        self.s.tracks.insert(i + 1, nueva);
        self.pin_video();
        self.dirty = true;
        Ok(())
    }
}
