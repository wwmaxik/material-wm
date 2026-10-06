pub mod audio;
pub mod bar;
pub mod drawer;
pub mod launcher;
pub mod shader;
pub mod text;
pub mod wallpaper;

use std::time::Instant;
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Logical, Point, Rectangle, Size};

pub use audio::AudioManager;
pub use bar::{StatusBar, StatusBarAction};
pub use drawer::{QuickSettingsDrawer, QuickTileId};
pub use launcher::{AppLauncher, find_external_launchers, scan_applications};
pub use shader::MaterialShaderPipeline;
pub use text::FontRenderer;
pub use wallpaper::WallpaperManager;

use crate::config::theme::M3Colors;
use crate::config::Config;

pub struct UiManager {
    pub bar: StatusBar,
    pub drawer: QuickSettingsDrawer,
    pub launcher: AppLauncher,
    pub wallpaper: WallpaperManager,
    pub font: FontRenderer,
    pub audio: AudioManager,
    pub shader_pipeline: Option<MaterialShaderPipeline>,
    pub colors: M3Colors,
    pub border_radius: f32,
    pub border_width: f32,
}

impl UiManager {
    pub fn new(config: &Config) -> Self {
        Self {
            bar: StatusBar::new(config),
            drawer: QuickSettingsDrawer::new(config),
            launcher: AppLauncher::new(config),
            wallpaper: WallpaperManager::new(),
            font: FontRenderer::new(),
            audio: AudioManager::new(config.audio_feedback),
            shader_pipeline: None,
            colors: config.colors.clone(),
            border_radius: config.border_radius,
            border_width: config.border_width,
        }
    }

    pub fn init_shaders(&mut self, renderer: &mut GlesRenderer) {
        match MaterialShaderPipeline::new(renderer) {
            Ok(pipe) => {
                tracing::info!("Initialized Material You GLES SDF shader pipeline");
                self.shader_pipeline = Some(pipe);
            }
            Err(e) => {
                tracing::warn!("Failed to compile Material You SDF shaders: {}. Using software fallback.", e);
            }
        }
    }

    pub fn update_animations(&mut self, now: Instant) -> bool {
        let b = self.bar.update_animations(now);
        let d = self.drawer.update_animations(now);
        let l = self.launcher.update_animations(now);
        b || d || l
    }

    /// Generate window border element
    pub fn render_window_border(
        &self,
        rect: Rectangle<i32, Logical>,
        is_focused: bool,
        alpha: f32,
    ) -> Option<PixelShaderElement> {
        let Some(pipe) = &self.shader_pipeline else {
            return None;
        };

        let border_color = if is_focused {
            M3Colors::hex_to_rgba(&self.colors.primary)
        } else {
            M3Colors::hex_to_rgba(&self.colors.outline_variant)
        };
        let bg_color = [0.0, 0.0, 0.0, 0.0];

        Some(pipe.create_border_element(
            rect,
            self.border_radius,
            self.border_width,
            border_color,
            bg_color,
            alpha,
        ))
    }

    /// Generate window drop shadow element
    pub fn render_window_shadow(
        &self,
        rect: Rectangle<i32, Logical>,
        alpha: f32,
    ) -> Option<PixelShaderElement> {
        let Some(pipe) = &self.shader_pipeline else {
            return None;
        };

        let shadow_color = [0.0, 0.0, 0.0, 0.45];
        Some(pipe.create_shadow_element(rect, self.border_radius, 16.0, shadow_color, alpha))
    }

    /// Render UI overlay (Top Bar + Quick Settings Drawer + Launcher) into elements in strict FRONT-TO-BACK order
    /// Render shader elements for Application Launcher
    pub fn render_launcher_shaders(
        &self,
        screen_size: Size<i32, Logical>,
    ) -> Vec<PixelShaderElement> {
        let Some(pipe) = &self.shader_pipeline else {
            return Vec::new();
        };
        if self.launcher.open_progress.value() > 0.01 {
            self.launcher.render_shader_elements(pipe, screen_size, &self.colors)
        } else {
            Vec::new()
        }
    }

    /// Render shader elements for Quick Settings Drawer
    pub fn render_drawer_shaders(
        &self,
        screen_size: Size<i32, Logical>,
    ) -> Vec<PixelShaderElement> {
        let Some(pipe) = &self.shader_pipeline else {
            return Vec::new();
        };
        if self.drawer.open_progress.value() > 0.01 {
            self.drawer.render_shader_elements(pipe, screen_size.w, self.bar.height, &self.colors)
        } else {
            Vec::new()
        }
    }

