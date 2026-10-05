use smithay::utils::{Logical, Point, Rectangle, Size};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    MasterStack,
    Monocle,
    Floating,
}

#[derive(Debug, Clone)]
pub struct WindowGeometry {
    pub rect: Rectangle<i32, Logical>,
    pub is_floating: bool,
}

#[derive(Debug, Clone)]
pub struct LayoutEngine {
    pub mode: LayoutMode,
    pub master_ratio: f32,
    pub inner_gap: i32,
    pub outer_gap: i32,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self {
            mode: LayoutMode::MasterStack,
            master_ratio: 0.55,
            inner_gap: 8,
            outer_gap: 12,
        }
    }
}

impl LayoutEngine {
    pub fn new(master_ratio: f32, inner_gap: i32, outer_gap: i32) -> Self {
        Self {
            mode: LayoutMode::MasterStack,
            master_ratio: master_ratio.clamp(0.2, 0.8),
            inner_gap,
            outer_gap,
        }
    }

    /// Calculate window target rectangles for a given workspace area
    pub fn calculate_layout(
        &self,
        usable_area: Rectangle<i32, Logical>,
        window_count: usize,
    ) -> Vec<Rectangle<i32, Logical>> {
        if window_count == 0 {
            return Vec::new();
        }

        let area_x = usable_area.loc.x + self.outer_gap;
        let area_y = usable_area.loc.y + self.outer_gap;
        let area_w = (usable_area.size.w - 2 * self.outer_gap).max(100);
        let area_h = (usable_area.size.h - 2 * self.outer_gap).max(100);

        if self.mode == LayoutMode::Monocle || window_count == 1 {
            return vec![Rectangle::new(
                Point::from((area_x, area_y)),
                Size::from((area_w, area_h)),
            )];
        }

        match self.mode {
            LayoutMode::MasterStack => {
                let mut rects = Vec::with_capacity(window_count);

                // Master window width
                let master_w = ((area_w as f32 * self.master_ratio) as i32 - self.inner_gap / 2).max(100);
                let stack_x = area_x + master_w + self.inner_gap;
                let stack_w = (area_w - master_w - self.inner_gap).max(100);

                // 1. Master window rect (first window)
                rects.push(Rectangle::new(
                    Point::from((area_x, area_y)),
                    Size::from((master_w, area_h)),
                ));

                // 2. Stack windows (remaining window_count - 1)
                let stack_count = window_count - 1;
                let total_gaps = (stack_count - 1) as i32 * self.inner_gap;
                let available_h = (area_h - total_gaps).max(stack_count as i32 * 50);
                let base_item_h = available_h / stack_count as i32;
                let mut remainder = available_h % stack_count as i32;

                let mut current_y = area_y;
                for _ in 0..stack_count {
                    let mut item_h = base_item_h;
                    if remainder > 0 {
                        item_h += 1;
                        remainder -= 1;
                    }

                    rects.push(Rectangle::new(
                        Point::from((stack_x, current_y)),
                        Size::from((stack_w, item_h)),
                    ));

                    current_y += item_h + self.inner_gap;
                }

                rects
            }
            LayoutMode::Monocle => {
                vec![
                    Rectangle::new(
                        Point::from((area_x, area_y)),
                        Size::from((area_w, area_h)),
                    );
                    window_count
                ]
            }
            LayoutMode::Floating => {
                // Floating handles geometry independently
                vec![
                    Rectangle::new(
                        Point::from((area_x + 40, area_y + 40)),
                        Size::from((800, 600)),
                    );
                    window_count
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_master_stack_layout() {
        let engine = LayoutEngine::new(0.5, 10, 20);
        let area = Rectangle::new(Point::from((0, 0)), (1920, 1080).into());
        let rects = engine.calculate_layout(area, 3);

        assert_eq!(rects.len(), 3);
        // Master window starts after outer gap
        assert_eq!(rects[0].loc.x, 20);
        assert_eq!(rects[0].loc.y, 20);

        // Stack windows start to the right
        assert!(rects[1].loc.x > rects[0].loc.x);
        assert_eq!(rects[2].loc.x, rects[1].loc.x);
        assert!(rects[2].loc.y > rects[1].loc.y);
    }
}
