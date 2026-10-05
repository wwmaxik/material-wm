use std::fs;
use std::path::{Path, PathBuf};

pub struct BacklightManager {
    device_path: Option<PathBuf>,
    max_brightness: u32,
}

impl BacklightManager {
    pub fn new() -> Self {
        let base_dir = Path::new("/sys/class/backlight");
        let mut device_path = None;
        let mut max_brightness = 100;

        if let Ok(entries) = fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                let max_p = p.join("max_brightness");
                if let Ok(content) = fs::read_to_string(max_p) {
                    if let Ok(max_val) = content.trim().parse::<u32>() {
                        device_path = Some(p);
                        max_brightness = max_val;
                        break;
                    }
                }
            }
        }

        Self {
            device_path,
            max_brightness,
        }
    }

    /// Read brightness as a ratio 0.0 to 1.0
    pub fn read_ratio(&self) -> f32 {
        let Some(path) = &self.device_path else {
            return 1.0;
        };

        let cur_p = path.join("brightness");
        if let Ok(content) = fs::read_to_string(cur_p) {
            if let Ok(val) = content.trim().parse::<u32>() {
                if self.max_brightness > 0 {
                    return (val as f32 / self.max_brightness as f32).clamp(0.0, 1.0);
                }
            }
        }

        1.0
    }

    /// Set brightness ratio (0.0 to 1.0)
    pub fn set_ratio(&self, ratio: f32) {
        let Some(path) = &self.device_path else {
            return;
        };

        let target_val = (ratio.clamp(0.02, 1.0) * self.max_brightness as f32).round() as u32;
        let cur_p = path.join("brightness");
        let _ = fs::write(cur_p, target_val.to_string());
    }
}
