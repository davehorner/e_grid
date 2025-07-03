// Window information and related structures

use std::collections::HashMap;
use std::fmt;
use winapi::shared::windef::{HWND, RECT};

#[derive(Clone,Copy)]
pub struct WindowInfo {
    pub hwnd: u64,
    pub title: [u16; 256], // Fixed-size UTF-16 buffer for window title
    pub title_len: u32, // Length of the title string
    pub rect: RectWrapper,
    // C ABI-safe representation: fixed-size arrays instead of Vec/HashMap
    pub grid_cells: [(usize, usize); 64], // Up to 16 grid cells
    pub grid_cells_len: u32,              // Actual number of grid cells

    pub monitor_ids: [usize; 8],          // Up to 8 monitors
    pub monitor_cells: [[(usize, usize); 8]; 8], // For each monitor, up to 8 cells
    pub monitor_cells_lens: [u32; 8],     // Number of cells per monitor
    pub monitor_cells_len: u32,           // Number of monitors
    pub z_order: u32, // Z-order index for the window
    pub is_visible: bool,
    pub is_minimized: bool,
    pub process_id: u32,
    pub class_name: [u16; 256],
    pub class_name_len: u32, // Length of the class name string
}

impl Default for WindowInfo {
    fn default() -> Self {
        Self {
            hwnd: 0,
            title: [0u16; 256],
            title_len: 0,
            rect: RectWrapper(RECT { left: 0, top: 0, right: 0, bottom: 0 }),
            grid_cells: [(0, 0); crate::MAX_WINDOW_GRID_CELLS],
            grid_cells_len: 0,
            monitor_ids: [0usize; 8],
            monitor_cells: [[(0, 0); 8]; 8],
            monitor_cells_lens: [0u32; 8],
            monitor_cells_len: 0,
            z_order: 0,
            is_visible: false,
            is_minimized: false,
            process_id: 0,
            class_name: [0u16; 256],
            class_name_len: 0,
        }
    }
}

impl fmt::Debug for WindowInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let title = String::from_utf16_lossy(&self.title[..self.title_len as usize]);
        let class_name = String::from_utf16_lossy(&self.class_name[..self.class_name_len as usize]);
        f.debug_struct("WindowInfo")
            .field("hwnd", &self.hwnd)
            .field("title", &title)
            .field("rect", &format_args!(
                "RECT({}, {}, {}, {})",
                self.rect.left, self.rect.top, self.rect.right, self.rect.bottom
            ))
            .field("grid_cells", &&self.grid_cells[..self.grid_cells_len as usize])
            .field("monitor_ids", &&self.monitor_ids[..self.monitor_cells_len as usize])
            .field("monitor_cells", &&self.monitor_cells[..self.monitor_cells_len as usize])
            .field("is_visible", &self.is_visible)
            .field("is_minimized", &self.is_minimized)
            .field("process_id", &self.process_id)
            .field("class_name", &class_name)
            .finish()
    }
}

impl WindowInfo {
    pub fn new(hwnd: HWND, title: &str, rect: RECT) -> Self {
        let mut title_buf = [0u16; 256];
        let mut title_len = 0u32;
        for (i, c) in title.encode_utf16().take(256).enumerate() {
            title_buf[i] = c;
            title_len += 1;
        }
        Self {
            hwnd: hwnd as u64,
            title: title_buf,
            title_len,
            rect: RectWrapper(rect),
            grid_cells: [(0, 0); crate::MAX_WINDOW_GRID_CELLS],
            grid_cells_len: 0,
            monitor_ids: [0usize; 8],
            monitor_cells: [[(0, 0); 8]; 8],
            monitor_cells_lens: [0u32; 8],
            monitor_cells_len: 0,
            z_order: 0,
            is_visible: true,
            is_minimized: false,
            process_id: 0,
            class_name: [0u16; 256],
            class_name_len: 0,
        }
    }


    pub fn update_rect(&mut self, new_rect: RECT) {
        self.rect = RectWrapper(new_rect);
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

// Wrapper for RECT to allow trait implementations
#[derive(Copy, Clone)]
pub struct RectWrapper(pub RECT);

impl fmt::Debug for RectWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rect = &self.0;
        f.debug_struct("RECT")
            .field("left", &rect.left)
            .field("top", &rect.top)
            .field("right", &rect.right)
            .field("bottom", &rect.bottom)
            .finish()
    }
}
impl RectWrapper {
    pub fn from_rect(rect: RECT) -> Self {
        RectWrapper(rect)
    }
    pub fn to_rect(&self) -> RECT {
        self.0
    }
}
// SAFETY: RECT is a plain-old-data struct (all i32 fields), so it's safe to implement Send/Sync for the wrapper
unsafe impl Send for RectWrapper {}
unsafe impl Sync for RectWrapper {}
use std::ops::Deref;

impl Deref for RectWrapper {
    type Target = RECT;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}