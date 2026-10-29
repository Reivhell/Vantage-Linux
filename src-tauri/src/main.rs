fn main() {
    if std::env::args().any(|arg| arg == "--discover") {
        match serde_json::to_string_pretty(&vantage_linux::discover()) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize capabilities: {e}"),
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--selftest") {
        let payload = serde_json::json!({
            "charge": vantage_linux::get_charge_mode(),
            "thermal": vantage_linux::get_thermal_mode(),
            "memory": vantage_linux::memory_info(),
            "display": vantage_linux::display_info(),
            "stats": vantage_linux::collect_stats(),
            "device": vantage_linux::device_info(),
            "battery": vantage_linux::battery_detail(),
            "presets": vantage_linux::preset_info(),
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("failed to serialize self-test: {e}"),
        }
        return;
    }
    vantage_linux::run();
}
