use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkKind {
    Ethernet(String),
    Wifi(String),
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatteryInfo {
    pub capacity: u8,
    pub is_charging: bool,
}

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub network: NetworkKind,
    pub battery: Option<BatteryInfo>,
}

impl SystemInfo {
    pub fn probe() -> Self {
        let battery = Self::read_battery();
        let network = Self::read_network();
        Self { network, battery }
    }

    fn read_battery() -> Option<BatteryInfo> {
        let ps_path = Path::new("/sys/class/power_supply");
        if !ps_path.exists() {
            return None;
        }

        let entries = fs::read_dir(ps_path).ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Check if it's a battery (e.g. BAT0, BAT1, or type == Battery)
            let path = entry.path();
            let is_bat = name.starts_with("BAT") || {
                fs::read_to_string(path.join("type"))
                    .map(|t| t.trim().eq_ignore_ascii_case("battery"))
                    .unwrap_or(false)
            };

            if is_bat {
                if let Ok(cap_str) = fs::read_to_string(path.join("capacity")) {
                    if let Ok(cap) = cap_str.trim().parse::<u8>() {
                        let status = fs::read_to_string(path.join("status"))
                            .unwrap_or_default();
                        let is_charging = status.trim().eq_ignore_ascii_case("charging")
                            || status.trim().eq_ignore_ascii_case("full");
                        return Some(BatteryInfo {
                            capacity: cap.min(100),
                            is_charging,
                        });
                    }
                }
            }
        }

        None
    }

    fn read_network() -> NetworkKind {
        let net_path = Path::new("/sys/class/net");
        if !net_path.exists() {
            return NetworkKind::Disconnected;
        }

        let mut candidate_eth = None;
        let mut candidate_wifi = None;

        if let Ok(entries) = fs::read_dir(net_path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "lo"
                    || name.starts_with("docker")
                    || name.starts_with("veth")
                    || name.starts_with("br-")
                    || name.starts_with("tun")
                    || name.starts_with("tap")
                {
                    continue;
                }

                let path = entry.path();
                let operstate = fs::read_to_string(path.join("operstate"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                let carrier = fs::read_to_string(path.join("carrier"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();

                let is_up = operstate == "up" || carrier == "1";

                if is_up {
                    if name.starts_with("wl") || name.starts_with("wlan") {
                        candidate_wifi = Some(name);
                    } else if name.starts_with("en") || name.starts_with("eth") {
                        candidate_eth = Some(name);
                    }
                }
            }
        }

        if let Some(iface) = candidate_eth {
            NetworkKind::Ethernet(iface)
        } else if let Some(iface) = candidate_wifi {
            NetworkKind::Wifi(iface)
        } else {
            NetworkKind::Disconnected
        }
    }
}
