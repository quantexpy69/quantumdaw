//! Dispositivos: en Linux, nodos de audio de PipeWire (vía `pw-dump`); en Windows (WASAPI) y macOS
//! (CoreAudio), los dispositivos del sistema vía cpal. Puertos MIDI con midir en todos los sistemas.
use crate::MidiEvent;
use std::sync::{Arc, Mutex};

#[derive(Clone, PartialEq)]
pub struct DeviceInfo {
    /// Nombre del nodo de PipeWire (se usa para enrutar el audio).
    pub id: String,
    pub name: String,
    /// "USB", "Bluetooth", "Integrado", "HDMI" o "Virtual".
    pub kind: &'static str,
    pub input: bool,
}

/// Salidas y entradas de audio del sistema (Windows y macOS): el id es el de cpal.
#[cfg(not(target_os = "linux"))]
pub fn audio_devices() -> Vec<DeviceInfo> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let lista = |input: bool| -> Vec<DeviceInfo> {
        let devs = if input { host.input_devices().map(|d| d.collect::<Vec<_>>()) } else { host.output_devices().map(|d| d.collect::<Vec<_>>()) };
        devs.unwrap_or_default()
            .into_iter()
            .filter_map(|d| {
                let id = d.id().ok()?.to_string();
                let name = d.description().ok().map_or_else(|| id.clone(), |n| n.name().to_string());
                let low = name.to_lowercase();
                let kind = match () {
                    _ if low.contains("bluetooth") || low.contains("airpods") => "Bluetooth",
                    _ if low.contains("hdmi") || low.contains("displayport") => "HDMI",
                    _ if low.contains("usb") => "USB",
                    _ => "Integrado",
                };
                Some(DeviceInfo { id, name, kind, input })
            })
            .collect()
    };
    let mut devices = lista(false);
    devices.extend(lista(true));
    devices
}

/// Salidas y entradas de audio reales del sistema, USB primero y HDMI al final.
#[cfg(target_os = "linux")]
pub fn audio_devices() -> Vec<DeviceInfo> {
    let Ok(out) = std::process::Command::new("pw-dump").output() else {
        return vec![];
    };
    let Ok(serde_json::Value::Array(objs)) = serde_json::from_slice(&out.stdout) else {
        return vec![];
    };
    let mut devices: Vec<DeviceInfo> = objs
        .iter()
        .filter(|o| o["type"] == "PipeWire:Interface:Node")
        .filter_map(|o| {
            let p = &o["info"]["props"];
            let input = match p["media.class"].as_str()? {
                "Audio/Sink" => false,
                "Audio/Source" => true,
                _ => return None,
            };
            let id = p["node.name"].as_str()?.to_string();
            let name = p["node.description"].as_str().unwrap_or(&id).to_string();
            let bus = p["device.bus"].as_str().unwrap_or("");
            let kind = match () {
                _ if id.starts_with("bluez") || bus == "bluetooth" => "Bluetooth",
                _ if id.contains("hdmi") || name.contains("HDMI") || name.contains("DisplayPort") => "HDMI",
                _ if bus == "usb" || id.contains(".usb-") => "USB",
                _ if bus == "pci" => "Integrado",
                _ => "Virtual",
            };
            Some(DeviceInfo { id, name, kind, input })
        })
        .collect();
    devices.sort_by_key(|d| (["USB", "Bluetooth", "Integrado", "Virtual", "HDMI"].iter().position(|k| *k == d.kind), d.name.clone()));
    devices
}

/// Tamaño de bloque que PipeWire impone a todos los programas (`clock.force-quantum`), si lo hay.
/// Lo fijan herramientas como Ubuntu Studio Audio Configuration; con él, el búfer elegido en QUANTUM
/// no puede ser más chico.
#[cfg(target_os = "linux")]
pub fn bloque_forzado() -> Option<u32> {
    let out = std::process::Command::new("pw-metadata").args(["-n", "settings"]).output().ok()?;
    leer_forzado(&String::from_utf8_lossy(&out.stdout))
}

/// `clock.force-quantum` de la salida de `pw-metadata -n settings` (0 = no forzado).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn leer_forzado(texto: &str) -> Option<u32> {
    let linea = texto.lines().find(|l| l.contains("'clock.force-quantum'"))?;
    let valor: u32 = linea.split("value:'").nth(1)?.split('\'').next()?.parse().ok()?;
    (valor > 0).then_some(valor)
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn bloque_forzado_de_pipewire() {
        let con = "Found \"settings\" metadata 32\nupdate: id:0 key:'clock.quantum' value:'1024' type:''\nupdate: id:0 key:'clock.force-quantum' value:'1024' type:''\n";
        assert_eq!(super::leer_forzado(con), Some(1024));
        assert_eq!(super::leer_forzado("update: id:0 key:'clock.force-quantum' value:'0' type:''"), None);
        assert_eq!(super::leer_forzado("update: id:0 key:'clock.quantum' value:'256' type:''"), None);
    }
}

