use std::os::unix::io::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use smithay::backend::input::KeyState;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::element::PixelShaderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::utils::on_commit_buffer_handler;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::keyboard::{FilterResult, KeyboardHandle, ModifiersState};
use smithay::input::pointer::{ButtonEvent, MotionEvent, PointerHandle};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::protocol::wl_seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::utils::{Logical, Point, Rectangle, Serial, Size};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState,
};
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::desktop::WindowSurfaceType;
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_seat, delegate_shm, delegate_xdg_shell,
};
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel;
use wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use wayland_server::protocol::wl_buffer;
use wayland_server::Client;
use xkbcommon::xkb::Keysym;

use crate::config::keybindings::Action;
use crate::config::Config;
use crate::shell::gestures::SwipeGestureTracker;
use crate::shell::workspace::WorkspaceManager;
use crate::system::SystemState;
use crate::ui::bar::StatusBarAction;
use crate::ui::{AppLauncher, UiManager};

smithay::backend::renderer::element::render_elements! {
    pub MaterialRenderElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Shader=PixelShaderElement,
    Memory=MemoryRenderBufferRenderElement<GlesRenderer>,
}

#[derive(Debug, Clone)]
pub enum WindowDragState {
    None,
    Move {
        window: Window,
        start_pointer: Point<f64, Logical>,
        start_window_loc: Point<i32, Logical>,
    },
    Resize {
        window: Window,
        start_pointer: Point<f64, Logical>,
        start_window_geo: Rectangle<i32, Logical>,
        edge: xdg_toplevel::ResizeEdge,
    },
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

pub struct MaterialWmState {
    pub dh: DisplayHandle,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<Self>,
    pub data_device_state: DataDeviceState,
    pub popup_manager: PopupManager,

    pub space: Space<Window>,
    pub seat: Seat<Self>,
    pub keyboard: KeyboardHandle<Self>,
    pub pointer: PointerHandle<Self>,

    pub workspaces: WorkspaceManager,
    pub gesture_tracker: SwipeGestureTracker,
    pub ui: UiManager,
    pub system: SystemState,
    pub config: Config,

    pub window_drag: WindowDragState,
    pub pointer_location: Point<f64, Logical>,
    pub screen_size: Size<i32, Logical>,
    pub running: Arc<AtomicBool>,
    pub needs_redraw: bool,
    pub start_time: Instant,
}

impl MaterialWmState {
    pub fn new(dh: DisplayHandle, config: Config) -> Self {
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        let mut seat = seat_state.new_wl_seat(&dh, "material-seat");
        let keyboard = seat
            .add_keyboard(Default::default(), 200, 25)
            .expect("Failed to initialize keyboard");
        let pointer = seat.add_pointer();

        let space = Space::default();
        let workspaces = WorkspaceManager::new(&config);
        let gesture_tracker = SwipeGestureTracker::new(1920, config.animation_duration_ms);
        let ui = UiManager::new(&config);
        let system = SystemState::new();

        Self {
            dh,
            compositor_state,
            xdg_shell_state,
            shm_state,
            seat_state,
            data_device_state,
            popup_manager: PopupManager::default(),

            space,
            seat,
            keyboard,
            pointer,

            workspaces,
            gesture_tracker,
            ui,
            system,
            config,

            window_drag: WindowDragState::None,
            pointer_location: Point::from((0.0, 0.0)),
            screen_size: Size::from((1920, 1080)),
            running: Arc::new(AtomicBool::new(true)),
            needs_redraw: true,
            start_time: Instant::now(),
        }
    }

    /// Calculate usable area for window tiling (below top status bar)
    pub fn usable_area(&self) -> Rectangle<i32, Logical> {
        let bar_h = if self.ui.bar.visible { self.ui.bar.height } else { 0 };
        Rectangle::new(
            Point::from((0, bar_h)),
            Size::from((self.screen_size.w, (self.screen_size.h - bar_h).max(200))),
        )
    }

