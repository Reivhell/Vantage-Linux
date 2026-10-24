use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::sync::Mutex;

use crate::capabilities::find_ite_hidraw;
use crate::types::SimpleResult;

// ---------------------------------------------------------------------------
// Keyboard (ITE HID via hidraw)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyboardZoneCmd {
    pub zone: usize,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct KeyboardAllCmd {
    pub zones: Vec<KeyboardZoneCmd>,
    pub effect: String,
    pub brightness_pct: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LightingInfo {
    pub zones: u32,
    pub default_color: String,
    pub effects: Vec<String>,
}

fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

fn keyboard_effect_code(name: &str) -> u8 {
    match name {
        "Breathing" => 0x03,
        "Wave" => 0x04,
        "Color Shift" => 0x06,
        "Off" => 0x00,
        _ => 0x01,
    }
}

#[tauri::command]
pub fn cmd_get_lighting_info() -> LightingInfo {
    LightingInfo {
        zones: 4,
        default_color: "#ffb454".into(),
        effects: vec![
            "Static".into(),
            "Breathing".into(),
            "Wave".into(),
            "Color Shift".into(),
            "Off".into(),
        ],
    }
}

#[derive(Debug, Clone)]
struct KeyboardState {
    colors: [(u8, u8, u8); 4],
    effect: u8,
    brightness: u8,
}

static KEYBOARD_STATE: Mutex<Option<KeyboardState>> = Mutex::new(None);

fn keyboard_state() -> KeyboardState {
    KEYBOARD_STATE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or(KeyboardState {
            colors: [(0xff, 0xb4, 0x54); 4],
            effect: 0x01,
            brightness: 100,
        })
}

fn keyboard_report(effect: u8, speed: u8, brightness: u8, colors: &[(u8, u8, u8); 4]) -> Vec<u8> {
    let mut data = vec![0u8; 33];
    data[0] = 0xCC;
    data[1] = 0x16;
    data[2] = effect;
    data[3] = speed;
    data[4] = brightness;
    for (i, (r, g, b)) in colors.iter().enumerate() {
        let off = 5 + i * 3;
        if off + 2 < data.len() {
            data[off] = *r;
            data[off + 1] = *g;
            data[off + 2] = *b;
        }
    }
    data
}

fn write_hidraw(report: &[u8]) -> SimpleResult {
    let path = match find_ite_hidraw().map(|(p, _, _)| p) {
        Some(p) => p,
        None => {
            return SimpleResult {
                ok: false,
                message: "ITE keyboard HID device not found".into(),
            }
        }
    };
    let mut file = match fs::OpenOptions::new().write(true).read(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            return SimpleResult {
                ok: false,
                message: format!("cannot open {path:?}: {e}"),
            }
        }
    };
    file.write_all(report)
        .map(|_| SimpleResult {
            ok: true,
            message: "ok".into(),
        })
        .unwrap_or_else(|e| SimpleResult {
            ok: false,
            message: e.to_string(),
        })
}

#[tauri::command]
pub fn cmd_keyboard_zone(cmd: KeyboardZoneCmd) -> SimpleResult {
    if cmd.zone >= 4 {
        return SimpleResult {
            ok: false,
            message: "zone must be 0-3".into(),
        };
    }
    let color = match parse_hex_color(&cmd.color) {
        Some(c) => c,
        None => {
            return SimpleResult {
                ok: false,
                message: "colour must be #rrggbb".into(),
            }
        }
    };
    let mut state = keyboard_state();
    state.colors[cmd.zone] = color;
    if state.effect == 0 || state.brightness == 0 {
        state.effect = 0x01;
        state.brightness = 100;
    }
    let report = keyboard_report(state.effect, 0x01, state.brightness, &state.colors);
    let res = write_hidraw(&report);
    if res.ok {
        *KEYBOARD_STATE.lock().unwrap() = Some(state);
    }
    res
}

#[tauri::command]
pub fn cmd_keyboard_all(cmd: KeyboardAllCmd) -> SimpleResult {
    let mut colors = [(0, 0, 0); 4];
    for z in &cmd.zones {
        if z.zone >= 4 {
            return SimpleResult {
                ok: false,
                message: "zone must be 0-3".into(),
            };
        }
        match parse_hex_color(&z.color) {
            Some(color) => colors[z.zone] = color,
            None => {
                return SimpleResult {
                    ok: false,
                    message: "colour must be #rrggbb".into(),
                }
            }
        }
    }
    let effect = keyboard_effect_code(&cmd.effect);
    let brightness = if cmd.effect == "Off" {
        0
    } else {
        cmd.brightness_pct.min(100)
    };
    let report = keyboard_report(effect, 0x01, brightness, &colors);
    let res = write_hidraw(&report);
    if res.ok {
        *KEYBOARD_STATE.lock().unwrap() = Some(KeyboardState {
            colors,
            effect,
            brightness,
        });
    }
    res
}

#[tauri::command]
pub fn cmd_keyboard_off() -> SimpleResult {
    let mut state = keyboard_state();
    state.effect = 0x00;
    state.brightness = 0;
    let report = keyboard_report(0x00, 0x00, 0x00, &state.colors);
    let res = write_hidraw(&report);
    if res.ok {
        *KEYBOARD_STATE.lock().unwrap() = Some(state);
    }
    res
}
