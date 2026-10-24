use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

const ALLOWED_GPU_MODES: [&str; 3] = ["integrated", "hybrid", "nvidia"];
const MAX_CURVE_POINTS: usize = 10;
const HWMON_NAME_PATTERNS: [&str; 4] = [
    "legion_laptop",
    "lenovo_wmi_fan",
    "lenovo_wmi_other",
    "ideapad",
];

const ALLOWED_POWER_LIMIT_ATTRS: &[&str] = &[
    "ppt_pl1_spl",
    "ppt_pl2_sppt",
    "ppt_pl1_tau",
    "ppt_cpu_cl",
    "gpu_nv_ctgp",
    "gpu_nv_ppab",
    "gpu_nv_ac_offset",
    "cpu_temp",
    "gpu_temp",
    "cpu_longterm_powerlimit",
    "cpu_shortterm_powerlimit",
    "cpu_peak_powerlimit",
    "cpu_cross_loading_powerlimit",
    "cpu_apu_spl",
    "gpu_ctgp_powerlimit",
    "gpu_ppab_powerlimit",
    "gpu_ac_offset_powerlimit",
    "CPULongTermPowerLimit",
    "CPUShortTermPowerLimit",
    "CPUPeakPowerLimit",
    "CPUCrossLoadingPowerLimit",
    "cTGP",
    "PPAB",
    "ACOffset",
];

const POWER_LIMIT_BASE_PATTERNS: &[&str] = &[
    "/sys/class/firmware-attributes/*/attributes",
    "/sys/module/legion_laptop/drivers/platform:legion/legion",
    "/sys/module/legion_laptop/drivers/platform:legion/PNP0C09:00",
    "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00",
    "/sys/bus/wmi/drivers/lenovo-wmi-other/*/*",
    "/sys/bus/wmi/drivers/lenovo-wmi-gamezone/*/*",
];

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: loq-helper <subcommand> [args...]\n  \
             fan-curve <fan_id> <json>\n  \
             fan-manual <fan_id> <pwm%>\n  \
             fan-auto <fan_id>\n  \
             gpu-switch <integrated|hybrid|nvidia>\n  \
             power-limit <attr_name> <watts>\n  \
             battery-conservation <0|1>\n  \
             charge-type <Fast|Standard|Long_Life>\n  \
             thermal-mode <platform_profile>\n  \
             charge-threshold <50-100>\n  \
             brightness <device> <0-100>\n  \
             smart-health <device>\n  \
             gpu-power-limit <watts>\n  \
             gpu-runtime-pm <auto|on>\n  \
             memory-info\n  \
             drop-caches\n  \
             version"
        );
        exit(1);
    }

    let code = match args[1].as_str() {
        "version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            0
        }
        "fan-curve" => match (args.get(2), args.get(3)) {
            (Some(id), Some(json)) => fan_curve(id, json),
            _ => usage("fan-curve <fan_id> <json>"),
        },
        "fan-manual" => match (args.get(2), args.get(3)) {
            (Some(id), Some(pwm)) => fan_manual(id, pwm),
            _ => usage("fan-manual <fan_id> <pwm%>"),
        },
        "fan-auto" => match args.get(2) {
            Some(id) => fan_auto(id),
            None => usage("fan-auto <fan_id>"),
        },
        "gpu-switch" => match args.get(2) {
            Some(mode) => gpu_switch(mode),
            None => usage("gpu-switch <integrated|hybrid|nvidia>"),
        },
        "power-limit" => match (args.get(2), args.get(3)) {
            (Some(attr), Some(val)) => power_limit(attr, val),
            _ => usage("power-limit <attr_name> <watts>"),
        },
        "battery-conservation" => match args.get(2) {
            Some(val) => battery_conservation(val),
            None => usage("battery-conservation <0|1>"),
        },
        "charge-type" => match args.get(2) {
            Some(mode) => charge_type(mode),
            None => usage("charge-type <Fast|Standard|Long_Life>"),
        },
        "thermal-mode" => match args.get(2) {
            Some(mode) => thermal_mode(mode),
            None => usage("thermal-mode <platform_profile>"),
        },
        "charge-threshold" => match args.get(2) {
            Some(percent) => charge_threshold(percent),
            None => usage("charge-threshold <50-100>"),
        },
        "brightness" => match (args.get(2), args.get(3)) {
            (Some(device), Some(percent)) => brightness(device, percent),
            _ => usage("brightness <device> <0-100>"),
        },
        "smart-health" => match args.get(2) {
            Some(device) => smart_health(device),
            None => usage("smart-health <device>"),
        },
        "gpu-power-limit" => match args.get(2) {
            Some(watts) => gpu_power_limit(watts),
            None => usage("gpu-power-limit <watts>"),
        },
        "gpu-runtime-pm" => match args.get(2) {
            Some(mode) => gpu_runtime_pm(mode),
            None => usage("gpu-runtime-pm <auto|on>"),
        },
        "memory-info" => memory_info(),
        "drop-caches" => drop_caches(),
        other => {
            eprintln!("ERROR: unknown subcommand: {other}");
            1
        }
    };
    exit(code);
}

