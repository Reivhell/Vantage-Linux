use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayOutput {
    pub name: String,
    pub primary: bool,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    pub max_refresh_hz: f64,
    pub modes: Vec<DisplayMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub server: String,
    pub adapter: String,
    pub outputs: Vec<DisplayOutput>,
    pub brightness_device: Option<String>,
    pub brightness_pct: Option<u32>,
    pub brightness_max: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayModeChoice {
    pub width: u32,
    pub height: u32,
    pub refresh_hz: f64,
    pub label: String,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayModeGroup {
    pub width: u32,
    pub height: u32,
    pub resolution_label: String,
    pub max_refresh_hz: f64,
    pub max_refresh_label: String,
    pub modes: Vec<DisplayModeChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayOutputView {
    pub name: String,
    pub primary: bool,
    pub current_width: u32,
    pub current_height: u32,
    pub current_refresh_hz: f64,
    pub current_resolution_label: String,
    pub current_refresh_label: String,
    pub max_refresh_hz: f64,
    pub max_refresh_label: String,
    pub groups: Vec<DisplayModeGroup>,
    pub selected_group: usize,
    pub selected_mode: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayMetaRow {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayView {
    pub server: String,
    pub adapter: String,
    pub outputs: Vec<DisplayOutputView>,
    pub refresh_ceiling_hz: f64,
    pub refresh_ceiling_label: String,
    pub brightness_device: Option<String>,
    pub brightness_pct: Option<u32>,
    pub brightness_max: Option<u32>,
    pub brightness_label: String,
    pub summary: Vec<DisplayMetaRow>,
    pub error: Option<String>,
}

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let bin = crate::which(cmd).ok_or_else(|| format!("{cmd} not found"))?;
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            format!("{cmd} failed with {}", out.status)
        } else {
            err
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_default()
}

fn wayland() -> bool {
    !env("WAYLAND_DISPLAY").is_empty() || env("XDG_SESSION_TYPE").eq_ignore_ascii_case("wayland")
}

pub fn detect_adapter() -> String {
    let desktop = env("XDG_CURRENT_DESKTOP").to_uppercase();
    if wayland() {
        if !env("HYPRLAND_INSTANCE_SIGNATURE").is_empty() && crate::which("hyprctl").is_some() {
            return "hyprland".into();
        }
        if desktop.contains("KDE") && crate::which("kscreen-doctor").is_some() {
            return "kde".into();
        }
        if desktop.contains("GNOME")
            && (crate::which("gnome-randr").is_some() || crate::which("gdctl").is_some())
        {
            return "gnome".into();
        }
        if crate::which("swaymsg").is_some() {
            return "sway".into();
        }
        if crate::which("wlr-randr").is_some() {
            return "wlroots".into();
        }
    }
    if !env("DISPLAY").is_empty() && crate::which("xrandr").is_some() {
        return "x11".into();
    }
    if desktop.contains("KDE") && crate::which("kscreen-doctor").is_some() {
        return "kde".into();
    }
    if crate::which("xrandr").is_some() {
        return "x11".into();
    }
    "unknown".into()
}

fn parse_geometry(token: &str) -> Option<(u32, u32)> {
    let base = token.split('+').next()?;
    let base = base.rsplit(':').next()?;
    let base = base.split('@').next()?;
    let (w, h) = base.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

fn parse_refresh(token: &str) -> Option<f64> {
    let value = token.trim_end_matches(|c: char| !(c.is_ascii_digit() || c == '.'));
    value.parse().ok()
}

fn parse_xrandr(text: &str) -> Vec<DisplayOutput> {
    let mut outputs: Vec<DisplayOutput> = Vec::new();
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) && line.contains(" connected") {
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() < 2 {
                continue;
            }
            let geometry = tokens.iter().find(|t| parse_geometry(t).is_some());
            let (width, height) = geometry.and_then(|g| parse_geometry(g)).unwrap_or((0, 0));
            outputs.push(DisplayOutput {
                name: tokens[0].to_string(),
                primary: tokens.iter().any(|t| *t == "primary"),
                width,
                height,
                refresh_hz: 0.0,
                max_refresh_hz: 0.0,
                modes: Vec::new(),
            });
        } else if line.starts_with(char::is_whitespace) {
            let Some(output) = outputs.last_mut() else {
                continue;
            };
            let tokens: Vec<&str> = line.split_whitespace().collect();
            let Some((width, height)) = tokens.first().and_then(|t| parse_geometry(t)) else {
                continue;
            };
            let Some(refresh) = tokens.get(1).and_then(|t| parse_refresh(t)) else {
                continue;
            };
            let current = tokens.get(1).is_some_and(|t| t.contains('*'));
            if output.width == 0 {
                output.width = width;
                output.height = height;
            }
            if current {
                output.width = width;
                output.height = height;
                output.refresh_hz = refresh;
            }
            if !output.modes.iter().any(|m| {
                m.width == width && m.height == height && (m.refresh_hz - refresh).abs() < 0.01
            }) {
                output.modes.push(DisplayMode {
                    width,
                    height,
                    refresh_hz: refresh,
                });
            }
        }
    }
    for output in &mut outputs {
        output.modes.sort_by(|a, b| {
            (b.width, b.height)
                .cmp(&(a.width, a.height))
                .then(b.refresh_hz.total_cmp(&a.refresh_hz))
        });
    }
    outputs
}

fn json_num(value: &serde_json::Value, key: &str) -> f64 {
    value.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> &'a str {
    value.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn parse_sway(text: &str) -> Option<Vec<DisplayOutput>> {
    let list: Vec<serde_json::Value> = serde_json::from_str(text).ok()?;
    let mut outputs = Vec::new();
    for item in list {
        if item.get("active").and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        let modes = item
            .get("modes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|m| DisplayMode {
                        width: json_num(m, "width") as u32,
                        height: json_num(m, "height") as u32,
                        refresh_hz: (json_num(m, "refresh") / 1000.0 * 100.0).round() / 100.0,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let current = item.get("current_mode").cloned().unwrap_or_default();
        outputs.push(DisplayOutput {
            name: json_str(&item, "name").to_string(),
            primary: false,
            width: json_num(&current, "width") as u32,
            height: json_num(&current, "height") as u32,
            refresh_hz: (json_num(&current, "refresh") / 1000.0 * 100.0).round() / 100.0,
            max_refresh_hz: 0.0,
            modes,
        });
    }
    Some(outputs)
}

fn parse_wlr(text: &str) -> Option<Vec<DisplayOutput>> {
    let list: Vec<serde_json::Value> = serde_json::from_str(text).ok()?;
    let mut outputs = Vec::new();
    for item in list {
        if item.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
            continue;
        }
        let modes = item
            .get("modes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|m| DisplayMode {
                        width: json_num(m, "width") as u32,
                        height: json_num(m, "height") as u32,
                        refresh_hz: (json_num(m, "refresh") / 1000.0 * 100.0).round() / 100.0,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let current = item.get("current_mode").cloned().unwrap_or_default();
        outputs.push(DisplayOutput {
            name: json_str(&item, "name").to_string(),
            primary: false,
            width: json_num(&current, "width") as u32,
            height: json_num(&current, "height") as u32,
            refresh_hz: (json_num(&current, "refresh") / 1000.0 * 100.0).round() / 100.0,
            max_refresh_hz: 0.0,
            modes,
        });
    }
    Some(outputs)
}

fn parse_hypr(text: &str) -> Option<Vec<DisplayOutput>> {
    let list: Vec<serde_json::Value> = serde_json::from_str(text).ok()?;
    let mut outputs = Vec::new();
    for item in list {
        let modes = item
            .get("availableModes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        let raw = m.as_str()?;
                        let (geometry, refresh) = raw.split_once('@')?;
                        let (w, h) = geometry.split_once('x')?;
                        Some(DisplayMode {
                            width: w.parse().ok()?,
                            height: h.parse().ok()?,
                            refresh_hz: refresh.trim_end_matches("Hz").parse().ok()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        outputs.push(DisplayOutput {
            name: json_str(&item, "name").to_string(),
            primary: item
                .get("focused")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            width: json_num(&item, "width") as u32,
            height: json_num(&item, "height") as u32,
            refresh_hz: (json_num(&item, "refreshRate") * 100.0).round() / 100.0,
            max_refresh_hz: 0.0,
            modes,
        });
    }
    Some(outputs)
}

fn parse_kde(text: &str) -> Vec<DisplayOutput> {
    let mut outputs: Vec<DisplayOutput> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Output:") {
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            if let Some(name) = tokens.get(1) {
                outputs.push(DisplayOutput {
                    name: name.to_string(),
                    primary: false,
                    width: 0,
                    height: 0,
                    refresh_hz: 0.0,
                    max_refresh_hz: 0.0,
                    modes: Vec::new(),
                });
            }
            continue;
        }
        let Some(output) = outputs.last_mut() else {
            continue;
        };
        if trimmed.starts_with("Geometry:") {
            if let Some((w, h)) = parse_geometry(trimmed.split_whitespace().last().unwrap_or("")) {
                output.width = w;
                output.height = h;
            }
        } else if trimmed.contains('x') && trimmed.contains('@') {
            for token in trimmed
                .split_whitespace()
                .filter(|t| t.contains('x') && t.contains('@'))
            {
                let clean = token.trim_end_matches(['*', '+', '!']);
                let Some((geometry, refresh)) = clean.split_once('@') else {
                    continue;
                };
                let Some((w, h)) = parse_geometry(geometry) else {
                    continue;
                };
                let hz = parse_refresh(refresh).unwrap_or(0.0);
                if token.contains('*') {
                    output.width = w;
                    output.height = h;
                    output.refresh_hz = hz;
                }
                if !output
                    .modes
                    .iter()
                    .any(|m| m.width == w && m.height == h && (m.refresh_hz - hz).abs() < 0.01)
                {
                    output.modes.push(DisplayMode {
                        width: w,
                        height: h,
                        refresh_hz: hz,
                    });
                }
            }
        }
    }
    outputs
}

fn parse_gnome(text: &str) -> Vec<DisplayOutput> {
    let mut outputs: Vec<DisplayOutput> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        if !line.starts_with(char::is_whitespace) {
            // gnome-randr also prints config keys (layout-mode, supported-color-modes,
            // ...) at the top level. Only connector headers identify a monitor.
            if tokens.len() >= 2
                && tokens[0].contains('-')
                && !tokens[0].contains(':')
                && !tokens[0].contains('x')
            {
                outputs.push(DisplayOutput {
                    name: tokens[0].to_string(),
                    primary: outputs.is_empty(),
                    width: 0,
                    height: 0,
                    refresh_hz: 0.0,
                    max_refresh_hz: 0.0,
                    modes: Vec::new(),
                });
            }
            continue;
        }

        let Some(output) = outputs.last_mut() else {
            continue;
        };
        let parsed = parse_mode_token(tokens[0]).or_else(|| {
            let (width, height) = parse_geometry(tokens[0])?;
            let refresh = tokens.get(1).and_then(|token| parse_refresh(token))?;
            Some((width, height, refresh))
        });
        let Some((width, height, refresh)) = parsed else {
            continue;
        };
        let current = tokens.iter().skip(1).any(|token| token.contains('*'));
        if output.width == 0 {
            output.width = width;
            output.height = height;
        }
        if current {
            output.width = width;
            output.height = height;
            output.refresh_hz = refresh;
        }
        if !output.modes.iter().any(|m| {
            m.width == width && m.height == height && (m.refresh_hz - refresh).abs() < 0.01
        }) {
            output.modes.push(DisplayMode {
                width,
                height,
                refresh_hz: refresh,
            });
        }
    }
    outputs
}

fn parse_mode_token(token: &str) -> Option<(u32, u32, f64)> {
    let start = token.find(|c: char| c.is_ascii_digit())?;
    let token = &token[start..];
    let token = token.trim_end_matches("+vrr").trim_end_matches(['*', '+']);
    let (geometry, refresh) = token.split_once('@')?;
    let (width, height) = parse_geometry(geometry)?;
    let refresh = refresh.trim_end_matches("Hz").parse().ok()?;
    Some((width, height, refresh))
}

fn parse_gdctl(show: &str, modes_text: &str) -> Vec<DisplayOutput> {
    let mut outputs: Vec<DisplayOutput> = Vec::new();
    let mut current_monitor: Option<usize> = None;
    let mut expecting_current = false;
    for line in show.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Logical monitors:") {
            break;
        }
        if let Some(pos) = trimmed.find("Monitor ") {
            let rest = &trimmed[pos + "Monitor ".len()..];
            let name = rest.split_whitespace().next().unwrap_or("");
            if !name.is_empty() && !trimmed.starts_with("Logical") {
                outputs.push(DisplayOutput {
                    name: name.to_string(),
                    primary: outputs.is_empty(),
                    width: 0,
                    height: 0,
                    refresh_hz: 0.0,
                    max_refresh_hz: 0.0,
                    modes: Vec::new(),
                });
                current_monitor = Some(outputs.len() - 1);
            }
            continue;
        }
        if trimmed.contains("Current mode") {
            expecting_current = true;
            continue;
        }
        if expecting_current {
            if let Some((w, h, hz)) = parse_mode_token(trimmed) {
                if let Some(index) = current_monitor {
                    outputs[index].width = w;
                    outputs[index].height = h;
                    outputs[index].refresh_hz = hz;
                    outputs[index].modes.push(DisplayMode {
                        width: w,
                        height: h,
                        refresh_hz: hz,
                    });
                }
                expecting_current = false;
            }
        }
    }

    let mut current_monitor: Option<usize> = None;
    for line in modes_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Logical monitors:") {
            break;
        }
        if let Some(pos) = trimmed.find("Monitor ") {
            let rest = &trimmed[pos + "Monitor ".len()..];
            let name = rest.split_whitespace().next().unwrap_or("");
            current_monitor = outputs.iter().position(|o| o.name == name);
            continue;
        }
        if let Some(index) = current_monitor {
            if let Some((w, h, hz)) = parse_mode_token(trimmed) {
                if !outputs[index]
                    .modes
                    .iter()
                    .any(|m| m.width == w && m.height == h && (m.refresh_hz - hz).abs() < 0.01)
                {
                    outputs[index].modes.push(DisplayMode {
                        width: w,
                        height: h,
                        refresh_hz: hz,
                    });
                }
            }
        }
    }

    for output in &mut outputs {
        output.modes.sort_by(|a, b| {
            (b.width, b.height)
                .cmp(&(a.width, a.height))
                .then(b.refresh_hz.total_cmp(&a.refresh_hz))
        });
    }
    outputs
}

#[derive(Debug, Default)]
struct GdctlLogical {
    x: i32,
    y: i32,
    scale: f64,
    transform: String,
    primary: bool,
    monitors: Vec<String>,
}

fn parse_gdctl_logical(text: &str) -> Vec<GdctlLogical> {
    let mut logicals = Vec::new();
    let mut in_logical = false;
    let mut in_monitors = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Logical monitors:") {
            in_logical = true;
            continue;
        }
        if !in_logical {
            continue;
        }
        if trimmed.find("Logical monitor #").is_some() {
            in_monitors = false;
            logicals.push(GdctlLogical {
                scale: 1.0,
                transform: "normal".into(),
                ..GdctlLogical::default()
            });
            continue;
        }
        let Some(logical) = logicals.last_mut() else {
            continue;
        };
        if let Some(pos) = trimmed.find("Position:") {
            let rest = trimmed[pos + "Position:".len()..].trim();
            let values = rest.trim_matches(['(', ')']);
            let mut parts = values.split(',');
            if let (Some(x), Some(y)) = (parts.next(), parts.next()) {
                logical.x = x.trim().parse().unwrap_or(0);
                logical.y = y.trim().parse().unwrap_or(0);
            }
            in_monitors = false;
            continue;
        }
        if let Some(pos) = trimmed.find("Scale:") {
            logical.scale = trimmed[pos + "Scale:".len()..]
                .trim()
                .parse()
                .unwrap_or(1.0);
            in_monitors = false;
            continue;
        }
        if let Some(pos) = trimmed.find("Transform:") {
            logical.transform = trimmed[pos + "Transform:".len()..].trim().to_string();
            in_monitors = false;
            continue;
        }
        if let Some(pos) = trimmed.find("Primary:") {
            logical.primary = trimmed[pos + "Primary:".len()..]
                .trim()
                .eq_ignore_ascii_case("yes");
            in_monitors = false;
            continue;
        }
        if trimmed.find("Monitors:").is_some() {
            in_monitors = true;
            continue;
        }
        if in_monitors {
            let name = trimmed.trim_start_matches(|c: char| {
                c.is_whitespace() || matches!(c, '│' | '├' | '└' | '─')
            });
            let name = name.split_whitespace().next().unwrap_or("");
            if name.contains('-') {
                logical.monitors.push(name.to_string());
            }
        }
    }
    logicals
}

