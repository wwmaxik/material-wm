use std::time::Instant;
use crate::shell::animation::AnimatedFloat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GesturePhase {
    Idle,
    Tracking3FingerSwipe,
}

#[derive(Debug, Clone)]
pub struct SwipeGestureTracker {
    pub phase: GesturePhase,
    pub accumulated_dx: f64,
    pub accumulated_dy: f64,
    pub velocity_dx: f64,
    pub last_update: Instant,
    pub screen_width: i32,
    pub active_workspace: usize,
    /// Offset for 1:1 continuous spatial animation
    pub spatial_offset_x: AnimatedFloat,
}

impl SwipeGestureTracker {
    pub fn new(screen_width: i32, duration_ms: u64) -> Self {
        Self {
            phase: GesturePhase::Idle,
            accumulated_dx: 0.0,
            accumulated_dy: 0.0,
            velocity_dx: 0.0,
            last_update: Instant::now(),
            screen_width,
            active_workspace: 1,
            spatial_offset_x: AnimatedFloat::new(0.0, duration_ms),
        }
    }

    pub fn on_gesture_swipe_begin(&mut self, fingers: u32, active_ws: usize) {
        if fingers == 3 {
            self.phase = GesturePhase::Tracking3FingerSwipe;
            self.accumulated_dx = 0.0;
            self.accumulated_dy = 0.0;
            self.velocity_dx = 0.0;
            self.last_update = Instant::now();
            self.active_workspace = active_ws;
            self.spatial_offset_x.set_immediate(0.0);
        }
    }

    pub fn on_gesture_swipe_update(&mut self, dx: f64, dy: f64) {
        if self.phase == GesturePhase::Tracking3FingerSwipe {
            let now = Instant::now();
            let dt = now.duration_since(self.last_update).as_secs_f64().max(0.001);
            self.last_update = now;

            self.accumulated_dx += dx;
            self.accumulated_dy += dy;
            self.velocity_dx = dx / dt;

            // 1:1 direct manipulation offset
            self.spatial_offset_x.set_immediate(self.accumulated_dx as f32);
        }
    }

    pub fn on_gesture_swipe_end(&mut self, max_workspaces: usize) -> Option<usize> {
        if self.phase != GesturePhase::Tracking3FingerSwipe {
            return None;
        }
        self.phase = GesturePhase::Idle;

        let threshold = (self.screen_width as f64 * 0.18).max(80.0);
        let mut target_ws = self.active_workspace;

        // If swipe left (dx negative), move to next workspace; if swipe right (dx positive), move to prev
        if self.accumulated_dx < -threshold || self.velocity_dx < -400.0 {
            if self.active_workspace < max_workspaces {
                target_ws = self.active_workspace + 1;
            }
        } else if self.accumulated_dx > threshold || self.velocity_dx > 400.0 {
            if self.active_workspace > 1 {
                target_ws = self.active_workspace - 1;
            }
        }

        // Retarget spatial offset back to 0 smoothly
        self.spatial_offset_x.retarget(0.0);
        Some(target_ws)
    }

    pub fn on_gesture_swipe_cancel(&mut self) {
        if self.phase == GesturePhase::Tracking3FingerSwipe {
            self.phase = GesturePhase::Idle;
            self.spatial_offset_x.retarget(0.0);
        }
    }

    pub fn current_offset_x(&mut self, now: Instant) -> f32 {
        self.spatial_offset_x.update(now)
    }
}
