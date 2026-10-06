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
use crate::ui::material::audio::AudioManager;
use crate::ui::material::theme::M3Colors;
use crate::utils::output_size;

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
}

pub struct CapsuleSlider {
    pub value: f32, // 0.0 to 1.0
    pub is_dragging: bool,
    pub last_tick_step: i32,
}

pub struct MaterialQuickSettingsDrawer {
    pub is_open: bool,
    pub progress: f64, // 0.0 to 1.0
    pub target_progress: f64,
    pub tiles: [QuickTile; 4],
    pub brightness: CapsuleSlider,
    pub volume: CapsuleSlider,
    last_minute: i32,
    dirty: bool,
    cached_buffer: Option<TextureBuffer<GlesTexture>>,
}

impl MaterialQuickSettingsDrawer {
    pub const CARD_W: f64 = 360.0;
    pub const CARD_H: f64 = 380.0;

    pub fn new() -> Self {
        let sys_info = super::system::SystemInfo::probe();
        let (net_title, net_sub, net_active) = match &sys_info.network {
            super::system::NetworkKind::Ethernet(iface) => ("Ethernet", format!("Подключено ({})", iface), true),
            super::system::NetworkKind::Wifi(iface) => ("Wi-Fi", format!("Подключено ({})", iface), true),
            super::system::NetworkKind::Disconnected => ("Сеть", "Отключено".into(), false),
        };

        let tiles = [
            QuickTile {
                id: QuickTileId::Wifi,
                active: net_active,
                title: net_title,
                subtitle: net_sub,
            },
            QuickTile {
                id: QuickTileId::Bluetooth,
                active: true,
                title: "Bluetooth",
                subtitle: "Включено".into(),
            },
            QuickTile {
                id: QuickTileId::NightLight,
                active: false,
                title: "Ночной режим",
                subtitle: "Выключено".into(),
            },
            QuickTile {
                id: QuickTileId::PowerProfile,
                active: true,
                title: "Режим питания",
                subtitle: "Производительность".into(),
            },
        ];

        Self {
            is_open: false,
            progress: 0.0,
            target_progress: 0.0,
            tiles,
            brightness: CapsuleSlider {
                value: 0.75,
                is_dragging: false,
                last_tick_step: 7,
            },
            volume: CapsuleSlider {
                value: 0.60,
                is_dragging: false,
                last_tick_step: 6,
            },
            last_minute: -1,
            dirty: true,
            cached_buffer: None,
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
            self.target_progress = 1.0;
            let sys_info = super::system::SystemInfo::probe();
            match &sys_info.network {
                super::system::NetworkKind::Ethernet(iface) => {
                    self.tiles[0].title = "Ethernet";
                    self.tiles[0].subtitle = format!("Подключено ({})", iface);
                    self.tiles[0].active = true;
                }
                super::system::NetworkKind::Wifi(iface) => {
                    self.tiles[0].title = "Wi-Fi";
                    self.tiles[0].subtitle = format!("Подключено ({})", iface);
                    self.tiles[0].active = true;
                }
                super::system::NetworkKind::Disconnected => {
                    self.tiles[0].title = "Сеть";
                    self.tiles[0].subtitle = "Отключено".into();
                    self.tiles[0].active = false;
                }
            }
            self.dirty = true;
            audio.play_click();
        }
    }

    pub fn close(&mut self, audio: &AudioManager) {
        if self.is_open {
            self.is_open = false;
            self.target_progress = 0.0;
            self.brightness.is_dragging = false;
            self.volume.is_dragging = false;
            self.dirty = true;
            audio.play_click();
        }
    }

    pub fn is_animating(&self) -> bool {
        (self.target_progress - self.progress).abs() > 0.005
    }