    /// Update layouts and animations
    pub fn tick(&mut self, now: Instant) {
        let usable = self.usable_area();
        self.workspaces.update_tiling(usable);

        let animating_ws = self.workspaces.update_animations(now);
        let animating_ui = self.ui.update_animations(now);

        if animating_ws || animating_ui {
            self.needs_redraw = true;
        }

        // Apply animated positions to Space for tiled windows
        let floating = self.workspaces.active_workspace().floating_windows.clone();
        let windows = self.workspaces.active_workspace().windows.clone();
        for w in &windows {
            if !floating.contains(w) {
                if let Some(anim) = self.workspaces.animations.get_mut(w) {
                    let (loc, _scale, _alpha) = anim.update(now);
                    self.space.map_element(w.clone(), loc, false);
                }
            }
        }
    }

    /// Handle keyboard input, matching hotkeys or forwarding to client
    pub fn on_keyboard_key(&mut self, keycode: u32, state: KeyState, serial: Serial, time: u32) {
        let keyboard = self.keyboard.clone();
        keyboard.input(
            self,
            keycode.into(),
            state,
            serial,
            time,
            |state_ref, modifiers, handle| {
                if state == KeyState::Pressed {
                    let keysym = handle.modified_sym();

                    // If launcher is open, direct keystrokes to it
                    if state_ref.ui.launcher.is_open {
                        if let Some(action) = state_ref.match_keybinding(modifiers, keysym) {
                            if action == Action::ToggleLauncher {
                                state_ref.execute_action(action);
                                return FilterResult::Intercept(());
                            }
                        }

                        if let Some(exec) = state_ref.ui.launcher.on_keyboard(
                            keysym,
                            &state_ref.config.terminal,
                            &state_ref.ui.audio,
                        ) {
                            tracing::info!("Launching application from launcher: {}", exec);
                            let _ = std::process::Command::new("sh").arg("-c").arg(&exec).spawn();
                        }
                        state_ref.needs_redraw = true;
                        return FilterResult::Intercept(());
                    }

                    // If Quick Settings drawer is open and Escape is pressed, close it
                    if state_ref.ui.drawer.is_open && keysym == Keysym::Escape {
                        state_ref.ui.drawer.close(&state_ref.ui.audio);
                        state_ref.needs_redraw = true;
                        return FilterResult::Intercept(());
                    }

                    if let Some(action) = state_ref.match_keybinding(modifiers, keysym) {
                        state_ref.execute_action(action);
                        return FilterResult::Intercept(());
                    }
                }
                FilterResult::Forward
            },
        );
    }

    /// Match keyboard event against configured KeyConfig
    fn match_keybinding(&self, modifiers: &ModifiersState, keysym: Keysym) -> Option<Action> {
        let super_pressed = modifiers.logo;
        let shift_pressed = modifiers.shift;
        let ctrl_pressed = modifiers.ctrl;
        let alt_pressed = modifiers.alt;

        for binding in &self.config.keys.bindings {
            let mut req_super = false;
            let mut req_shift = false;
            let mut req_ctrl = false;
            let mut req_alt = false;

            for m in &binding.modifiers {
                match m.to_lowercase().as_str() {
                    "super" | "logo" | "mod4" => req_super = true,
                    "shift" => req_shift = true,
                    "ctrl" | "control" => req_ctrl = true,
                    "alt" | "mod1" => req_alt = true,
                    _ => {}
                }
            }

            if super_pressed == req_super
                && shift_pressed == req_shift
                && ctrl_pressed == req_ctrl
                && alt_pressed == req_alt
            {
                let name = xkbcommon::xkb::keysym_get_name(keysym);
                if name.eq_ignore_ascii_case(&binding.key) {
                    return Some(binding.action.clone());
                }
            }
        }

        None
    }

