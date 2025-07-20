//! window_easing/framework.rs
//! Provides a reusable background thread and channel-based animation framework for HWND window animations.

#[cfg(target_os = "windows")]
use crate::window_easing::{ease_in_out_quad, WindowAnimationGuard};
#[cfg(target_os = "windows")]
use std::sync::mpsc::{channel, Receiver, Sender};
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
pub use winapi::shared::windef::HWND;

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
pub enum WindowAnimationCmd {
    Animate {
        hwnd: isize,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        duration_ms: u32,
        easing: fn(f32) -> f32,
    },
}

#[cfg(target_os = "windows")]
pub struct WindowAnimationFramework {
    pub tx: Sender<WindowAnimationCmd>,
    handle: Option<thread::JoinHandle<()>>,
}

#[cfg(target_os = "windows")]
impl WindowAnimationFramework {
    pub fn new() -> Self {
        let (tx, rx): (Sender<WindowAnimationCmd>, Receiver<WindowAnimationCmd>) = channel();
        let handle = thread::spawn(move || {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    WindowAnimationCmd::Animate {
                        hwnd,
                        x,
                        y,
                        w,
                        h,
                        duration_ms,
                        easing,
                    } => {
                        let hwnd = hwnd as HWND;
                        let _guard =
                            WindowAnimationGuard::animate(hwnd, (x, y, w, h), duration_ms, easing);
                        // Guard is dropped after animation completes
                    }
                }
            }
        });
        Self {
            tx,
            handle: Some(handle),
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowAnimationFramework {
    fn drop(&mut self) {
        // Dropping tx will close the channel and end the thread
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(target_os = "windows")]
pub fn default_easing() -> fn(f32) -> f32 {
    ease_in_out_quad
}
