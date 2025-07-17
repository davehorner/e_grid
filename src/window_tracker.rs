use dashmap::DashMap;
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::*;

use crate::grid::layout::GridLayout;
use crate::grid::GridConfig;
use crate::monitor_grid::MonitorGrid;
use crate::window::info::{RectWrapper, WindowInfo};
use crate::window::{self, WindowAnimation};
use crate::{CellState, EasingType, WindowEventCallbackBox};

// Window enumeration callback function
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    let counter = tracker.enum_counter.fetch_add(1, Ordering::SeqCst) + 1;

    if WindowTracker::is_manageable_window(hwnd as u64) {
        if tracker.add_window(hwnd as u64) { // store as u64
             // println!("  -> Added successfully");
        } else {
            // println!("  -> Failed to add window");
        }
    }

    1 // Continue enumeration
}

pub struct WindowTracker {
    pub windows: DashMap<u64, WindowInfo>, // Lock-free concurrent HashMap, now u64
    pub monitor_rect: RECT,                // Virtual screen rect
    pub config: crate::grid::GridConfig,   // Dynamic grid configuration
    pub grid: Vec<Vec<CellState>>,         // Virtual grid (dynamic)
    pub monitor_grids: Vec<MonitorGrid>,   // Individual monitor grids
    pub enum_counter: AtomicUsize,         // Atomic counter for lock-free access
    pub active_animations: DashMap<u64, WindowAnimation>, // Lock-free animations, now u64
    pub saved_layouts: DashMap<String, GridLayout>, // Lock-free layouts
    pub event_callbacks: Vec<WindowEventCallbackBox>, // Event callbacks
    pub desktop_hwnds: Vec<u64>,           // Track all desktop (Progman/WorkerW) HWNDs
    pub last_scan_time: std::sync::Mutex<std::time::Instant>,
}