    /// Execute a compositor action
    pub fn execute_action(&mut self, action: Action) {
        match action {
            Action::SpawnTerminal => {
                let term = self.config.terminal.clone();
                tracing::info!("Spawning terminal: {}", term);
                let _ = std::process::Command::new(term).spawn();
                self.ui.audio.play_click();
            }
            Action::KillActive => {
                let ws = self.workspaces.active_workspace();
                if let Some(focused) = &ws.focused_window {
                    if let Some(toplevel) = focused.toplevel() {
                        toplevel.send_close();
                        self.ui.audio.play_pop();
                    }
                }
            }
            Action::ToggleFloating => {
                let ws = self.workspaces.active_workspace_mut();
                if let Some(focused) = ws.focused_window.clone() {
                    ws.toggle_floating(&focused);
                    let usable = self.usable_area();
                    self.workspaces.update_tiling(usable);
                    self.needs_redraw = true;
                    self.ui.audio.play_click();
                }
            }
            Action::ToggleLauncher => {
                self.ui.launcher.toggle(&self.ui.audio);
                self.needs_redraw = true;
            }
            Action::SpawnExternalLauncher => {
                let external = crate::ui::find_external_launchers();
                if let Some(ext) = external.first() {
                    tracing::info!("Spawning external launcher ({}): {}", ext.name, ext.command);
                    let _ = std::process::Command::new("sh").arg("-c").arg(&ext.command).spawn();
                    self.ui.audio.play_click();
                } else {
                    tracing::warn!("No external launcher (rofi/fuzzel/wofi) found. Opening native launcher.");
                    self.ui.launcher.open(&self.ui.audio);
                    self.needs_redraw = true;
                }
            }
            Action::ToggleDrawer => {
                self.ui.drawer.toggle(&self.ui.audio);
                self.needs_redraw = true;
            }
            Action::ToggleBar => {
                self.ui.bar.visible = !self.ui.bar.visible;
                let usable = self.usable_area();
                self.workspaces.update_tiling(usable);
                self.needs_redraw = true;
            }
            Action::SwitchWorkspace(idx) => {
                self.workspaces.switch_workspace(idx, self.screen_size.w);
                self.ui.bar.set_active_workspace(idx);
                self.ui.audio.play_click();
                self.needs_redraw = true;
            }
            Action::MoveToWorkspace(idx) => {
                let ws = self.workspaces.active_workspace();
                if let Some(focused) = ws.focused_window.clone() {
                    self.workspaces.move_window_to_workspace(&focused, idx);
                    let usable = self.usable_area();
                    self.workspaces.update_tiling(usable);
                    self.needs_redraw = true;
                }
            }
            Action::FocusNext => {
                if let Some(w) = self.workspaces.focus_next() {
                    self.focus_window(&w);
                }
            }
            Action::FocusPrev => {
                if let Some(w) = self.workspaces.focus_prev() {
                    self.focus_window(&w);
                }
            }
            Action::SwapMaster => {
                self.workspaces.swap_master();
                let usable = self.usable_area();
                self.workspaces.update_tiling(usable);
                self.needs_redraw = true;
            }
            Action::IncreaseMasterRatio => {
                let ws = self.workspaces.active_workspace_mut();
                ws.layout.master_ratio = (ws.layout.master_ratio + 0.05).min(0.85);
                let usable = self.usable_area();
                self.workspaces.update_tiling(usable);
                self.needs_redraw = true;
            }
            Action::DecreaseMasterRatio => {
                let ws = self.workspaces.active_workspace_mut();
                ws.layout.master_ratio = (ws.layout.master_ratio - 0.05).max(0.15);
                let usable = self.usable_area();
                self.workspaces.update_tiling(usable);
                self.needs_redraw = true;
            }
            Action::BrightnessUp => {
                let current = self.system.poll_brightness();
                self.system.set_brightness(current + 0.1);
                self.ui.drawer.brightness.value = (current + 0.1).clamp(0.0, 1.0);
                self.ui.audio.play_tick();
                self.needs_redraw = true;
            }
            Action::BrightnessDown => {
                let current = self.system.poll_brightness();
                self.system.set_brightness(current - 0.1);
                self.ui.drawer.brightness.value = (current - 0.1).clamp(0.0, 1.0);
                self.ui.audio.play_tick();
                self.needs_redraw = true;
            }
            Action::VolumeUp => {
                let current = self.ui.drawer.volume.value;
                self.ui.drawer.volume.value = (current + 0.05).min(1.0);
                self.ui.audio.play_tick();
                self.needs_redraw = true;
            }
            Action::VolumeDown => {
                let current = self.ui.drawer.volume.value;
                self.ui.drawer.volume.value = (current - 0.05).max(0.0);
                self.ui.audio.play_tick();
                self.needs_redraw = true;
            }
            Action::VolumeMute => {
                self.ui.drawer.volume.value = 0.0;
                self.ui.audio.play_tick();
                self.needs_redraw = true;
            }
            Action::Quit => {
                tracing::info!("Received Quit action. Terminating compositor.");
                self.running.store(false, Ordering::SeqCst);
            }
        }
    }

