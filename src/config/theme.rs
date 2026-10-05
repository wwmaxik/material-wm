use serde::{Deserialize, Serialize};
use smithay::backend::renderer::Color32F;

/// Dynamic Material You (M3) Tonal Palette
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct M3Colors {
    /// Dark background (e.g. #141218)
    pub surface: String,
    /// Cards, top bar (e.g. #211F26)
    pub surface_container: String,
    /// Inactive quick setting tiles, elevated cards (e.g. #2B2930)
    pub surface_container_high: String,
    /// Accent tone, pastel violet/blue (e.g. #D0BCFF or #A8C7FA)
    pub primary: String,
    /// Contrasting text on active tiles (e.g. #381E72)
    pub on_primary: String,
    /// Standard text and glyphs on surface (e.g. #E6E1E5)
    pub on_surface: String,
    /// Subtle text and secondary labels (e.g. #CAC4D0)
    pub on_surface_variant: String,
    /// Delicate boundaries (e.g. #938F99)
    pub outline: String,
    /// Softer boundaries and dividers (e.g. #49454F)
    pub outline_variant: String,
    /// Secondary accent (e.g. #CCC2DC)
    pub secondary: String,
    /// Error / warning alert (e.g. #F2B8B5)
    pub error: String,
}

impl Default for M3Colors {
    fn default() -> Self {
        Self {
            // crDroid / Android 14 Material You Dark default (Lavender Violet accent)
            surface: "#141218".to_string(),
            surface_container: "#211F26".to_string(),
            surface_container_high: "#2B2930".to_string(),
            primary: "#D0BCFF".to_string(),
            on_primary: "#381E72".to_string(),
            on_surface: "#E6E1E5".to_string(),
            on_surface_variant: "#CAC4D0".to_string(),
            outline: "#938F99".to_string(),
            outline_variant: "#49454F".to_string(),
            secondary: "#CCC2DC".to_string(),
            error: "#F2B8B5".to_string(),
        }
    }
}

impl M3Colors {
    pub fn hex_to_rgba(hex: &str) -> [f32; 4] {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
            [r, g, b, 1.0]
        } else if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255) as f32 / 255.0;
            [r, g, b, a]
        } else {
            [1.0, 1.0, 1.0, 1.0]
        }
    }

    pub fn surface_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.surface);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn surface_container_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.surface_container);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn surface_container_high_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.surface_container_high);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn primary_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.primary);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn on_primary_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.on_primary);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn on_surface_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.on_surface);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn outline_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.outline);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn outline_variant_color(&self) -> Color32F {
        let c = Self::hex_to_rgba(&self.outline_variant);
        Color32F::new(c[0], c[1], c[2], c[3])
    }

    pub fn hex_to_rgba_u8(hex: &str) -> [u8; 4] {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            [r, g, b, 255]
        } else if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
            let a = u8::from_str_radix(&hex[6..8], 16).unwrap_or(255);
            [r, g, b, a]
        } else {
            [255, 255, 255, 255]
        }
    }
}
