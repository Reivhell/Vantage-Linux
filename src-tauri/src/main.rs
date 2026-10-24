fn main() {
    if std::env::args().any(|arg| arg == "--discover") {
        match serde_json::to_string_pretty(&loq_control::discover()) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize capabilities: {e}"),
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--selftest") {
        let payload = serde_json::json!({
            "charge": loq_control::get_charge_mode(),
            "thermal": loq_control::get_thermal_mode(),
            "memory": loq_control::memory_info(),
            "display": loq_control::display_info(),
            "stats": loq_control::collect_stats(),
            "device": loq_control::device_info(),
            "battery": loq_control::battery_detail(),
            "presets": loq_control::preset_info(),
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize self-test: {e}"),
        }
        return;
    }
    loq_control::run();
}