    /// Focus a given window
    pub fn focus_window(&mut self, window: &Window) {
        self.space.raise_element(window, true);
        if let Some(toplevel) = window.toplevel() {
            toplevel.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Activated);
            });
            toplevel.send_configure();
            let surface = toplevel.wl_surface().clone();
            let serial = Serial::from(0);
            let keyboard = self.keyboard.clone();
            keyboard.set_focus(self, Some(surface), serial);
            self.needs_redraw = true;
        }
    }

    /// Handle pointer motion
    pub fn on_pointer_motion(&mut self, location: Point<f64, Logical>, serial: Serial, time: u32) {
        self.pointer_location = location;

        // 1. Handle interactive window dragging or resizing
        match &mut self.window_drag {
            WindowDragState::Move { window, start_pointer, start_window_loc } => {
                let delta = location - *start_pointer;
                let new_loc = Point::from((
                    start_window_loc.x + delta.x as i32,
                    start_window_loc.y + delta.y as i32,
                ));
                self.space.map_element(window.clone(), new_loc, true);
                self.needs_redraw = true;
                return;
            }
            WindowDragState::Resize { window, start_pointer, start_window_geo, edge } => {
                let delta = location - *start_pointer;
                let mut new_w = start_window_geo.size.w;
                let mut new_h = start_window_geo.size.h;
                let mut new_x = start_window_geo.loc.x;
                let mut new_y = start_window_geo.loc.y;

                match edge {
                    xdg_toplevel::ResizeEdge::Top => {
                        new_h = (start_window_geo.size.h - delta.y as i32).max(150);
                        new_y = start_window_geo.loc.y + (start_window_geo.size.h - new_h);
                    }
                    xdg_toplevel::ResizeEdge::Bottom => {
                        new_h = (start_window_geo.size.h + delta.y as i32).max(150);
                    }
                    xdg_toplevel::ResizeEdge::Left => {
                        new_w = (start_window_geo.size.w - delta.x as i32).max(200);
                        new_x = start_window_geo.loc.x + (start_window_geo.size.w - new_w);
                    }
                    xdg_toplevel::ResizeEdge::Right => {
                        new_w = (start_window_geo.size.w + delta.x as i32).max(200);
                    }
                    xdg_toplevel::ResizeEdge::TopLeft => {
                        new_w = (start_window_geo.size.w - delta.x as i32).max(200);
                        new_x = start_window_geo.loc.x + (start_window_geo.size.w - new_w);
                        new_h = (start_window_geo.size.h - delta.y as i32).max(150);
                        new_y = start_window_geo.loc.y + (start_window_geo.size.h - new_h);
                    }
                    xdg_toplevel::ResizeEdge::TopRight => {
                        new_w = (start_window_geo.size.w + delta.x as i32).max(200);
                        new_h = (start_window_geo.size.h - delta.y as i32).max(150);
                        new_y = start_window_geo.loc.y + (start_window_geo.size.h - new_h);
                    }
                    xdg_toplevel::ResizeEdge::BottomLeft => {
                        new_w = (start_window_geo.size.w - delta.x as i32).max(200);
                        new_x = start_window_geo.loc.x + (start_window_geo.size.w - new_w);
                        new_h = (start_window_geo.size.h + delta.y as i32).max(150);
                    }
                    xdg_toplevel::ResizeEdge::BottomRight | xdg_toplevel::ResizeEdge::None => {
                        new_w = (start_window_geo.size.w + delta.x as i32).max(200);
                        new_h = (start_window_geo.size.h + delta.y as i32).max(150);
                    }
                    _ => {
                        new_w = (start_window_geo.size.w + delta.x as i32).max(200);
                        new_h = (start_window_geo.size.h + delta.y as i32).max(150);
                    }
                }

                if let Some(toplevel) = window.toplevel() {
                    toplevel.with_pending_state(|state| {
                        state.size = Some((new_w, new_h).into());
                    });
                    toplevel.send_configure();
                }
                let new_loc = Point::from((new_x, new_y));
                self.space.map_element(window.clone(), new_loc, true);
                self.needs_redraw = true;
                return;
            }
            WindowDragState::None => {}
        }

        // 2. If drawer is open and dragging slider
        if self.ui.drawer.open_progress.value() > 0.01 {
            self.ui.drawer.on_pointer_motion(
                location,
                self.screen_size.w,
                self.ui.bar.height,
                &self.ui.audio,
            );
            self.needs_redraw = true;
        }

        // 3. Forward to client surface under pointer if not captured by drawer or launcher
        let pointer = self.pointer.clone();
        let ui_captures_pointer = self.ui.drawer.open_progress.value() > 0.5
            || self.ui.launcher.open_progress.value() > 0.5;

        if !ui_captures_pointer {
            let under = self.space.element_under(location);
            if let Some((window, render_loc)) = under {
                let point_in_window = location - render_loc.to_f64();
                if let Some((surf, point_in_surf)) = window.surface_under(point_in_window, WindowSurfaceType::ALL) {
                    pointer.motion(
                        self,
                        Some((surf, point_in_surf.to_f64())),
                        &MotionEvent {
                            location: point_in_surf.to_f64(),
                            serial,
                            time,
                        },
                    );
                    return;
                }
            }
            pointer.motion(
                self,
                None,
                &MotionEvent {
                    location,
                    serial,
                    time,
                },
            );
        } else {
            pointer.motion(
                self,
                None,
                &MotionEvent {
                    location,
                    serial,
                    time,
                },
            );
        }
    }

    /// Handle pointer button press / release
    pub fn on_pointer_button(&mut self, button: u32, state: smithay::backend::input::ButtonState, serial: Serial, time: u32) {
        let is_press = state == smithay::backend::input::ButtonState::Pressed;

        if is_press {
            // Check Super key for window movement / resizing
            let mods = self.keyboard.modifier_state();
            if mods.logo {
                if button == 0x110 {
                    // Super + Left Click -> Move Window
                    let under = self.space.element_under(self.pointer_location);
                    if let Some((window, render_loc)) = under {
                        let win = window.clone();
                        let ws = self.workspaces.active_workspace_mut();
                        if !ws.is_floating(&win) {
                            ws.toggle_floating(&win);
                            let usable = self.usable_area();
                            self.workspaces.update_tiling(usable);
                        }
                        self.focus_window(&win);
                        self.window_drag = WindowDragState::Move {
                            window: win,
                            start_pointer: self.pointer_location,
                            start_window_loc: render_loc,
                        };
                        self.ui.audio.play_click();
                        self.needs_redraw = true;
                        return;
                    }
                } else if button == 0x111 {
                    // Super + Right Click -> Resize Window
                    let under = self.space.element_under(self.pointer_location);
                    if let Some((window, render_loc)) = under {
                        let win = window.clone();
                        let ws = self.workspaces.active_workspace_mut();
                        if !ws.is_floating(&win) {
                            ws.toggle_floating(&win);
                            let usable = self.usable_area();
                            self.workspaces.update_tiling(usable);
                        }
                        let geo = Rectangle::new(render_loc, win.geometry().size);
                        self.focus_window(&win);
                        self.window_drag = WindowDragState::Resize {
                            window: win,
                            start_pointer: self.pointer_location,
                            start_window_geo: geo,
                            edge: xdg_toplevel::ResizeEdge::BottomRight,
                        };
                        self.ui.audio.play_click();
                        self.needs_redraw = true;
                        return;
                    }
                }
            }

            // 1. Check Application Launcher modal
            let launcher_was_open = self.ui.launcher.is_open && self.ui.launcher.open_progress.value() > 0.1;
            if launcher_was_open {
                if let Some(cmd) = self.ui.launcher.on_pointer_down(
                    self.pointer_location,
                    self.screen_size,
                    &self.config.terminal,
                    &self.ui.audio,
                ) {
                    tracing::info!("Launching application from launcher click: {}", cmd);
                    let _ = std::process::Command::new("sh").arg("-c").arg(&cmd).spawn();
                }
                self.needs_redraw = true;
                return;
            }

            // 2. Check Quick Settings Drawer
            if self.ui.drawer.on_pointer_down(
                self.pointer_location,
                self.screen_size.w,
                self.ui.bar.height,
                &self.ui.audio,
            ) {
                self.needs_redraw = true;
                return;
            }

            // 3. Check Top Bar
            if let Some(action) = self.ui.bar.on_pointer_down(
                self.pointer_location,
                self.screen_size.w,
                &self.ui.audio,
            ) {
                match action {
                    StatusBarAction::ToggleAppLauncher => {
                        self.execute_action(Action::ToggleLauncher);
                    }
                    StatusBarAction::SwitchWorkspace(ws) => {
                        self.execute_action(Action::SwitchWorkspace(ws));
                    }
                    StatusBarAction::ToggleQuickSettings => {
                        self.execute_action(Action::ToggleDrawer);
                    }
                }
                return;
            }

            // 4. Client window click -> focus window and set focus to surface
            let under = self.space.element_under(self.pointer_location);
            if let Some((window, render_loc)) = under {
                let win = window.clone();
                self.workspaces.active_workspace_mut().focused_window = Some(win.clone());
                self.focus_window(&win);

                let point_in_window = self.pointer_location - render_loc.to_f64();
                if let Some((surf, point_in_surf)) = win.surface_under(point_in_window, WindowSurfaceType::ALL) {
                    let pointer = self.pointer.clone();
                    pointer.motion(
                        self,
                        Some((surf, point_in_surf.to_f64())),
                        &MotionEvent {
                            location: point_in_surf.to_f64(),
                            serial,
                            time,
                        },
                    );
                }
            }
        } else {
            // Pointer release
            if !matches!(self.window_drag, WindowDragState::None) {
                self.window_drag = WindowDragState::None;
                self.ui.audio.play_tick();
                self.needs_redraw = true;
                return;
            }

            self.ui.drawer.on_pointer_up(&self.ui.audio);
            self.needs_redraw = true;
        }

        let pointer = self.pointer.clone();
        pointer.button(
            self,
            &ButtonEvent {
                button,
                state,
                serial,
                time,
            },
        );
    }

    /// Handle pointer axis (mouse wheel / touchpad scroll)
    pub fn on_pointer_axis(&mut self, _h_scroll: f64, v_scroll: f64, time: u32) {
        if self.ui.launcher.is_open && self.ui.launcher.open_progress.value() > 0.1 {
            if v_scroll > 0.0 {
                // Scroll down
                if self.ui.launcher.selected_index + 1 < self.ui.launcher.filtered_indices.len() {
                    self.ui.launcher.selected_index += 1;
                    if self.ui.launcher.selected_index >= self.ui.launcher.scroll_offset + AppLauncher::VISIBLE_ITEMS {
                        self.ui.launcher.scroll_offset += 1;
                    }
                    self.ui.launcher.buffer_dirty = true;
                    self.needs_redraw = true;
                }
            } else if v_scroll < 0.0 {
                // Scroll up
                if self.ui.launcher.selected_index > 0 {
                    self.ui.launcher.selected_index -= 1;
                    if self.ui.launcher.selected_index < self.ui.launcher.scroll_offset {
                        self.ui.launcher.scroll_offset = self.ui.launcher.scroll_offset.saturating_sub(1);
                    }
                    self.ui.launcher.buffer_dirty = true;
                    self.needs_redraw = true;
                }
            }
            return;
        }

        // Forward to pointer
        let pointer = self.pointer.clone();
        let frame = smithay::input::pointer::AxisFrame::new(time)
            .value(smithay::backend::input::Axis::Vertical, v_scroll);
        pointer.axis(self, frame);
    }
}

