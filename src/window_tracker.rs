use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use ringbuf::wrap::{Cons, Prod};
use ringbuf::{traits::*, HeapRb};
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::*;

use crate::grid::layout::GridLayout;
use crate::grid::GridConfig;
use crate::monitor_grid::MonitorGrid;
use crate::window::info::WindowInfo;
use crate::window::WindowAnimation;
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
}

impl WindowTracker {
    pub fn new() -> Self {
        let mut ret = Self::new_with_config(GridConfig::default());
        ret.find_desktop_hwnds();
        ret
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
            let title = Self::get_window_title(hwnd);

            if IsWindow(hwnd as HWND) == 0 || IsWindowVisible(hwnd as HWND) == 0 {
                return false;
            }

            // Skip minimized windows
            if IsIconic(hwnd as HWND) != 0 {
                return false;
            }

            let ex_style = GetWindowLongW(hwnd as HWND, GWL_EXSTYLE) as u32;
            if (ex_style & WS_EX_TOOLWINDOW) != 0 {
                return false;
            }

            true
        }
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates (like minimized windows)
        if rect.left < -30000
            || rect.top < -30000
            || rect.right < rect.left
            || rect.bottom < rect.top
        {
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
                        cells.push((row, col));
                    }
                }
            }
        }

        cells
    }

    pub fn update_grid(&mut self) {
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
            if (*hwnd & 0xFF) == 0x2E {
                println!(
                    "[DEBUG] HWND ending in 2E: hwnd=0x{:X}, title='{}', class='{}'",
                    hwnd, window_info.title, window_info.class_name
                );
            }
            // Skip desktop windows for occupancy
            if self.is_desktop_hwnd(*hwnd) {
                continue;
            }
            for (row, col) in &window_info.grid_cells {
                if *row < self.config.rows && *col < self.config.cols {
                    self.grid[*row][*col] = CellState::Occupied(*hwnd);
                }
            }
        }
    }

    /// Get the primary monitor rectangle for window positioning
    fn get_primary_monitor_rect(&self) -> RECT {
        if !self.monitor_grids.is_empty() {
            // Find the monitor at (0,0) - this is the true primary monitor
            for monitor_grid in &self.monitor_grids {
                if monitor_grid.monitor_rect.0 == 0 && monitor_grid.monitor_rect.1 == 0 {
                    let rect = RECT {
                        left: monitor_grid.monitor_rect.0,
                        top: monitor_grid.monitor_rect.1,
                        right: monitor_grid.monitor_rect.2,
                        bottom: monitor_grid.monitor_rect.3,
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
                left: primary_monitor.monitor_rect.0,
                top: primary_monitor.monitor_rect.1,
                right: primary_monitor.monitor_rect.2,
                bottom: primary_monitor.monitor_rect.3,
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
    fn primary_monitor_cell_to_rect(&self, row: usize, col: usize) -> Option<RECT> {
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
            return false;
        }
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let title = Self::get_window_title(hwnd);
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);

            // Get additional required fields for WindowInfo
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

            let window_info = WindowInfo {
                hwnd,
                title,
                grid_cells,
                monitor_cells,
                rect,
                is_visible,
                is_minimized,
                process_id,
                class_name,
            };

            self.windows.insert(hwnd, window_info.clone());
            self.update_grid();
            self.update_monitor_grids();

            // Trigger callback
            self.trigger_window_created(hwnd, &window_info);

            return true;
        }
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
                window_entry.rect = rect;
                window_entry.grid_cells = grid_cells;
                window_entry.monitor_cells = monitor_cells;
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

    pub fn print_grid(&self) {
        println!();
        println!("{}", "=".repeat(60));
        println!(
            "Window Grid Tracker - {}x{} Grid ({} windows)",
            self.config.rows,
            self.config.cols,
            self.windows.len()
        );
        println!(
            "Monitor: {}x{} px",
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
                                if window_info.grid_cells.contains(&(row, col)) {
                                    let z_map = crate::util::get_hwnd_z_order_map();
                                    if let Some(&z) = z_map.get(hwnd) {
                                        if topmost_z.map_or(true, |tz| z < tz) {
                                            topmost_hwnd = Some(*hwnd);
                                            topmost_z = Some(z);
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
        self.print_grid();
        let z_map = crate::util::get_hwnd_z_order_map();
        // Print individual monitor grids
        for (index, monitor_grid) in self.monitor_grids.iter().enumerate() {
            println!();
            println!("=== MONITOR {} GRID ===", index + 1);
            println!(
                "Monitor bounds: ({}, {}) to ({}, {})",
                monitor_grid.monitor_rect.0,
                monitor_grid.monitor_rect.1,
                monitor_grid.monitor_rect.2,
                monitor_grid.monitor_rect.3
            );

            // Count windows on this monitor
            let mut windows_on_monitor = 0;
            for entry in &self.windows {
                let (_, window_info) = entry.pair();
                if window_info
                    .monitor_cells
                    .contains_key(&monitor_grid.monitor_id)
                {
                    windows_on_monitor += 1;
                }
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
                monitor_grid.monitor_rect.2 - monitor_grid.monitor_rect.0,
                monitor_grid.monitor_rect.3 - monitor_grid.monitor_rect.1
            );
            // Print column headers
            print!("   ");
            for col in 0..monitor_grid.config.cols {
                print!("{:2} ", col);
            }
            println!();

            // Print grid rows
            for row in 0..monitor_grid.config.rows {
                print!("{:2} ", row);
                for col in 0..monitor_grid.config.cols {
                    match monitor_grid.grid[row][col] {
                        CellState::Occupied(_hwnd) => {
                            // Print the last two hex digits of the hwnd for visual distinction
                            // Find all windows occupying this cell, pick the topmost by Z-order
                            let mut topmost_hwnd: Option<u64> = None;
                            let mut topmost_z: Option<usize> = None;
                            for entry in &self.windows {
                                let (hwnd, window_info) = entry.pair();
                                if let Some(cells) =
                                    window_info.monitor_cells.get(&monitor_grid.monitor_id)
                                {
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
        }
        println!();
    }

    pub fn scan_existing_windows(&mut self) {
        println!("Starting window enumeration...");

        // Initialize grid with off-screen areas marked
        self.initialize_grid();

        self.enum_counter.store(0, Ordering::SeqCst); // Reset counter
        unsafe {
            let result = EnumWindows(Some(enum_windows_proc), self as *mut _ as LPARAM);
            println!("EnumWindows completed with result: {}", result);
        }
        println!(
            "Window enumeration finished. Found {} windows.",
            self.windows.len()
        );
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
        for monitor_grid in &mut self.monitor_grids {
            monitor_grid.update_grid(&self.windows);
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
        if let Some(current_rect) = Self::get_window_rect(hwnd) {
            let animation = WindowAnimation::new(hwnd, current_rect, target_rect, duration, easing);
            self.active_animations.insert(hwnd, animation);
            println!(
                "🎬 Started animation for window {:?}: {} -> {} over {:?}",
                hwnd,
                format!(
                    "({},{},{},{})",
                    current_rect.left, current_rect.top, current_rect.right, current_rect.bottom
                ),
                format!(
                    "({},{},{},{})",
                    target_rect.left, target_rect.top, target_rect.right, target_rect.bottom
                ),
                duration
            );
            Ok(())
        } else {
            Err(format!("Failed to get current rect for window {:?}", hwnd))
        }
    }

    pub fn update_animations(&mut self) -> Vec<u64> {
        let mut completed_animations = Vec::new();

        // Collect keys that need to be processed
        let animation_keys: Vec<u64> = self
            .active_animations
            .iter()
            .map(|entry| *entry.key())
            .collect();

        for hwnd in animation_keys {
            if let Some(mut animation_entry) = self.active_animations.get_mut(&hwnd) {
                if animation_entry.is_completed() {
                    completed_animations.push(hwnd);
                } else {
                    let current_rect = animation_entry.get_current_rect();
                    // Move window to current animation position
                    unsafe {
                        use winapi::um::winuser::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
                        SetWindowPos(
                            hwnd as HWND,
                            std::ptr::null_mut(),
                            current_rect.left,
                            current_rect.top,
                            current_rect.right - current_rect.left,
                            current_rect.bottom - current_rect.top,
                            SWP_NOZORDER | SWP_NOACTIVATE,
                        );
                    }
                }
            }
        }

        // Remove completed animations
        for hwnd in &completed_animations {
            self.active_animations.remove(hwnd);
            println!("🎬 Animation completed for window {:?}", hwnd);
        }

        completed_animations
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
        if let Some(mut window_entry) = self.windows.get_mut(&hwnd) {
            window_entry.grid_cells.clear();
            window_entry.grid_cells.push((target_row, target_col));
        }

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
        if let Some(mut window_entry) = self.windows.get_mut(&hwnd) {
            let cells = vec![(target_row, target_col)];
            window_entry.monitor_cells.insert(monitor_id, cells);
        }

        Ok(())
    }

    /// Get monitor information by monitor ID for debugging
    pub fn get_monitor_info_by_id(&self, monitor_id: usize) -> Option<(i32, i32, i32, i32)> {
        if monitor_id < self.monitor_grids.len() {
            let monitor = &self.monitor_grids[monitor_id];
            Some((
                monitor.monitor_rect.0,
                monitor.monitor_rect.1,
                monitor.monitor_rect.2,
                monitor.monitor_rect.3,
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
                monitor.monitor_rect.0,
                monitor.monitor_rect.1,
                monitor.monitor_rect.2,
                monitor.monitor_rect.3,
                monitor.monitor_rect.2 - monitor.monitor_rect.0,
                monitor.monitor_rect.3 - monitor.monitor_rect.1
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

    pub fn set_grid_size(&mut self, rows: usize, cols: usize) {
        self.config.rows = rows;
        self.config.cols = cols;
        self.grid = vec![vec![CellState::Empty; cols]; rows];
        self.initialize_grid();
        self.update_grid();
        self.initialize_monitor_grids();
        self.update_monitor_grids();
    }
}
