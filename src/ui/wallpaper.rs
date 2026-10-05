use std::path::{Path, PathBuf};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Size, Transform};

use crate::config::theme::M3Colors;

pub struct WallpaperManager {
    buffer: Option<MemoryRenderBuffer>,
    current_size: Size<i32, Logical>,
    path: Option<PathBuf>,
}

impl WallpaperManager {
    pub fn new() -> Self {
        // Preferred wallpapers in user directory
        let candidates = [
            "/home/wwmaxik/Изображения/wallpapers/mashina4k.jpg",
            "/home/wwmaxik/Изображения/wallpapers/Purple-Blue-spasm.jpg",
            "/home/wwmaxik/Изображения/wallpapers/purple_gasstation_abstract_dark_night.jpg",
            "/usr/share/wallpapers/Next/contents/images_dark/1920x1080.png",
        ];

        let path = candidates.iter().find(|p| Path::new(p).exists()).map(PathBuf::from);

        Self {
            buffer: None,
            current_size: Size::from((0, 0)),
            path,
        }
    }

    pub fn render_element(
        &mut self,
        renderer: &mut GlesRenderer,
        screen_size: Size<i32, Logical>,
        colors: &M3Colors,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if self.buffer.is_none() || self.current_size != screen_size {
            let w = screen_size.w.max(1);
            let h = screen_size.h.max(1);
            let mut mem_buf = MemoryRenderBuffer::new(
                Fourcc::Abgr8888,
                (w, h),
                1,
                Transform::Normal,
                None,
            );

            // Load and resize image, or render procedural gradient
            let loaded = if let Some(ref path) = self.path {
                if let Ok(img) = image::open(path) {
                    let resized = img.resize_exact(w as u32, h as u32, image::imageops::FilterType::Triangle);
                    let rgba = resized.to_rgba8();
                    let raw = rgba.as_raw();

                    let mut ctx = mem_buf.render();
                    let _ = ctx.draw(|slice| {
                        // Apply subtle darkening (0.82 brightness) for high window/text contrast
                        let len = slice.len().min(raw.len());
                        for i in (0..len).step_by(4) {
                            slice[i] = ((raw[i] as u32 * 210) / 255) as u8;
                            slice[i + 1] = ((raw[i + 1] as u32 * 210) / 255) as u8;
                            slice[i + 2] = ((raw[i + 2] as u32 * 210) / 255) as u8;
                            slice[i + 3] = 255;
                        }
                        Ok::<_, ()>(vec![Rectangle::from_size((w, h).into())])
                    });
                    true
                } else {
                    false
                }
            } else {
                false
            };

            if !loaded {
                // Procedural Material You dark radial gradient
                let surface_rgb = M3Colors::hex_to_rgba_u8(&colors.surface);
                let surface_cont_rgb = M3Colors::hex_to_rgba_u8(&colors.surface_container);
                let mut ctx = mem_buf.render();
                let _ = ctx.draw(|slice| {
                    let cx = (w / 2) as f32;
                    let cy = (h / 2) as f32;
                    let max_dist = (cx * cx + cy * cy).sqrt();

                    for y in 0..h {
                        for x in 0..w {
                            let dx = x as f32 - cx;
                            let dy = y as f32 - cy;
                            let dist = (dx * dx + dy * dy).sqrt() / max_dist;
                            let factor = (1.0 - dist).clamp(0.0, 1.0);

                            let offset = ((y * w + x) * 4) as usize;
                            if offset + 3 < slice.len() {
                                slice[offset] = (surface_rgb[0] as f32 * (1.0 - factor * 0.4) + surface_cont_rgb[0] as f32 * factor * 0.4) as u8;
                                slice[offset + 1] = (surface_rgb[1] as f32 * (1.0 - factor * 0.4) + surface_cont_rgb[1] as f32 * factor * 0.4) as u8;
                                slice[offset + 2] = (surface_rgb[2] as f32 * (1.0 - factor * 0.4) + surface_cont_rgb[2] as f32 * factor * 0.4) as u8;
                                slice[offset + 3] = 255;
                            }
                        }
                    }
                    Ok::<_, ()>(vec![Rectangle::from_size((w, h).into())])
                });
            }

            self.buffer = Some(mem_buf);
            self.current_size = screen_size;
        }

        let mem_buf = self.buffer.as_ref()?;
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            Point::from((0.0, 0.0)),
            mem_buf,
            Some(1.0),
            None,
            None,
            Kind::Unspecified,
        )
        .ok()
    }
}
