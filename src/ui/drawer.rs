use std::time::Instant;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Transform};

use crate::config::theme::M3Colors;
use crate::config::Config;
use crate::shell::animation::AnimatedFloat;
use crate::ui::audio::AudioManager;
use crate::ui::shader::MaterialShaderPipeline;
use crate::ui::text::FontRenderer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTileId {
    Wifi,
    Bluetooth,
    NightLight,
    PowerProfile,
}

#[derive(Debug, Clone)]
pub struct QuickTile {
    pub id: QuickTileId,
    pub active: bool,
    pub title: &'static str,
    pub subtitle: String,
    pub press_scale: AnimatedFloat,
    pub is_pressed: bool,
}

impl QuickTile {
    pub fn new(id: QuickTileId, title: &'static str, subtitle: &str, active: bool, duration_ms: u64) -> Self {
        Self {
            id,
            active,
            title,
            subtitle: subtitle.to_string(),
            press_scale: AnimatedFloat::new(1.0, duration_ms),
            is_pressed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapsuleSlider {
    pub value: f32, // 0.0 to 1.0
    pub is_dragging: bool,
    pub last_tick_step: i32,
}

impl CapsuleSlider {
    pub fn new(initial: f32) -> Self {
        let step = (initial * 10.0).round() as i32;
        Self {
            value: initial.clamp(0.0, 1.0),
            is_dragging: false,
            last_tick_step: step,
        }
    }
}

pub struct QuickSettingsDrawer {
    pub is_open: bool,
    pub open_progress: AnimatedFloat,
    pub width: i32,
    pub height: i32,
    pub tiles: [QuickTile; 4],
    pub brightness: CapsuleSlider,
    pub volume: CapsuleSlider,
    pub text_buffer: Option<MemoryRenderBuffer>,
    pub buffer_dirty: bool,
    pub last_rendered_minute: i32,
}

impl QuickSettingsDrawer {
    pub const CARD_WIDTH: i32 = 360;
    pub const CARD_HEIGHT: i32 = 380;

    pub fn new(config: &Config) -> Self {
        let ms = config.animation_duration_ms;
        let tiles = [
            QuickTile::new(QuickTileId::Wifi, "Интернет", "Wi-Fi (Подключено)", true, ms),
            QuickTile::new(QuickTileId::Bluetooth, "Bluetooth", "Включено", true, ms),
            QuickTile::new(QuickTileId::NightLight, "Ночной режим", "Выключено", false, ms),
            QuickTile::new(QuickTileId::PowerProfile, "Режим питания", "Производительность", true, ms),
        ];

        Self {
            is_open: false,
            open_progress: AnimatedFloat::new(0.0, ms),
            width: Self::CARD_WIDTH,
            height: Self::CARD_HEIGHT,
            tiles,
            brightness: CapsuleSlider::new(0.75),
            volume: CapsuleSlider::new(0.60),
            text_buffer: None,
            buffer_dirty: true,
            last_rendered_minute: -1,
        }
    }

    pub fn toggle(&mut self, audio: &AudioManager) {
        if self.is_open {
            self.close(audio);
        } else {
            self.open(audio);
        }
    }

    pub fn open(&mut self, audio: &AudioManager) {
        if !self.is_open {
            self.is_open = true;
            self.open_progress.retarget(1.0);
            self.buffer_dirty = true;
            audio.play_pop();
        }
    }

    pub fn close(&mut self, audio: &AudioManager) {
        if self.is_open {
            self.is_open = false;
            self.open_progress.retarget(0.0);
            self.buffer_dirty = true;
            audio.play_click();
        }
    }

    pub fn update_animations(&mut self, now: Instant) -> bool {
        let mut animating = self.open_progress.is_animating(now);
        for tile in &mut self.tiles {
            if tile.press_scale.is_animating(now) {
                animating = true;
            }
        }
        animating
    }

    /// Calculate drawer geometry on screen (top-right floating panel)
    pub fn drawer_rect(&self, screen_w: i32, top_bar_h: i32) -> Rectangle<i32, Logical> {
        let margin_right = 16;
        let margin_top = top_bar_h + 8;
        let x = screen_w - self.width - margin_right;
        let y = margin_top;

        Rectangle::new(Point::from((x, y)), (self.width, self.height).into())
    }

    /// Handle pointer down inside the drawer
    pub fn on_pointer_down(&mut self, pt: Point<f64, Logical>, screen_w: i32, top_bar_h: i32, audio: &AudioManager) -> bool {
        if self.open_progress.value() < 0.1 {
            return false;
        }

        let rect = self.drawer_rect(screen_w, top_bar_h);
        let rel_x = pt.x as i32 - rect.loc.x;
        let rel_y = pt.y as i32 - rect.loc.y;

        // Check if inside drawer boundary
        if rel_x < 0 || rel_x > rect.size.w || rel_y < 0 || rel_y > rect.size.h {
            if self.is_open {
                // Click outside closes drawer
                self.toggle(audio);
                return true;
            }
            return false;
        }

        let slider_w = rect.size.w - 32;
        let slider_x = 16;

        // 1. Header quick action buttons (y: 14..48)
        // Settings button: x: 280..312
        if rel_x >= 280 && rel_x <= 312 && rel_y >= 14 && rel_y <= 48 {
            audio.play_click();
            let _ = std::process::Command::new("sh").arg("-c").arg("gnome-control-center || kcmshell6 || xfce4-settings-manager || echo 'settings'").spawn();
            return true;
        }
        // Power button: x: 314..348
        if rel_x >= 314 && rel_x <= 348 && rel_y >= 14 && rel_y <= 48 {
            audio.play_click();
            let _ = std::process::Command::new("sh").arg("-c").arg("wlogout || loginctl lock-session || echo 'power'").spawn();
            return true;
        }

        // 2. Brightness slider (y: 56..100)
        if rel_x >= slider_x && rel_x <= slider_x + slider_w && rel_y >= 56 && rel_y <= 100 {
            self.brightness.is_dragging = true;
            let ratio = (rel_x - slider_x) as f32 / slider_w as f32;
            self.brightness.value = ratio.clamp(0.0, 1.0);
            self.buffer_dirty = true;
            audio.play_tick();
            return true;
        }

        // 3. Volume slider (y: 110..154)
        if rel_x >= slider_x && rel_x <= slider_x + slider_w && rel_y >= 110 && rel_y <= 154 {
            self.volume.is_dragging = true;
            let ratio = (rel_x - slider_x) as f32 / slider_w as f32;
            self.volume.value = ratio.clamp(0.0, 1.0);
            self.buffer_dirty = true;
            audio.play_tick();
            return true;
        }

        // 4. 2-Column Tile Grid (y: 164..308)
        let tile_w = (slider_w - 12) / 2;
        let tile_h = 68;
        let start_y = 164;

        for row in 0..2 {
            for col in 0..2 {
                let idx = (row * 2 + col) as usize;
                let tx = slider_x + col * (tile_w + 12);
                let ty = start_y + row * (tile_h + 8);

                // Forgiving hitbox padding (+2px)
                if rel_x >= tx - 2 && rel_x <= tx + tile_w + 2 && rel_y >= ty - 2 && rel_y <= ty + tile_h + 2 {
                    self.tiles[idx].is_pressed = true;
                    self.tiles[idx].press_scale.retarget(0.97);
                    self.buffer_dirty = true;
                    return true;
                }
            }
        }

        true
    }

    /// Handle pointer motion for slider dragging
    pub fn on_pointer_motion(&mut self, pt: Point<f64, Logical>, screen_w: i32, top_bar_h: i32, audio: &AudioManager) {
        let rect = self.drawer_rect(screen_w, top_bar_h);
        let rel_x = pt.x as i32 - rect.loc.x;
        let slider_w = rect.size.w - 32;
        let slider_x = 16;

        if self.brightness.is_dragging {
            let ratio = (rel_x - slider_x) as f32 / slider_w as f32;
            self.brightness.value = ratio.clamp(0.0, 1.0);
            self.buffer_dirty = true;
            let step = (self.brightness.value * 10.0).round() as i32;
            if step != self.brightness.last_tick_step {
                self.brightness.last_tick_step = step;
                audio.play_tick();
            }
        }

        if self.volume.is_dragging {
            let ratio = (rel_x - slider_x) as f32 / slider_w as f32;
            self.volume.value = ratio.clamp(0.0, 1.0);
            self.buffer_dirty = true;
            let step = (self.volume.value * 10.0).round() as i32;
            if step != self.volume.last_tick_step {
                self.volume.last_tick_step = step;
                audio.play_tick();
            }
        }
    }

    /// Handle pointer up to toggle pills and release sliders
    pub fn on_pointer_up(&mut self, audio: &AudioManager) {
        self.brightness.is_dragging = false;
        self.volume.is_dragging = false;

        for tile in &mut self.tiles {
            if tile.is_pressed {
                tile.is_pressed = false;
                tile.press_scale.retarget(1.0);
                tile.active = !tile.active;
                self.buffer_dirty = true;
                audio.play_click();

                // Update status label in Russian
                match tile.id {
                    QuickTileId::Wifi => {
                        tile.subtitle = if tile.active { "Wi-Fi (Подключено)".into() } else { "Отключено".into() };
                    }
                    QuickTileId::Bluetooth => {
                        tile.subtitle = if tile.active { "Включено".into() } else { "Выключено".into() };
                    }
                    QuickTileId::NightLight => {
                        tile.subtitle = if tile.active { "Включено".into() } else { "Выключено".into() };
                    }
                    QuickTileId::PowerProfile => {
                        tile.subtitle = if tile.active { "Производительность".into() } else { "Баланс".into() };
                    }
                }
            }
        }
    }

    /// Render shader elements (card, sliders, tiles, header buttons) in FRONT-TO-BACK order
    pub fn render_shader_elements(
        &self,
        pipe: &MaterialShaderPipeline,
        screen_w: i32,
        top_bar_h: i32,
        colors: &M3Colors,
    ) -> Vec<PixelShaderElement> {
        let mut elements = Vec::new();
        let progress = self.open_progress.value();
        if progress < 0.01 {
            return elements;
        }

        let drect = self.drawer_rect(screen_w, top_bar_h);
        let d_alpha = progress;
        let slider_w = drect.size.w - 32;
        let slider_h = 44;
        let slider_x = drect.loc.x + 16;
        let tile_w = (slider_w - 12) / 2;
        let tile_h = 68;
        let start_y = drect.loc.y + 164;

        let primary_color = M3Colors::hex_to_rgba(&colors.primary);
        let high_color = M3Colors::hex_to_rgba(&colors.surface_container_high);
        let card_color = M3Colors::hex_to_rgba(&colors.surface_container);
        let border_color = M3Colors::hex_to_rgba(&colors.outline_variant);

        // 1. 2-Column Tile Grid (4 large pill cards in front)
        for row in 0..2 {
            for col in 0..2 {
                let idx = row * 2 + col;
                let tile = &self.tiles[idx];
                let scale = tile.press_scale.value();

                let w = (tile_w as f32 * scale) as i32;
                let h = (tile_h as f32 * scale) as i32;
                let offset_x = (tile_w - w) / 2;
                let offset_y = (tile_h - h) / 2;

                let tx = slider_x + col as i32 * (tile_w + 12) + offset_x;
                let ty = start_y + row as i32 * (tile_h + 8) + offset_y;
                let trect = Rectangle::new(Point::from((tx, ty)), (w, h).into());

                let tile_color = if tile.active { primary_color } else { high_color };
                elements.push(pipe.create_pill_element(trect, 22.0, tile_color, d_alpha));
            }
        }

        // 2. Volume Slider: active fill, then background
        let v_fill_w = ((slider_w as f32 * self.volume.value) as i32).max(44);
        let v_y = drect.loc.y + 110;
        let v_fill_rect = Rectangle::new(Point::from((slider_x, v_y)), (v_fill_w, slider_h).into());
        elements.push(pipe.create_pill_element(v_fill_rect, 22.0, primary_color, d_alpha));

        let v_bg_rect = Rectangle::new(Point::from((slider_x, v_y)), (slider_w, slider_h).into());
        elements.push(pipe.create_pill_element(v_bg_rect, 22.0, high_color, d_alpha));

        // 3. Brightness Slider: active fill, then background
        let b_fill_w = ((slider_w as f32 * self.brightness.value) as i32).max(44);
        let b_y = drect.loc.y + 56;
        let b_fill_rect = Rectangle::new(Point::from((slider_x, b_y)), (b_fill_w, slider_h).into());
        elements.push(pipe.create_pill_element(b_fill_rect, 22.0, primary_color, d_alpha));

        let b_bg_rect = Rectangle::new(Point::from((slider_x, b_y)), (slider_w, slider_h).into());
        elements.push(pipe.create_pill_element(b_bg_rect, 22.0, high_color, d_alpha));

        // 4. Header action buttons: Settings and Power pills
        let settings_rect = Rectangle::new(Point::from((drect.loc.x + 282, drect.loc.y + 16)), (30, 30).into());
        elements.push(pipe.create_pill_element(settings_rect, 15.0, high_color, d_alpha));

        let power_rect = Rectangle::new(Point::from((drect.loc.x + 316, drect.loc.y + 16)), (30, 30).into());
        elements.push(pipe.create_pill_element(power_rect, 15.0, high_color, d_alpha));

        // 5. Footer mini-card
        let footer_rect = Rectangle::new(Point::from((slider_x, drect.loc.y + 326)), (slider_w, 36).into());
        elements.push(pipe.create_pill_element(footer_rect, 14.0, high_color, d_alpha * 0.7));

        // 6. Card border
        elements.push(pipe.create_border_element(drect, 28.0, 1.5, border_color, [0.0, 0.0, 0.0, 0.0], d_alpha));

        // 7. Card background
        elements.push(pipe.create_pill_element(drect, 28.0, card_color, d_alpha));

        // 8. Elevation drop shadow
        elements.push(pipe.create_shadow_element(drect, 28.0, 24.0, [0.0, 0.0, 0.0, 0.65], d_alpha));

        elements
    }

    /// Render rasterized typography, labels, percentages, and icons on the drawer
    pub fn render_text_element(
        &mut self,
        renderer: &mut GlesRenderer,
        font: &FontRenderer,
        screen_w: i32,
        top_bar_h: i32,
        colors: &M3Colors,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        let progress = self.open_progress.value();
        if progress < 0.01 {
            return None;
        }

        let rect = self.drawer_rect(screen_w, top_bar_h);
        let w = rect.size.w;
        let h = rect.size.h;

        let now = unsafe { libc::time(std::ptr::null_mut()) };
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&now, &mut tm) };
        let current_minute = tm.tm_min;
        if current_minute != self.last_rendered_minute {
            self.buffer_dirty = true;
            self.last_rendered_minute = current_minute;
        }

        if self.text_buffer.is_none() || self.buffer_dirty {
            let mut mem_buf = self.text_buffer.take().unwrap_or_else(|| {
                MemoryRenderBuffer::new(
                    Fourcc::Abgr8888,
                    (w, h),
                    1,
                    Transform::Normal,
                    None,
                )
            });

            let on_surface = M3Colors::hex_to_rgba_u8(&colors.on_surface);
            let on_primary = M3Colors::hex_to_rgba_u8(&colors.on_primary);
            let outline = M3Colors::hex_to_rgba_u8(&colors.outline);
            let primary = M3Colors::hex_to_rgba_u8(&colors.primary);
            let error = M3Colors::hex_to_rgba_u8(&colors.error);

            let (time_str, date_str) = get_local_time_strings();
            let b_val = (self.brightness.value * 100.0).round() as i32;
            let v_val = (self.volume.value * 100.0).round() as i32;
            let b_pct_str = format!("{}%", b_val);
            let v_pct_str = format!("{}%", v_val);

            let tiles_snapshot = self.tiles.clone();

            {
                let mut ctx = mem_buf.render();
                let _ = ctx.draw(|slice| {
                    slice.fill(0);

                    // 1. Header (y: 16..50)
                    font.draw_text(slice, w, h, &time_str, 20, 18, 22.0, on_surface);
                    font.draw_text(slice, w, h, &date_str, 88, 24, 12.0, outline);

                    // Settings icon \u{F0493} (󰒓)
                    font.draw_text(slice, w, h, "\u{F0493}", 289, 23, 16.0, on_surface);
                    // Power icon \u{F0425} (󰐥)
                    font.draw_text(slice, w, h, "\u{F0425}", 323, 23, 16.0, error);

                    // 2. Brightness Slider (y: 56..100)
                    // Sun icon inside slider active fill on the left
                    font.draw_text(slice, w, h, "\u{F00E0}", 28, 69, 18.0, on_primary);
                    font.draw_text(slice, w, h, "Яркость", 54, 71, 13.0, on_primary);
                    font.draw_text(slice, w, h, &b_pct_str, 304, 71, 12.0, on_surface);

                    // 3. Volume Slider (y: 110..154)
                    // Speaker icon inside slider active fill on the left
                    font.draw_text(slice, w, h, "\u{F057E}", 28, 123, 18.0, on_primary);
                    font.draw_text(slice, w, h, "Громкость", 54, 125, 13.0, on_primary);
                    font.draw_text(slice, w, h, &v_pct_str, 304, 125, 12.0, on_surface);

                    // 4. 2-Column Tile Grid (y: 164..308)
                    let slider_w = w - 32;
                    let tile_w = (slider_w - 12) / 2;
                    let tile_h = 68;
                    let start_y = 164;

                    for row in 0..2 {
                        for col in 0..2 {
                            let idx = (row * 2 + col) as usize;
                            let tile = &tiles_snapshot[idx];
                            let tx = 16 + col * (tile_w + 12);
                            let ty = start_y + row * (tile_h + 8);

                            let (icon_color, title_color, sub_color) = if tile.active {
                                (on_primary, on_primary, [on_primary[0], on_primary[1], on_primary[2], 210])
                            } else {
                                (primary, on_surface, outline)
                            };

                            let icon_str = match tile.id {
                                QuickTileId::Wifi => "\u{F0928}",
                                QuickTileId::Bluetooth => "\u{F00AF}",
                                QuickTileId::NightLight => "\u{F0594}",
                                QuickTileId::PowerProfile => "\u{F037A}",
                            };

                            font.draw_text(slice, w, h, icon_str, tx + 12, ty + 23, 20.0, icon_color);
                            font.draw_text(slice, w, h, tile.title, tx + 40, ty + 18, 13.0, title_color);
                            font.draw_text(slice, w, h, &tile.subtitle, tx + 40, ty + 38, 10.0, sub_color);
                        }
                    }

                    // 5. Footer info (y: 326..362)
                    font.draw_text(slice, w, h, "\u{F0007} Максим", 28, 336, 12.0, on_surface);
                    font.draw_text(slice, w, h, "material-wm • 85%", 220, 336, 11.0, outline);

                    Ok::<_, ()>(vec![Rectangle::from_size((w, h).into())])
                });
            }

            self.text_buffer = Some(mem_buf);
            self.buffer_dirty = false;
        }

        let mem_buf = self.text_buffer.as_ref()?;
        let loc = Point::from((rect.loc.x as f64, rect.loc.y as f64));
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            loc,
            mem_buf,
            Some(progress),
            None,
            None,
            Kind::Unspecified,
        )
        .ok()
    }
}

pub fn get_local_time_strings() -> (String, String) {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };

    let time_str = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
    let weekdays = ["Вс", "Пн", "Вт", "Ср", "Чт", "Пт", "Сб"];
    let months = [
        "янв", "фев", "мар", "апр", "май", "июн",
        "июл", "авг", "сен", "окт", "ноя", "дек",
    ];
    let wday = weekdays.get(tm.tm_wday as usize).unwrap_or(&"");
    let mon = months.get(tm.tm_mon as usize).unwrap_or(&"");
    let date_str = format!("{}, {:02} {}", wday, tm.tm_mday, mon);

    (time_str, date_str)
}