fn gdctl_outputs() -> Result<Vec<DisplayOutput>, String> {
    let show = run("gdctl", &["show"])?;
    let modes = run("gdctl", &["show", "--modes"])?;
    Ok(parse_gdctl(&show, &modes))
}

fn gdctl_set_mode(output: &str, width: u32, height: u32, refresh: f64) -> Result<String, String> {
    let show = run("gdctl", &["show"])?;
    let logicals = parse_gdctl_logical(&show);
    if logicals.is_empty() {
        return Err("could not read the GNOME logical monitor layout".into());
    }
    if !logicals
        .iter()
        .any(|logical| logical.monitors.iter().any(|monitor| monitor == output))
    {
        return Err(format!("{output} is not in the GNOME monitor layout"));
    }
    let outputs = gdctl_outputs()?;
    let mut args: Vec<String> = vec!["set".into(), "--persistent".into()];
    for logical in &logicals {
        if logical.monitors.is_empty() {
            return Err("GNOME logical monitor has no connector".into());
        }
        args.push("--logical-monitor".into());
        for monitor in &logical.monitors {
            args.push("--monitor".into());
            args.push(monitor.clone());
            let mode = if monitor == output {
                format!("{width}x{height}@{refresh:.3}")
            } else {
                let found = outputs
                    .iter()
                    .find(|item| &item.name == monitor)
                    .ok_or_else(|| format!("missing current mode for {monitor}"))?;
                if found.width == 0 || found.height == 0 || found.refresh_hz <= 0.0 {
                    return Err(format!("cannot preserve the current mode of {monitor}"));
                }
                format!("{}x{}@{:.3}", found.width, found.height, found.refresh_hz)
            };
            args.push("--mode".into());
            args.push(mode);
        }
        if logical.primary {
            args.push("--primary".into());
        }
        args.push("--scale".into());
        args.push(format!("{:.2}", logical.scale));
        args.push("--x".into());
        args.push(logical.x.to_string());
        args.push("--y".into());
        args.push(logical.y.to_string());
        args.push("--transform".into());
        args.push(logical.transform.clone());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run("gdctl", &refs)
}

fn finalize_outputs(mut outputs: Vec<DisplayOutput>) -> Vec<DisplayOutput> {
    for output in &mut outputs {
        output.max_refresh_hz = output
            .modes
            .iter()
            .map(|mode| mode.refresh_hz)
            .filter(|hz| hz.is_finite() && *hz > 0.0)
            .fold(0.0, f64::max);
    }
    outputs
}

fn matching_mode<'a>(
    output: &'a DisplayOutput,
    width: u32,
    height: u32,
    refresh: f64,
) -> Option<&'a DisplayMode> {
    output
        .modes
        .iter()
        .filter(|mode| mode.width == width && mode.height == height)
        .min_by(|a, b| {
            (a.refresh_hz - refresh)
                .abs()
                .total_cmp(&(b.refresh_hz - refresh).abs())
        })
        .filter(|mode| (mode.refresh_hz - refresh).abs() < 0.06)
}

