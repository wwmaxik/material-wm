use std::time::{Duration, Instant};
use smithay::utils::{Logical, Point, Rectangle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EasingCurve {
    EaseOutCubic,
    EaseOutExpo,
}

impl EasingCurve {
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            EasingCurve::EaseOutCubic => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
            EasingCurve::EaseOutExpo => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - 2.0f32.powf(-10.0 * t)
                }
            }
        }
    }
}

/// A smoothly animatable scalar (e.g., opacity, scale, split ratio)
#[derive(Debug, Clone)]
pub struct AnimatedFloat {
    start: f32,
    target: f32,
    current: f32,
    start_time: Instant,
    duration: Duration,
    easing: EasingCurve,
}

impl AnimatedFloat {
    pub fn new(initial: f32, duration_ms: u64) -> Self {
        Self {
            start: initial,
            target: initial,
            current: initial,
            start_time: Instant::now(),
            duration: Duration::from_millis(duration_ms),
            easing: EasingCurve::EaseOutCubic,
        }
    }

    /// Retarget to a new value without snapping (interruptible animation)
    pub fn retarget(&mut self, new_target: f32) {
        self.retarget_at(new_target, Instant::now());
    }

    pub fn retarget_at(&mut self, new_target: f32, now: Instant) {
        if (self.target - new_target).abs() < 0.0001 {
            return;
        }
        self.update(now);
        self.start = self.current;
        self.target = new_target;
        self.start_time = now;
    }

    pub fn set_immediate(&mut self, val: f32) {
        self.start = val;
        self.current = val;
        self.target = val;
        self.start_time = Instant::now();
    }

    pub fn update(&mut self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.start_time).as_secs_f32();
        let total = self.duration.as_secs_f32();
        if total <= 0.0001 || elapsed >= total {
            self.current = self.target;
        } else {
            let progress = self.easing.sample(elapsed / total);
            self.current = self.start + (self.target - self.start) * progress;
        }
        self.current
    }

    pub fn value(&self) -> f32 {
        self.current
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        now.duration_since(self.start_time) < self.duration && (self.current - self.target).abs() > 0.001
    }
}

/// A smoothly animatable 2D point (e.g., window position)
#[derive(Debug, Clone)]
pub struct AnimatedPoint {
    pub x: AnimatedFloat,
    pub y: AnimatedFloat,
}

impl AnimatedPoint {
    pub fn new(initial: Point<i32, Logical>, duration_ms: u64) -> Self {
        Self {
            x: AnimatedFloat::new(initial.x as f32, duration_ms),
            y: AnimatedFloat::new(initial.y as f32, duration_ms),
        }
    }

    pub fn retarget(&mut self, target: Point<i32, Logical>) {
        self.x.retarget(target.x as f32);
        self.y.retarget(target.y as f32);
    }

    pub fn set_immediate(&mut self, p: Point<i32, Logical>) {
        self.x.set_immediate(p.x as f32);
        self.y.set_immediate(p.y as f32);
    }

    pub fn update(&mut self, now: Instant) -> Point<i32, Logical> {
        let x = self.x.update(now).round() as i32;
        let y = self.y.update(now).round() as i32;
        Point::from((x, y))
    }

    pub fn current(&self) -> Point<i32, Logical> {
        Point::from((self.x.value().round() as i32, self.y.value().round() as i32))
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.x.is_animating(now) || self.y.is_animating(now)
    }
}

/// Fluid Matrix-based transformation state for window animations
#[derive(Debug, Clone)]
pub struct WindowAnimation {
    /// Window animated display position
    pub position: AnimatedPoint,
    /// Scale factor for spawn / dismiss / focus transitions
    pub scale: AnimatedFloat,
    /// Opacity transition
    pub opacity: AnimatedFloat,
    /// Target layout geometry (unaltered during animation to avoid configure storm)
    pub target_rect: Rectangle<i32, Logical>,
}

impl WindowAnimation {
    pub fn new(rect: Rectangle<i32, Logical>, duration_ms: u64) -> Self {
        let mut pos = AnimatedPoint::new(rect.loc, duration_ms);
        pos.set_immediate(rect.loc);
        let mut scale = AnimatedFloat::new(1.0, duration_ms);
        scale.set_immediate(1.0);
        let mut opacity = AnimatedFloat::new(1.0, duration_ms);
        opacity.set_immediate(1.0);

        Self {
            position: pos,
            scale,
            opacity,
            target_rect: rect,
        }
    }

    /// Retarget to new layout rectangle smoothly
    pub fn retarget_geometry(&mut self, new_rect: Rectangle<i32, Logical>) {
        self.target_rect = new_rect;
        self.position.retarget(new_rect.loc);
    }

    /// Spawn animation: start slightly scaled down and faded, ease in
    pub fn trigger_spawn(&mut self) {
        self.scale.set_immediate(0.92);
        self.scale.retarget(1.0);
        self.opacity.set_immediate(0.0);
        self.opacity.retarget(1.0);
    }

    /// Update all animated properties
    pub fn update(&mut self, now: Instant) -> (Point<i32, Logical>, f32, f32) {
        let loc = self.position.update(now);
        let s = self.scale.update(now);
        let a = self.opacity.update(now);
        (loc, s, a)
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.position.is_animating(now) || self.scale.is_animating(now) || self.opacity.is_animating(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_easing_curve() {
        assert_eq!(EasingCurve::EaseOutCubic.sample(0.0), 0.0);
        assert_eq!(EasingCurve::EaseOutCubic.sample(1.0), 1.0);
        assert!(EasingCurve::EaseOutCubic.sample(0.5) > 0.5); // Ease out decelerates
    }

    #[test]
    fn test_interruptible_animation() {
        let mut float = AnimatedFloat::new(0.0, 160);
        float.retarget(100.0);

        // Immediate next tick
        let now = Instant::now();
        let val1 = float.update(now);
        assert!(val1 < 0.01);

        // Advance 50ms
        let t1 = now + Duration::from_millis(50);
        let val2 = float.update(t1);
        assert!(val2 > 0.0 && val2 < 100.0);

        // Interrupt and retarget to 50.0 mid-flight
        float.retarget_at(50.0, t1);
        assert!((float.value() - val2).abs() < 0.001); // Retarget preserves instantaneous position
    }
}
