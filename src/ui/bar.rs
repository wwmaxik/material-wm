use std::time::Instant;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::memory::{MemoryRenderBuffer, MemoryRenderBufferRenderElement};
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Transform};

use crate::config::theme::M3Colors;
use crate::config::Config;
use crate::shell::animation::AnimatedFloat;
use crate::ui::audio::AudioManager;
use crate::ui::drawer::get_local_time_strings;
use crate::ui::text::FontRenderer;

pub struct WorkspacePill {
    pub id: usize,
    pub width: AnimatedFloat,
}

impl WorkspacePill {
    pub fn new(id: usize, duration_ms: u64, is_active: bool) -> Self {
        let initial_w = if is_active { 28.0 } else { 10.0 };
        let mut width = AnimatedFloat::new(initial_w, duration_ms);
        width.set_immediate(initial_w);
        Self { id, width }
    }
}

pub struct StatusBar {
    pub height: i32,
    pub visible: bool,
    pub workspace_pills: Vec<WorkspacePill>,
    pub active_workspace: usize,
    pub quick_settings_pill_rect: Rectangle<i32, Logical>,
    pub clock_text: String,
    pub focused_title: String,
    pub battery_percent: u8,
    pub volume_percent: u8,
    pub text_buffer: Option<MemoryRenderBuffer>,
    pub buffer_dirty: bool,
    pub last_rendered_minute: i32,
}

impl StatusBar {
    pub fn new(config: &Config) -> Self {
        let mut pills = Vec::with_capacity(9);
        for i in 1..=9 {
            pills.push(WorkspacePill::new(i, config.animation_duration_ms, i == 1));
        }

        Self {
            height: config.bar_height,
            visible: true,
            workspace_pills: pills,
            active_workspace: 1,
            quick_settings_pill_rect: Rectangle::new(Point::from((0, 0)), (140, 24).into()),
            clock_text: "12:00".to_string(),
            focused_title: String::new(),
            battery_percent: 85,
            volume_percent: 60,
            text_buffer: None,
            buffer_dirty: true,
            last_rendered_minute: -1,
        }
    }

    pub fn set_active_workspace(&mut self, active: usize) {
        if self.active_workspace == active {
            return;
        }
        self.active_workspace = active;

        for pill in &mut self.workspace_pills {
            let target_w = if pill.id == active { 28.0 } else { 10.0 };
            pill.width.retarget(target_w);
        }
    }

    pub fn update_animations(&mut self, now: Instant) -> bool {
        let mut animating = false;
        for pill in &mut self.workspace_pills {
            if pill.width.is_animating(now) {
                animating = true;
            }
        }
        animating
    }

    /// Check if pointer clicked on status bar elements (Fitts's Law generous targets)
    pub fn on_pointer_down(
        &mut self,
        pt: Point<f64, Logical>,
        screen_w: i32,
        audio: &AudioManager,
    ) -> Option<StatusBarAction> {
        if !self.visible || pt.y > (self.height + 4) as f64 {
            return None;
        }

        let px = pt.x as i32;

        // 1. App Launcher button hit testing (leftmost pill, x: 0..46)
        if px >= 0 && px <= 46 {
            audio.play_click();
            return Some(StatusBarAction::ToggleAppLauncher);
        }

        // 2. Workspace pills hit testing (starting at x: 50)
        let mut pill_x = 50;
        for pill in &self.workspace_pills {
            let pw = pill.width.value() as i32;
            // Generous hitbox (+4px left/right, full bar height)
            if px >= pill_x - 4 && px <= pill_x + pw + 4 {
                let target_ws = pill.id;
                self.set_active_workspace(target_ws);
                audio.play_click();
                return Some(StatusBarAction::SwitchWorkspace(target_ws));
            }
            pill_x += pw + 8;
        }

        // 3. Quick Settings pill hit testing (right side)
        let qs_w = 160;
        let qs_x = screen_w - qs_w - 12;
        if px >= qs_x - 6 && px <= screen_w {
            audio.play_click();
            return Some(StatusBarAction::ToggleQuickSettings);
        }

        None
    }

    /// Render status bar text (workspace numbers, window title, clock, battery)
    pub fn render_text_element(
        &mut self,
        renderer: &mut GlesRenderer,
        font: &FontRenderer,
        screen_w: i32,
        colors: &M3Colors,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        if !self.visible {
            return None;
        }

        let w = screen_w;
        let h = self.height;

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
            let (time_str, _) = get_local_time_strings();

            let active_ws = self.active_workspace;
            let title = self.focused_title.clone();
            let bat_pct = self.battery_percent;

            {
                let mut ctx = mem_buf.render();
                let _ = ctx.draw(|slice| {
                    slice.fill(0);

                    // 1. Workspace active number inside elongated capsule
                    let mut pill_x = 50;
                    let pill_y = (h - 12) / 2;
                    for pill in &self.workspace_pills {
                        let pw = pill.width.value() as i32;
                        if pill.id == active_ws && pw >= 20 {
                            let id_str = format!("{}", pill.id);
                            let text_w = font.measure_text_width(&id_str, 9.0);
                            let tx = pill_x + (pw - text_w) / 2;
                            font.draw_text(slice, w, h, &id_str, tx, pill_y + 1, 9.0, on_primary);
                        }
                        pill_x += pw + 8;
                    }

                    // 2. Active Window Title (Centered)
                    if !title.is_empty() {
                        let max_chars = 36;
                        let display_title = if title.chars().count() > max_chars {
                            let truncated: String = title.chars().take(max_chars - 3).collect();
                            format!("{}...", truncated)
                        } else {
                            title
                        };
                        let title_w = font.measure_text_width(&display_title, 12.0);
                        let title_x = (w - title_w) / 2;
                        let title_y = (h - 12) / 2;
                        font.draw_text(slice, w, h, &display_title, title_x, title_y, 12.0, on_surface);
                    }

                    // 3. Quick Settings chip (Top Right): Wi-Fi, Battery, Clock
                    let qs_w = 170;
                    let qs_x = w - qs_w - 12;
                    let qs_y = (h - 14) / 2;

                    // Wi-Fi icon \u{F0928}
                    font.draw_text(slice, w, h, "\u{F0928}", qs_x + 12, qs_y - 1, 14.0, on_surface);

                    // Battery icon \u{F0079} + %
                    let bat_str = format!("{}%", bat_pct);
                    font.draw_text(slice, w, h, "\u{F0079}", qs_x + 36, qs_y - 1, 14.0, on_surface);
                    font.draw_text(slice, w, h, &bat_str, qs_x + 54, qs_y, 11.0, on_surface);

                    // Clock HH:MM
                    font.draw_text(slice, w, h, &time_str, qs_x + 110, qs_y, 12.0, on_surface);

                    Ok::<_, ()>(vec![Rectangle::from_size((w, h).into())])
                });
            }

            self.text_buffer = Some(mem_buf);
            self.buffer_dirty = false;
        }

        let mem_buf = self.text_buffer.as_ref()?;
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

pub enum StatusBarAction {
    SwitchWorkspace(usize),
    ToggleQuickSettings,
    ToggleAppLauncher,
}