fn read_brightness() -> (Option<String>, Option<u32>, Option<u32>) {
    let Ok(entries) = fs::read_dir("/sys/class/backlight") else {
        return (None, None, None);
    };
    let Some(entry) = entries.flatten().next() else {
        return (None, None, None);
    };
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().to_string();
    let current = fs::read_to_string(path.join("brightness"))
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok());
    let max = fs::read_to_string(path.join("max_brightness"))
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok());
    let pct = match (current, max) {
        (Some(c), Some(m)) if m > 0 => Some((c as f64 / m as f64 * 100.0).round() as u32),
        _ => None,
    };
    (Some(name), pct, max)
}

fn hz_label(hz: f64) -> String {
    if !hz.is_finite() || hz <= 0.0 {
        return "--".into();
    }
    let rounded = hz.round();
    if (hz - rounded).abs() < 0.5 {
        return format!("{rounded:.0}");
    }
    format!("{hz:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn near_hz(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.06
}

fn build_output_view(output: &DisplayOutput) -> DisplayOutputView {
    let mut groups: Vec<(u32, u32, Vec<DisplayMode>)> = Vec::new();
    for mode in &output.modes {
        if mode.width == 0
            || mode.height == 0
            || !mode.refresh_hz.is_finite()
            || mode.refresh_hz <= 0.0
        {
            continue;
        }
        let group = match groups
            .iter_mut()
            .find(|(width, height, _)| *width == mode.width && *height == mode.height)
        {
            Some((_, _, modes)) => modes,
            None => {
                groups.push((mode.width, mode.height, Vec::new()));
                &mut groups.last_mut().unwrap().2
            }
        };
        let bucket = mode.refresh_hz.round() as i64;
        match group
            .iter()
            .position(|item| item.refresh_hz.round() as i64 == bucket)
        {
            Some(index) => {
                let previous_delta = (group[index].refresh_hz - bucket as f64).abs();
                let next_delta = (mode.refresh_hz - bucket as f64).abs();
                if next_delta < previous_delta {
                    group[index] = mode.clone();
                }
            }
            None => group.push(mode.clone()),
        }
    }

    groups.sort_by(|a, b| (b.0 * b.1).cmp(&(a.0 * a.1)).then_with(|| b.0.cmp(&a.0)));

    let views: Vec<DisplayModeGroup> = groups
        .into_iter()
        .map(|(width, height, mut modes)| {
            modes.sort_by(|a, b| b.refresh_hz.total_cmp(&a.refresh_hz));
            let max_refresh_hz = modes.iter().map(|mode| mode.refresh_hz).fold(0.0, f64::max);
            let choices = modes
                .into_iter()
                .map(|mode| DisplayModeChoice {
                    width: mode.width,
                    height: mode.height,
                    refresh_hz: mode.refresh_hz,
                    label: hz_label(mode.refresh_hz),
                    current: mode.width == output.width
                        && mode.height == output.height
                        && near_hz(mode.refresh_hz, output.refresh_hz),
                })
                .collect();
            DisplayModeGroup {
                width,
                height,
                resolution_label: format!("{width} × {height}"),
                max_refresh_hz,
                max_refresh_label: hz_label(max_refresh_hz),
                modes: choices,
            }
        })
        .collect();

    let selected_group = views
        .iter()
        .position(|group| group.width == output.width && group.height == output.height)
        .unwrap_or(0);
    let selected_mode = views
        .get(selected_group)
        .and_then(|group| group.modes.iter().position(|mode| mode.current))
        .unwrap_or(0);
    let max_refresh_hz = output.max_refresh_hz.max(
        views
            .iter()
            .map(|group| group.max_refresh_hz)
            .fold(0.0, f64::max),
    );

    DisplayOutputView {
        name: output.name.clone(),
        primary: output.primary,
        current_width: output.width,
        current_height: output.height,
        current_refresh_hz: output.refresh_hz,
        current_resolution_label: format!("{} × {}", output.width, output.height),
        current_refresh_label: hz_label(output.refresh_hz),
        max_refresh_hz,
        max_refresh_label: hz_label(max_refresh_hz),
        groups: views,
        selected_group,
        selected_mode,
    }
}

pub fn read_display_view() -> DisplayView {
    let info = read_display_info_cached();
    let outputs: Vec<DisplayOutputView> = info.outputs.iter().map(build_output_view).collect();
    let refresh_ceiling_hz = outputs
        .iter()
        .map(|output| output.max_refresh_hz)
        .fold(0.0, f64::max);
    let brightness_label = info
        .brightness_pct
        .map(|pct| format!("{pct}%"))
        .unwrap_or_else(|| "--".into());
    let summary = vec![
        DisplayMetaRow {
            label: "Session".into(),
            value: info.server.clone(),
        },
        DisplayMetaRow {
            label: "Adapter".into(),
            value: info.adapter.clone(),
        },
        DisplayMetaRow {
            label: "Refresh ceiling".into(),
            value: if refresh_ceiling_hz > 0.0 {
                format!("{} Hz", hz_label(refresh_ceiling_hz))
            } else {
                "--".into()
            },
        },
        DisplayMetaRow {
            label: "Brightness".into(),
            value: brightness_label.clone(),
        },
    ];
    DisplayView {
        server: info.server,
        adapter: info.adapter,
        outputs,
        refresh_ceiling_hz,
        refresh_ceiling_label: hz_label(refresh_ceiling_hz),
        brightness_device: info.brightness_device,
        brightness_pct: info.brightness_pct,
        brightness_max: info.brightness_max,
        brightness_label,
        summary,
        error: info.error,
    }
}

struct DisplayInfoCache {
    info: Option<(SystemTime, DisplayInfo)>,
    ttl_seconds: f64,
}

static DISPLAY_INFO_CACHE: OnceLock<Mutex<DisplayInfoCache>> = OnceLock::new();

fn display_info_cache() -> &'static Mutex<DisplayInfoCache> {
    DISPLAY_INFO_CACHE.get_or_init(|| {
        Mutex::new(DisplayInfoCache {
            info: None,
            ttl_seconds: 2.0,
        })
    })
}

