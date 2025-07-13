// Window animation support
// Moved from lib.rs to maintain modular structure

use crate::grid::animation::EasingType;
use std::fmt;
use std::time::{Duration, Instant};
use winapi::shared::windef::RECT;

#[derive(Clone)]
pub struct WindowAnimation {
    pub hwnd: u64,
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
        hwnd: u64,
        start_rect: crate::window::info::RectWrapper,
        target_rect: crate::window::info::RectWrapper,
        duration: Duration,
        easing: EasingType,
    ) -> Self {
        Self {
            hwnd,
            start_rect: start_rect.to_rect(),
            target_rect: target_rect.to_rect(),
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
        // https://easings.net/
        match easing {
            EasingType::Linear => t,
            EasingType::EaseInQuad => t * t,
            EasingType::EaseOutQuad => 1.0 - (1.0 - t) * (1.0 - t),
            EasingType::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powf(2.0) / 2.0
                }
            }
            EasingType::EaseIn => t * t,
            EasingType::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            EasingType::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                }
            }
            EasingType::Bounce => {
                let n1 = 7.5625;
                let d1 = 2.75;
                if t < 1.0 / d1 {
                    n1 * t * t
                } else if t < 2.0 / d1 {
                    let t = t - 1.5 / d1;
                    n1 * t * t + 0.75
                } else if t < 2.5 / d1 {
                    let t = t - 2.25 / d1;
                    n1 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / d1;
                    n1 * t * t + 0.984375
                }
            }
            EasingType::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    -((2.0_f32).powf(10.0 * (t - 1.0))
                        * ((t - 1.0 - s) * (2.0 * std::f32::consts::PI) / p).sin())
                }
            }
            EasingType::Back => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
            // Additional match arms for more easing types:
            EasingType::EaseInCubic => t * t * t,
            EasingType::EaseOutCubic => 1.0 - (1.0 - t).powi(3),
            EasingType::EaseInOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - ((-2.0 * t + 2.0).powi(3)) / 2.0
                }
            }
            EasingType::EaseInQuart => t * t * t * t,
            EasingType::EaseOutQuart => 1.0 - (1.0 - t).powi(4),
            EasingType::EaseInOutQuart => {
                if t < 0.5 {
                    8.0 * t * t * t * t
                } else {
                    1.0 - ((-2.0 * t + 2.0).powi(4)) / 2.0
                }
            }
            EasingType::EaseInQuint => t * t * t * t * t,
            EasingType::EaseOutQuint => 1.0 - (1.0 - t).powi(5),
            EasingType::EaseInOutQuint => {
                if t < 0.5 {
                    16.0 * t * t * t * t * t
                } else {
                    1.0 - ((-2.0 * t + 2.0).powi(5)) / 2.0
                }
            }
            EasingType::EaseInSine => 1.0 - (std::f32::consts::PI * t / 2.0).cos(),
            EasingType::EaseOutSine => (std::f32::consts::PI * t / 2.0).sin(),
            EasingType::EaseInOutSine => -(std::f32::consts::PI * t).cos() / 2.0 + 0.5,
            EasingType::EaseInExpo => {
                if t == 0.0 {
                    0.0
                } else {
                    (2.0_f32).powf(10.0 * t - 10.0)
                }
            }
            EasingType::EaseOutExpo => {
                if t == 1.0 {
                    1.0
                } else {
                    1.0 - (2.0_f32).powf(-10.0 * t)
                }
            }
            EasingType::EaseInOutExpo => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else if t < 0.5 {
                    (2.0_f32).powf(20.0 * t - 10.0) / 2.0
                } else {
                    (2.0 - (2.0_f32).powf(-20.0 * t + 10.0)) / 2.0
                }
            }
            EasingType::EaseInCirc => 1.0 - (1.0 - t * t).sqrt(),
            EasingType::EaseOutCirc => (1.0 - (t - 1.0).powi(2)).sqrt(),
            EasingType::EaseInOutCirc => {
                if t < 0.5 {
                    (1.0 - (2.0 * t).powi(2)).sqrt() / 2.0
                } else {
                    ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0
                }
            }
            EasingType::EaseInBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
            EasingType::EaseOutBack => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
            }
            EasingType::EaseInOutBack => {
                let c1 = 1.70158;
                let c2 = c1 * 1.525;
                if t < 0.5 {
                    ((2.0 * t).powi(2) * ((c2 + 1.0) * 2.0 * t - c2)) / 2.0
                } else {
                    ((2.0 * t - 2.0).powi(2) * ((c2 + 1.0) * (2.0 * t - 2.0) + c2) + 2.0) / 2.0
                }
            }
            EasingType::EaseInElastic => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                    -(2.0_f32).powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * c4).sin()
                }
            }
            EasingType::EaseOutElastic => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                    (2.0_f32).powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
                }
            }
            EasingType::EaseInOutElastic => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    let c5 = (2.0 * std::f32::consts::PI) / 4.5;
                    if t < 0.5 {
                        -((2.0_f32).powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
                    } else {
                        ((2.0_f32).powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * c5).sin()) / 2.0
                            + 1.0
                    }
                }
            }
            EasingType::EaseInBounce => {
                // Bounce out reversed
                let bounce_out = |x: f32| {
                    let n1 = 7.5625;
                    let d1 = 2.75;
                    if x < 1.0 / d1 {
                        n1 * x * x
                    } else if x < 2.0 / d1 {
                        let x = x - 1.5 / d1;
                        n1 * x * x + 0.75
                    } else if x < 2.5 / d1 {
                        let x = x - 2.25 / d1;
                        n1 * x * x + 0.9375
                    } else {
                        let x = x - 2.625 / d1;
                        n1 * x * x + 0.984375
                    }
                };
                1.0 - bounce_out(1.0 - t)
            }
            EasingType::EaseOutBounce => {
                let n1 = 7.5625;
                let d1 = 2.75;
                if t < 1.0 / d1 {
                    n1 * t * t
                } else if t < 2.0 / d1 {
                    let t = t - 1.5 / d1;
                    n1 * t * t + 0.75
                } else if t < 2.5 / d1 {
                    let t = t - 2.25 / d1;
                    n1 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / d1;
                    n1 * t * t + 0.984375
                }
            }
            EasingType::EaseInOutBounce => {
                let bounce_out = |x: f32| {
                    let n1 = 7.5625;
                    let d1 = 2.75;
                    if x < 1.0 / d1 {
                        n1 * x * x
                    } else if x < 2.0 / d1 {
                        let x = x - 1.5 / d1;
                        n1 * x * x + 0.75
                    } else if x < 2.5 / d1 {
                        let x = x - 2.25 / d1;
                        n1 * x * x + 0.9375
                    } else {
                        let x = x - 2.625 / d1;
                        n1 * x * x + 0.984375
                    }
                };
                if t < 0.5 {
                    (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
                } else {
                    (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
                }
            }
        }
    }
}
