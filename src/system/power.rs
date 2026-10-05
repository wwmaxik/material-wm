use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct BatteryInfo {
    pub capacity: u8,
    pub is_charging: bool,
    pub is_present: bool,
}

impl Default for BatteryInfo {
    fn default() -> Self {
        Self {
            capacity: 100,
            is_charging: false,
            is_present: false,
        }
    }
}

pub struct PowerManager;

impl PowerManager {
    /// Read battery information from /sys/class/power_supply
    pub fn read_battery() -> BatteryInfo {
        let base_dir = Path::new("/sys/class/power_supply");
        if !base_dir.exists() {
            return BatteryInfo::default();
        }

        // Search for BAT0, BAT1, etc.
        if let Ok(entries) = fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("BAT") {
                    let cap_path = entry.path().join("capacity");
                    let status_path = entry.path().join("status");

                    let capacity = fs::read_to_string(cap_path)
                        .ok()
                        .and_then(|s| s.trim().parse::<u8>().ok())
                        .unwrap_or(100);

                    let status = fs::read_to_string(status_path)
                        .unwrap_or_default();
                    let is_charging = status.trim().eq_ignore_ascii_case("Charging");

                    return BatteryInfo {
                        capacity,
                        is_charging,
                        is_present: true,
                    };
                }
            }
        }

        BatteryInfo::default()
    }
}