/// Cache-aware wrapper around display detection. Re-detects only when the TTL
/// expires or on explicit invalidation (called by set_display_mode).
pub fn read_display_info_cached() -> DisplayInfo {
    let now = SystemTime::now();
    let mut guard = display_info_cache().lock().unwrap();
    if let Some((cached_at, info)) = &guard.info {
        if now
            .duration_since(*cached_at)
            .unwrap_or_default()
            .as_secs_f64()
            < guard.ttl_seconds
        {
            return info.clone();
        }
    }
    let info = read_display_info_inner();
    guard.info = Some((now, info.clone()));
    info
}

pub fn invalidate_display_cache() {
    let mut guard = display_info_cache().lock().unwrap();
    guard.info = None;
}

pub fn read_display_info() -> DisplayInfo {
    read_display_info_inner()
}

fn read_display_info_inner() -> DisplayInfo {
    let adapter = detect_adapter();
    let (device, brightness, max) = read_brightness();
    let mut info = DisplayInfo {
        server: if wayland() {
            "wayland".into()
        } else {
            "x11".into()
        },
        adapter: adapter.clone(),
        outputs: Vec::new(),
        brightness_device: device,
        brightness_pct: brightness,
        brightness_max: max,
        error: None,
    };
    let result = match adapter.as_str() {
        "x11" => run("xrandr", &["--query"]).map(|t| parse_xrandr(&t)),
        "sway" => run("swaymsg", &["-t", "get_outputs", "-r"])
            .and_then(|t| parse_sway(&t).ok_or_else(|| "failed to parse sway output".to_string())),
        "wlroots" => run("wlr-randr", &["--json"]).and_then(|t| {
            parse_wlr(&t).ok_or_else(|| "failed to parse wlr-randr output".to_string())
        }),
        "hyprland" => run("hyprctl", &["monitors", "-j"]).and_then(|t| {
            parse_hypr(&t).ok_or_else(|| "failed to parse hyprctl output".to_string())
        }),
        "kde" => run("kscreen-doctor", &["-o"]).map(|t| parse_kde(&t)),
        "gnome" => (|| {
            if crate::which("gdctl").is_some() {
                gdctl_outputs()
            } else if crate::which("gnome-randr").is_some() {
                run("gnome-randr", &[]).map(|t| parse_gnome(&t))
            } else {
                Err("install gdctl or gnome-randr for GNOME display control".into())
            }
        })(),
        other => Err(format!("no display adapter for '{other}'")),
    };
    match result {
        Ok(outputs) => info.outputs = finalize_outputs(outputs),
        Err(e) => info.error = Some(e),
    }
    info
}

