// Window information and related structures

use std::collections::HashMap;
use std::fmt;
use winapi::shared::windef::{HWND, RECT};

#[derive(Clone)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub rect: RECT,
    pub grid_cells: Vec<(usize, usize)>, // Virtual grid cells this window occupies
    pub monitor_cells: HashMap<usize, Vec<(usize, usize)>>, // Per-monitor grid cells (monitor_id -> cells)
    pub is_visible: bool,
    pub is_minimized: bool,
    pub process_id: u32,
    pub class_name: String,
}

impl fmt::Debug for WindowInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowInfo")
            .field("hwnd", &self.hwnd)
            .field("title", &self.title)
            .field(
                "rect",
                &format!(
                    "RECT({}, {}, {}, {})",
                    self.rect.left, self.rect.top, self.rect.right, self.rect.bottom
                ),
            )
            .field("grid_cells", &self.grid_cells)
            .field("monitor_cells", &self.monitor_cells)
            .field("is_visible", &self.is_visible)
            .field("is_minimized", &self.is_minimized)
            .field("process_id", &self.process_id)
            .field("class_name", &self.class_name)
            .finish()
    }
}

impl WindowInfo {
    pub fn new(hwnd: HWND, title: String, rect: RECT) -> Self {
        Self {
            hwnd,
            title,
            rect,
            grid_cells: Vec::new(),
            monitor_cells: HashMap::new(),
            is_visible: true,
            is_minimized: false,
            process_id: 0,
            class_name: String::new(),
        }
    }

    pub fn update_rect(&mut self, new_rect: RECT) {
        self.rect = new_rect;
    }

    pub fn width(&self) -> i32 {
        self.rect.right - self.rect.left
    }

    pub fn height(&self) -> i32 {
        self.rect.bottom - self.rect.top
    }

    pub fn center(&self) -> (i32, i32) {
        (
            self.rect.left + self.width() / 2,
            self.rect.top + self.height() / 2,
        )
    }

    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }
}