    pub fn on_pointer_down(&mut self, x: f64, y: f64, screen_w: f64, audio: &AudioManager) -> bool {
        if !self.is_open {
            return false;
        }

        let card_x = screen_w - Self::CARD_W - 16.0;
        let card_y = 46.0;
        let slider_x = card_x + 16.0;
        let slider_w = Self::CARD_W - 32.0;

        // Brightness slider (y: card_y + 56..100)
        let b_y = card_y + 56.0;
        if x >= slider_x && x <= slider_x + slider_w && y >= b_y && y <= b_y + 44.0 {
            self.brightness.is_dragging = true;
            let ratio = (x - slider_x) / slider_w;
            self.brightness.value = (ratio as f32).clamp(0.0, 1.0);
            self.dirty = true;
            audio.play_tick();
            return true;
        }

        // Volume slider (y: card_y + 110..154)
        let v_y = card_y + 110.0;
        if x >= slider_x && x <= slider_x + slider_w && y >= v_y && y <= v_y + 44.0 {
            self.volume.is_dragging = true;
            let ratio = (x - slider_x) / slider_w;
            self.volume.value = (ratio as f32).clamp(0.0, 1.0);
            self.dirty = true;
            audio.play_tick();
            return true;
        }

        // 4 Quick Tiles (y: card_y + 164..316)
        let tile_w = (slider_w - 12.0) / 2.0;
        let tile_h = 68.0;
        let start_y = card_y + 164.0;
        for row in 0..2 {
            for col in 0..2 {
                let tx = slider_x + col as f64 * (tile_w + 12.0);
                let ty = start_y + row as f64 * (tile_h + 8.0);
                if x >= tx && x <= tx + tile_w && y >= ty && y <= ty + tile_h {
                    let idx = row * 2 + col;
                    self.tiles[idx].active = !self.tiles[idx].active;
                    match self.tiles[idx].id {
                        QuickTileId::Wifi => {
                            let sys_info = super::system::SystemInfo::probe();
                            match &sys_info.network {
                                super::system::NetworkKind::Ethernet(iface) => {
                                    self.tiles[idx].title = "Ethernet";
                                    self.tiles[idx].subtitle = if self.tiles[idx].active {
                                        format!("Подключено ({})", iface)
                                    } else {
                                        "Отключено".into()
                                    };
                                }
                                super::system::NetworkKind::Wifi(iface) => {
                                    self.tiles[idx].title = "Wi-Fi";
                                    self.tiles[idx].subtitle = if self.tiles[idx].active {
                                        format!("Подключено ({})", iface)
                                    } else {
                                        "Отключено".into()
                                    };
                                }
                                super::system::NetworkKind::Disconnected => {
                                    self.tiles[idx].title = "Сеть";
                                    self.tiles[idx].subtitle = "Отключено".into();
                                }
                            }
                        }
                        QuickTileId::Bluetooth => {
                            self.tiles[idx].subtitle = if self.tiles[idx].active {
                                "Включено".into()
                            } else {
                                "Выключено".into()
                            };
                        }
                        QuickTileId::NightLight => {
                            self.tiles[idx].subtitle = if self.tiles[idx].active {
                                "Включено".into()
                            } else {
                                "Выключено".into()
                            };
                        }
                        QuickTileId::PowerProfile => {
                            self.tiles[idx].subtitle = if self.tiles[idx].active {
                                "Производительность".into()
                            } else {
                                "Баланс".into()
                            };
                        }
                    }
                    self.dirty = true;
                    audio.play_click();
                    return true;
                }
            }
        }

        // Click outside card closes drawer
        if x < card_x || x > card_x + Self::CARD_W || y < card_y || y > card_y + Self::CARD_H {
            self.close(audio);
            return true;
        }

        true
    }

    pub fn on_pointer_motion(&mut self, x: f64, screen_w: f64, audio: &AudioManager) {
        if !self.is_open {
            return;
        }

        let card_x = screen_w - Self::CARD_W - 16.0;
        let slider_x = card_x + 16.0;
        let slider_w = Self::CARD_W - 32.0;

        if self.brightness.is_dragging {
            let ratio = (x - slider_x) / slider_w;
            self.brightness.value = (ratio as f32).clamp(0.0, 1.0);
            self.dirty = true;
            let step = (self.brightness.value * 10.0).round() as i32;
            if step != self.brightness.last_tick_step {
                self.brightness.last_tick_step = step;
                audio.play_tick();
            }
        }

        if self.volume.is_dragging {
            let ratio = (x - slider_x) / slider_w;
            self.volume.value = (ratio as f32).clamp(0.0, 1.0);
            self.dirty = true;
            let step = (self.volume.value * 10.0).round() as i32;
            if step != self.volume.last_tick_step {
                self.volume.last_tick_step = step;
                audio.play_tick();
            }
        }
    }

    pub fn on_pointer_up(&mut self) {
        self.brightness.is_dragging = false;
        self.volume.is_dragging = false;
    }