pub fn set_display_mode(
    output: &str,
    width: u32,
    height: u32,
    refresh: f64,
) -> crate::SimpleResult {
    if output.is_empty() || output.contains(' ') {
        return crate::SimpleResult {
            ok: false,
            message: "invalid output name".into(),
        };
    }
    if width == 0 || height == 0 || !refresh.is_finite() || refresh <= 0.0 {
        return crate::SimpleResult {
            ok: false,
            message: "invalid display mode".into(),
        };
    }
    let info = read_display_info_cached();
    let Some(target) = info.outputs.iter().find(|item| item.name == output) else {
        return crate::SimpleResult {
            ok: false,
            message: format!("{output} is not reported by the active display adapter"),
        };
    };
    if target.modes.is_empty() {
        return crate::SimpleResult {
            ok: false,
            message: format!("{output} reports no supported modes"),
        };
    }
    let Some(supported) = matching_mode(target, width, height, refresh) else {
        return crate::SimpleResult {
            ok: false,
            message: format!("{output} does not report {width}x{height} at {refresh:.2} Hz"),
        };
    };
    let refresh = supported.refresh_hz;
    let adapter = detect_adapter();
    let mode = format!("{width}x{height}");
    let mode_two = format!("{width}x{height}@{refresh:.2}");
    let mode_three = format!("{width}x{height}@{refresh:.3}");
    let mode_sway = format!("{width}x{height}@{refresh:.3}Hz");
    let result = match adapter.as_str() {
        "x11" => run(
            "xrandr",
            &[
                "--output",
                output,
                "--mode",
                &mode,
                "--rate",
                &format!("{refresh:.2}"),
            ],
        ),
        "sway" => run("swaymsg", &["output", output, "mode", &mode_sway]),
        "wlroots" => run("wlr-randr", &["--output", output, "--mode", &mode_sway]),
        "hyprland" => {
            let monitors = run("hyprctl", &["monitors", "-j"])
                .ok()
                .and_then(|t| serde_json::from_str::<Vec<serde_json::Value>>(&t).ok())
                .and_then(|list| list.into_iter().find(|m| json_str(m, "name") == output));
            match monitors {
                Some(monitor) => {
                    let x = json_num(&monitor, "x") as i64;
                    let y = json_num(&monitor, "y") as i64;
                    let scale = monitor.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    let spec = format!("{output},{mode_two},{x}x{y},{scale}");
                    run("hyprctl", &["keyword", "monitor", &spec])
                }
                None => Err("output not found".into()),
            }
        }
        "kde" => run(
            "kscreen-doctor",
            &[&format!("output.{output}.mode.{mode_two}")],
        ),
        "gnome" => {
            if crate::which("gdctl").is_some() {
                gdctl_set_mode(output, width, height, refresh)
            } else if crate::which("gnome-randr").is_some() {
                run("gnome-randr", &["modify", output, "--mode", &mode_three])
            } else {
                Err("install gdctl or gnome-randr for GNOME display control".into())
            }
        }
        other => Err(format!("no display adapter for '{other}'")),
    };
    match result {
        Ok(_) => {
            invalidate_display_cache();
            crate::SimpleResult {
                ok: true,
                message: format!("{output} → {mode_two}"),
            }
        }
        Err(e) => crate::SimpleResult {
            ok: false,
            message: e,
        },
    }
}

