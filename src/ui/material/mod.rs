pub mod audio;
pub mod bar;
pub mod drawer;
pub mod launcher;
pub mod theme;
pub mod wallpaper;

use std::cell::RefCell;
use std::sync::Arc;
use smithay::output::Output;

use crate::render_helpers::renderer::NiriRenderer;
use self::audio::AudioManager;
use self::bar::MaterialTopBar;
use self::drawer::MaterialQuickSettingsDrawer;
use self::launcher::MaterialAppLauncher;
use self::theme::M3Colors;
use self::wallpaper::MaterialWallpaper;

pub struct MaterialUiState {
    pub colors: M3Colors,
    pub audio: Arc<AudioManager>,
    pub bar: RefCell<MaterialTopBar>,
    pub drawer: RefCell<MaterialQuickSettingsDrawer>,
    pub launcher: RefCell<MaterialAppLauncher>,
    pub wallpaper: RefCell<MaterialWallpaper>,
}

impl MaterialUiState {
    pub fn new() -> Self {
        let audio = Arc::new(AudioManager::new(true));
        Self {
            colors: M3Colors::default(),
            audio,
            bar: RefCell::new(MaterialTopBar::new()),
            drawer: RefCell::new(MaterialQuickSettingsDrawer::new()),
            launcher: RefCell::new(MaterialAppLauncher::new()),
            wallpaper: RefCell::new(MaterialWallpaper::new()),
        }
    }

    pub fn render_top_layers<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
        active_ws_idx: usize,
        ws_count: usize,
        active_title: Option<&str>,
        push: &mut dyn FnMut(crate::niri::OutputRenderElements<R>),
    ) {
        // 1. Application Launcher modal layer (if open)
        self.launcher
            .borrow_mut()
            .render(renderer, output, &self.colors, push);

        // 2. Quick Settings Drawer overlay (if open)
        if let Some(elem) = self.drawer.borrow_mut().render(renderer, output, &self.colors) {
            push(elem.into());
        }

        // 3. Top Status Bar
        let launcher_open = self.launcher.borrow().is_open;
        if let Some(elem) = self.bar.borrow_mut().render(
            renderer,
            output,
            &self.colors,
            active_ws_idx,
            ws_count,
            active_title,
            launcher_open,
        ) {
            push(elem.into());
        }
    }

    pub fn render_background<R: NiriRenderer>(
        &self,
        renderer: &mut R,
        output: &Output,
        push: &mut dyn FnMut(crate::niri::OutputRenderElements<R>),
    ) {
        if let Some(elem) = self.wallpaper.borrow_mut().render(renderer, output) {
            push(elem.into());
        }
    }

    pub fn on_pointer_button(
        &self,
        x: f64,
        y: f64,
        screen_w: f64,
        screen_h: f64,
        ws_count: usize,
        pressed: bool,
    ) -> MaterialPointerResult {
        if !pressed {
            let was_dragging = self.drawer.borrow().brightness.is_dragging || self.drawer.borrow().volume.is_dragging;
            self.drawer.borrow_mut().on_pointer_up();
            return if was_dragging {
                MaterialPointerResult::Handled
            } else {
                MaterialPointerResult::None
            };
        }

        // 1. If Launcher is open, handle clicks
        if self.launcher.borrow().is_open {
            if self.launcher.borrow_mut().on_pointer_click(x, y, screen_w, screen_h, &self.audio) {
                return MaterialPointerResult::Handled;
            }
        }

        // 2. If Drawer is open, handle clicks/drags
        if self.drawer.borrow().is_open {
            if self.drawer.borrow_mut().on_pointer_down(x, y, screen_w, &self.audio) {
                return MaterialPointerResult::Handled;
            }
        }

        // 3. Top Bar (height 38px)
        if y <= 38.0 {
            // Launcher grid button (left: 8..48)
            if x >= 8.0 && x <= 48.0 {
                self.launcher.borrow_mut().toggle(&self.audio);
                return MaterialPointerResult::Handled;
            }

            // Quick settings status chip (right: screen_w - 190.0 .. screen_w - 12.0)
            if x >= (screen_w - 190.0).max(0.0) && x <= screen_w - 12.0 {
                self.drawer.borrow_mut().toggle(&self.audio);
                return MaterialPointerResult::Handled;
            }

            // Workspace indicators (50.0 .. 50.0 + ws_count * 36.0)
            if x >= 50.0 && x < 50.0 + (ws_count as f64 * 36.0) {
                let ws_idx = ((x - 50.0) / 36.0) as usize;
                if ws_idx < ws_count {
                    self.audio.play_tick();
                    return MaterialPointerResult::SwitchWorkspace(ws_idx);
                }
            }

            return MaterialPointerResult::Handled;
        }

        MaterialPointerResult::None
    }

    pub fn on_pointer_motion(&self, x: f64, screen_w: f64) -> bool {
        if self.drawer.borrow().is_open {
            let is_dragging =
                self.drawer.borrow().brightness.is_dragging || self.drawer.borrow().volume.is_dragging;
            if is_dragging {
                self.drawer.borrow_mut().on_pointer_motion(x, screen_w, &self.audio);
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialPointerResult {
    None,
    Handled,
    SwitchWorkspace(usize),
}

