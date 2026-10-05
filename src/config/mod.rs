pub mod keybindings;
pub mod theme;

use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub use keybindings::{Action, KeyBinding, KeyConfig};
pub use theme::M3Colors;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Terminal command
    pub terminal: String,
    /// Window border radius in pixels (SDF shader)
    pub border_radius: f32,
    /// Window border width in pixels
    pub border_width: f32,
    /// Inner gap between windows
    pub inner_gap: i32,
    /// Outer gap between windows and monitor edge
    pub outer_gap: i32,
    /// Default master split ratio
    pub master_ratio: f32,
    /// Fluid animation duration in milliseconds (150-180ms)
    pub animation_duration_ms: u64,
    /// Status bar height (28-32px)
    pub bar_height: i32,
    /// Quick Settings drawer width
    pub drawer_width: i32,
    /// Quick Settings drawer height
    pub drawer_height: i32,
    /// Enable subtle audio feedback cues
    pub audio_feedback: bool,
    /// Material You M3 tonal palette
    pub colors: M3Colors,
    /// Key bindings
    pub keys: KeyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            terminal: "kitty".to_string(),
            border_radius: 14.0,
            border_width: 2.0,
            inner_gap: 8,
            outer_gap: 12,
            master_ratio: 0.55,
            animation_duration_ms: 160,
            bar_height: 30,
            drawer_width: 420,
            drawer_height: 520,
            audio_feedback: true,
            colors: M3Colors::default(),
            keys: KeyConfig::default(),
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        Path::new(&home).join(".config/material-wm/config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str::<Config>(&content) {
                    Ok(cfg) => {
                        tracing::info!("Loaded configuration from {:?}", path);
                        return cfg;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to parse config file {:?}: {}. Using defaults.", path, e);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read config file {:?}: {}. Using defaults.", path, e);
                }
            }
        } else {
            tracing::info!("No config file found at {:?}. Writing default template.", path);
            let default_cfg = Self::default();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(toml_str) = toml::to_string_pretty(&default_cfg) {
                let _ = fs::write(&path, toml_str);
            }
            return default_cfg;
        }

        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_load() {
        let cfg = Config::load();
        assert_eq!(cfg.terminal, "kitty");
        assert_eq!(cfg.border_radius, 14.0);
        assert!(cfg.animation_duration_ms > 0);
    }

    #[test]
    fn test_color_hex_parsing() {
        let rgba = M3Colors::hex_to_rgba("#141218");
        assert!((rgba[0] - 0.0784).abs() < 0.01);
        assert!((rgba[3] - 1.0).abs() < 0.01);
    }
}
