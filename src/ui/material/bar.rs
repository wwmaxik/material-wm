use pangocairo::cairo::{self, ImageSurface};
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::output::Output;
use smithay::utils::{Point, Transform};

use crate::render_helpers::memory::MemoryBuffer;
use crate::render_helpers::primary_gpu_texture::PrimaryGpuTextureRenderElement;
use crate::render_helpers::renderer::NiriRenderer;
use crate::render_helpers::texture::{TextureBuffer, TextureRenderElement};
use crate::ui::material::theme::M3Colors;
use crate::utils::output_size;

pub struct MaterialTopBar {
    pub height: i32,
    pub last_minute: i32,
    pub last_title: String,
    pub last_ws: usize,
    pub last_ws_count: usize,
    cached_buffer: Option<TextureBuffer<GlesTexture>>,
}

impl MaterialTopBar {
    pub const HEIGHT: i32 = 38;

    pub fn new() -> Self {
        Self {
            height: Self::HEIGHT,
            last_minute: -1,
            last_title: String::new(),
            last_ws: 0,
            last_ws_count: 1,
            cached_buffer: None,
        }
    }

    pub fn render<R: NiriRenderer>(
        &mut self,
        renderer: &mut R,
        output: &Output,
        colors: &M3Colors,
        active_ws_idx: usize,
        ws_count: usize,
        active_title: Option<&str>,
        launcher_open: bool,
    ) -> Option<PrimaryGpuTextureRenderElement> {
        let scale = output.current_scale().fractional_scale();
        let size = output_size(output);
        let screen_w = size.w;

        let now = unsafe { libc::time(std::ptr::null_mut()) };
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&now, &mut tm) };
        let cur_minute = tm.tm_min;
        let time_str = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);

        let title_str = active_title.unwrap_or("").to_string();
        let needs_redraw = self.cached_buffer.is_none()
            || self.last_minute != cur_minute
            || self.last_title != title_str
            || self.last_ws != active_ws_idx
            || self.last_ws_count != ws_count;

        if needs_redraw {
            self.last_minute = cur_minute;
            self.last_title = title_str.clone();
            self.last_ws = active_ws_idx;
            self.last_ws_count = ws_count;

            let phys_w = (screen_w * scale).round() as i32;
            let phys_h = (Self::HEIGHT as f64 * scale).round() as i32;

            if let Ok(surface) = ImageSurface::create(cairo::Format::ARgb32, phys_w, phys_h) {
                if let Ok(cr) = cairo::Context::new(&surface) {
                    cr.scale(scale, scale);

                    // 1. Top bar background
                    Self::set_source_color(&cr, colors.surface_container);
                    cr.rectangle(0.0, 0.0, screen_w, Self::HEIGHT as f64);
                    let _ = cr.fill();

                    // Bottom divider line
                    Self::set_source_color(&cr, colors.outline_variant);
                    cr.rectangle(0.0, (Self::HEIGHT - 1) as f64, screen_w, 1.0);
                    let _ = cr.fill();

                    // 2. App Launcher trigger button (top-left)
                    let btn_x = 8.0;
                    let btn_y = 8.0;
                    let btn_w = 34.0;
                    let btn_h = 22.0;
                    let launch_bg = if launcher_open {
                        colors.primary
                    } else {
                        colors.surface_container_high
                    };
                    Self::set_source_color(&cr, launch_bg);
                    Self::draw_rounded_rect(&cr, btn_x, btn_y, btn_w, btn_h, 11.0);
                    let _ = cr.fill();

                    // Launcher grid dots (2x2)
                    let dot_col = if launcher_open {
                        colors.on_primary
                    } else {
                        colors.primary
                    };
                    Self::set_source_color(&cr, dot_col);
                    let dot_r = 1.8;
                    let dots = [
                        (btn_x + 12.0, btn_y + 7.5),
                        (btn_x + 20.0, btn_y + 7.5),
                        (btn_x + 12.0, btn_y + 14.5),
                        (btn_x + 20.0, btn_y + 14.5),
                    ];
                    for (dx, dy) in dots {
                        cr.arc(dx, dy, dot_r, 0.0, 2.0 * std::f64::consts::PI);
                        let _ = cr.fill();
                    }

                    // 3. Workspace indicators
                    let mut pill_x = 50.0;
                    let total_ws = ws_count.max(1).max(active_ws_idx + 1).min(10);
                    for i in 0..total_ws {
                        if i == active_ws_idx {
                            // Active stretched pill with workspace number
                            let pw = 28.0;
                            let ph = 18.0;
                            let py = (Self::HEIGHT as f64 - ph) / 2.0;
                            Self::set_source_color(&cr, colors.primary_container);
                            Self::draw_rounded_rect(&cr, pill_x, py, pw, ph, 9.0);
                            let _ = cr.fill();

                            let ws_num_str = format!("{}", i + 1);
                            Self::draw_text(
                                &cr,
                                &ws_num_str,
                                "sans bold 10px",
                                colors.on_primary_container,
                                pill_x + 9.5,
                                py + 1.5,
                            );
                            pill_x += pw + 8.0;
                        } else {
                            // Inactive dot
                            let dot_r = 3.5;
                            let dy = Self::HEIGHT as f64 / 2.0;
                            Self::set_source_color(&cr, colors.outline_variant);
                            cr.arc(pill_x + dot_r, dy, dot_r, 0.0, 2.0 * std::f64::consts::PI);
                            let _ = cr.fill();
                            pill_x += dot_r * 2.0 + 8.0;
                        }
                    }

                    // 4. Centered active window title
                    if !title_str.is_empty() {
                        let _max_title_w = 400.0;
                        let trunc_title = if title_str.len() > 50 {
                            format!("{}...", &title_str[..47])
                        } else {
                            title_str.clone()
                        };
                        let font = pango::FontDescription::from_string("sans medium 12px");
                        let layout = pangocairo::functions::create_layout(&cr);
                        layout.set_font_description(Some(&font));
                        layout.set_text(&trunc_title);
                        let (tw, th) = layout.pixel_size();
                        let title_x = ((screen_w - tw as f64) / 2.0).max(pill_x + 10.0);
                        let title_y = (Self::HEIGHT as f64 - th as f64) / 2.0;
                        let tw_f64 = tw as f64;
                        if title_x + tw_f64 < screen_w - 200.0 {
                            Self::set_source_color(&cr, colors.on_surface);
                            cr.move_to(title_x, title_y);
                            pangocairo::functions::show_layout(&cr, &layout);
                        }
                    }

                    // 5. Unified Status Chip (top-right)
                    let sys_info = super::system::SystemInfo::probe();
                    let chip_w = if sys_info.battery.is_some() { 170.0 } else { 96.0 };
                    let chip_h = 24.0;
                    let chip_x = screen_w - chip_w - 12.0;
                    let chip_y = (Self::HEIGHT as f64 - chip_h) / 2.0;

                    Self::set_source_color(&cr, colors.surface_container_high);
                    Self::draw_rounded_rect(&cr, chip_x, chip_y, chip_w, chip_h, 12.0);
                    let _ = cr.fill();

                    // Network icon (Ethernet vs Wi-Fi)
                    match &sys_info.network {
                        super::system::NetworkKind::Ethernet(_) => {
                            let ex = chip_x + 10.0;
                            let ey = chip_y + 6.0;
                            Self::set_source_color(&cr, colors.primary);
                            Self::draw_rounded_rect(&cr, ex, ey, 14.0, 12.0, 2.5);
                            cr.set_line_width(1.3);
                            let _ = cr.stroke();
                            cr.rectangle(ex + 4.0, ey + 8.5, 6.0, 3.5);
                            let _ = cr.fill();
                        }
                        super::system::NetworkKind::Wifi(_) => {
                            Self::set_source_color(&cr, colors.primary);
                            let wx = chip_x + 16.0;
                            let wy = chip_y + 16.0;
                            cr.arc(wx, wy, 2.0, 0.0, 2.0 * std::f64::consts::PI);
                            let _ = cr.fill();
                            cr.set_line_width(1.5);
                            cr.arc(wx, wy, 5.0, -std::f64::consts::PI * 0.75, -std::f64::consts::PI * 0.25);
                            let _ = cr.stroke();
                            cr.arc(wx, wy, 8.5, -std::f64::consts::PI * 0.75, -std::f64::consts::PI * 0.25);
                            let _ = cr.stroke();
                        }
                        super::system::NetworkKind::Disconnected => {
                            Self::set_source_color(&cr, colors.outline);
                            let wx = chip_x + 16.0;
                            let wy = chip_y + 12.0;
                            cr.arc(wx, wy, 3.0, 0.0, 2.0 * std::f64::consts::PI);
                            let _ = cr.fill();
                        }
                    }

                    // Battery icon (only if battery actually exists on system)
                    if let Some(bat) = sys_info.battery {
                        let bx = chip_x + 34.0;
                        let by = chip_y + 7.0;
                        Self::set_source_color(&cr, colors.outline);
                        Self::draw_rounded_rect(&cr, bx, by, 18.0, 10.0, 2.0);
                        cr.set_line_width(1.2);
                        let _ = cr.stroke();
                        cr.rectangle(bx + 18.0, by + 3.0, 2.0, 4.0);
                        let _ = cr.fill();
                        let fill_w = (14.0 * (bat.capacity as f64 / 100.0)).clamp(2.0, 14.0);
                        Self::set_source_color(&cr, colors.primary);
                        cr.rectangle(bx + 2.0, by + 2.0, fill_w, 6.0);
                        let _ = cr.fill();

                        let cap_text = format!("{}%", bat.capacity);
                        Self::draw_text(
                            &cr,
                            &cap_text,
                            "sans 10px",
                            colors.on_surface_variant,
                            bx + 26.0,
                            chip_y + 5.0,
                        );
                    }

                    // Clock HH:MM
                    let clock_x = if sys_info.battery.is_some() {
                        chip_x + 115.0
                    } else {
                        chip_x + 36.0
                    };
                    Self::draw_text(
                        &cr,
                        &time_str,
                        "sans bold 12px",
                        colors.on_surface,
                        clock_x,
                        chip_y + 3.5,
                    );

                    drop(cr);
                    surface.flush();
                    if let Ok(data) = surface.take_data() {
                        let buffer = MemoryBuffer::new(
                            data.to_vec(),
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

    fn set_source_color(cr: &cairo::Context, c: [f64; 4]) {
        cr.set_source_rgba(c[0], c[1], c[2], c[3]);
    }

    fn draw_rounded_rect(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let r = r.min(w / 2.0).min(h / 2.0);
        let deg = std::f64::consts::PI / 180.0;
        cr.new_sub_path();
        cr.arc(x + w - r, y + r, r, -90.0 * deg, 0.0 * deg);
        cr.arc(x + w - r, y + h - r, r, 0.0 * deg, 90.0 * deg);
        cr.arc(x + r, y + h - r, r, 90.0 * deg, 180.0 * deg);
        cr.arc(x + r, y + r, r, 180.0 * deg, 270.0 * deg);
        cr.close_path();
    }

    fn draw_text(
        cr: &cairo::Context,
        text: &str,
        font_desc: &str,
        color: [f64; 4],
        x: f64,
        y: f64,
    ) {
        Self::set_source_color(cr, color);
        let font = pango::FontDescription::from_string(font_desc);
        let layout = pangocairo::functions::create_layout(cr);
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        cr.move_to(x, y);
        pangocairo::functions::show_layout(cr, &layout);
    }
}
