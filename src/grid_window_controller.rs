//! High-level controller for grid-based window management.
//! Encapsulates threading, channels, and grid manager for ergonomic use.

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[cfg(target_os = "windows")]
pub struct GridWindowController {
    tx: Sender<GridWindowCmd>,
    event_rx: Receiver<GridWindowEvent>,
    thread: thread::JoinHandle<()>,
    // Map marker_id to (host_hwnd, pinned_hwnd)
    associations: std::sync::Arc<dashmap::DashMap<u64, (isize, isize)>>,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone)]
pub enum GridWindowEvent {
    WindowMoved {
        marker_id: u64,
        host_hwnd: isize,
        pinned_hwnd: isize,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
    // Add more events as needed
}

#[cfg(target_os = "windows")]
pub enum GridWindowCmd {
    RegisterAssociation {
        marker_id: u64,
        host_hwnd: isize,
        pinned_hwnd: isize,
    },
    MoveToRect {
        marker_id: u64,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
}

#[cfg(target_os = "windows")]
impl GridWindowController {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let associations = std::sync::Arc::new(dashmap::DashMap::new());
        let associations_thread = associations.clone();
        let thread = thread::spawn(move || {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    GridWindowCmd::RegisterAssociation {
                        marker_id,
                        host_hwnd,
                        pinned_hwnd,
                    } => {
                        associations_thread.insert(marker_id, (host_hwnd, pinned_hwnd));
                        println!("[GridWindowController] Registered association: marker_id={}, host_hwnd={:?}, pinned_hwnd={:?}", marker_id, host_hwnd, pinned_hwnd);
                    }
                    GridWindowCmd::MoveToRect {
                        marker_id,
                        x,
                        y,
                        w,
                        h,
                    } => {
                        if let Some((host_hwnd, pinned_hwnd)) =
                            associations_thread.get(&marker_id).map(|v| *v)
                        {
                            // Actually move the pinned window using WinAPI
                            unsafe {
                                use winapi::shared::windef::HWND as WinHWND;
                                use winapi::um::winuser::SetWindowPos;
                                use winapi::um::winuser::{SWP_NOSIZE, SWP_NOZORDER};
                                let hwnd_win = pinned_hwnd as WinHWND;
                                let ok = SetWindowPos(
                                    hwnd_win,
                                    std::ptr::null_mut(),
                                    x,
                                    y,
                                    w,
                                    h,
                                    SWP_NOZORDER | SWP_NOSIZE,
                                );
                                println!("[GridWindowController] move_window result: {}", ok);
                                let _ = event_tx.send(GridWindowEvent::WindowMoved {
                                    marker_id,
                                    host_hwnd,
                                    pinned_hwnd,
                                    x,
                                    y,
                                    w,
                                    h,
                                });
                            }
                        }
                    }
                }
            }
        });
        Self {
            tx,
            event_rx,
            thread,
            associations,
        }
    }

    pub fn register_association(&self, marker_id: u64, host_hwnd: isize, pinned_hwnd: isize) {
        let _ = self.tx.send(GridWindowCmd::RegisterAssociation {
            marker_id,
            host_hwnd,
            pinned_hwnd,
        });
    }

    pub fn move_window_to_rect(&self, marker_id: u64, x: i32, y: i32, w: i32, h: i32) {
        let _ = self.tx.send(GridWindowCmd::MoveToRect {
            marker_id,
            x,
            y,
            w,
            h,
        });
    }

    pub fn event_receiver(&self) -> &Receiver<GridWindowEvent> {
        &self.event_rx
    }
}

#[cfg(not(target_os = "windows"))]
pub struct GridWindowController;
#[cfg(not(target_os = "windows"))]
impl GridWindowController {
    pub fn new() -> Self {
        Self
    }
    pub fn launch_window(&self, _id: usize, _url: &str) {}
    pub fn move_window_to_grid(&self, _id: usize, _grid_x: i32, _grid_y: i32) {}
    pub fn move_window_to_rect(&self, _hwnd: isize, _x: i32, _y: i32, _w: i32, _h: i32) {}
}