    /// Render shader elements for Top Status Bar
    pub fn render_bar_shaders(
        &self,
        screen_size: Size<i32, Logical>,
    ) -> Vec<PixelShaderElement> {
        let mut elements = Vec::new();
        let Some(pipe) = &self.shader_pipeline else {
            return elements;
        };

        if self.bar.visible {
            let qs_w = 170;
            let qs_h = 24;
            let qs_x = screen_size.w - qs_w - 12;
            let qs_y = (self.bar.height - qs_h) / 2;

            // 1. Quick settings trigger pill capsule (top right)
            let qs_rect = Rectangle::new(Point::from((qs_x, qs_y)), (qs_w, qs_h).into());
            let qs_color = M3Colors::hex_to_rgba(&self.colors.surface_container_high);
            elements.push(pipe.create_pill_element(qs_rect, 12.0, qs_color, 1.0));

            // 2. Workspace stretching pills (starting at x: 50)
            let mut pill_x = 50;
            let pill_y = (self.bar.height - 12) / 2;
            for pill in &self.bar.workspace_pills {
                let pw = pill.width.value() as i32;
                let prect = Rectangle::new(Point::from((pill_x, pill_y)), (pw, 12).into());
                let pcolor = if pill.id == self.bar.active_workspace {
                    M3Colors::hex_to_rgba(&self.colors.primary)
                } else {
                    M3Colors::hex_to_rgba(&self.colors.outline_variant)
                };
                elements.push(pipe.create_pill_element(prect, 6.0, pcolor, 1.0));
                pill_x += pw + 8;
            }

            // 3. App Launcher trigger pill & grid icon dots (top left, x: 8)
            let launch_w = 34;
            let launch_h = 22;
            let launch_x = 8;
            let launch_y = (self.bar.height - launch_h) / 2;
            let dot_color = if self.launcher.is_open {
                M3Colors::hex_to_rgba(&self.colors.on_primary)
            } else {
                M3Colors::hex_to_rgba(&self.colors.primary)
            };
            // 4 dots in 2x2 grid inside launcher pill
            for r in 0..2 {
                for c in 0..2 {
                    let dx = launch_x + 12 + c * 6;
                    let dy = launch_y + 6 + r * 6;
                    elements.push(pipe.create_pill_element(
                        Rectangle::new(Point::from((dx, dy)), (4, 4).into()),
                        2.0,
                        dot_color,
                        1.0,
                    ));
                }
            }
            let launch_rect = Rectangle::new(Point::from((launch_x, launch_y)), (launch_w, launch_h).into());
            let launch_color = if self.launcher.is_open {
                M3Colors::hex_to_rgba(&self.colors.primary)
            } else {
                M3Colors::hex_to_rgba(&self.colors.surface_container_high)
            };
            elements.push(pipe.create_pill_element(launch_rect, 11.0, launch_color, 1.0));

            // 4. Subtle divider line under status bar
            let line_rect = Rectangle::new(Point::from((0, self.bar.height - 1)), (screen_size.w, 1).into());
            let line_color = M3Colors::hex_to_rgba(&self.colors.outline_variant);
            elements.push(pipe.create_pill_element(line_rect, 0.0, line_color, 0.5));

            // 5. Top Bar background
            let bar_rect = Rectangle::new(Point::from((0, 0)), (screen_size.w, self.bar.height).into());
            let bar_color = M3Colors::hex_to_rgba(&self.colors.surface_container);
            elements.push(pipe.create_pill_element(bar_rect, 0.0, bar_color, 0.96));
        }

        elements
    }

    /// Render rasterized text on the Application Launcher card
    pub fn render_launcher_text(
        &mut self,
        renderer: &mut GlesRenderer,
        screen_size: Size<i32, Logical>,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        self.launcher.render_text_element(renderer, &self.font, screen_size, &self.colors)
    }

    /// Render rasterized text on the Quick Settings Drawer
    pub fn render_drawer_text(
        &mut self,
        renderer: &mut GlesRenderer,
        screen_size: Size<i32, Logical>,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        self.drawer.render_text_element(renderer, &self.font, screen_size.w, self.bar.height, &self.colors)
    }

    /// Render rasterized text on the Top Status Bar
    pub fn render_bar_text(
        &mut self,
        renderer: &mut GlesRenderer,
        screen_size: Size<i32, Logical>,
        active_ws: usize,
        focused_title: Option<&str>,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        self.bar.set_active_workspace(active_ws);
        let new_title = focused_title.unwrap_or("").to_string();
        if self.bar.focused_title != new_title {
            self.bar.focused_title = new_title;
            self.bar.buffer_dirty = true;
        }
        self.bar.render_text_element(renderer, &self.font, screen_size.w, &self.colors)
    }

    /// Render desktop wallpaper behind all windows
    pub fn render_wallpaper(
        &mut self,
        renderer: &mut GlesRenderer,
        screen_size: Size<i32, Logical>,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        self.wallpaper.render_element(renderer, screen_size, &self.colors)
    }
}
