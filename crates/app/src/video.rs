//! Video para música de cine: importación con ffmpeg (cualquier formato que ffmpeg lea), audio a
//! una pista, fotogramas ligeros en caché (RGB 320 px a 12 fps) y visor sincronizado con el cursor.
use crate::*;
use std::{
    io::{Read, Seek, SeekFrom},
    process::Command,
    sync::mpsc,
};

pub const VIDEO_EXT: [&str; 7] = ["mp4", "mov", "mkv", "webm", "avi", "m4v", "mpg"];
const FPS: f64 = 12.0;
const WIDTH: u32 = 320;

pub fn is_video(p: &Path) -> bool {
    p.extension().is_some_and(|e| VIDEO_EXT.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

/// Fotogramas en caché de un video del proyecto.
pub struct VideoData {
    /// Archivo de video, relativo al proyecto.
    pub file: String,
    raw: PathBuf,
    pub w: u32,
    pub h: u32,
    pub frames: u64,
}

impl VideoData {
    pub fn duration(&self) -> f64 {
        self.frames as f64 / FPS
    }

    /// Fotograma en el segundo `t` (desde el inicio del video).
    pub fn frame(&self, t: f64) -> Option<egui::ColorImage> {
        if t < 0.0 {
            return None;
        }
        let idx = ((t * FPS) as u64).min(self.frames.checked_sub(1)?);
        let size = (self.w * self.h * 3) as usize;
        let mut f = fs::File::open(&self.raw).ok()?;
        f.seek(SeekFrom::Start(idx * size as u64)).ok()?;
        let mut buf = vec![0u8; size];
        f.read_exact(&mut buf).ok()?;
        Some(egui::ColorImage::from_rgb([self.w as usize, self.h as usize], &buf))
    }

    pub fn index(&self, t: f64) -> u64 {
        ((t.max(0.0) * FPS) as u64).min(self.frames.saturating_sub(1))
    }
}

/// Resultado de importar un video: datos, audio extraído (archivo y muestras) y posición.
pub struct Imported {
    pub data: Arc<VideoData>,
    pub audio: Option<(String, Vec<[f32; 2]>)>,
    pub at: u64,
}

fn ffprobe(src: &Path, entries: &str) -> anyhow::Result<serde_json::Value> {
    let out = Command::new("ffprobe").args(["-v", "error", "-select_streams", "v:0", "-show_entries", entries, "-of", "json"]).arg(src).output()?;
    anyhow::ensure!(out.status.success(), "ffprobe no pudo leer el video");
    Ok(serde_json::from_slice(&out.stdout)?)
}

/// Prepara (o reutiliza) la caché de fotogramas de un video ya copiado al proyecto.
pub fn open(dir: &Path, file: &str) -> anyhow::Result<VideoData> {
    let src = dir.join(file);
    let raw = src.with_extension("frames.rgb");
    let info = ffprobe(&src, "stream=width,height")?;
    let (w0, h0) = (info["streams"][0]["width"].as_u64().unwrap_or(16), info["streams"][0]["height"].as_u64().unwrap_or(9));
    let h = ((WIDTH as u64 * h0 / w0.max(1)) / 2 * 2).max(2) as u32;
    if !raw.exists() {
        let filter = format!("fps={FPS},scale={WIDTH}:{h}");
        let status = Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&src).args(["-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgb24"]).arg(&raw).status()?;
        anyhow::ensure!(status.success(), "ffmpeg no pudo extraer los fotogramas");
    }
    let frames = fs::metadata(&raw)?.len() / (WIDTH * h * 3) as u64;
    Ok(VideoData { file: file.to_string(), raw, w: WIDTH, h, frames })
}

/// Importa en segundo plano: copia a "Video/", extrae el audio a "Audio Files/" y prepara fotogramas.
pub fn import(dir: PathBuf, src: PathBuf, at: u64, rate: u32) -> mpsc::Receiver<Result<Imported, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let r = (|| -> anyhow::Result<Imported> {
            let name = src.file_name().ok_or_else(|| anyhow::anyhow!("ruta inválida"))?.to_string_lossy().to_string();
            fs::create_dir_all(dir.join("Video"))?;
            let file = format!("Video/{name}");
            if !dir.join(&file).exists() {
                fs::copy(&src, dir.join(&file))?;
            }
            let data = open(&dir, &file)?;
            let wav = format!("{AUDIO_DIR}/{}-video.wav", safe(&src.file_stem().unwrap_or_default().to_string_lossy()));
            fs::create_dir_all(dir.join(AUDIO_DIR))?;
            let ok = Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&src).args(["-vn", "-ac", "2", "-ar", &rate.to_string()]).arg(dir.join(&wav)).status()?.success();
            let audio = ok.then(|| engine::decode(&dir.join(&wav), rate).ok().map(|f| (wav, f))).flatten();
            Ok(Imported { data: Arc::new(data), audio, at })
        })();
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    rx
}