// ----------------------------------------------------------------------------
// Smithay Protocol Handler Implementations
// ----------------------------------------------------------------------------

impl BufferHandler for MaterialWmState {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl CompositorHandler for MaterialWmState {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        self.space.refresh();
        self.popup_manager.commit(surface);
        self.needs_redraw = true;
    }
}

impl ShmHandler for MaterialWmState {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

impl SeatHandler for MaterialWmState {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&WlSurface>) {}
    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: smithay::input::pointer::CursorImageStatus) {}
}

impl SelectionHandler for MaterialWmState {
    type SelectionUserData = ();
}

impl DataDeviceHandler for MaterialWmState {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for MaterialWmState {}
impl ServerDndGrabHandler for MaterialWmState {
    fn send(&mut self, _mime_type: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl XdgShellHandler for MaterialWmState {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        let window = Window::new_wayland_window(surface.clone());
        let usable = self.usable_area();
        let initial_rect = Rectangle::new(usable.loc, (800, 600).into());

        self.workspaces.add_window(window.clone(), self.workspaces.active_idx, initial_rect);
        self.space.map_element(window.clone(), usable.loc, true);
        self.focus_window(&window);
        self.ui.audio.play_pop();
        self.needs_redraw = true;
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: wl_seat::WlSeat, _serial: Serial) {
        let window = self
            .space
            .elements()
            .find(|w| w.toplevel().map(|t| t == &surface).unwrap_or(false))
            .cloned();
        if let Some(win) = window {
            let ws = self.workspaces.active_workspace_mut();
            if !ws.is_floating(&win) {
                ws.toggle_floating(&win);
                let usable = self.usable_area();
                self.workspaces.update_tiling(usable);
            }
            let loc = self.space.element_location(&win).unwrap_or_default();
            self.window_drag = WindowDragState::Move {
                window: win,
                start_pointer: self.pointer_location,
                start_window_loc: loc,
            };
            self.ui.audio.play_click();
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: wl_seat::WlSeat,
        _serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let window = self
            .space
            .elements()
            .find(|w| w.toplevel().map(|t| t == &surface).unwrap_or(false))
            .cloned();
        if let Some(win) = window {
            let ws = self.workspaces.active_workspace_mut();
            if !ws.is_floating(&win) {
                ws.toggle_floating(&win);
                let usable = self.usable_area();
                self.workspaces.update_tiling(usable);
            }
            let loc = self.space.element_location(&win).unwrap_or_default();
            let size = win.geometry().size;
            self.window_drag = WindowDragState::Resize {
                window: win,
                start_pointer: self.pointer_location,
                start_window_geo: Rectangle::new(loc, size),
                edge: edges,
            };
            self.ui.audio.play_click();
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        if let Err(err) = self.popup_manager.track_popup(surface.into()) {
            tracing::warn!("Failed to track popup: {:?}", err);
        }
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let _ = (surface, seat, serial);
    }

    fn reposition_request(&mut self, surface: PopupSurface, positioner: PositionerState, token: u32) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
    }
}

delegate_compositor!(MaterialWmState);
delegate_shm!(MaterialWmState);
delegate_seat!(MaterialWmState);
delegate_data_device!(MaterialWmState);
delegate_xdg_shell!(MaterialWmState);