fn usage(text: &str) -> i32 {
    eprintln!("Usage: loq-helper {text}");
    1
}

fn find_hwmon() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hwmon").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if let Ok(name) = fs::read_to_string(path.join("name")) {
            if HWMON_NAME_PATTERNS.contains(&name.trim()) {
                return Some(path);
            }
        }
    }
    None
}

fn write_sysfs(path: &Path, value: &str) -> Result<(), String> {
    let data = format!("{}\n", value.trim());
    if fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|mut f| f.write_all(data.as_bytes()))
        .is_ok()
    {
        return Ok(());
    }
    if fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|mut f| f.write_all(data.as_bytes()))
        .is_ok()
    {
        return Ok(());
    }
    fs::write(path, data.as_bytes()).map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn pwm_from_percent(percent: i64) -> i64 {
    (percent * 255 / 100).clamp(0, 255)
}

fn fan_curve(fan_id_raw: &str, json: &str) -> i32 {
    let fan_id = match fan_id_raw.parse::<i64>() {
        Ok(v) if v == 1 || v == 2 => v,
        _ => {
            eprintln!("ERROR: fan_id must be 1 or 2.");
            return 1;
        }
    };

    let points: Vec<serde_json::Value> = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR: invalid JSON: {e}");
            return 1;
        }
    };
    if points.len() < 2 || points.len() > MAX_CURVE_POINTS {
        eprintln!("ERROR: expected 2–{MAX_CURVE_POINTS} curve points.");
        return 1;
    }

    let mut parsed: Vec<(i64, i64)> = Vec::new();
    for pt in &points {
        let temp = pt.get("temp").and_then(|v| v.as_i64()).unwrap_or(-1);
        let pwm = pt.get("pwm").and_then(|v| v.as_i64()).unwrap_or(-1);
        if !(0..=110).contains(&temp) || !(0..=100).contains(&pwm) {
            eprintln!("ERROR: temp 0–110, pwm 0–100 required.");
            return 1;
        }
        parsed.push((temp, pwm));
    }
    parsed.sort_by_key(|(t, _)| *t);

    let hwmon = match find_hwmon() {
        Some(p) => p,
        None => {
            eprintln!("ERROR: fan hwmon interface not found.");
            return 1;
        }
    };

    // Validate every node first so an unsupported curve never gets half-applied.
    for i in 1..=parsed.len() {
        let temp_path = hwmon.join(format!("pwm{fan_id}_auto_point{i}_temp"));
        let pwm_path = hwmon.join(format!("pwm{fan_id}_auto_point{i}_pwm"));
        if !temp_path.exists() || !pwm_path.exists() {
            eprintln!(
                "ERROR: curve node {i} not found at {} (unsupported hardware).",
                hwmon.display()
            );
            return 1;
        }
    }
    if let Err(e) = write_sysfs(&hwmon.join(format!("pwm{fan_id}_enable")), "1") {
        eprintln!("ERROR: could not switch fan {fan_id} to manual control: {e}");
        return 1;
    }

    for (i, (temp, pwm)) in parsed.iter().enumerate() {
        let idx = i + 1;
        let temp_path = hwmon.join(format!("pwm{fan_id}_auto_point{idx}_temp"));
        let pwm_path = hwmon.join(format!("pwm{fan_id}_auto_point{idx}_pwm"));
        let scale = match fs::read_to_string(&temp_path)
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
        {
            Some(raw) if raw > 1000 => 1000,
            _ => 1,
        };
        if let Err(e) = write_sysfs(&temp_path, &(temp * scale).to_string()) {
            eprintln!("ERROR: {e}");
            return 1;
        }
        if let Err(e) = write_sysfs(&pwm_path, &pwm_from_percent(*pwm).to_string()) {
            eprintln!("ERROR: {e}");
            return 1;
        }
    }

    println!("Fan {fan_id} curve applied ({} points).", parsed.len());
    0
}