impl App {
    pub fn import_video(&mut self, src: PathBuf, at: u64) {
        self.status = format!("Importando video {}… (extrayendo audio y fotogramas)", src.display());
        self.video_jobs.push(video::import(self.dir.clone(), src, at, self.engine.sample_rate));
    }

    /// Recoge importaciones terminadas: pista de video y, si tiene sonido, pista de audio.
    pub fn poll_video(&mut self) {
        let mut done = vec![];
        for (k, rx) in self.video_jobs.iter().enumerate() {
            if let Ok(r) = rx.try_recv() {
                done.push((k, r));
            }
        }
        for (k, r) in done.into_iter().rev() {
            self.video_jobs.remove(k);
            match r {
                Ok(imp) => {
                    let name = Path::new(&imp.data.file).file_stem().unwrap_or_default().to_string_lossy().to_string();
                    self.add_track(format!("Video · {name}"), TrackKind::Video);
                    let t = self.s.tracks.last_mut().unwrap();
                    (t.video, t.video_start, t.height) = (Some(imp.data), imp.at, 70.0);
                    if let Some((file, frames)) = imp.audio {
                        self.add_track(format!("Audio · {name}"), TrackKind::AudioStereo);
                        self.s.tracks.last_mut().unwrap().clips.push(Clip::audio(AudioBuf::new(file, frames), imp.at));
                    }
                    self.show_video = true;
                    self.status = format!("Video «{name}» importado");
                }
                Err(e) => self.status = format!("Error al importar el video: {e}"),
            }
        }
    }

    /// Fotograma (como textura, con caché) de la pista `i` en el frame de audio `pos`.
    pub fn video_texture(&mut self, i: usize, pos: u64) -> Option<(egui::TextureHandle, [u32; 2])> {
        let t = &self.s.tracks[i];
        let v = t.video.clone()?;
        let secs = (pos as f64 - t.video_start as f64) / self.sr();
        if secs < 0.0 || secs > v.duration() {
            return None;
        }
        let key = (t.id, v.index(secs));
        if !self.video_cache.contains_key(&key) {
            if self.video_cache.len() > 300 {
                self.video_cache.clear();
            }
            let img = v.frame(secs)?;
            let tex = self.ctx.load_texture(format!("video-{}-{}", key.0, key.1), img, egui::TextureOptions::LINEAR);
            self.video_cache.insert(key, tex);
        }
        Some((self.video_cache[&key].clone(), [v.w, v.h]))
    }

    /// Visor de video flotante, sincronizado con el cursor de reproducción.
    pub fn video_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.s.tracks.iter().position(|t| t.video.is_some()).filter(|_| self.show_video) else {
            return;
        };
        let mut open = true;
        let frame = self.video_texture(i, self.pos());
        egui::Window::new("Video").id(egui::Id::new("video-win")).open(&mut open).default_size([480.0, 300.0]).show(ctx, |ui| {
            let w = ui.available_width();
            match frame {
                Some((tex, [vw, vh])) => _ = ui.add(egui::Image::new(&tex).fit_to_exact_size(egui::vec2(w, w * vh as f32 / vw as f32)).corner_radius(widgets::rr(6.0))),
                None => _ = ui.label(egui::RichText::new(tr("El cursor está fuera del video")).color(TEXT_DIM)),
            }
            let s = self.pos() as f64 / self.sr();
            ui.label(egui::RichText::new(format!("{:02}:{:02}:{:02}.{:02}", (s / 3600.0) as u32, (s / 60.0) as u32 % 60, s as u32 % 60, ((s.fract()) * 100.0) as u32)).monospace().size(15.0));
        });
        self.show_video &= open;
    }
}