#[cfg(not(target_os = "linux"))]
pub fn bloque_forzado() -> Option<u32> {
    None
}

pub type MidiConnections = Vec<midir::MidiInputConnection<()>>;

/// Puertos de entrada MIDI (teclados y controladores USB), sin el puerto "Midi Through".
pub fn midi_ports() -> Vec<String> {
    let Ok(m) = midir::MidiInput::new("Quantum DAW") else {
        return vec![];
    };
    m.ports().iter().filter_map(|p| m.port_name(p).ok()).filter(|n| !n.contains("Midi Through")).collect()
}

/// Conecta los puertos MIDI (todos si `only` está vacío, ninguno si es "-") y reenvía notas y controles al motor.
/// Notas MIDI con la hora exacta en que llegaron (teclados externos y el teclado del PC), para
/// transcribir sin depender del tamaño del bloque de audio. Guarda las últimas 4096.
static LLEGADAS: std::sync::Mutex<std::collections::VecDeque<(std::time::Instant, MidiEvent)>> = std::sync::Mutex::new(std::collections::VecDeque::new());

pub fn registrar_llegada(e: MidiEvent) {
    if let Ok(mut l) = LLEGADAS.lock() {
        if l.len() >= 4096 {
            l.pop_front();
        }
        l.push_back((std::time::Instant::now(), e));
    }
}

/// Notas llegadas desde `t`: (segundos desde `t`, evento).
pub fn llegadas_desde(t: std::time::Instant) -> Vec<(f64, MidiEvent)> {
    LLEGADAS.lock().map(|l| l.iter().filter(|x| x.0 >= t).map(|x| ((x.0 - t).as_secs_f64(), x.1)).collect()).unwrap_or_default()
}

/// Controles (CC) del teclado: la app los lee para el MIDI Learn y para mover lo que tienen asignado.
static CONTROLES: Mutex<std::collections::VecDeque<MidiEvent>> = Mutex::new(std::collections::VecDeque::new());
/// Números de control asignados en la app (un bit por CC): esos no llegan a los instrumentos.
static TOMADOS: [std::sync::atomic::AtomicU64; 2] = [const { std::sync::atomic::AtomicU64::new(0) }; 2];
/// Avisa a la interfaz que llegó un control (para que se redibuje aunque esté quieta).
static AVISO: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

pub fn al_llegar_control(f: impl Fn() + Send + Sync + 'static) {
    let _ = AVISO.set(Box::new(f));
}

/// Marca qué controles maneja la app; los demás siguen yendo a los instrumentos.
pub fn tomar_controles(ccs: impl IntoIterator<Item = u8>) {
    let mut bits = [0u64; 2];
    ccs.into_iter().for_each(|c| bits[(c as usize >> 6) & 1] |= 1 << (c & 63));
    TOMADOS.iter().zip(bits).for_each(|(t, b)| t.store(b, std::sync::atomic::Ordering::Relaxed));
}

fn tomado(cc: u8) -> bool {
    TOMADOS[(cc as usize >> 6) & 1].load(std::sync::atomic::Ordering::Relaxed) & (1 << (cc & 63)) != 0
}

/// Controles llegados desde la última llamada (los últimos 256 como mucho).
pub fn controles() -> Vec<MidiEvent> {
    CONTROLES.lock().map(|mut c| c.drain(..).collect()).unwrap_or_default()
}

/// Un control del teclado: lo anota para la app y dice si queda en ella (no va a los instrumentos).
fn control(e: MidiEvent) -> bool {
    if let Ok(mut c) = CONTROLES.lock() {
        if c.len() >= 256 {
            c.pop_front();
        }
        c.push_back(e);
    }
    if let Some(f) = AVISO.get() {
        f();
    }
    tomado(e[1])
}

pub fn connect_midi(only: &str, tx: Arc<Mutex<rtrb::Producer<MidiEvent>>>) -> MidiConnections {
    let Ok(probe) = midir::MidiInput::new("Quantum DAW") else {
        return vec![];
    };
    let ports = probe.ports();
    ports
        .iter()
        .filter_map(|port| {
            let name = probe.port_name(port).ok()?;
            if only == "-" || name.contains("Midi Through") || (!only.is_empty() && name != only) {
                return None;
            }
            let tx = tx.clone();
            let forward = move |_: u64, msg: &[u8], _: &mut ()| {
                // Notas, controles y rueda de tono (pitch bend).
                if msg.len() == 3 && matches!(msg[0] & 0xF0, 0x80 | 0x90 | 0xB0 | 0xE0) {
                    let e = [msg[0], msg[1], msg[2]];
                    if e[0] & 0xF0 == 0xB0 && control(e) {
                        return;
                    }
                    registrar_llegada(e);
                    if let Ok(mut tx) = tx.lock() {
                        let _ = tx.push([msg[0], msg[1], msg[2]]);
                    }
                }
            };
            midir::MidiInput::new("Quantum DAW").ok()?.connect(port, "entrada", forward, ()).ok()
        })
        .collect()
}
