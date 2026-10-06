/// Material Design 3 (Material You / crDroid) Dynamic Tonal Palette
#[derive(Debug, Clone)]
pub struct M3Colors {
    pub surface: [f64; 4],
    pub surface_container: [f64; 4],
    pub surface_container_high: [f64; 4],
    pub surface_container_highest: [f64; 4],
    pub primary: [f64; 4],
    pub primary_container: [f64; 4],
    pub on_primary: [f64; 4],
    pub on_primary_container: [f64; 4],
    pub outline: [f64; 4],
    pub outline_variant: [f64; 4],
    pub on_surface: [f64; 4],
    pub on_surface_variant: [f64; 4],
    pub scrim: [f64; 4],
}

impl Default for M3Colors {
    fn default() -> Self {
        Self {
            // Dark baseline theme inspired by Android 14 / crDroid Material You
            surface: Self::hex("#141218", 1.0),
            surface_container: Self::hex("#211F26", 1.0),
            surface_container_high: Self::hex("#2B2930", 1.0),
            surface_container_highest: Self::hex("#36343B", 1.0),
            primary: Self::hex("#D0BCFF", 1.0),
            primary_container: Self::hex("#4F378B", 1.0),
            on_primary: Self::hex("#381E72", 1.0),
            on_primary_container: Self::hex("#EADDFF", 1.0),
            outline: Self::hex("#49454F", 1.0),
            outline_variant: Self::hex("#938F99", 0.4),
            on_surface: Self::hex("#E6E1E5", 1.0),
            on_surface_variant: Self::hex("#CAC4D0", 1.0),
            scrim: [0.0, 0.0, 0.0, 0.55],
        }
    }
}

impl M3Colors {
    pub const fn hex(hex: &str, alpha: f64) -> [f64; 4] {
        let bytes = hex.as_bytes();
        let offset = if bytes.len() > 0 && bytes[0] == b'#' { 1 } else { 0 };
        if bytes.len() < offset + 6 {
            return [0.0, 0.0, 0.0, alpha];
        }

        let r = Self::hex_byte(bytes[offset], bytes[offset + 1]);
        let g = Self::hex_byte(bytes[offset + 2], bytes[offset + 3]);
        let b = Self::hex_byte(bytes[offset + 4], bytes[offset + 5]);

        [r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0, alpha]
    }

    const fn hex_byte(h: u8, l: u8) -> u8 {
        (Self::nibble(h) << 4) | Self::nibble(l)
    }

    const fn nibble(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            b'A'..=b'F' => c - b'A' + 10,
            _ => 0,
        }
    }
}
