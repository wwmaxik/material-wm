use std::fs;
use std::path::Path;
use fontdue::{Font, FontSettings};

pub struct FontRenderer {
    font: Option<Font>,
    icon_font: Option<Font>,
}

impl FontRenderer {
    pub fn new() -> Self {
        // 1. Primary UI font (Roboto, Noto Sans, DejaVu Sans, Liberation Sans)
        let text_paths = [
            "/home/wwmaxik/.fonts/Roboto-Regular.ttf",
            "/usr/share/fonts/truetype/roboto/unhinted/RobotoTTF/Roboto-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ];

        let mut font = None;
        for path in text_paths {
            if Path::new(path).exists() {
                if let Ok(data) = fs::read(path) {
                    if let Ok(parsed) = Font::from_bytes(data, FontSettings::default()) {
                        tracing::info!("Loaded primary font from {}", path);
                        font = Some(parsed);
                        break;
                    }
                }
            }
        }

        // 2. Icon font (Nerd Font / FontAwesome for Material You glyphs)
        let icon_paths = [
            "/home/wwmaxik/.local/share/fonts/JetBrainsMonoNerdFontPropo-Regular.ttf",
            "/home/wwmaxik/.local/share/fonts/JetBrainsMonoNerd/JetBrainsMonoNLNerdFontPropo-Regular.ttf",
            "/usr/local/share/fonts/JetBrainsMonoNerd/JetBrainsMonoNerd/JetBrainsMonoNLNerdFontPropo-Regular.ttf",
            "/usr/share/fonts/truetype/font-awesome/fontawesome-webfont.ttf",
        ];

        let mut icon_font = None;
        for path in icon_paths {
            if Path::new(path).exists() {
                if let Ok(data) = fs::read(path) {
                    if let Ok(parsed) = Font::from_bytes(data, FontSettings::default()) {
                        tracing::info!("Loaded icon font from {}", path);
                        icon_font = Some(parsed);
                        break;
                    }
                }
            }
        }

        if font.is_none() {
            tracing::warn!("No TTF font found on system; falling back to basic font rendering");
        }

        Self { font, icon_font }
    }

    /// Rasterize a single character with icon font fallback
    pub fn rasterize_glyph(&self, ch: char, font_size: f32) -> (fontdue::Metrics, Vec<u8>) {
        let is_pua = ('\u{E000}'..='\u{F8FF}').contains(&ch) || ('\u{F0000}'..='\u{10FFFF}').contains(&ch);

        if is_pua {
            if let Some(ref icon) = self.icon_font {
                let (m, b) = icon.rasterize(ch, font_size);
                if !b.is_empty() {
                    return (m, b);
                }
            }
        }

        if let Some(ref primary) = self.font {
            let (m, b) = primary.rasterize(ch, font_size);
            if !b.is_empty() || !is_pua {
                return (m, b);
            }
        }

        if let Some(ref icon) = self.icon_font {
            let (m, b) = icon.rasterize(ch, font_size);
            if !b.is_empty() {
                return (m, b);
            }
        }

        (fontdue::Metrics::default(), Vec::new())
    }

    /// Measure width of character with font fallback
    pub fn char_advance(&self, ch: char, font_size: f32) -> f32 {
        let is_pua = ('\u{E000}'..='\u{F8FF}').contains(&ch) || ('\u{F0000}'..='\u{10FFFF}').contains(&ch);

        if is_pua {
            if let Some(ref icon) = self.icon_font {
                let m = icon.metrics(ch, font_size);
                if m.advance_width > 0.0 {
                    return m.advance_width;
                }
            }
        }

        if let Some(ref primary) = self.font {
            let m = primary.metrics(ch, font_size);
            if m.advance_width > 0.0 || !is_pua {
                return m.advance_width;
            }
        }

        if let Some(ref icon) = self.icon_font {
            return icon.metrics(ch, font_size).advance_width;
        }

        font_size * 0.6
    }

    /// Render a single-line string into an existing RGBA buffer
    pub fn draw_text(
        &self,
        buffer: &mut [u8],
        buf_w: i32,
        buf_h: i32,
        text: &str,
        start_x: i32,
        start_y: i32,
        font_size: f32,
        color_rgba: [u8; 4],
    ) {
        let mut pen_x = start_x as f32;

        for ch in text.chars() {
            let (metrics, bitmap) = self.rasterize_glyph(ch, font_size);
            let char_x = pen_x as i32 + metrics.xmin;
            let char_y = start_y + (font_size as i32 - metrics.ymin - metrics.height as i32);

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let alpha = bitmap[row * metrics.width + col];
                    if alpha == 0 {
                        continue;
                    }

                    let px = char_x + col as i32;
                    let py = char_y + row as i32;

                    if px >= 0 && px < buf_w && py >= 0 && py < buf_h {
                        let offset = ((py * buf_w + px) * 4) as usize;
                        if offset + 3 < buffer.len() {
                            let src_a = (alpha as u32 * color_rgba[3] as u32) / 255;
                            let inv_a = 255 - src_a;

                            // Premultiplied alpha compositing
                            let dst_r = buffer[offset] as u32;
                            let dst_g = buffer[offset + 1] as u32;
                            let dst_b = buffer[offset + 2] as u32;
                            let dst_a = buffer[offset + 3] as u32;

                            buffer[offset] = ((color_rgba[0] as u32 * src_a + dst_r * inv_a) / 255) as u8;
                            buffer[offset + 1] = ((color_rgba[1] as u32 * src_a + dst_g * inv_a) / 255) as u8;
                            buffer[offset + 2] = ((color_rgba[2] as u32 * src_a + dst_b * inv_a) / 255) as u8;
                            buffer[offset + 3] = (src_a + dst_a * inv_a / 255).min(255) as u8;
                        }
                    }
                }
            }

            pen_x += if metrics.advance_width > 0.0 {
                metrics.advance_width
            } else {
                self.char_advance(ch, font_size)
            };
        }
    }

    /// Draw centered text horizontally within a given width
    pub fn draw_centered_text(
        &self,
        buffer: &mut [u8],
        buf_w: i32,
        buf_h: i32,
        text: &str,
        center_x: i32,
        start_y: i32,
        font_size: f32,
        color_rgba: [u8; 4],
    ) {
        let text_w = self.measure_text_width(text, font_size);
        let start_x = center_x - text_w / 2;
        self.draw_text(buffer, buf_w, buf_h, text, start_x, start_y, font_size, color_rgba);
    }

    /// Measure text width in pixels
    pub fn measure_text_width(&self, text: &str, font_size: f32) -> i32 {
        let mut width = 0.0f32;
        for ch in text.chars() {
            width += self.char_advance(ch, font_size);
        }
        width.round() as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_renderer_draw() {
        let font = FontRenderer::new();
        assert!(font.font.is_some(), "System TTF font should be loaded");

        let w = 200;
        let h = 50;
        let mut buf = vec![0u8; (w * h * 4) as usize];
        font.draw_text(&mut buf, w, h, "Terminal", 10, 10, 16.0, [255, 255, 255, 255]);

        let mut min_y = h;
        let mut max_y = 0;
        let mut min_x = w;
        let mut max_x = 0;
        for y in 0..h {
            for x in 0..w {
                let offset = ((y * w + x) * 4) as usize;
                if buf[offset + 3] > 0 {
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                    min_x = min_x.min(x);
                    max_x = max_x.max(x);
                }
            }
        }
        println!("Rendered text bbox: x=[{}..{}], y=[{}..{}]", min_x, max_x, min_y, max_y);
        assert!(min_y >= 10 && max_y <= 35, "Text should be vertically within expected line bounds");
    }

    #[test]
    fn test_icon_glyph_rasterization() {
        let font = FontRenderer::new();
        let (metrics, bitmap) = font.rasterize_glyph('\u{F0928}', 20.0); // nf-md-wifi
        if font.icon_font.is_some() {
            assert!(!bitmap.is_empty(), "Nerd Font should rasterize Wi-Fi icon");
            assert!(metrics.width > 0, "Wi-Fi icon should have non-zero width");
        }
    }
}