pub fn set_brightness(percent: u32) -> crate::SimpleResult {
    let percent = percent.min(100);
    let (device, _, max) = read_brightness();
    let Some(device) = device else {
        return crate::SimpleResult {
            ok: false,
            message: "no backlight device found".into(),
        };
    };
    let Some(max) = max else {
        return crate::SimpleResult {
            ok: false,
            message: "backlight max_brightness unavailable".into(),
        };
    };
    let raw = (max * percent as u32 / 100).max(if percent > 0 { 1 } else { 0 });
    let path = PathBuf::from("/sys/class/backlight")
        .join(&device)
        .join("brightness");
    if fs::write(&path, raw.to_string()).is_ok() {
        return crate::SimpleResult {
            ok: true,
            message: format!("brightness {percent}%"),
        };
    }
    crate::run_helper("brightness", &[&device, &percent.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_xrandr() {
        let text = "eDP-1 connected primary 1920x1080+0+0 (normal left inverted right x axis y axis) 344mm x 194mm\n   1920x1080     144.00*+  60.00\n   1280x720      60.00\n";
        let outputs = parse_xrandr(text);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "eDP-1");
        assert_eq!(outputs[0].refresh_hz, 144.0);
        assert_eq!(outputs[0].modes.len(), 2);
    }

    #[test]
    fn computes_refresh_ceiling_and_matches_tolerance() {
        let show = "Monitors:\n└──Monitor eDP-1 (Built-in display)\n   ├──Current mode\n   │   └──1920x1080@60.001\nLogical monitors:\n";
        let modes = "Monitors:\n└──Monitor eDP-1 (Built-in display)\n   └──Modes (3)\n       ├──1920x1080@144.001\n       ├──1920x1080@120.000\n       └──1920x1080@60.001\nLogical monitors:\n";
        let outputs = finalize_outputs(parse_gdctl(show, modes));
        assert_eq!(outputs[0].max_refresh_hz, 144.001);
        assert_eq!(
            matching_mode(&outputs[0], 1920, 1080, 60.0)
                .unwrap()
                .refresh_hz,
            60.001
        );
        assert!(matching_mode(&outputs[0], 1920, 1080, 165.0).is_none());
        assert!(matching_mode(&outputs[0], 2560, 1440, 60.0).is_none());
    }

    #[test]
    fn groups_modes_and_marks_current_selection() {
        let output = DisplayOutput {
            name: "eDP-1".into(),
            primary: true,
            width: 1920,
            height: 1080,
            refresh_hz: 60.001,
            max_refresh_hz: 144.001,
            modes: vec![
                DisplayMode {
                    width: 1920,
                    height: 1080,
                    refresh_hz: 144.001,
                },
                DisplayMode {
                    width: 1920,
                    height: 1080,
                    refresh_hz: 60.001,
                },
                DisplayMode {
                    width: 1920,
                    height: 1080,
                    refresh_hz: 60.002,
                },
                DisplayMode {
                    width: 1280,
                    height: 720,
                    refresh_hz: 59.999,
                },
            ],
        };
        let view = build_output_view(&output);
        assert_eq!(view.groups.len(), 2);
        assert_eq!(view.groups[0].modes.len(), 2);
        assert_eq!(view.groups[0].max_refresh_label, "144");
        assert_eq!(view.selected_group, 0);
        assert_eq!(view.selected_mode, 1);
        assert_eq!(view.current_refresh_label, "60");
        assert!(view.groups[0].modes[1].current);
        assert_eq!(view.max_refresh_hz, 144.001);
    }

    #[test]
    fn parses_hypr() {
        let text = r#"[{"name":"eDP-1","width":2560,"height":1600,"refreshRate":165.0,"x":0,"y":0,"scale":1.25,"focused":true,"availableModes":["2560x1600@165.00Hz","1920x1200@60.00Hz"]}]"#;
        let outputs = parse_hypr(text).unwrap();
        assert_eq!(outputs[0].refresh_hz, 165.0);
        assert_eq!(outputs[0].modes.len(), 2);
    }

    #[test]
    fn parses_kde() {
        let text = "Output: 1 eDP-1\n\tenabled\n\tconnected\n\tpriority 1\n\tGeometry: 0,0 1920x1080\n\tScale: 1\n\tModes: 1:1920x1080@60.00*! 2:1920x1080@144.00! 3:1280x720@60.00\n";
        let outputs = parse_kde(text);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "eDP-1");
        assert_eq!(outputs[0].width, 1920);
        assert_eq!(outputs[0].refresh_hz, 60.0);
        assert_eq!(outputs[0].modes.len(), 3);
    }

    #[test]
    fn parses_gnome_physical_monitor() {
        let text = "eDP-1 AUO 0x7ead 0x00007ead\n  1920x1080 60.001*+\n  1920x1080 144.001+\n  1280x720 59.999+\n";
        let outputs = parse_gnome(text);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "eDP-1");
        assert_eq!(outputs[0].refresh_hz, 60.001);
        assert_eq!(outputs[0].modes.len(), 3);
    }

    #[test]
    fn ignores_gnome_randr_config_keys() {
        let text = "supports-mirroring: true\nlayout-mode: logical\nsupports-changing-layout-mode: true\nglobal-scale-required: false\n\nlogical monitor 0:\nx: 0, y: 0, scale: 1, rotation: normal, primary: yes\nassociated physical monitors:\n\teDP-1 AUO 0x7ead 0x00007ead\n\neDP-1 AUO 0x7ead 0x00007ead\n             1920x1080@144.001\t1920x1080 \t144.00+   \t[x1.00+, x1.25]\n              1920x1080@60.001\t1920x1080 \t60.00*    \t[x1.00+, x1.25]\nis-for-lease: false\ndisplay-name: \"Built-in display\"\ncolor-mode: 0\nsupported-color-modes: [0, 2]\n";
        let outputs = parse_gnome(text);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "eDP-1");
        assert_eq!(outputs[0].width, 1920);
        assert_eq!(outputs[0].refresh_hz, 60.001);
        assert_eq!(outputs[0].modes.len(), 2);
    }

    #[test]
    fn parses_gdctl_logical_layout() {
        let text = "Monitors:\n└──Monitor eDP-1 (Built-in display)\nLogical monitors:\n└──Logical monitor #1\n   ├──Position: (0, 0)\n   ├──Scale: 1.0\n   ├──Transform: normal\n   ├──Primary: yes\n   └──Monitors: (1)\n       └──eDP-1 (Built-in display)\n";
        let logicals = parse_gdctl_logical(text);
        assert_eq!(logicals.len(), 1);
        assert_eq!(logicals[0].x, 0);
        assert_eq!(logicals[0].y, 0);
        assert_eq!(logicals[0].scale, 1.0);
        assert_eq!(logicals[0].transform, "normal");
        assert!(logicals[0].primary);
        assert_eq!(logicals[0].monitors, vec!["eDP-1"]);
    }

    #[test]
    fn parses_gdctl() {
        let show = "Monitors:\n└──Monitor eDP-1 (Built-in display)\n   ├──Vendor: AUO\n   ├──Current mode\n   │   └──1920x1080@60.001\nLogical monitors:\n└──Logical monitor #1\n";
        let modes = "Monitors:\n└──Monitor eDP-1 (Built-in display)\n   └──Modes (2)\n       ├──1920x1080@144.001\n       └──1920x1080@60.001\nLogical monitors:\n";
        let outputs = parse_gdctl(show, modes);
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].name, "eDP-1");
        assert_eq!(outputs[0].refresh_hz, 60.001);
        assert_eq!(outputs[0].modes.len(), 2);
    }
}
