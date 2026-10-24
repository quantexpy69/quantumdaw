//! Plugins QUANTUM Pro: el motor solo lleva las licencias, el modo demo y el registro donde se
//! conecta su procesamiento. El DSP y las interfaces son propietarios y viven en un repositorio
//! aparte (`pro/`); sin él, los plugins Pro no aparecen y un proyecto que los use los deja pasar
//! sin procesar.
//!
//! Sin licencia funcionan en modo demo (un segundo de silencio cada 45 s). Las licencias las
//! verifica la app y las marca con `activar`.
use crate::AtomicF32;
use crate::fx::FxKind;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

/// Licencias activas: un bit por plugin (`FxKind::pro`).
static LICENCIAS: AtomicU32 = AtomicU32::new(0);

/// Lecturas para la interfaz de cada plugin Pro (`Fx::medidores`).
pub const MEDIDORES: usize = 16;

/// Procesador de un plugin Pro.
pub trait ProDsp: Send {
    /// `p`: parámetros; `m`: lecturas para la interfaz.
    fn process(&mut self, buf: &mut [[f32; 2]], sr: f32, bpm: f32, p: &[f32; 32], m: &[AtomicF32], offline: bool);
}

type Fabrica = fn(FxKind, u32) -> Option<Box<dyn ProDsp>>;
static FABRICA: OnceLock<Fabrica> = OnceLock::new();

/// Conecta el procesamiento de los plugins Pro (lo llama el módulo privado al iniciar).
pub fn registrar(f: Fabrica) {
    let _ = FABRICA.set(f);
}

/// ¿Están los plugins Pro en esta compilación?
pub fn disponible() -> bool {
    FABRICA.get().is_some()
}

pub(crate) fn crear(kind: FxKind, sr: u32) -> Option<Box<dyn ProDsp>> {
    FABRICA.get().and_then(|f| f(kind, sr))
}

/// Marca las licencias válidas (bits de `FxKind::pro().0`).
pub fn activar(bits: u32) {
    LICENCIAS.store(bits, Relaxed);
}

/// Licencia de QUANTUM Drummer Pro (no es un efecto: escribe ritmos en una pista).
pub const BIT_DRUMMER: u32 = 16;

/// ¿Está activa la licencia de ese bit?
pub fn tiene_licencia(bit: u32) -> bool {
    LICENCIAS.load(Relaxed) & bit != 0
}

pub fn licenciado(kind: FxKind) -> bool {
    kind.pro().is_none_or(|(bit, ..)| LICENCIAS.load(Relaxed) & bit != 0)
}

/// Modo demo: un segundo de silencio (con fundidos de 30 ms) cada 45 s.
pub fn demo(buf: &mut [[f32; 2]], n: &mut u64, sr: f32) {
    let (periodo, silencio, fundido) = ((45.0 * sr) as u64, (1.0 * sr) as u64, (0.03 * sr).max(1.0));
    for s in buf.iter_mut() {
        let t = *n % periodo;
        *n += 1;
        // El silencio empieza a los 30 s de cada ciclo.
        let inicio = periodo - silencio - (15.0 * sr) as u64;
        if t >= inicio && t < inicio + silencio {
            let dentro = (t - inicio) as f32;
            let g = (1.0 - (dentro / fundido).min(1.0)).max(((dentro - silencio as f32 + fundido) / fundido).clamp(0.0, 1.0));
            *s = [s[0] * g, s[1] * g];
        }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn demo_silencia_un_segundo() {
        let sr = 1000.0;
        let mut n = 0;
        let mut b = vec![[1.0f32; 2]; 45_000];
        demo(&mut b, &mut n, sr);
        let silencio = b.iter().filter(|s| s[0] == 0.0).count();
        assert!((900..=1000).contains(&silencio), "{silencio}");
    }
}
