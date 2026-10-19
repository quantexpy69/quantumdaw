//! Partitura de la aplicación QUANTUM Partitura: un documento propio (independiente de las pistas)
//! con pentagramas, clave, instrumento y notas en negras.
use serde::{Deserialize, Serialize};

/// Claves disponibles.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Clave {
    #[default]
    Sol,
    Fa,
    /// Do en tercera línea (viola).
    Alto,
    /// Do en cuarta línea (violonchelo y fagot agudos, trombón).
    Tenor,
    /// Sol octava baja (guitarra, voz de tenor).
    Sol8vb,
    Percusion,
}

impl Clave {
    pub const TODAS: [Clave; 6] = [Clave::Sol, Clave::Fa, Clave::Alto, Clave::Tenor, Clave::Sol8vb, Clave::Percusion];

    pub fn nombre(self) -> &'static str {
        match self {
            Clave::Sol => "Sol",
            Clave::Fa => "Fa",
            Clave::Alto => "Do en 3.ª (alto)",
            Clave::Tenor => "Do en 4.ª (tenor)",
            Clave::Sol8vb => "Sol octava baja",
            Clave::Percusion => "Percusión",
        }
    }

    /// Paso diatónico (octava × 7 + letra) de la primera línea (la de abajo).
    pub fn paso_primera_linea(self) -> i32 {
        match self {
            Clave::Sol => 30,    // Mi4
            Clave::Fa => 18,     // Sol2
            Clave::Alto => 24,   // Fa3
            Clave::Tenor => 22,  // Re3
            Clave::Sol8vb => 23, // Mi3
            Clave::Percusion => 30,
        }
    }
}

/// Una nota: inicio y duración en negras, altura MIDI (sonido real) y alteración elegida
/// (-1 bemol, 1 sostenido; None = según la tonalidad).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub struct NotaP {
    pub inicio: f64,
    pub dur: f64,
    pub altura: u8,
    #[serde(default)]
    pub alteracion: Option<i8>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Pentagrama {
    pub nombre: String,
    pub clave: Clave,
    /// Semitonos que se escriben por encima del sonido real (clarinete en Si♭: 2; trompa: 7).
    pub transposicion: i32,
    pub notas: Vec<NotaP>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Partitura {
    pub titulo: String,
    pub compositor: String,
    pub tempo: f32,
    /// Compás (numerador, denominador).
    pub compas: (u32, u32),
    /// Armadura en quintas (-7 a 7).
    pub armadura: i32,
    pub pentagramas: Vec<Pentagrama>,
    /// Cifrado de acordes: (inicio en negras, nombre).
    pub acordes: Vec<(f64, String)>,
}

impl Default for Partitura {
    fn default() -> Self {
        Self {
            titulo: String::new(),
            compositor: String::new(),
            tempo: 120.0,
            compas: (4, 4),
            armadura: 0,
            pentagramas: vec![Pentagrama { nombre: "Piano".into(), ..Default::default() }],
            acordes: vec![],
        }
    }
}

impl Partitura {
    /// Negras por compás.
    pub fn negras_compas(&self) -> f64 {
        self.compas.0 as f64 * 4.0 / self.compas.1.max(1) as f64
    }

    /// Final de la música (en negras).
    pub fn fin(&self) -> f64 {
        self.pentagramas.iter().flat_map(|p| p.notas.iter().map(|n| n.inicio + n.dur)).fold(0.0, f64::max)
    }
}