impl WindowTracker {
    /// Returns the HWND (u64) of the current foreground window, or None if not available.
    pub fn get_foreground_window() -> Option<u64> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                None
            } else {
                Some(hwnd as u64)
            }
        }
    }

    pub fn move_window_to_rect(&self, hwnd: u64, rect: RECT) -> Result<(), String> {
        unsafe {
            use winapi::um::winuser::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
            let result = SetWindowPos(
                hwnd as winapi::shared::windef::HWND,
                std::ptr::null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if result == 0 {
                Err(format!("SetWindowPos failed for 0x{:X}", hwnd))
            } else {
                Ok(())
            }
        }
    }

    pub fn new() -> Self {
        let mut ret = Self::new_with_config(GridConfig::default());
        ret.find_desktop_hwnds();
        ret.last_scan_time =
            std::sync::Mutex::new(std::time::Instant::now() - std::time::Duration::from_secs(1));
        ret
    }
    /// Returns the current grid state in IPC protocol format (GridState)
    pub fn get_ipc_grid_state(&self) -> crate::ipc_protocol::GridState {
        crate::ipc_protocol::GridState {
            rows: self.config.rows as u32,
            cols: self.config.cols as u32,
            grid: {
                let mut arr = [[0u64; 32]; 32];
                for (i, row) in self.grid.iter().enumerate().take(self.config.rows) {
                    for (j, cell) in row.iter().enumerate().take(self.config.cols) {
                        arr[i][j] = match cell {
                            CellState::Occupied(hwnd) => *hwnd,
                            _ => 0,
                        };
                    }
                }
                arr
            },
        }
    }

    /// Returns the current window list in IPC protocol format (Vec<crate::grid::WindowInfo>)
    pub fn get_ipc_window_list(&self) -> Vec<crate::grid::WindowInfo> {
        self.windows
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }
    pub fn new_with_config(config: GridConfig) -> Self {
        // Get the virtual screen dimensions (all monitors combined)
        let rect = unsafe {
            RECT {
                left: GetSystemMetrics(SM_XVIRTUALSCREEN),
                top: GetSystemMetrics(SM_YVIRTUALSCREEN),
                right: GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN),
                bottom: GetSystemMetrics(SM_YVIRTUALSCREEN) + GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        };

        let grid = vec![vec![CellState::Empty; config.cols]; config.rows];
        let mut tracker = Self {
            windows: DashMap::new(),
            monitor_rect: rect,
            config: config.clone(),
            grid,
            monitor_grids: Vec::new(),
            enum_counter: AtomicUsize::new(0),
            active_animations: DashMap::new(),
            saved_layouts: DashMap::new(),
            event_callbacks: Vec::new(),
            desktop_hwnds: Vec::new(),
            //nitialize it to "2 seconds ago" so the first call is not throttled.
            last_scan_time: std::sync::Mutex::new(
                std::time::Instant::now() - std::time::Duration::from_secs(2),
            ),
        };

        // Initialize individual monitor grids
        tracker.initialize_monitor_grids();
        tracker
    }

    pub fn find_desktop_hwnds(&mut self) {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use winapi::um::winuser::{EnumWindows, GetClassNameW};
        self.desktop_hwnds.clear();
        unsafe {
            extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
                let desktop_hwnds = unsafe { &mut *(lparam as *mut Vec<u64>) };
                let mut class_buf = [0u16; 256];
                let len = unsafe { GetClassNameW(hwnd, class_buf.as_mut_ptr(), 256) };
                if len > 0 {
                    let class = OsString::from_wide(&class_buf[..len as usize])
                        .to_string_lossy()
                        .to_string();
                    if class == "Progman"
                        || class == "WorkerW"
                        || class == "ApplicationFrameWindow"
                        || class == "Windows.UI.Core.CoreWindow"
                    {
                        desktop_hwnds.push(hwnd as u64);
                    }
                }
                1
            }
            EnumWindows(Some(enum_proc), &mut self.desktop_hwnds as *mut _ as LPARAM);
        }
    }

    pub fn is_desktop_hwnd(&self, hwnd: u64) -> bool {
        self.desktop_hwnds.contains(&hwnd)
    }
    pub fn initialize_monitor_grids(&mut self) {
        self.monitor_grids.clear();
        let monitors = self.get_actual_monitor_bounds();

        for (index, monitor_rect) in monitors.iter().enumerate() {
            let monitor_grid =
                MonitorGrid::new_with_config(index, *monitor_rect, self.config.clone());
            self.monitor_grids.push(monitor_grid);
        }

        println!(
            "Initialized {} individual monitor grids",
            self.monitor_grids.len()
        );
    }

    // Event callback management
    pub fn register_event_callback(&mut self, callback: WindowEventCallbackBox) {
        self.event_callbacks.push(callback);
    }

    pub fn clear_event_callbacks(&mut self) {
        self.event_callbacks.clear();
    }

    // Call event callbacks
    fn trigger_window_created(&self, hwnd: u64, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_created(hwnd, window_info);
        }
    }

    fn trigger_window_destroyed(&self, hwnd: u64) {
        for callback in &self.event_callbacks {
            callback.on_window_destroyed(hwnd);
        }
    }

    fn trigger_window_moved(&self, hwnd: u64, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_moved(hwnd, window_info);
        }
    }

    fn trigger_window_activated(&self, hwnd: u64, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_activated(hwnd, window_info);
        }
    }

    fn trigger_window_minimized(&self, hwnd: u64) {
        for callback in &self.event_callbacks {
            callback.on_window_minimized(hwnd);
        }
    }

    fn trigger_window_restored(&self, hwnd: u64, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_restored(hwnd, window_info);
        }
    }

    pub fn get_window_class(hwnd: u64) -> String {
        unsafe {
            let mut buffer = [0u16; 256];
            let len = GetClassNameW(hwnd as HWND, buffer.as_mut_ptr(), buffer.len() as i32);
            if len > 0 {
                String::from_utf16_lossy(&buffer[..len as usize])
                    .chars()
                    .take(50)
                    .collect()
            } else {
                String::new()
            }
        }
    }

    pub fn get_window_title(hwnd: u64) -> String {
        unsafe {
            let mut buffer = [0u16; 256];
            let len = GetWindowTextW(hwnd as HWND, buffer.as_mut_ptr(), buffer.len() as i32);
            if len > 0 {
                String::from_utf16_lossy(&buffer[..len as usize])
                    .chars()
                    .take(50)
                    .collect()
            } else {
                String::new()
            }
        }
    }

    pub fn get_window_rect(hwnd: u64) -> Option<RECT> {
        unsafe {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd as HWND, &mut rect) != 0 {
                Some(rect)
            } else {
                None
            }
        }
    }

    pub fn is_manageable_window(hwnd: u64) -> bool {
        unsafe {
            // let title = Self::get_window_title(hwnd);
            if IsWindow(hwnd as HWND) == 0 {
                //println!("[DEBUG] Skipping hwnd=0x{:X}: not a valid window", hwnd);
                return false;
            }
            if IsWindowVisible(hwnd as HWND) == 0 {
                //println!("[DEBUG] Skipping hwnd=0x{:X}: not visible", hwnd);
                return false;
            }
            // Skip minimized windows
            if IsIconic(hwnd as HWND) != 0 {
                //println!("[DEBUG] Skipping hwnd=0x{:X}: minimized", hwnd);
                return false;
            }

            let ex_style = GetWindowLongW(hwnd as HWND, GWL_EXSTYLE) as u32;
            if (ex_style & WS_EX_TOOLWINDOW) != 0 {
                //println!("[DEBUG] Skipping hwnd=0x{:X}: toolwindow", hwnd);
                return false;
            }
            let style = GetWindowLongW(hwnd as HWND, GWL_STYLE) as u32;
            if (style & WS_CHILD) != 0 {
                //println!("[DEBUG] Skipping hwnd=0x{:X}: is a child window", hwnd);
                return false;
            }
            // Exclude "Windows.UI.Composition.DesktopWindowContentBridge" windows
            let class_name = Self::get_window_class_name(hwnd);
            if class_name.is_empty()
                || class_name == "Windows.UI.Composition.DesktopWindowContentBridge"
                || class_name == "Windows.UI.Core.CoreWindow"
                || class_name == "XamlExplorerHostIslandWindow"
            {
                return false;
            }
            // Exclude Chrome_WidgetWin_1 windows with no border (WS_BORDER not set)
            if class_name == "Chrome_WidgetWin_1" {
                let style = GetWindowLongW(hwnd as HWND, GWL_STYLE) as u32;
                if (style & WS_BORDER) == 0 {
                    return false; //tooltip!
                }
            }
            //println!("[DEBUG] Accepting hwnd=0x{:X}: '{}'", hwnd, title);
            true
        }
    }

    /// Given a window RECT, return the bounding grid rectangle as UsizeRect (start_row, start_col, end_row, end_col)
    pub fn window_to_grid_rect(&self, rect: &RECT) -> crate::window::info::UsizeRect {
        // Skip invalid rectangles
        if rect.right <= rect.left || rect.bottom <= rect.top {
            return crate::window::info::UsizeRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
        }

        let cell_width =
            (self.monitor_rect.right - self.monitor_rect.left) / self.config.cols as i32;
        let cell_height =
            (self.monitor_rect.bottom - self.monitor_rect.top) / self.config.rows as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return crate::window::info::UsizeRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
        }

        let start_col = ((rect.left - self.monitor_rect.left) / cell_width).max(0) as usize;
        let end_col = (((rect.right - self.monitor_rect.left - 1) / cell_width)
            .min(self.config.cols as i32 - 1)
            .max(0)) as usize;
        let start_row = ((rect.top - self.monitor_rect.top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom - self.monitor_rect.top - 1) / cell_height)
            .min(self.config.rows as i32 - 1)
            .max(0) as usize;

        if start_row > end_row
            || start_col > end_col
            || start_row >= self.config.rows
            || end_row >= self.config.rows
        {
            return crate::window::info::UsizeRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
        }

        return crate::window::info::UsizeRect {
            left: start_col,
            top: start_row,
            right: end_col,
            bottom: end_row,
        };
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid rectangle (right must be > left, bottom > top)
        if rect.right < rect.left || rect.bottom < rect.top {
            return cells;
        }

        let cell_width =
            (self.monitor_rect.right - self.monitor_rect.left) / self.config.cols as i32;
        let cell_height =
            (self.monitor_rect.bottom - self.monitor_rect.top) / self.config.rows as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return cells;
        }

        // Calculate potential range of cells that might be affected
        let start_col = ((rect.left - self.monitor_rect.left) / cell_width).max(0) as usize;
        let end_col = ((rect.right - self.monitor_rect.left) / cell_width)
            .min(self.config.cols as i32 - 1) as usize;
        let start_row = ((rect.top - self.monitor_rect.top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom - self.monitor_rect.top) / cell_height)
            .min(self.config.rows as i32 - 1) as usize;

        // Additional bounds checking
        if start_col >= self.config.cols || start_row >= self.config.rows {
            return cells;
        }

        // Check coverage for each potentially affected cell
        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < self.config.rows && col < self.config.cols {
                    // Calculate the exact bounds of this grid cell
                    let cell_rect = RECT {
                        left: self.monitor_rect.left + (col as i32 * cell_width),
                        top: self.monitor_rect.top + (row as i32 * cell_height),
                        right: self.monitor_rect.left + ((col + 1) as i32 * cell_width),
                        bottom: self.monitor_rect.top + ((row + 1) as i32 * cell_height),
                    };
                    // Only include cell if window meets coverage threshold
                    if crate::util::meets_coverage_threshold(rect, &cell_rect) {
                        // Push all cells from start to end (full rectangle coverage)
                        cells.push((row, col));
                    }
                }
            }
        }

        cells
    }

    pub fn update_grid(&mut self) {
        // Throttle: only allow once every 1 seconds
        {
            if let Ok(mut last) = self.last_scan_time.try_lock() {
                let now = std::time::Instant::now();
                if now.duration_since(*last) < std::time::Duration::from_secs(1) {
                    // Too soon, skip update
                    // println!("[DEBUG] Skipping grid update: too soon since last update");
                    return;
                }
                *last = now;
            }
        }
        // Reset grid to initial state (keeping off-screen cells marked)
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                match self.grid[row][col] {
                    CellState::OffScreen => {
                        // Keep off-screen cells as they are
                    }
                    _ => {
                        // Reset other cells to empty
                        self.grid[row][col] = CellState::Empty;
                    }
                }
            }
        }
        // Place windows on the grid (only once)
        for entry in &self.windows {
            let (hwnd, window_info) = entry.pair();
            // if (*hwnd & 0xFF) == 0x2E {
            //     let title =
            //         String::from_utf16_lossy(&window_info.title[..window_info.title_len as usize]);
            //     let class_name = String::from_utf16_lossy(
            //         &window_info.class_name[..window_info.class_name_len as usize],
            //     );
            //     println!(
            //         "[DEBUG] HWND ending in 2E: hwnd=0x{:X}, title='{}', class='{}'",
            //         hwnd, title, class_name
            //     );
            // }
            // Skip desktop windows for occupancy
            if self.is_desktop_hwnd(*hwnd) {
                continue;
            }
            // Compute the grid cells this window occupies based on its geometry
            if let Some(rect) = Self::get_window_rect(*hwnd) {
                let cells = self.window_to_grid_cells(&rect);
                for (row, col) in cells {
                    if row < self.config.rows && col < self.config.cols {
                        match self.grid[row][col] {
                            CellState::Occupied(existing_hwnd) if existing_hwnd == *hwnd => {
                                // Already occupied by this hwnd, skip
                            }
                            _ => {
                                self.grid[row][col] = CellState::Occupied(*hwnd);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get the primary monitor rectangle for window positioning
    fn get_primary_monitor_rect(&self) -> RECT {
        if !self.monitor_grids.is_empty() {
            // Find the monitor at (0,0) - this is the true primary monitor
            for monitor_grid in &self.monitor_grids {
                if monitor_grid.monitor_rect.left == 0 && monitor_grid.monitor_rect.top == 0 {
                    let rect = RECT {
                        left: monitor_grid.monitor_rect.left,
                        top: monitor_grid.monitor_rect.top,
                        right: monitor_grid.monitor_rect.right,
                        bottom: monitor_grid.monitor_rect.bottom,
                    };
                    println!(
                        "🖥️  Using true primary monitor at (0,0): ({}, {}) to ({}, {})",
                        rect.left, rect.top, rect.right, rect.bottom
                    );
                    return rect;
                }
            }

            // Fallback to first monitor if no monitor at (0,0) found
            let primary_monitor = &self.monitor_grids[0];
            let rect = RECT {
                left: primary_monitor.monitor_rect.left,
                top: primary_monitor.monitor_rect.top,
                right: primary_monitor.monitor_rect.right,
                bottom: primary_monitor.monitor_rect.bottom,
            };
            println!(
                "🖥️  Fallback to first monitor: ({}, {}) to ({}, {})",
                rect.left, rect.top, rect.right, rect.bottom
            );
            rect
        } else {
            // Fallback to virtual screen if no monitor grids
            println!(
                "⚠️  No monitor grids available, using virtual screen: ({}, {}) to ({}, {})",
                self.monitor_rect.left,
                self.monitor_rect.top,
                self.monitor_rect.right,
                self.monitor_rect.bottom
            );
            self.monitor_rect
        }
    }

    /// Convert grid cell to window rectangle on primary monitor
    pub fn primary_monitor_cell_to_rect(&self, row: usize, col: usize) -> Option<RECT> {
        if row >= self.config.rows || col >= self.config.cols {
            println!(
                "❌ Invalid cell coordinates: ({}, {}) for grid ({}x{})",
                row, col, self.config.rows, self.config.cols
            );
            return None;
        }

        let monitor_rect = self.get_primary_monitor_rect();
        let grid_width = monitor_rect.right - monitor_rect.left;
        let grid_height = monitor_rect.bottom - monitor_rect.top;

        let cell_width = grid_width / self.config.cols as i32;
        let cell_height = grid_height / self.config.rows as i32;

        println!("🧮 Cell calculation for ({}, {}):", row, col);
        println!(
            "   Monitor rect: ({}, {}) to ({}, {})",
            monitor_rect.left, monitor_rect.top, monitor_rect.right, monitor_rect.bottom
        );
        println!(
            "   Grid dimensions: {}x{} ({}x{} cells)",
            grid_width, grid_height, self.config.cols, self.config.rows
        );
        println!("   Cell size: {}x{}", cell_width, cell_height);

        let left = monitor_rect.left + (col as i32 * cell_width);
        let top = monitor_rect.top + (row as i32 * cell_height);
        let right = left + cell_width;
        let bottom = top + cell_height;

        println!(
            "   Calculated cell rect: ({}, {}) to ({}, {})",
            left, top, right, bottom
        );

        Some(RECT {
            left,
            top,
            right,
            bottom,
        })
    }

    pub fn add_window(&mut self, hwnd: u64) -> bool {
        if self.is_desktop_hwnd(hwnd) {
            println!("[DEBUG] Skipping hwnd=0x{:X}: is desktop window", hwnd);
            return false;
        }
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let mut title_buf = [0u16; 256];
            let title_len = unsafe {
                GetWindowTextW(hwnd as HWND, title_buf.as_mut_ptr(), title_buf.len() as i32)
            };
            let title = if title_len > 0 {
                String::from_utf16_lossy(&title_buf[..title_len as usize])
            } else {
                String::new()
            };
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);

            // Get additional required fields for WindowInfo
            let is_maximized = WindowTracker::is_window_maximized(hwnd);
            let is_visible = unsafe { IsWindowVisible(hwnd as HWND) != 0 };
            let is_minimized = unsafe { IsIconic(hwnd as HWND) != 0 };
            let mut process_id: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd as HWND, &mut process_id);
            }
            let mut class_name_buf = [0u16; 256];
            let class_name_len = unsafe {
                GetClassNameW(
                    hwnd as HWND,
                    class_name_buf.as_mut_ptr(),
                    class_name_buf.len() as i32,
                )
            };
            let class_name = if class_name_len > 0 {
                String::from_utf16_lossy(&class_name_buf[..class_name_len as usize])
            } else {
                String::new()
            };
            // println!(
            //     "[DEBUG] Adding window: hwnd=0x{:X}, title='{}', class='{}', rect=({}, {}, {}, {})",
            //     hwnd, title, class_name, rect.left, rect.top, rect.right, rect.bottom
            // );

            let window_info = WindowInfo {
                hwnd,
                title: title_buf,
                title_len: title_len as u32,
                // grid_cells: {
                //     let mut arr = [(0, 0); crate::MAX_WINDOW_GRID_CELLS];
                //     for (i, cell) in grid_cells
                //         .iter()
                //         .enumerate()
                //         .take(crate::MAX_WINDOW_GRID_CELLS)
                //     {
                //         arr[i] = *cell;
                //     }
                //     arr
                // },
                // grid_cells_len: grid_cells.len() as u32,
                monitor_ids: {
                    let mut arr = [0usize; 8];
                    for (i, id) in monitor_cells.keys().cloned().enumerate().take(8) {
                        arr[i] = id;
                    }
                    arr
                },
                // monitor_cells: {
                //     let mut arr = [[(0, 0); 8]; 8];
                //     for (monitor_id, cells) in monitor_cells.iter() {
                //         for (i, cell) in cells.iter().enumerate().take(8) {
                //             if *monitor_id < 8 {
                //                 arr[*monitor_id][i] = *cell;
                //             }
                //         }
                //     }
                //     arr
                // },
                // monitor_cells_lens: {
                //     let mut arr = [0u32; 8];
                //     for (monitor_id, cells) in monitor_cells.iter() {
                //         if *monitor_id < 8 {
                //             arr[*monitor_id] = cells.len() as u32;
                //         }
                //     }
                //     arr
                // },
                // monitor_cells_len: monitor_cells.len() as u32,
                z_order: crate::util::get_hwnd_z_order_map()
                    .get(&hwnd)
                    .copied()
                    .unwrap_or(0) as u32,
                window_rect: RectWrapper::from_rect(rect),
                is_visible,
                is_minimized,
                is_maximized,
                process_id,
                class_name: class_name_buf,
                class_name_len: class_name.len() as u32,
                // grid_rect: self.window_to_grid_rect(&rect),
            };

            self.windows.insert(hwnd, window_info.clone());
            self.update_grid();
            self.update_monitor_grids();

            // Trigger callback
            self.trigger_window_created(hwnd, &window_info);

            return true;
        }
        println!("[DEBUG] Skipping hwnd=0x{:X}: could not get rect", hwnd);
        false
    }

    pub fn remove_window(&mut self, hwnd: u64) -> bool {
        if self.windows.remove(&hwnd).is_some() {
            self.update_grid();
            self.update_monitor_grids();

            // Trigger callback
            self.trigger_window_destroyed(hwnd);

            return true;
        }
        false
    }

    pub fn update_window(&mut self, hwnd: u64) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);

            // Update the window info
            let updated = if let Some(mut window_entry) = self.windows.get_mut(&hwnd) {
                window_entry.window_rect = RectWrapper::from_rect(rect);
                // window_entry.grid_rect = self.window_to_grid_rect(&rect);
                window_entry.is_visible = unsafe { IsWindowVisible(hwnd as HWND) != 0 };
                // let mut arr = [(0, 0); crate::MAX_WINDOW_GRID_CELLS];
                // for (i, cell) in grid_cells
                //     .iter()
                //     .enumerate()
                //     .take(crate::MAX_WINDOW_GRID_CELLS)
                // {
                //     arr[i] = *cell;
                // }
                // // Convert HashMap<usize, Vec<(usize, usize)>> to [[(usize, usize); 8]; 8]
                // let mut arr = [[(0, 0); 8]; 8];
                // for (monitor_id, cells) in monitor_cells.iter() {
                //     for (i, cell) in cells.iter().enumerate().take(8) {
                //         if *monitor_id < 8 {
                //             arr[*monitor_id][i] = *cell;
                //         }
                //     }
                // }
                // window_entry.monitor_cells = arr;
                true
            } else {
                false
            };

            if updated {
                self.update_grid();
                self.update_monitor_grids();

                // Get the updated window info for callback
                if let Some(window_info) = self.windows.get(&hwnd) {
                    self.trigger_window_moved(hwnd, &*window_info);
                }

                return true;
            }
        }
        false
    }

    pub fn print_virtual_grid(&self) {
        println!();
        println!("{}", "=".repeat(60));
        println!(
            "Virtual Grid Tracker - {}x{} Grid ({} windows)",
            self.config.rows,
            self.config.cols,
            self.windows.len()
        );
        println!(
            "Virtual Monitor: {}x{} px",
            self.monitor_rect.right - self.monitor_rect.left,
            self.monitor_rect.bottom - self.monitor_rect.top
        );
        println!("{}", "=".repeat(60));

        // Print column headers
        print!("   ");
        for col in 0..self.config.cols {
            print!("{:2} ", col);
        }
        println!();

        // Print grid with different symbols for different states
        for row in 0..self.config.rows {
            print!("{:2} ", row);

            for col in 0..self.config.cols {
                match self.grid[row][col] {
                    CellState::Occupied(_hwnd) => {
                        // Print the last two hex digits of the hwnd for visual distinction
                        // Find all windows occupying this cell, pick the topmost by Z-order
                        let mut topmost_hwnd: Option<u64> = None;
                        let mut topmost_z: Option<usize> = None;
                        for entry in &self.windows {
                            let (hwnd, window_info) = entry.pair();
                            // For the virtual grid, use window_info.grid_cells
                            // Check if any monitor_cells contain (row, col)
                            if let CellState::Occupied(existing_hwnd) = self.grid[row][col] {
                                let z_map = crate::util::get_hwnd_z_order_map();
                                if *hwnd == existing_hwnd {
                                    if let Some(&z) = z_map.get(hwnd) {
                                        if topmost_z.map_or(true, |tz| z < tz) {
                                            topmost_hwnd = Some(*hwnd);
                                            topmost_z = Some(z);
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(hwnd) = topmost_hwnd {
                            if self.is_desktop_hwnd(hwnd) {
                                print!(".. ");
                            } else {
                                print!("{:02X} ", hwnd & 0xFF);
                            }
                        } else {
                            print!(".. ");
                        }
                    }
                    CellState::Empty => {
                        print!(".. ");
                    }
                    CellState::OffScreen => {
                        print!("XX ");
                    }
                }
            }
            println!();
        }

        println!();
        // println!("Active Windows:");
        // println!("{}", "-".repeat(60));

        // for (i, (_hwnd, window_info)) in self.windows.iter().enumerate() {
        //     if i < 15 {
        //         println!("## {} ({} cells)",
        //             window_info.title,
        //             window_info.grid_cells.len()
        //         );
        //     }
        // }

        // if self.windows.len() > 15 {
        //     println!("... and {} more windows", self.windows.len() - 15);
        // }

        println!();
    }

    pub fn print_all_grids(&self) {
        // Print virtual grid
        println!();
        println!("=== VIRTUAL GRID (All Monitors Combined) ===");
        self.print_virtual_grid();
        let z_map = crate::util::get_hwnd_z_order_map();

        // Find the foremost (topmost) window across all windows
        let mut foremost_hwnd: Option<u64> = None;
        let mut foremost_z: Option<usize> = None;
        let mut fg_hwnd = None;
        for entry in &self.windows {
            let (hwnd, _) = entry.pair();
            if let Some(&z) = z_map.get(hwnd) {
                if foremost_z.map_or(true, |fz| z < fz) {
                    foremost_hwnd = Some(*hwnd);
                    foremost_z = Some(z);
                }
            }
        }

        // Get the current foreground window and store its HWND
        fg_hwnd = unsafe { GetForegroundWindow().as_ref() }.map(|hwnd| hwnd as *const _ as u64);

        if let Some(fg_hwnd) = fg_hwnd {
            println!("Foreground window: \x1b[34m0x{:X}\x1b[0m", fg_hwnd);
        }
        // Print individual monitor grids
        for (index, monitor_grid) in self.monitor_grids.iter().enumerate() {
            println!();
            println!("=== MONITOR {} GRID ===", index + 1);
            println!(
                "Monitor bounds: ({}, {}) to ({}, {})",
                monitor_grid.monitor_rect.left,
                monitor_grid.monitor_rect.top,
                monitor_grid.monitor_rect.right,
                monitor_grid.monitor_rect.bottom
            );

            // Count windows on this monitor
            let mut windows_on_monitor = 0;
            for entry in &self.windows {
                let (hwnd, window_info) = entry.pair();

                // For this monitor, print grid cell info for each window
                // We no longer use monitor_cells, so just print window info if it overlaps this monitor
                let window_rect = window_info.window_rect.0;
                let monitor_rect = RECT {
                    left: monitor_grid.monitor_rect.left,
                    top: monitor_grid.monitor_rect.top,
                    right: monitor_grid.monitor_rect.right,
                    bottom: monitor_grid.monitor_rect.bottom,
                };
                // Check if window overlaps this monitor
                let overlaps = window_rect.left < monitor_rect.right
                    && window_rect.right > monitor_rect.left
                    && window_rect.top < monitor_rect.bottom
                    && window_rect.bottom > monitor_rect.top;
                if overlaps {
                    windows_on_monitor += 1;
                }
            }
            if windows_on_monitor > 100 {
                println!("    ... and {} more windows", windows_on_monitor - 100);
            }
            println!("Windows on this monitor: {}", windows_on_monitor);
            println!(
                "Grid size: {} rows x {} cols ({} cells)",
                monitor_grid.config.rows,
                monitor_grid.config.cols,
                monitor_grid.config.rows * monitor_grid.config.cols
            );
            println!(
                "Monitor resolution: {}x{} px",
                monitor_grid.monitor_rect.right - monitor_grid.monitor_rect.left,
                monitor_grid.monitor_rect.bottom - monitor_grid.monitor_rect.top
            );
            // Print column headers
            print!("   ");
            for col in 0..monitor_grid.config.cols {
                print!("{:2} ", col);
            }
            println!();

            // Determine max row to print (support up to 91+ for special visualization)
            let max_display_row = monitor_grid.config.rows.min(95);

            // Print grid rows
            for row in 0..max_display_row {
                print!("{:2} ", row);
                for col in 0..monitor_grid.config.cols.min(32) {
                    if row == 89 {
                        // Row 89: Display cell count for each window (number of cells it occupies)
                        let mut max_cell_count = 0;
                        for entry in &self.windows {
                            let (hwnd, window_info) = entry.pair();
                            let window_rect = window_info.window_rect.0;
                            let monitor_rect = RECT {
                                left: monitor_grid.monitor_rect.left,
                                top: monitor_grid.monitor_rect.top,
                                right: monitor_grid.monitor_rect.right,
                                bottom: monitor_grid.monitor_rect.bottom,
                            };
                            let overlaps = window_rect.left < monitor_rect.right
                                && window_rect.right > monitor_rect.left
                                && window_rect.top < monitor_rect.bottom
                                && window_rect.bottom > monitor_rect.top;

                            if overlaps {
                                let cells = monitor_grid.window_to_grid_cells(&window_rect);
                                // Check if this window has any cells at the current column
                                let has_cell_in_col = cells.iter().any(|(_, c)| *c == col);
                                if has_cell_in_col {
                                    max_cell_count = max_cell_count.max(cells.len());
                                }
                            }
                        }
                        if max_cell_count > 0 {
                            print!("{:02} ", max_cell_count.min(99));
                        } else {
                            print!(".. ");
                        }
                    } else if row == 90 {
                        // Row 90: Display fully spanning virtual grid
                        let mut has_spanning_window = false;
                        for entry in &self.windows {
                            let (hwnd, window_info) = entry.pair();
                            let window_rect = window_info.window_rect.0;

                            // Check if window spans multiple monitors or is maximized
                            let window_width = window_rect.right - window_rect.left;
                            let window_height = window_rect.bottom - window_rect.top;
                            let monitor_width =
                                monitor_grid.monitor_rect.right - monitor_grid.monitor_rect.left;
                            let monitor_height =
                                monitor_grid.monitor_rect.bottom - monitor_grid.monitor_rect.top;

                            let is_spanning = window_width >= monitor_width * 2
                                || window_height >= monitor_height * 2;

                            if is_spanning {
                                let cells = monitor_grid.window_to_grid_cells(&window_rect);
                                // Check if this spanning window has any cells at the current column
                                let has_cell_in_col = cells.iter().any(|(_, c)| *c == col);
                                if has_cell_in_col {
                                    has_spanning_window = true;
                                    break;
                                }
                            }
                        }
                        if has_spanning_window {
                            print!("## ");
                        } else {
                            print!(".. ");
                        }
                    } else if row >= 91 {
                        // Row 91+: Display windows by Z-order depth (91 = foremost, 92 = second layer, etc.)
                        let depth_layer = row - 91;
                        let mut windows_at_col: Vec<(u64, usize)> = Vec::new();

                        for entry in &self.windows {
                            let (hwnd, window_info) = entry.pair();
                            let window_rect = window_info.window_rect.0;
                            let monitor_rect = RECT {
                                left: monitor_grid.monitor_rect.left,
                                top: monitor_grid.monitor_rect.top,
                                right: monitor_grid.monitor_rect.right,
                                bottom: monitor_grid.monitor_rect.bottom,
                            };
                            let overlaps = window_rect.left < monitor_rect.right
                                && window_rect.right > monitor_rect.left
                                && window_rect.top < monitor_rect.bottom
                                && window_rect.bottom > monitor_rect.top;

                            if overlaps {
                                let cells = monitor_grid.window_to_grid_cells(&window_rect);
                                // Check if this window has any cells at the current column
                                let has_cell_in_col = cells.iter().any(|(_, c)| *c == col);
                                if has_cell_in_col {
                                    if let Some(&z) = z_map.get(hwnd) {
                                        windows_at_col.push((*hwnd, z));
                                    }
                                }
                            }
                        }

                        // Sort by Z-order (lower Z = more foreground)
                        windows_at_col.sort_by_key(|&(_, z)| z);

                        if let Some(&(hwnd, _)) = windows_at_col.get(depth_layer) {
                            if self.is_desktop_hwnd(hwnd) {
                                print!(".. ");
                            } else {
                                print!("{:02X} ", hwnd & 0xFF);
                            }
                        } else {
                            print!(".. ");
                        }
                    } else {
                        // Standard grid display for rows 0-88
                        // Find all windows occupying this cell using dynamic calculation
                        let mut topmost_hwnd: Option<u64> = None;
                        let mut topmost_z: Option<usize> = None;

                        for entry in &self.windows {
                            let (hwnd, window_info) = entry.pair();
                            let window_rect = window_info.window_rect.0;

                            // Check if window overlaps this monitor
                            let monitor_rect = RECT {
                                left: monitor_grid.monitor_rect.left,
                                top: monitor_grid.monitor_rect.top,
                                right: monitor_grid.monitor_rect.right,
                                bottom: monitor_grid.monitor_rect.bottom,
                            };
                            let overlaps = window_rect.left < monitor_rect.right
                                && window_rect.right > monitor_rect.left
                                && window_rect.top < monitor_rect.bottom
                                && window_rect.bottom > monitor_rect.top;

                            if overlaps {
                                // Use dynamic calculation to check if this window covers this cell
                                let cells = monitor_grid.window_to_grid_cells(&window_rect);
                                if cells.contains(&(row, col)) {
                                    if let Some(&z) = z_map.get(hwnd) {
                                        if topmost_z.map_or(true, |tz| z < tz) {
                                            topmost_hwnd = Some(*hwnd);
                                            topmost_z = Some(z);
                                        }
                                    }
                                }
                            }
                        }

                        if let Some(hwnd) = topmost_hwnd {
                            if self.is_desktop_hwnd(hwnd) {
                                print!(".. ");
                            } else {
                                // Check if this is the foremost window and color it red
                                if foremost_hwnd == Some(hwnd) {
                                    print!("\x1b[31m{:02X}\x1b[0m ", hwnd & 0xFF);
                                // Red foreground
                                } else if fg_hwnd == Some(hwnd) {
                                    print!("\x1b[34m{:02X}\x1b[0m ", hwnd & 0xFF);
                                // Blue foreground
                                } else {
                                    print!("{:02X} ", hwnd & 0xFF);
                                }
                            }
                        } else {
                            // Check if cell is off-screen
                            match monitor_grid.grid[row][col] {
                                CellState::OffScreen => {
                                    print!("XX ");
                                }
                                _ => {
                                    print!(".. ");
                                }
                            }
                        }
                    }
                }

                // Add row description for special rows
                match row {
                    89 => println!(" <- Cell counts"),
                    90 => println!(" <- Spanning windows"),
                    91 => println!(" <- Foremost visible"),
                    92 => println!(" <- Second layer"),
                    93 => println!(" <- Third layer"),
                    94 => println!(" <- Fourth layer"),
                    _ => println!(),
                }
            }
        }

        // Print legend
        if foremost_hwnd.is_some() {
            println!();
            println!("Legend: \x1b[31mRed\x1b[0m = Foremost window (topmost Z-order)");
        }
        println!();
    }

    pub fn scan_existing_windows(&mut self) {
        println!("Starting window enumeration...");

        // Initialize grid with off-screen areas marked
        self.initialize_grid();
        // Throttle: only allow once every 2 seconds
        // {
        //     let mut last = self.last_scan_time.lock().unwrap();
        //     let now = std::time::Instant::now();
        //     if now.duration_since(*last) < std::time::Duration::from_secs(2) {
        //         // Too soon, skip scan
        //         return;
        //     }
        //     *last = now;
        // }
        self.enum_counter.store(0, Ordering::SeqCst); // Reset counter
        unsafe {
            let result = EnumWindows(Some(enum_windows_proc), self as *mut _ as LPARAM);
            println!("EnumWindows completed with result: {}", result);
        }
        println!(
            "Window enumeration finished. Found {} windows.",
            self.windows.len()
        );
        self.update_grid();
        self.update_monitor_grids();
    }

    pub fn get_monitor_info(&self) -> (i32, i32, i32, i32) {
        (
            self.monitor_rect.left,
            self.monitor_rect.top,
            self.monitor_rect.right - self.monitor_rect.left,
            self.monitor_rect.bottom - self.monitor_rect.top,
        )
    }

    pub fn initialize_grid(&mut self) {
        // Get actual monitor bounds (not virtual screen)
        let actual_monitors = self.get_actual_monitor_bounds();

        let cell_width =
            (self.monitor_rect.right - self.monitor_rect.left) / self.config.cols as i32;
        let cell_height =
            (self.monitor_rect.bottom - self.monitor_rect.top) / self.config.rows as i32;

        // Initialize all cells based on whether they're on an actual monitor
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let cell_left = self.monitor_rect.left + (col as i32 * cell_width);
                let cell_top = self.monitor_rect.top + (row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;

                // Check if this cell overlaps with any actual monitor
                let mut is_on_screen = false;
                for monitor_rect in &actual_monitors {
                    if cell_left < monitor_rect.right
                        && cell_right > monitor_rect.left
                        && cell_top < monitor_rect.bottom
                        && cell_bottom > monitor_rect.top
                    {
                        is_on_screen = true;
                        break;
                    }
                }

                self.grid[row][col] = if is_on_screen {
                    CellState::Empty
                } else {
                    CellState::OffScreen
                };
            }
        }
    }

    fn get_actual_monitor_bounds(&self) -> Vec<RECT> {
        let mut monitors = Vec::new();

        unsafe {
            // Enumerate all monitors
            extern "system" fn monitor_enum_proc(
                _hmonitor: winapi::shared::windef::HMONITOR,
                _hdc: winapi::shared::windef::HDC,
                rect: *mut RECT,
                data: LPARAM,
            ) -> i32 {
                unsafe {
                    let monitors = &mut *(data as *mut Vec<RECT>);
                    monitors.push(*rect);
                }
                1 // Continue enumeration
            }

            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(monitor_enum_proc),
                &mut monitors as *mut Vec<RECT> as LPARAM,
            );
        }

        monitors
    }

    pub fn calculate_monitor_cells(&self, rect: &RECT) -> HashMap<usize, Vec<(usize, usize)>> {
        let mut monitor_cells = HashMap::new();

        for monitor_grid in &self.monitor_grids {
            let cells = monitor_grid.window_to_grid_cells(rect);
            if !cells.is_empty() {
                monitor_cells.insert(monitor_grid.monitor_id, cells);
            }
        }

        monitor_cells
    }

    pub fn update_monitor_grids(&mut self) {
        // println!("Updating monitor grids...");
        for monitor_grid in &mut self.monitor_grids {
            // println!(
            //     "Updating grid for monitor {}: {}x{}",
            //     monitor_grid.monitor_id,
            //     monitor_grid.config.rows,
            //     monitor_grid.config.cols
            // );
            monitor_grid.update_grid_for_monitor(&self.windows);
        }
    }

    // Animation Management Methods

    pub fn start_window_animation(
        &mut self,
        hwnd: u64,
        target_rect: RECT,
        duration: Duration,
        easing: EasingType,
    ) -> Result<(), String> {
        if WindowTracker::is_window_maximized(hwnd) {
            println!(
                "HWND 0x{:X} is maximized (WindowTracker::is_maximized), skipping animation.",
                hwnd
            );
            return Ok(());
        }

        if let Some(current_rect) = Self::get_window_rect(hwnd) {
            if current_rect.left == target_rect.left
                && current_rect.top == target_rect.top
                && current_rect.right == target_rect.right
                && current_rect.bottom == target_rect.bottom
            {
                return Ok(());
            }
            let distance = (current_rect.left - target_rect.left).abs()
                + (current_rect.top - target_rect.top).abs()
                + (current_rect.right - target_rect.right).abs()
                + (current_rect.bottom - target_rect.bottom).abs();
            if distance <= 4 {
                // If the window is already at the target (within 2 pixel), skip animation
                self.move_window_to_rect(hwnd, target_rect)?;
                return Ok(());
            }
            let animation = WindowAnimation::new(
                hwnd,
                window::info::RectWrapper(current_rect),
                window::info::RectWrapper(target_rect),
                duration,
                easing.clone(),
            );
            self.active_animations.insert(hwnd, animation);
            let title = Self::get_window_title(hwnd);
            let class = Self::get_window_class(hwnd);
            println!(
                "🎬 Started animation for window {:?}: '{}' [{}] {} -> {} over {:?} {} {} {} {}",
                hwnd,
                title,
                class,
                format!(
                    "({},{},{},{})",
                    current_rect.left, current_rect.top, current_rect.right, current_rect.bottom
                ),
                format!(
                    "({},{},{},{})",
                    target_rect.left, target_rect.top, target_rect.right, target_rect.bottom
                ),
                duration,
                (current_rect.left - target_rect.left),
                (current_rect.top - target_rect.top),
                (current_rect.right - target_rect.right),
                (current_rect.bottom - target_rect.bottom)
            );
            Ok(())
        } else {
            Err(format!("Failed to get current rect for window {:?}", hwnd))
        }
    }

    pub fn update_animations(&mut self) -> (Vec<u64>, Vec<u64>) {
        let mut completed_animations = Vec::new();
        let mut failed_animations = Vec::new();
        // Collect keys that need to be processed
        let animation_keys: Vec<u64> = self
            .active_animations
            .iter()
            .map(|entry| *entry.key())
            .collect();

        for hwnd in animation_keys {
            if let Some(mut animation_entry) = self.active_animations.get_mut(&hwnd) {
                let is_window_maximized = WindowTracker::is_window_maximized(hwnd);
                if animation_entry.is_completed() || is_window_maximized {
                    completed_animations.push(hwnd);
                } else {
                    let current_rect = animation_entry.get_current_rect();

                    // Move window to current animation position
                    unsafe {
                        let result = SetWindowPos(
                            hwnd as HWND,
                            std::ptr::null_mut(),
                            current_rect.left,
                            current_rect.top,
                            current_rect.right - current_rect.left,
                            current_rect.bottom - current_rect.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                        if result == 0 {
                            let error = GetLastError();
                            println!(
                                "[DEBUG] SetWindowPos failed for hwnd=0x{:X} with error code: {}",
                                hwnd, error
                            );
                            // If error is 5 (access denied), try moving without resizing
                            if error == 5 {
                                let move_only_result = SetWindowPos(
                                    hwnd as HWND,
                                    std::ptr::null_mut(),
                                    current_rect.left,
                                    current_rect.top,
                                    0,
                                    0,
                                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOSIZE,
                                );
                                if move_only_result == 0 {
                                    let move_error = GetLastError();
                                    println!(
                                        "[DEBUG] Move-only SetWindowPos also failed for hwnd=0x{:X} with error code: {}",
                                        hwnd, move_error
                                    );
                                } else {
                                    println!(
                                        "[DEBUG] Move-only SetWindowPos succeeded for hwnd=0x{:X}",
                                        hwnd
                                    );
                                }
                                // Send window to backmost
                                SetWindowPos(
                                    hwnd as HWND,
                                    HWND_BOTTOM,
                                    0,
                                    0,
                                    0,
                                    0,
                                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                                );
                                // Mark as failed
                                failed_animations.push(hwnd);
                            }
                        }
                    }

                    if let Some(prev_rect) = WindowTracker::get_window_rect(hwnd) {
                        if prev_rect.left != current_rect.left
                            || prev_rect.top != current_rect.top
                            || prev_rect.right != current_rect.right
                            || prev_rect.bottom != current_rect.bottom
                        {
                            if !failed_animations.contains(&hwnd) {
                                failed_animations.push(hwnd);
                            }
                            // println!(
                            //     "[DEBUG] Window 0x{:X} moved: prev=({}, {}, {}, {}), curr=({}, {}, {}, {}), size=({}x{}), requested=({}x{})",
                            //     hwnd,
                            //     prev_rect.left, prev_rect.top, prev_rect.right, prev_rect.bottom,
                            //     current_rect.left, current_rect.top, current_rect.right, current_rect.bottom,
                            //     current_rect.right - current_rect.left,
                            //     current_rect.bottom - current_rect.top,
                            //     animation_entry.target_rect.right - animation_entry.target_rect.left,
                            //     animation_entry.target_rect.bottom - animation_entry.target_rect.top
                            // );
                        }
                    }
                }
            }
        }

        // Remove completed animations
        for hwnd in &completed_animations {
            self.active_animations.remove(hwnd);
            println!("🎬 Animation completed for window {:?}", hwnd);
        }

        (completed_animations, failed_animations)
    }

    pub fn apply_grid_layout(
        &mut self,
        layout: &GridLayout,
        duration: Duration,
        easing: EasingType,
    ) -> Result<usize, String> {
        let mut animations_started = 0;

        // Apply virtual grid layout
        for row in 0..layout.config.rows {
            for col in 0..layout.config.cols {
                if let Some(target_hwnd) = layout.virtual_grid[row][col] {
                    // Calculate target position from grid coordinates
                    if let Some(target_rect) = self.virtual_cell_to_window_rect(row, col) {
                        if self.windows.contains_key(&target_hwnd) {
                            match self.start_window_animation(
                                target_hwnd,
                                target_rect,
                                duration,
                                easing,
                            ) {
                                Ok(_) => animations_started += 1,
                                Err(e) => println!(
                                    "⚠️ Failed to start animation for window {:?}: {}",
                                    target_hwnd, e
                                ),
                            }
                        }
                    }
                }
            }
        }

        println!(
            "🎬 Started {} animations for grid layout '{}'",
            animations_started, layout.name
        );
        Ok(animations_started)
    }

    pub fn save_current_layout(&mut self, name: String) {
        // let layout = GridLayout::from_current_state(self, name.clone());
        // self.saved_layouts.insert(name.clone(), layout);
        println!("💾 Saved current grid layout as '{}'", name);
    }

    pub fn get_saved_layout(&self, name: &str) -> Option<GridLayout> {
        self.saved_layouts
            .get(name)
            .map(|layout_ref| layout_ref.clone())
    }

    pub fn list_saved_layouts(&self) -> Vec<String> {
        self.saved_layouts
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    fn virtual_cell_to_window_rect(&self, row: usize, col: usize) -> Option<RECT> {
        if row >= self.config.rows || col >= self.config.cols {
            return None;
        }

        let grid_width = self.monitor_rect.right - self.monitor_rect.left;
        let grid_height = self.monitor_rect.bottom - self.monitor_rect.top;

        let cell_width = grid_width / self.config.cols as i32;
        let cell_height = grid_height / self.config.rows as i32;

        let left = self.monitor_rect.left + (col as i32 * cell_width);
        let top = self.monitor_rect.top + (row as i32 * cell_height);
        let right = left + cell_width;
        let bottom = top + cell_height;

        Some(RECT {
            left,
            top,
            right,
            bottom,
        })
    }

    /// Move a window to a specific grid cell
    pub fn move_window_to_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), String> {
        if target_row >= self.config.rows || target_col >= self.config.cols {
            return Err(format!(
                "Invalid grid coordinates: ({}, {})",
                target_row, target_col
            ));
        }

        // Validate window handle first
        unsafe {
            if IsWindow(hwnd as HWND) == 0 {
                return Err(format!("Invalid window handle: {:?}", hwnd));
            }
        }

        // Check if window is manageable
        if !Self::is_manageable_window(hwnd) {
            return Err(format!("Window {:?} is not manageable", hwnd));
        }

        // Debug: Show all monitors before positioning
        self.list_all_monitors();

        // Get target rectangle on primary monitor instead of virtual grid
        if let Some(target_rect) = self.primary_monitor_cell_to_rect(target_row, target_col) {
            let primary_rect = self.get_primary_monitor_rect();
            println!(
                "🖥️  Using primary monitor: left={}, top={}, right={}, bottom={}",
                primary_rect.left, primary_rect.top, primary_rect.right, primary_rect.bottom
            );
            println!(
                "🖥️  Primary monitor dimensions: {}x{}",
                primary_rect.right - primary_rect.left,
                primary_rect.bottom - primary_rect.top
            );
            println!(
                "🎯 Target cell ({}, {}) of grid ({}x{})",
                target_row, target_col, self.config.rows, self.config.cols
            );
            println!(
                "🎯 Calculated target rect: left={}, top={}, width={}, height={}",
                target_rect.left,
                target_rect.top,
                target_rect.right - target_rect.left,
                target_rect.bottom - target_rect.top
            );

            // Validate that the target rectangle is within primary monitor bounds
            if target_rect.left < primary_rect.left
                || target_rect.top < primary_rect.top
                || target_rect.right > primary_rect.right
                || target_rect.bottom > primary_rect.bottom
            {
                println!("⚠️  WARNING: Target rectangle is outside primary monitor bounds!");
                println!(
                    "   Target: ({}, {}) to ({}, {})",
                    target_rect.left, target_rect.top, target_rect.right, target_rect.bottom
                );
                println!(
                    "   Monitor: ({}, {}) to ({}, {})",
                    primary_rect.left, primary_rect.top, primary_rect.right, primary_rect.bottom
                );
            } else {
                println!("✅ Target rectangle is within primary monitor bounds");
            }

            println!(
                "🎯 Moving window {:?} to rect: left={}, top={}, width={}, height={}",
                hwnd,
                target_rect.left,
                target_rect.top,
                target_rect.right - target_rect.left,
                target_rect.bottom - target_rect.top
            );

            // Safely move the window (this operation doesn't need the tracker lock)
            unsafe {
                let result = SetWindowPos(
                    hwnd as HWND,
                    ptr::null_mut(),
                    target_rect.left,
                    target_rect.top,
                    target_rect.right - target_rect.left,
                    target_rect.bottom - target_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );

                if result == 0 {
                    let error = GetLastError();
                    return Err(format!("SetWindowPos failed with error: {}", error));
                }
            }

            println!("✅ Successfully moved window {:?}", hwnd);

            // Update the grid tracking (keep this lock brief)
            self.assign_window_to_virtual_cell(hwnd, target_row, target_col)?;

            // Trigger callback notification if the window is tracked
            if let Some(window_info) = self.windows.get(&hwnd) {
                // Use a reference instead of cloning to avoid potential issues
                self.trigger_window_moved(hwnd, &*window_info);
            }
        } else {
            return Err(format!(
                "Could not calculate target rectangle for cell ({}, {})",
                target_row, target_col
            ));
        }

        Ok(())
    }

    /// Assign a window to a virtual grid cell (tracking only, no movement)
    pub fn assign_window_to_virtual_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), String> {
        if target_row >= self.config.rows || target_col >= self.config.cols {
            return Err(format!(
                "Invalid grid coordinates: ({}, {})",
                target_row, target_col
            ));
        }

        // Clear the old position
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if let CellState::Occupied(existing_hwnd) = self.grid[row][col] {
                    if existing_hwnd == hwnd {
                        self.grid[row][col] = CellState::Empty;
                    }
                }
            }
        }

        // Set the new position
        self.grid[target_row][target_col] = CellState::Occupied(hwnd);

        // Update the window info if it exists
        // No grid_cells field in WindowInfo, so nothing to update here.

        Ok(())
    }

    /// Assign a window to a monitor-specific grid cell
    pub fn assign_window_to_monitor_cell(
        &mut self,
        hwnd: u64,
        monitor_id: usize,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), String> {
        if monitor_id >= self.monitor_grids.len() {
            return Err(format!("Invalid monitor ID: {}", monitor_id));
        }

        if target_row >= self.config.rows || target_col >= self.config.cols {
            return Err(format!(
                "Invalid monitor grid coordinates: ({}, {}) for monitor {}",
                target_row, target_col, monitor_id
            ));
        }

        // Clear the old position in all monitor grids
        for grid in &mut self.monitor_grids {
            for row in 0..grid.config.rows {
                for col in 0..grid.config.cols {
                    if let CellState::Occupied(existing_hwnd) = grid.grid[row][col] {
                        if existing_hwnd == hwnd {
                            grid.grid[row][col] = CellState::Empty;
                        }
                    }
                }
            }
        }

        // Set the new position
        self.monitor_grids[monitor_id].grid[target_row][target_col] = CellState::Occupied(hwnd);

        // Update the window info if it exists
        // if let Some(mut window_entry) = self.windows.get_mut(&hwnd) {
        //     let mut arr = [(0, 0); 8];
        //     arr[0] = (target_row, target_col);
        //     if monitor_id < window_entry.monitor_cells.len() {
        //         window_entry.monitor_cells[monitor_id] = arr;
        //     }
        // }

        Ok(())
    }

    /// Get monitor information by monitor ID for debugging
    pub fn get_monitor_info_by_id(&self, monitor_id: usize) -> Option<(i32, i32, i32, i32)> {
        if monitor_id < self.monitor_grids.len() {
            let monitor = &self.monitor_grids[monitor_id];
            Some((
                monitor.monitor_rect.left,
                monitor.monitor_rect.top,
                monitor.monitor_rect.right,
                monitor.monitor_rect.bottom,
            ))
        } else {
            None
        }
    }

    /// List all monitor configurations for debugging
    pub fn list_all_monitors(&self) {
        println!("🖥️  All Monitor Configurations:");
        for (id, monitor) in self.monitor_grids.iter().enumerate() {
            println!(
                "   Monitor {}: ({}, {}) to ({}, {}) - Size: {}x{}",
                id,
                monitor.monitor_rect.left,
                monitor.monitor_rect.top,
                monitor.monitor_rect.right,
                monitor.monitor_rect.bottom,
                monitor.monitor_rect.right - monitor.monitor_rect.left,
                monitor.monitor_rect.bottom - monitor.monitor_rect.top
            );
        }
    }

    /// Returns the process ID for the given window handle, or None if not found.
    pub fn get_window_process_id(hwnd: u64) -> Option<u32> {
        let mut process_id: u32 = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd as HWND, &mut process_id);
        }
        if process_id != 0 {
            Some(process_id)
        } else {
            None
        }
    }

    /// Returns the class name for the given window handle, or an empty string if not found.
    pub fn get_window_class_name(hwnd: u64) -> String {
        let mut class_name_buf = [0u16; 256];
        let class_name_len = unsafe {
            GetClassNameW(
                hwnd as HWND,
                class_name_buf.as_mut_ptr(),
                class_name_buf.len() as i32,
            )
        };
        if class_name_len > 0 {
            String::from_utf16_lossy(&class_name_buf[..class_name_len as usize])
        } else {
            String::new()
        }
    }

    /// Returns true if the window is visible.
    pub fn is_window_visible(hwnd: u64) -> bool {
        unsafe { IsWindowVisible(hwnd as HWND) != 0 }
    }

    /// Returns true if the window is minimized (iconic).
    pub fn is_window_minimized(hwnd: u64) -> bool {
        unsafe { IsIconic(hwnd as HWND) != 0 }
    }

    /// Returns true if the window is maximized.
    pub fn is_window_maximized(hwnd: u64) -> bool {
        unsafe {
            if IsWindow(hwnd as HWND) == 0 {
                return false;
            }
            // Check window placement
            let mut placement = std::mem::zeroed::<WINDOWPLACEMENT>();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
            if GetWindowPlacement(hwnd as HWND, &mut placement) != 0 {
                if placement.showCmd == SW_MAXIMIZE as u32 {
                    return true;
                }
            }
            // Fallback to IsZoomed and WS_MAXIMIZE style
            let is_zoomed = IsZoomed(hwnd as HWND) != 0;
            let style = GetWindowLongW(hwnd as HWND, GWL_STYLE) as u32;
            let is_maximized_style = (style & WS_MAXIMIZE) != 0;
            if is_zoomed || is_maximized_style {
                return true;
            }
            // Check if window rect matches any monitor rect
            if let Some(rect) = Self::get_window_rect(hwnd) {
                // Get all monitor bounds
                let monitor_rects = Self::get_actual_monitor_bounds_static();
                for monitor_rect in monitor_rects {
                    if rect.left == monitor_rect.left
                        && rect.top == monitor_rect.top
                        && rect.right == monitor_rect.right
                        && rect.bottom == monitor_rect.bottom
                    {
                        return true;
                    }
                }
            }
            false
        }
    }

    // Helper for static monitor bounds (for use in static fn)
    fn get_actual_monitor_bounds_static() -> Vec<RECT> {
        let mut monitors = Vec::new();
        unsafe {
            extern "system" fn monitor_enum_proc(
                _hmonitor: winapi::shared::windef::HMONITOR,
                _hdc: winapi::shared::windef::HDC,
                rect: *mut RECT,
                data: LPARAM,
            ) -> i32 {
                let monitors = unsafe { &mut *(data as *mut Vec<RECT>) };
                monitors.push(unsafe { *rect });
                1
            }
            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(monitor_enum_proc),
                &mut monitors as *mut Vec<RECT> as LPARAM,
            );
        }
        monitors
    }

    pub fn set_grid_size(&mut self, rows: usize, cols: usize) {
        self.config.rows = rows;
        self.config.cols = cols;
        self.grid = vec![vec![CellState::Empty; cols]; rows];
        self.initialize_grid();
        self.update_grid();
        self.initialize_monitor_grids();
        self.update_monitor_grids();
    }

    /// Add window creation/destruction detection
    pub fn detect_window_lifecycle_events(&mut self) -> Vec<crate::ipc_protocol::GridEvent> {
        let mut events = Vec::new();

        // Get current windows from system
        let current_windows = self.enumerate_windows();

        // Detect NEW windows (create events)
        for entry in current_windows.iter() {
            let hwnd = entry.key();
            let window_info = entry.value();
            if !self.windows.contains_key(hwnd) {
                // This is a new window!
                let create_event = crate::ipc_protocol::GridEvent::WindowCreated {
                    hwnd: *hwnd,
                    title: String::from_utf16_lossy(&window_info.title),
                    row: 0, // Will be calculated properly
                    col: 0,
                    grid_top_left_row: 0,
                    grid_top_left_col: 0,
                    grid_bottom_right_row: 0,
                    grid_bottom_right_col: 0,
                    real_x: window_info.window_rect.left,
                    real_y: window_info.window_rect.top,
                    real_width: (window_info.window_rect.right - window_info.window_rect.left)
                        as u32,
                    real_height: (window_info.window_rect.bottom - window_info.window_rect.top)
                        as u32,
                    monitor_id: 0, // Will be determined
                };
                events.push(create_event);

                println!(
                    "✨ [LIFECYCLE] Window CREATED: HWND 0x{:X} - '{}'",
                    hwnd,
                    String::from_utf16_lossy(&window_info.title)
                        .chars()
                        .take(50)
                        .collect::<String>()
                );
            }
        }

        // Detect DESTROYED windows (destroy events)
        let mut destroyed_windows = Vec::new();
        for entry in self.windows.iter() {
            let hwnd = entry.key();
            if !current_windows.contains_key(hwnd) {
                // This window was destroyed!
                let title = if let Some(window_info) = self.windows.get(hwnd) {
                    String::from_utf16_lossy(&window_info.title)
                } else {
                    "(Unknown)".to_string()
                };

                let destroy_event = crate::ipc_protocol::GridEvent::WindowDestroyed {
                    hwnd: *hwnd,
                    title: title.clone(),
                };
                events.push(destroy_event);
                destroyed_windows.push(*hwnd);

                println!(
                    "💀 [LIFECYCLE] Window DESTROYED: HWND 0x{:X} - '{}'",
                    hwnd,
                    title.chars().take(50).collect::<String>()
                );
            }
        }

        // Remove destroyed windows from our tracking
        for hwnd in destroyed_windows {
            self.windows.remove(&hwnd);
        }

        // Add new windows to our tracking
        for entry in current_windows.iter() {
            let hwnd = entry.key();
            if !self.windows.contains_key(hwnd) {
                self.windows.insert(*hwnd, entry.value().clone());
            }
        }

        events
    }

    /// Call this periodically to detect window lifecycle changes
    pub fn update_window_lifecycle(&mut self) -> Vec<crate::ipc_protocol::GridEvent> {
        self.detect_window_lifecycle_events()
    }

    /// Enumerate all windows currently visible to the system and return a DashMap of them
    pub fn enumerate_windows(&mut self) -> DashMap<u64, WindowInfo> {
        let found_windows = DashMap::new();

        // Callback for EnumWindows to collect windows
        unsafe extern "system" fn enum_windows_proc_collect(hwnd: HWND, lparam: LPARAM) -> i32 {
            let found_windows = &*(lparam as *const DashMap<u64, WindowInfo>);
            // println!("Checking window: {:?}", hwnd);
            if WindowTracker::is_manageable_window(hwnd as u64) {
                // println!("Found manageable window: {:?}", hwnd);
                if let Some(rect) = WindowTracker::get_window_rect(hwnd as u64) {
                // println!(
                //     "[ENUM] Window HWND=0x{:X} rect: left={}, top={}, right={}, bottom={} (w={}, h={})", 
                //     hwnd as u64, rect.left, rect.top, rect.right, rect.bottom, rect.right-rect.left, rect.bottom-rect.top
                // );
                    let mut title_buf = [0u16; 256];
                    let title_len =
                        GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
                    let mut class_name_buf = [0u16; 256];
                    let class_name_len = GetClassNameW(
                        hwnd,
                        class_name_buf.as_mut_ptr(),
                        class_name_buf.len() as i32,
                    );
                    let is_visible = IsWindowVisible(hwnd) != 0;
                    let is_minimized = IsIconic(hwnd) != 0;
                    let is_maximized = WindowTracker::is_window_maximized(hwnd as u64);
                    let mut process_id: u32 = 0;
                    GetWindowThreadProcessId(hwnd, &mut process_id);

                    let window_info = WindowInfo {
                        hwnd: hwnd as u64,
                        title: title_buf,
                        title_len: title_len as u32,
                        monitor_ids: [0usize; 8],
                        z_order: 0,
                        window_rect: RectWrapper::from_rect(rect),
                        is_visible,
                        is_minimized,
                        is_maximized,
                        process_id,
                        class_name: class_name_buf,
                        class_name_len: class_name_len as u32,
                    };
                    found_windows.insert(hwnd as u64, window_info);
                }
            }
            1
        }

        unsafe {
            EnumWindows(
                Some(enum_windows_proc_collect),
                &found_windows as *const _ as LPARAM,
            );
        }

        found_windows
    }
}