fn fan_manual(fan_id_raw: &str, pwm_raw: &str) -> i32 {
    let fan_id = match fan_id_raw.parse::<i64>() {
        Ok(v) if v == 1 || v == 2 => v,
        _ => {
            eprintln!("ERROR: fan_id must be 1 or 2.");
            return 1;
        }
    };
    let pwm_percent = match pwm_raw.parse::<i64>() {
        Ok(v) if (0..=100).contains(&v) => v,
        _ => {
            eprintln!("ERROR: pwm must be 0–100.");
            return 1;
        }
    };

    let hwmon = match find_hwmon() {
        Some(p) => p,
        None => {
            eprintln!("ERROR: fan hwmon interface not found.");
            return 1;
        }
    };

    if let Err(e) = write_sysfs(&hwmon.join(format!("pwm{fan_id}_enable")), "1") {
        eprintln!("ERROR: {e}");
        return 1;
    }
    if let Err(e) = write_sysfs(
        &hwmon.join(format!("pwm{fan_id}")),
        &pwm_from_percent(pwm_percent).to_string(),
    ) {
        eprintln!("ERROR: {e}");
        return 1;
    }
    println!("Fan {fan_id} manual speed set to {pwm_percent}%.");
    0
}

fn fan_auto(fan_id_raw: &str) -> i32 {
    let fan_id = match fan_id_raw.parse::<i64>() {
        Ok(v) if (0..=2).contains(&v) => v,
        _ => {
            eprintln!("ERROR: fan_id must be 0 (all), 1 or 2.");
            return 1;
        }
    };
    let hwmon = match find_hwmon() {
        Some(p) => p,
        None => {
            eprintln!("ERROR: fan hwmon interface not found.");
            return 1;
        }
    };
    let ids: Vec<i64> = if fan_id == 0 {
        vec![1, 2]
    } else {
        vec![fan_id]
    };
    for id in ids {
        let path = hwmon.join(format!("pwm{id}_enable"));
        if path.exists() {
            if let Err(e) = write_sysfs(&path, "2") {
                eprintln!("ERROR: {e}");
                return 1;
            }
        }
    }
    println!("Fan automatic control restored.");
    0
}

fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn glob(pattern: &str) -> Vec<PathBuf> {
    // Supports '*' per path segment only, which covers the sysfs patterns above.
    let mut current = vec![PathBuf::from("/")];
    for segment in pattern.split('/').filter(|s| !s.is_empty()) {
        let mut next = Vec::new();
        for base in &current {
            if let Some(prefix) = segment.strip_suffix('*') {
                if let Ok(entries) = fs::read_dir(base) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        if name.to_string_lossy().starts_with(prefix) {
                            next.push(entry.path());
                        }
                    }
                }
            } else {
                let candidate = base.join(segment);
                if candidate.exists() {
                    next.push(candidate);
                }
            }
        }
        current = next;
    }
    current
}

fn gpu_switch(mode: &str) -> i32 {
    if !ALLOWED_GPU_MODES.contains(&mode) {
        eprintln!("ERROR: mode must be integrated, hybrid, or nvidia.");
        return 1;
    }

    if let Some(envy) = which("envycontrol") {
        let status = Command::new(envy).arg("-s").arg(mode).status();
        if let Ok(s) = status {
            if s.success() {
                let _ = Command::new("udevadm")
                    .args(["control", "--reload-rules"])
                    .status();
                let _ = Command::new("udevadm").arg("trigger").status();
                println!("GPU mode switched to: {mode} (via envycontrol)");
                return 0;
            }
        }
        eprintln!("ERROR: envycontrol failed.");
        return 1;
    }

    if let Some(supergfx) = which("supergfxctl") {
        let mapped = match mode {
            "integrated" => "Integrated",
            "nvidia" => "Dedicated",
            _ => "Hybrid",
        };
        if Command::new(supergfx)
            .args(["--mode", mapped])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            println!("GPU mode switched to: {mode} (via supergfxctl)");
            return 0;
        }
        eprintln!("ERROR: supergfxctl failed.");
        return 1;
    }

    if let Some(prime) = which("prime-select") {
        let mapped = match mode {
            "integrated" => "integrated",
            "nvidia" => "nvidia",
            _ => "on-demand",
        };
        if Command::new(prime)
            .args(["set", mapped])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            println!("GPU mode switched to: {mode} (via prime-select)");
            return 0;
        }
        eprintln!("ERROR: prime-select failed.");
        return 1;
    }

    eprintln!("ERROR: no supported GPU switcher found in PATH.");
    1
}

