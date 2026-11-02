//! Expresión en vivo de los instrumentos propios: rueda de tono (pitch bend), rueda de modulación
//! (vibrato) y pedal de sustain, como los manda un teclado controlador.
use crate::MidiEvent;
use std::f32::consts::TAU;

/// Semitonos de la rueda de tono a fondo, hacia arriba o hacia abajo.
pub const RANGO_BEND: f32 = 2.0;
/// Vibrato con la modulación a fondo: profundidad en semitonos y velocidad en Hz.
const VIBRATO: (f32, f32) = (0.5, 5.5);

#[derive(Default)]
pub struct Expresion {
    /// Rueda de tono de -1 a 1 y modulación de 0 a 1.
    bend: f32,
    modulacion: f32,
    lfo: f32,
    pub pedal: bool,
}

/// Lo que un evento cambió en la expresión.
#[derive(PartialEq, Debug)]
pub enum Cambio {
    Nada,
    /// El evento era de expresión y ya se aplicó.
    Hecho,
    /// Se soltó el pedal: hay que liberar las notas que sostenía.
    PedalSuelto,
}

impl Expresion {
    pub fn evento(&mut self, e: MidiEvent) -> Cambio {
        match (e[0] & 0xF0, e[1]) {
            (0xE0, _) => self.bend = ((e[1] as i32 | (e[2] as i32) << 7) - 8192) as f32 / 8192.0,
            (0xB0, 1) => self.modulacion = e[2] as f32 / 127.0,
            (0xB0, 64) => {
                let antes = std::mem::replace(&mut self.pedal, e[2] >= 64);
                if antes && !self.pedal {
                    return Cambio::PedalSuelto;
                }
            }
            // 121: «reset all controllers».
            (0xB0, 121) => *self = Self { lfo: self.lfo, ..Self::default() },
            _ => return Cambio::Nada,
        }
        Cambio::Hecho
    }

    /// Factor de frecuencia de esta muestra (tono más vibrato); avanza el vibrato.
    pub fn factor(&mut self, sr: f32) -> f32 {
        let mut semitonos = self.bend * RANGO_BEND;
        if self.modulacion > 0.0 {
            self.lfo = (self.lfo + VIBRATO.1 / sr).fract();
            semitonos += self.modulacion * VIBRATO.0 * (TAU * self.lfo).sin();
        }
        if semitonos == 0.0 { 1.0 } else { 2f32.powf(semitonos / 12.0) }
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn rueda_de_tono_y_pedal() {
        let mut x = Expresion::default();
        assert_eq!(x.factor(48_000.0), 1.0);
        assert_eq!(x.evento([0xE0, 0x7F, 0x7F]), Cambio::Hecho);
        assert!((x.factor(48_000.0) - 2f32.powf(2.0 / 12.0)).abs() < 1e-3);
        x.evento([0xE0, 0, 0x40]);
        assert_eq!(x.factor(48_000.0), 1.0);
        assert_eq!(x.evento([0xB0, 64, 127]), Cambio::Hecho);
        assert!(x.pedal);
        assert_eq!(x.evento([0xB0, 64, 0]), Cambio::PedalSuelto);
        assert_eq!(x.evento([0x90, 60, 100]), Cambio::Nada);
    }
}
