//! window_easing/mod.rs
//! Provides window movement easing/animation for HWNDs on Windows, now as WindowHwndEasing.

#[cfg(target_os = "windows")]
pub use winapi::shared::windef::HWND;
#[cfg(target_os = "windows")]
use winapi::shared::windef::RECT;
#[cfg(target_os = "windows")]
use winapi::um::winuser::GetWindowRect;
#[cfg(target_os = "windows")]
use winapi::um::winuser::SetWindowPos;

#[cfg(target_os = "windows")]
pub mod framework;
#[cfg(target_os = "windows")]
pub use framework::{default_easing, WindowAnimationCmd, WindowAnimationFramework};

pub struct WindowHwndEasing {
    pub hwnd: HWND,
    pub target_rect: (i32, i32, i32, i32),
    pub duration_ms: u32,
    pub start_time: std::time::Instant,
    pub easing_fn: fn(f32) -> f32,
    pub start_rect: (i32, i32, i32, i32),
}

#[cfg(target_os = "windows")]
impl WindowHwndEasing {
    pub unsafe fn new(
        hwnd: HWND,
        target_rect: (i32, i32, i32, i32),
        duration_ms: u32,
        easing_fn: fn(f32) -> f32,
    ) -> Self {
        let mut rect: RECT = unsafe { std::mem::zeroed() };
        let got_rect = unsafe { GetWindowRect(hwnd, &mut rect) };
        let start_rect = if got_rect != 0 {
            (
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
            )
        } else {
            target_rect
        };
        Self {
            hwnd,
            target_rect,
            duration_ms,
            start_time: std::time::Instant::now(),
            easing_fn,
            start_rect,
        }
    }

    /// Call this every frame to animate the window
    pub fn update(&mut self) -> bool {
        let elapsed = self.start_time.elapsed().as_millis() as u32;
        let t = (elapsed as f32 / self.duration_ms as f32).min(1.0);
        let ease = (self.easing_fn)(t);
        let (sx, sy, sw, sh) = self.start_rect;
        let (tx, ty, tw, th) = self.target_rect;
        let x = sx + ((tx - sx) as f32 * ease).round() as i32;
        let y = sy + ((ty - sy) as f32 * ease).round() as i32;
        let w = sw + ((tw - sw) as f32 * ease).round() as i32;
        let h = sh + ((th - sh) as f32 * ease).round() as i32;
        let result = unsafe {
            SetWindowPos(
                self.hwnd,
                winapi::um::winuser::HWND_TOPMOST,
                x,
                y,
                w,
                h,
                winapi::um::winuser::SWP_SHOWWINDOW | winapi::um::winuser::SWP_NOACTIVATE,
            )
        };
        t >= 1.0 || result == 0
    }
}

#[cfg(target_os = "windows")]
pub struct WindowAnimationGuard {
    handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl WindowAnimationGuard {
    /// Spawns a background thread to animate the HWND to the target rect using the given easing function and duration.
    /// The thread is joined on drop, ensuring the animation completes or is stopped cleanly.
    pub fn animate(
        hwnd: HWND,
        target_rect: (i32, i32, i32, i32),
        duration_ms: u32,
        easing_fn: fn(f32) -> f32,
    ) -> Self {
        // Convert HWND to isize for thread safety
        let hwnd_val = hwnd as isize;
        let handle = std::thread::spawn(move || {
            let hwnd = hwnd_val as HWND;
            let mut rect: RECT = unsafe { std::mem::zeroed() };
            let got_rect = unsafe { GetWindowRect(hwnd, &mut rect) };
            let start_rect = if got_rect != 0 {
                (
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                )
            } else {
                target_rect
            };
            let start_time = std::time::Instant::now();
            loop {
                let elapsed = start_time.elapsed().as_millis() as u32;
                let t = (elapsed as f32 / duration_ms as f32).min(1.0);
                let ease = (easing_fn)(t);
                let (sx, sy, sw, sh) = start_rect;
                let (tx, ty, tw, th) = target_rect;
                let x = sx + ((tx - sx) as f32 * ease).round() as i32;
                let y = sy + ((ty - sy) as f32 * ease).round() as i32;
                let w = sw + ((tw - sw) as f32 * ease).round() as i32;
                let h = sh + ((th - sh) as f32 * ease).round() as i32;
                let result = unsafe {
                    SetWindowPos(
                        hwnd,
                        winapi::um::winuser::HWND_TOPMOST,
                        x,
                        y,
                        w,
                        h,
                        winapi::um::winuser::SWP_SHOWWINDOW | winapi::um::winuser::SWP_NOACTIVATE,
                    )
                };
                if t >= 1.0 || result == 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
        });
        Self {
            handle: Some(handle),
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowAnimationGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_os = "windows")]
pub fn ease_in_out_quad(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        -1.0 + (4.0 - 2.0 * t) * t
    }
}

// Usage:
// let _guard = WindowAnimationGuard::animate(hwnd, (x, y, w, h), 500, ease_in_out_quad);
// // Animation runs in background and stops on drop.
