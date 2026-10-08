//! Dispositivos: nodos de audio de PipeWire (vía `pw-dump`) y puertos MIDI (ALSA seq vía midir).
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

/// Salidas y entradas de audio reales del sistema, USB primero y HDMI al final.
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

pub type MidiConnections = Vec<midir::MidiInputConnection<()>>;

/// Puertos de entrada MIDI (teclados y controladores USB), sin el puerto "Midi Through".
pub fn midi_ports() -> Vec<String> {
    let Ok(m) = midir::MidiInput::new("Quantum DAW") else {
        return vec![];
    };
    m.ports().iter().filter_map(|p| m.port_name(p).ok()).filter(|n| !n.contains("Midi Through")).collect()
}

/// Conecta los puertos MIDI (todos si `only` está vacío, ninguno si es "-") y reenvía notas y controles al motor.
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
                if msg.len() == 3
                    && matches!(msg[0] & 0xF0, 0x80 | 0x90 | 0xB0)
                    && let Ok(mut tx) = tx.lock()
                {
                    let _ = tx.push([msg[0], msg[1], msg[2]]);
                }
            };
            midir::MidiInput::new("Quantum DAW").ok()?.connect(port, "entrada", forward, ()).ok()
        })
        .collect()
}