    pub fn render<R: NiriRenderer>(
        &mut self,
        renderer: &mut R,
        output: &Output,
        colors: &M3Colors,
    ) -> Option<PrimaryGpuTextureRenderElement> {
        if !self.is_open && self.progress < 0.01 {
            return None;
        }

        let diff = self.target_progress - self.progress;
        if diff.abs() > 0.005 {
            self.progress += diff * 0.28;
        } else {
            self.progress = self.target_progress;
        }

        if !self.is_open && self.progress < 0.01 {
            return None;
        }

        let scale = output.current_scale().fractional_scale();
        let size = output_size(output);
        let screen_w = size.w;

        let now = unsafe { libc::time(std::ptr::null_mut()) };
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&now, &mut tm) };
        let cur_minute = tm.tm_min;

        if self.cached_buffer.is_none() || self.dirty || self.last_minute != cur_minute {
            self.dirty = false;
            self.last_minute = cur_minute;

            let card_w = Self::CARD_W;
            let card_h = Self::CARD_H;
            let shadow_padding = 24.0;
            let total_w = card_w + shadow_padding * 2.0;
            let total_h = card_h + shadow_padding * 2.0;

            let phys_w = (total_w * scale).round() as i32;
            let phys_h = (total_h * scale).round() as i32;

            if let Ok(surface) = ImageSurface::create(cairo::Format::ARgb32, phys_w, phys_h) {
                if let Ok(cr) = cairo::Context::new(&surface) {
                    cr.scale(scale, scale);

                    let ox = shadow_padding;
                    let oy = shadow_padding;

                    // 1. Elevation drop shadow (multi-layer soft blur)
                    for (pad, alpha) in [(18.0, 0.05), (12.0, 0.10), (6.0, 0.18)] {
                        Self::set_source_color(&cr, [0.0, 0.0, 0.0, alpha]);
                        Self::draw_rounded_rect(
                            &cr,
                            ox - pad,
                            oy - pad + 6.0,
                            card_w + pad * 2.0,
                            card_h + pad * 2.0,
                            28.0 + pad,
                        );
                        let _ = cr.fill();
                    }

                    // 2. Card background & border
                    Self::set_source_color(&cr, colors.surface_container);
                    Self::draw_rounded_rect(&cr, ox, oy, card_w, card_h, 28.0);
                    let _ = cr.fill();

                    Self::set_source_color(&cr, colors.outline_variant);
                    Self::draw_rounded_rect(&cr, ox, oy, card_w, card_h, 28.0);
                    cr.set_line_width(1.2);
                    let _ = cr.stroke();

                    // 3. Header: Time & Date
                    let time_str = format!("{:02}:{:02}", tm.tm_hour, tm.tm_min);
                    let weekdays = ["Вс", "Пн", "Вт", "Ср", "Чт", "Пт", "Сб"];
                    let months = [
                        "янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт",
                        "ноя", "дек",
                    ];
                    let w_str = weekdays.get(tm.tm_wday as usize).unwrap_or(&"Вт");
                    let m_str = months.get(tm.tm_mon as usize).unwrap_or(&"окт");
                    let date_str = format!("{}, {:02} {}", w_str, tm.tm_mday, m_str);

                    Self::draw_text(
                        &cr,
                        &time_str,
                        "sans bold 24px",
                        colors.on_surface,
                        ox + 20.0,
                        oy + 16.0,
                    );
                    Self::draw_text(
                        &cr,
                        &date_str,
                        "sans 12px",
                        colors.on_surface_variant,
                        ox + 105.0,
                        oy + 25.0,
                    );

                    // Header buttons: Settings & Power
                    let set_x = ox + card_w - 78.0;
                    let pwr_x = ox + card_w - 42.0;
                    for btn_x in [set_x, pwr_x] {
                        Self::set_source_color(&cr, colors.surface_container_high);
                        cr.arc(btn_x + 15.0, oy + 28.0, 15.0, 0.0, 2.0 * std::f64::consts::PI);
                        let _ = cr.fill();
                    }
                    // Gear icon
                    Self::set_source_color(&cr, colors.on_surface_variant);
                    cr.arc(set_x + 15.0, oy + 28.0, 5.0, 0.0, 2.0 * std::f64::consts::PI);
                    cr.set_line_width(2.0);
                    let _ = cr.stroke();
                    // Power icon
                    cr.arc(
                        pwr_x + 15.0,
                        oy + 28.0,
                        5.5,
                        -std::f64::consts::PI * 0.3,
                        std::f64::consts::PI * 1.3,
                    );
                    let _ = cr.stroke();
                    cr.move_to(pwr_x + 15.0, oy + 22.0);
                    cr.line_to(pwr_x + 15.0, oy + 28.0);
                    let _ = cr.stroke();

                    // 4. Brightness Slider
                    let slider_w = card_w - 32.0;
                    let slider_x = ox + 16.0;
                    let b_y = oy + 56.0;
                    Self::set_source_color(&cr, colors.surface_container_high);
                    Self::draw_rounded_rect(&cr, slider_x, b_y, slider_w, 44.0, 22.0);
                    let _ = cr.fill();

                    let b_fill_w = ((slider_w as f32 * self.brightness.value) as f64).max(44.0);
                    Self::set_source_color(&cr, colors.primary);
                    Self::draw_rounded_rect(&cr, slider_x, b_y, b_fill_w, 44.0, 22.0);
                    let _ = cr.fill();

                    // Brightness icon (sun) + label
                    let b_icon_color = if self.brightness.value > 0.25 {
                        colors.on_primary
                    } else {
                        colors.on_surface
                    };
                    Self::set_source_color(&cr, b_icon_color);
                    cr.arc(slider_x + 22.0, b_y + 22.0, 6.0, 0.0, 2.0 * std::f64::consts::PI);
                    let _ = cr.fill();
                    Self::draw_text(
                        &cr,
                        "Яркость",
                        "sans bold 12px",
                        b_icon_color,
                        slider_x + 36.0,
                        b_y + 14.5,
                    );

                    let b_pct_str = format!("{}%", (self.brightness.value * 100.0).round() as i32);
                    Self::draw_text(
                        &cr,
                        &b_pct_str,
                        "sans medium 11px",
                        colors.on_surface_variant,
                        slider_x + slider_w - 36.0,
                        b_y + 15.0,
                    );

                    // 5. Volume Slider
                    let v_y = oy + 110.0;
                    Self::set_source_color(&cr, colors.surface_container_high);
                    Self::draw_rounded_rect(&cr, slider_x, v_y, slider_w, 44.0, 22.0);
                    let _ = cr.fill();

                    let v_fill_w = ((slider_w as f32 * self.volume.value) as f64).max(44.0);
                    Self::set_source_color(&cr, colors.primary);
                    Self::draw_rounded_rect(&cr, slider_x, v_y, v_fill_w, 44.0, 22.0);
                    let _ = cr.fill();

                    // Volume icon (speaker) + label
                    let v_icon_color = if self.volume.value > 0.25 {
                        colors.on_primary
                    } else {
                        colors.on_surface
                    };
                    Self::set_source_color(&cr, v_icon_color);
                    cr.move_to(slider_x + 18.0, v_y + 22.0);
                    cr.line_to(slider_x + 23.0, v_y + 16.0);
                    cr.line_to(slider_x + 23.0, v_y + 28.0);
                    cr.close_path();
                    let _ = cr.fill();
                    Self::draw_text(
                        &cr,
                        "Громкость",
                        "sans bold 12px",
                        v_icon_color,
                        slider_x + 36.0,
                        v_y + 14.5,
                    );

                    let v_pct_str = format!("{}%", (self.volume.value * 100.0).round() as i32);
                    Self::draw_text(
                        &cr,
                        &v_pct_str,
                        "sans medium 11px",
                        colors.on_surface_variant,
                        slider_x + slider_w - 36.0,
                        v_y + 15.0,
                    );

                    // 6. 2-Column Quick Tiles
                    let tile_w = (slider_w - 12.0) / 2.0;
                    let tile_h = 68.0;
                    let start_y = oy + 164.0;
                    for row in 0..2 {
                        for col in 0..2 {
                            let idx = row * 2 + col;
                            let tile = &self.tiles[idx];
                            let tx = slider_x + col as f64 * (tile_w + 12.0);
                            let ty = start_y + row as f64 * (tile_h + 8.0);

                            let (bg_color, fg_title, fg_sub) = if tile.active {
                                (colors.primary, colors.on_primary, colors.on_primary)
                            } else {
                                (
                                    colors.surface_container_high,
                                    colors.on_surface,
                                    colors.on_surface_variant,
                                )
                            };

                            Self::set_source_color(&cr, bg_color);
                            Self::draw_rounded_rect(&cr, tx, ty, tile_w, tile_h, 22.0);
                            let _ = cr.fill();

                            // Tile Icon
                            Self::set_source_color(&cr, fg_title);
                            match tile.id {
                                QuickTileId::Wifi => {
                                    let sys_info = super::system::SystemInfo::probe();
                                    match &sys_info.network {
                                        super::system::NetworkKind::Ethernet(_) => {
                                            let ex = tx + 14.0;
                                            let ey = ty + 28.0;
                                            Self::draw_rounded_rect(&cr, ex, ey, 16.0, 14.0, 2.5);
                                            cr.set_line_width(1.6);
                                            let _ = cr.stroke();
                                            cr.rectangle(ex + 4.5, ey + 10.0, 7.0, 4.0);
                                            let _ = cr.fill();
                                        }
                                        _ => {
                                            cr.arc(tx + 22.0, ty + 36.0, 2.0, 0.0, 2.0 * std::f64::consts::PI);
                                            let _ = cr.fill();
                                            cr.set_line_width(1.6);
                                            cr.arc(
                                                tx + 22.0,
                                                ty + 36.0,
                                                6.0,
                                                -std::f64::consts::PI * 0.75,
                                                -std::f64::consts::PI * 0.25,
                                            );
                                            let _ = cr.stroke();
                                        }
                                    }
                                }
                                QuickTileId::Bluetooth => {
                                    cr.set_line_width(1.8);
                                    cr.move_to(tx + 22.0, ty + 24.0);
                                    cr.line_to(tx + 22.0, ty + 44.0);
                                    cr.line_to(tx + 28.0, ty + 39.0);
                                    cr.line_to(tx + 17.0, ty + 29.0);
                                    cr.move_to(tx + 17.0, ty + 39.0);
                                    cr.line_to(tx + 28.0, ty + 29.0);
                                    cr.line_to(tx + 22.0, ty + 24.0);
                                    let _ = cr.stroke();
                                }
                                QuickTileId::NightLight => {
                                    cr.arc(
                                        tx + 22.0,
                                        ty + 34.0,
                                        8.0,
                                        std::f64::consts::PI * 0.2,
                                        std::f64::consts::PI * 1.5,
                                    );
                                    cr.close_path();
                                    let _ = cr.fill();
                                }
                                QuickTileId::PowerProfile => {
                                    Self::draw_rounded_rect(&cr, tx + 14.0, ty + 26.0, 16.0, 12.0, 2.0);
                                    cr.set_line_width(1.5);
                                    let _ = cr.stroke();
                                    cr.move_to(tx + 18.0, ty + 40.0);
                                    cr.line_to(tx + 26.0, ty + 40.0);
                                    let _ = cr.stroke();
                                }
                            }

                            Self::draw_text(
                                &cr,
                                tile.title,
                                "sans bold 12px",
                                fg_title,
                                tx + 38.0,
                                ty + 16.0,
                            );
                            Self::draw_text(
                                &cr,
                                &tile.subtitle,
                                "sans 10px",
                                fg_sub,
                                tx + 38.0,
                                ty + 36.0,
                            );
                        }
                    }

                    // 7. Footer mini-card
                    let footer_y = oy + 328.0;
                    Self::set_source_color(&cr, colors.surface_container_highest);
                    Self::draw_rounded_rect(&cr, slider_x, footer_y, slider_w, 36.0, 14.0);
                    let _ = cr.fill();

                    // User badge icon & name
                    Self::set_source_color(&cr, colors.primary);
                    cr.arc(slider_x + 20.0, footer_y + 18.0, 6.0, 0.0, 2.0 * std::f64::consts::PI);
                    let _ = cr.fill();
                    Self::draw_text(
                        &cr,
                        "Максим",
                        "sans bold 11px",
                        colors.on_surface,
                        slider_x + 34.0,
                        footer_y + 11.0,
                    );

                    let sys_info = super::system::SystemInfo::probe();
                    let footer_text = if let Some(bat) = sys_info.battery {
                        format!("material-wm • {}%", bat.capacity)
                    } else {
                        match &sys_info.network {
                            super::system::NetworkKind::Ethernet(iface) => format!("material-wm • {}", iface),
                            super::system::NetworkKind::Wifi(iface) => format!("material-wm • {}", iface),
                            super::system::NetworkKind::Disconnected => "material-wm • Офлайн".into(),
                        }
                    };

                    Self::draw_text(
                        &cr,
                        &footer_text,
                        "sans 11px",
                        colors.on_surface_variant,
                        slider_x + slider_w - 120.0,
                        footer_y + 11.0,
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
        let card_x = screen_w - Self::CARD_W - 16.0 - 24.0;
        let slide_y = (1.0 - self.progress) * -40.0;
        let card_y = 46.0 - 24.0 + slide_y;
        let elem = TextureRenderElement::from_texture_buffer(
            buffer,
            Point::new(card_x, card_y),
            self.progress as f32,
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
