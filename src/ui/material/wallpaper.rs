use std::path::{Path, PathBuf};

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::output::Output;
use smithay::utils::{Point, Transform};

use crate::render_helpers::memory::MemoryBuffer;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::utils::output_size;

pub struct MaterialWallpaper {
    path: Option<PathBuf>,
    cached_buffer: Option<TextureBuffer<GlesTexture>>,
    last_size: (i32, i32),
}

impl MaterialWallpaper {
    pub fn new() -> Self {
        let candidates = [
            "/home/wwmaxik/Изображения/wallpapers/mashina4k.jpg",
            "/home/wwmaxik/Изображения/wallpapers/Purple-Blue-spasm.jpg",
            "/home/wwmaxik/Изображения/wallpapers/purple_gasstation_abstract_dark_night.jpg",
            "/usr/share/wallpapers/Next/contents/images_dark/1920x1080.png",
        ];

        let path = candidates
            .iter()
            .find(|p| Path::new(p).exists())
            .map(PathBuf::from);

        Self {
            path,
            cached_buffer: None,
            last_size: (0, 0),
        }
    }

    pub fn render<R: NiriRenderer>(
        &mut self,
        renderer: &mut R,
        output: &Output,
    ) -> Option<PrimaryGpuTextureRenderElement> {
        let scale = output.current_scale().fractional_scale();
        let size = output_size(output);
        let phys_w = (size.w * scale).round() as i32;
        let phys_h = (size.h * scale).round() as i32;

        if self.cached_buffer.is_none() || self.last_size != (phys_w, phys_h) {
            self.last_size = (phys_w, phys_h);

            let mut rgba_data = None;
            if let Some(ref path) = self.path {
                if let Ok(img) = image::open(path) {
                    let resized = img.resize_exact(
                        phys_w as u32,
                        phys_h as u32,
                        image::imageops::FilterType::Triangle,
                    );
                    let mut rgba = resized.to_rgba8();
                    // Apply subtle darkening (0.82) for high text contrast
                    for pixel in rgba.pixels_mut() {
                        pixel[0] = ((pixel[0] as u32 * 210) / 255) as u8;
                        pixel[1] = ((pixel[1] as u32 * 210) / 255) as u8;
                        pixel[2] = ((pixel[2] as u32 * 210) / 255) as u8;
                    }
                    // Cairo ARgb32 format conversion: RGBA -> ARGB pre-multiplied
                    let mut argb_data = vec![0u8; (phys_w * phys_h * 4) as usize];
                    let raw = rgba.as_raw();
                    for i in (0..raw.len()).step_by(4) {
                        let r = raw[i];
                        let g = raw[i + 1];
                        let b = raw[i + 2];
                        let a = raw[i + 3];
                        argb_data[i] = b;
                        argb_data[i + 1] = g;
                        argb_data[i + 2] = r;
                        argb_data[i + 3] = a;
                    }
                    rgba_data = Some(argb_data);
                }
            }

            if let Some(data) = rgba_data {
                let buffer = MemoryBuffer::new(
                    data,
                    Fourcc::Argb8888,
                    (phys_w, phys_h),
                    scale,
                    Transform::Normal,
                );
                if let Ok(tex_buf) =
                    TextureBuffer::from_memory_buffer(renderer.as_gles_renderer(), &buffer)
                {
                    self.cached_buffer = Some(tex_buf);
                }
            }
        }

        let buffer = self.cached_buffer.clone()?;
        let elem = TextureRenderElement::from_texture_buffer(
            buffer,
            Point::new(0.0, 0.0),
            1.0,
            None,
            None,
            Kind::Unspecified,
        );
        Some(PrimaryGpuTextureRenderElement(elem))
    }
}
