// Window animation support
// Moved from lib.rs to maintain modular structure

use crate::grid::animation::EasingType;
use std::fmt;
use std::time::{Duration, Instant};
use winapi::shared::windef::{HWND, RECT};

#[derive(Clone)]
pub struct WindowAnimation {
    pub hwnd: HWND,
    pub start_rect: RECT,
    pub target_rect: RECT,
    pub start_time: Instant,
    pub duration: Duration,
    pub easing: EasingType,
    pub completed: bool,
}

impl fmt::Debug for WindowAnimation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowAnimation")
            .field("hwnd", &self.hwnd)
            .field(
                "start_rect",
                &format!(
                    "RECT({}, {}, {}, {})",
                    self.start_rect.left,
                    self.start_rect.top,
                    self.start_rect.right,
                    self.start_rect.bottom
                ),
            )
            .field(
                "target_rect",
                &format!(
                    "RECT({}, {}, {}, {})",
                    self.target_rect.left,
                    self.target_rect.top,
                    self.target_rect.right,
                    self.target_rect.bottom
                ),
            )
            .field("start_time", &self.start_time)
            .field("duration", &self.duration)
            .field("easing", &self.easing)
            .field("completed", &self.completed)
            .finish()
    }
}

impl WindowAnimation {
    pub fn new(
        hwnd: HWND,
        start_rect: RECT,
        target_rect: RECT,
        duration: Duration,
        easing: EasingType,
    ) -> Self {
        Self {
            hwnd,
            start_rect,
            target_rect,
            start_time: Instant::now(),
            duration,
            easing,
            completed: false,
        }
    }

    /// Get the current animation progress (0.0 to 1.0)
    pub fn get_progress(&self) -> f32 {
        if self.completed {
            return 1.0;
        }

        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            1.0
        } else {
            elapsed.as_secs_f32() / self.duration.as_secs_f32()
        }
    }

    /// Get the current interpolated rectangle
    pub fn get_current_rect(&mut self) -> RECT {
        let progress = self.get_progress();

        if progress >= 1.0 {
            self.completed = true;
            return self.target_rect;
        }

        // Apply easing
        let eased_progress = Self::apply_easing(progress, &self.easing);

        // Interpolate rectangle
        RECT {
            left: Self::lerp(self.start_rect.left, self.target_rect.left, eased_progress),
            top: Self::lerp(self.start_rect.top, self.target_rect.top, eased_progress),
            right: Self::lerp(
                self.start_rect.right,
                self.target_rect.right,
                eased_progress,
            ),
            bottom: Self::lerp(
                self.start_rect.bottom,
                self.target_rect.bottom,
                eased_progress,
            ),
        }
    }

    /// Check if the animation is completed
    pub fn is_completed(&self) -> bool {
        self.completed || self.start_time.elapsed() >= self.duration
    }

    /// Linear interpolation between two i32 values
    fn lerp(start: i32, end: i32, t: f32) -> i32 {
        (start as f32 + (end - start) as f32 * t) as i32
    }

    /// Apply easing function to progress
    fn apply_easing(t: f32, easing: &EasingType) -> f32 {
        match easing {
            EasingType::Linear => t,
            EasingType::EaseIn => t * t * t,
            EasingType::EaseOut => {
                let u = 1.0 - t;
                1.0 - (u * u * u)
            }
            EasingType::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = 1.0 - t;
                    1.0 - 4.0 * u * u * u
                }
            }
            EasingType::Bounce => {
                if t < 1.0 / 2.75 {
                    7.5625 * t * t
                } else if t < 2.0 / 2.75 {
                    let t = t - 1.5 / 2.75;
                    7.5625 * t * t + 0.75
                } else if t < 2.5 / 2.75 {
                    let t = t - 2.25 / 2.75;
                    7.5625 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / 2.75;
                    7.5625 * t * t + 0.984375
                }
            }
            EasingType::Elastic => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                    -(2.0_f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
                }
            }
            EasingType::Back => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
        }
    }
}