fn find_power_limit_path(attr: &str) -> Option<PathBuf> {
    for pattern in POWER_LIMIT_BASE_PATTERNS {
        for base in glob(pattern) {
            let candidate = base.join(attr);
            let value_file = candidate.join("current_value");
            if value_file.is_file() {
                return Some(value_file);
            }
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn charge_type(mode: &str) -> i32 {
    let canonical = match mode.to_lowercase().replace(' ', "_").as_str() {
        "fast" | "rapid" | "rapid_charge" => "Fast",
        "standard" | "normal" => "Standard",
        "long_life" | "longlife" | "conservation" => "Long_Life",
        _ => {
            eprintln!("ERROR: charge mode must be Fast, Standard, or Long_Life.");
            return 1;
        }
    };
    if let Some(path) = find_battery_file("charge_types") {
        return match write_sysfs(&path, canonical) {
            Ok(()) => {
                println!("Charge mode set to: {canonical}");
                0
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                1
            }
        };
    }
    if canonical == "Fast" {
        eprintln!("ERROR: rapid charge is not exposed by this firmware.");
        return 1;
    }
    battery_conservation(if canonical == "Long_Life" { "1" } else { "0" })
}

fn find_battery_file(name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let base = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if base.starts_with("BAT") {
            let candidate = path.join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn charge_threshold(percent_raw: &str) -> i32 {
    let percent = match percent_raw.parse::<u32>() {
        Ok(v) if (50..=100).contains(&v) => v,
        _ => {
            eprintln!("ERROR: threshold must be 50-100.");
            return 1;
        }
    };
    let path = match find_battery_file("charge_control_end_threshold") {
        Some(p) => p,
        None => {
            eprintln!("ERROR: charge_control_end_threshold is not available.");
            return 1;
        }
    };
    match write_sysfs(&path, &percent.to_string()) {
        Ok(()) => {
            println!("Charge threshold set to: {percent}");
            0
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}

fn brightness(device: &str, percent_raw: &str) -> i32 {
    if device.contains('/') || device.contains("..") {
        eprintln!("ERROR: invalid backlight device.");
        return 1;
    }
    let percent = match percent_raw.parse::<u32>() {
        Ok(v) if v <= 100 => v,
        _ => {
            eprintln!("ERROR: brightness must be 0-100.");
            return 1;
        }
    };
    let base = Path::new("/sys/class/backlight").join(device);
    let max = match fs::read_to_string(base.join("max_brightness"))
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
    {
        Some(v) if v > 0 => v,
        _ => {
            eprintln!("ERROR: backlight device not found.");
            return 1;
        }
    };
    let raw = (max * percent as u64 / 100)
        .max(if percent > 0 { 1 } else { 0 })
        .to_string();
    match write_sysfs(&base.join("brightness"), &raw) {
        Ok(()) => {
            println!("Brightness set to: {percent}%");
            0
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}

fn smart_health(device: &str) -> i32 {
    if !device.starts_with("/dev/")
        || device.len() < 6
        || device[5..].contains('/')
        || device[5..].contains("..")
    {
        eprintln!("ERROR: invalid block device.");
        return 1;
    }
    let out = Command::new("smartctl").args(["-H", device]).output();
    match out {
        Ok(o) => {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            println!("{}", text.trim());
            if o.status.success() {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("ERROR: smartctl failed: {e}");
            1
        }
    }
}

fn gpu_power_limit(watts_raw: &str) -> i32 {
    let watts = match watts_raw.parse::<u32>() {
        Ok(v) if (1..=1000).contains(&v) => v,
        _ => {
            eprintln!("ERROR: wattage must be 1-1000.");
            return 1;
        }
    };
    match Command::new("nvidia-smi")
        .args(["-pl", &watts.to_string()])
        .output()
    {
        Ok(o) if o.status.success() => {
            println!("GPU power limit set to: {watts} W");
            0
        }
        Ok(o) => {
            eprintln!(
                "ERROR: nvidia-smi failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
            1
        }
        Err(e) => {
            eprintln!("ERROR: nvidia-smi not available: {e}");
            1
        }
    }
}

fn gpu_runtime_pm(mode: &str) -> i32 {
    if mode != "auto" && mode != "on" {
        eprintln!("ERROR: mode must be auto or on.");
        return 1;
    }
    let Ok(entries) = fs::read_dir("/sys/bus/pci/devices") else {
        eprintln!("ERROR: PCI sysfs not available.");
        return 1;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let vendor = fs::read_to_string(path.join("vendor")).unwrap_or_default();
        let class = fs::read_to_string(path.join("class")).unwrap_or_default();
        if vendor.trim() == "0x10de" && class.trim().starts_with("0x030") {
            return match write_sysfs(&path.join("power/control"), mode) {
                Ok(()) => {
                    println!("NVIDIA runtime PM set to: {mode}");
                    0
                }
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    1
                }
            };
        }
    }
    eprintln!("ERROR: NVIDIA PCI device not found.");
    1
}

fn thermal_mode(mode: &str) -> i32 {
    let choices = match fs::read_to_string("/sys/firmware/acpi/platform_profile_choices") {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR: platform_profile not available: {e}");
            return 1;
        }
    };
    if !choices.split_whitespace().any(|c| c == mode) {
        eprintln!("ERROR: '{mode}' is not offered by this firmware.");
        return 1;
    }
    match write_sysfs(Path::new("/sys/firmware/acpi/platform_profile"), mode) {
        Ok(()) => {
            println!("Thermal mode set to: {mode}");
            0
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}

fn dmi_field(block: &str, key: &str) -> String {
    for line in block.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(value) = rest.strip_prefix(':') {
                let value = value.trim();
                if !value.is_empty() {
                    return value.to_string();
                }
            }
        }
    }
    String::new()
}

fn memory_info() -> i32 {
    let output = match Command::new("dmidecode").args(["-t", "memory"]).output() {
        Ok(o) if o.status.success() => o,
        Ok(_) => {
            eprintln!("ERROR: dmidecode failed (root and dmidecode package required).");
            return 1;
        }
        Err(e) => {
            eprintln!("ERROR: dmidecode not found: {e}");
            return 1;
        }
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut dimms = Vec::new();
    for block in text.split("\n\n") {
        if !block.contains("Memory Device") {
            continue;
        }
        let size = dmi_field(block, "Size");
        if size.is_empty() || size.contains("No Module") || size.starts_with('0') {
            continue;
        }
        dimms.push(serde_json::json!({
            "locator": dmi_field(block, "Locator"),
            "size": size,
            "speed": dmi_field(block, "Speed"),
            "mem_type": dmi_field(block, "Type"),
            "manufacturer": dmi_field(block, "Manufacturer"),
        }));
    }
    println!("{}", serde_json::Value::Array(dimms));
    0
}

fn drop_caches() -> i32 {
    match Command::new("sync").status() {
        Ok(s) if s.success() => {}
        _ => {
            eprintln!("ERROR: sync failed.");
            return 1;
        }
    }
    match write_sysfs(Path::new("/proc/sys/vm/drop_caches"), "3") {
        Ok(()) => {
            println!("Page cache, dentries and inodes dropped.");
            0
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}

fn power_limit(attr: &str, value_raw: &str) -> i32 {
    if !ALLOWED_POWER_LIMIT_ATTRS.contains(&attr) {
        eprintln!("ERROR: '{attr}' is not in the allowed power-limit list.");
        return 1;
    }
    let value = match value_raw.parse::<i64>() {
        Ok(v) if (0..=200).contains(&v) => v,
        _ => {
            eprintln!("ERROR: value must be an integer 0–200.");
            return 1;
        }
    };
    let path = match find_power_limit_path(attr) {
        Some(p) => p,
        None => {
            eprintln!("ERROR: sysfs node for '{attr}' not found.");
            return 1;
        }
    };
    match write_sysfs(&path, &value.to_string()) {
        Ok(()) => {
            println!("Power limit '{attr}' set to {value}.");
            0
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}

fn battery_conservation(val: &str) -> i32 {
    if val != "0" && val != "1" {
        eprintln!("ERROR: value must be 0 or 1.");
        return 1;
    }
    let candidates = [
        "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode",
        "/sys/devices/platform/ideapad_laptop/conservation_mode",
        "/sys/bus/platform/devices/ideapad_laptop/conservation_mode",
        "/sys/class/power_supply/BAT0/charge_control_end_threshold",
        "/sys/class/power_supply/BAT1/charge_control_end_threshold",
    ];
    let path = match candidates.iter().map(PathBuf::from).find(|p| p.exists()) {
        Some(p) => p,
        None => {
            eprintln!("ERROR: conservation_mode sysfs node not found.");
            return 1;
        }
    };
    let value = if path
        .to_string_lossy()
        .contains("charge_control_end_threshold")
    {
        if val == "1" {
            "80"
        } else {
            "100"
        }
    } else {
        val
    };
    match write_sysfs(&path, value) {
        Ok(()) => {
            println!("Battery conservation mode set to: {val}");
            0
        }
        Err(e) => {
            eprintln!("ERROR: {e}");
            1
        }
    }
}
