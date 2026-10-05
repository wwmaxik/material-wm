use std::collections::HashMap;
use std::time::Instant;

use smithay::desktop::Window;
use smithay::utils::{Logical, Rectangle};

use crate::config::Config;
use crate::shell::animation::{AnimatedFloat, WindowAnimation};
use crate::shell::layout::LayoutEngine;

pub struct Workspace {
    pub id: usize,
    pub windows: Vec<Window>,
    pub focused_window: Option<Window>,
    pub floating_windows: Vec<Window>,
    pub layout: LayoutEngine,
}

impl Workspace {
    pub fn new(id: usize, config: &Config) -> Self {
        Self {
            id,
            windows: Vec::new(),
            focused_window: None,
            floating_windows: Vec::new(),
            layout: LayoutEngine::new(config.master_ratio, config.inner_gap, config.outer_gap),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }

    pub fn is_floating(&self, window: &Window) -> bool {
        self.floating_windows.contains(window)
    }

    pub fn toggle_floating(&mut self, window: &Window) {
        if let Some(pos) = self.floating_windows.iter().position(|w| w == window) {
            self.floating_windows.remove(pos);
        } else {
            self.floating_windows.push(window.clone());
        }
    }
}

pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub active_idx: usize,
    pub active_transition_x: AnimatedFloat,
    pub animations: HashMap<Window, WindowAnimation>,
    pub duration_ms: u64,
}

impl WorkspaceManager {
    pub fn new(config: &Config) -> Self {
        let mut workspaces = Vec::with_capacity(9);
        for id in 1..=9 {
            workspaces.push(Workspace::new(id, config));
        }

        Self {
            workspaces,
            active_idx: 1,
            active_transition_x: AnimatedFloat::new(0.0, config.animation_duration_ms),
            animations: HashMap::new(),
            duration_ms: config.animation_duration_ms,
        }
    }

    pub fn active_workspace(&self) -> &Workspace {
        &self.workspaces[self.active_idx - 1]
    }

    pub fn active_workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active_idx - 1]
    }

    pub fn switch_workspace(&mut self, target_idx: usize, screen_width: i32) {
        if target_idx < 1 || target_idx > self.workspaces.len() || target_idx == self.active_idx {
            return;
        }

        let direction = if target_idx > self.active_idx { 1.0 } else { -1.0 };
        self.active_idx = target_idx;

        // Visual slide transition
        self.active_transition_x.set_immediate(direction * screen_width as f32);
        self.active_transition_x.retarget(0.0);
    }

    pub fn add_window(&mut self, window: Window, target_idx: usize, initial_rect: Rectangle<i32, Logical>) {
        let ws_idx = if target_idx >= 1 && target_idx <= self.workspaces.len() {
            target_idx
        } else {
            self.active_idx
        };

        let ws = &mut self.workspaces[ws_idx - 1];
        if !ws.windows.contains(&window) {
            ws.windows.push(window.clone());
            ws.focused_window = Some(window.clone());

            let mut anim = WindowAnimation::new(initial_rect, self.duration_ms);
            anim.trigger_spawn();
            self.animations.insert(window, anim);
        }
    }

    pub fn remove_window(&mut self, window: &Window) {
        for ws in &mut self.workspaces {
            if let Some(pos) = ws.windows.iter().position(|w| w == window) {
                ws.windows.remove(pos);
                if ws.focused_window.as_ref() == Some(window) {
                    ws.focused_window = ws.windows.last().cloned();
                }
            }
            if let Some(pos) = ws.floating_windows.iter().position(|w| w == window) {
                ws.floating_windows.remove(pos);
            }
        }
        self.animations.remove(window);
    }

    pub fn move_window_to_workspace(&mut self, window: &Window, target_idx: usize) {
        if target_idx < 1 || target_idx > self.workspaces.len() {
            return;
        }

        let mut found = false;
        for ws in &mut self.workspaces {
            if let Some(pos) = ws.windows.iter().position(|w| w == window) {
                ws.windows.remove(pos);
                if ws.focused_window.as_ref() == Some(window) {
                    ws.focused_window = ws.windows.last().cloned();
                }
                found = true;
                break;
            }
        }

        if found {
            let target_ws = &mut self.workspaces[target_idx - 1];
            target_ws.windows.push(window.clone());
            target_ws.focused_window = Some(window.clone());
        }
    }

    pub fn focus_next(&mut self) -> Option<Window> {
        let ws = self.active_workspace_mut();
        if ws.windows.is_empty() {
            return None;
        }

        let next = if let Some(current) = &ws.focused_window {
            let idx = ws.windows.iter().position(|w| w == current).unwrap_or(0);
            ws.windows[(idx + 1) % ws.windows.len()].clone()
        } else {
            ws.windows[0].clone()
        };

        ws.focused_window = Some(next.clone());
        Some(next)
    }

    pub fn focus_prev(&mut self) -> Option<Window> {
        let ws = self.active_workspace_mut();
        if ws.windows.is_empty() {
            return None;
        }

        let prev = if let Some(current) = &ws.focused_window {
            let idx = ws.windows.iter().position(|w| w == current).unwrap_or(0);
            let len = ws.windows.len();
            ws.windows[(idx + len - 1) % len].clone()
        } else {
            ws.windows[0].clone()
        };

        ws.focused_window = Some(prev.clone());
        Some(prev)
    }

    pub fn swap_master(&mut self) {
        let ws = self.active_workspace_mut();
        if ws.windows.len() <= 1 {
            return;
        }

        if let Some(focused) = &ws.focused_window {
            if let Some(idx) = ws.windows.iter().position(|w| w == focused) {
                if idx != 0 {
                    ws.windows.swap(0, idx);
                }
            }
        }
    }

    /// Recalculates tiling layout and retargets window animations smoothly without spamming configure
    pub fn update_tiling(&mut self, usable_area: Rectangle<i32, Logical>) {
        let ws = self.active_workspace();
        let tiled_windows: Vec<Window> = ws
            .windows
            .iter()
            .filter(|w| !ws.is_floating(w))
            .cloned()
            .collect();

        let rects = ws.layout.calculate_layout(usable_area, tiled_windows.len());

        for (w, rect) in tiled_windows.into_iter().zip(rects.into_iter()) {
            if let Some(anim) = self.animations.get_mut(&w) {
                anim.retarget_geometry(rect);
            } else {
                self.animations.insert(w, WindowAnimation::new(rect, self.duration_ms));
            }
        }
    }

    pub fn update_animations(&mut self, now: Instant) -> bool {
        let mut animating = self.active_transition_x.is_animating(now);
        for anim in self.animations.values_mut() {
            if anim.is_animating(now) {
                animating = true;
            }
        }
        animating
    }
}
