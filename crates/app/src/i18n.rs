//! Idioma de la interfaz: español (por defecto), inglés y portugués. Las traducciones están en
//! `i18n.tsv` (español → inglés → portugués); lo que no está en la tabla se muestra en español.
use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering::Relaxed},
    },
};

pub const LANGS: [&str; 3] = ["Español", "English", "Português"];
pub static LANG: AtomicU8 = AtomicU8::new(0);

fn table() -> &'static HashMap<&'static str, [&'static str; 2]> {
    static T: OnceLock<HashMap<&'static str, [&'static str; 2]>> = OnceLock::new();
    T.get_or_init(|| {
        include_str!("i18n.tsv")
            .lines()
            .filter_map(|l| {
                let mut c = l.split('\t');
                Some((c.next()?, [c.next()?, c.next()?]))
            })
            .collect()
    })
}

/// Traduce un texto de la interfaz al idioma elegido.
pub fn tr(s: &str) -> &str {
    match LANG.load(Relaxed) {
        0 => s,
        l => table().get(s).map_or(s, |t| t[l as usize - 1]),
    }
}

pub fn set(lang: u8) {
    LANG.store(lang.min(2), Relaxed)
}

#[cfg(test)]
mod tests {
    #[test]
    fn table_complete() {
        assert!(include_str!("i18n.tsv").lines().all(|l| l.split('\t').count() == 3));
        super::set(1);
        assert_eq!(super::tr("Archivo"), "File");
        assert_eq!(super::tr("texto sin traducir"), "texto sin traducir");
        super::set(0);
    }
}
