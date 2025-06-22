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

// Import our error handling module
pub mod grid_client_errors;
pub use grid_client_errors::{
    retry_with_backoff, safe_arc_lock, safe_lock, validate_grid_coordinates, GridClientError,
    GridClientResult, RetryConfig,
};

// Import the centralized grid display module
pub mod grid_display;

pub mod config;
pub mod display;
pub mod grid;
pub mod grid_client_config;
pub mod monitor;
pub mod performance_monitor;
pub mod window;
pub use crate::grid_client_config::GridClientConfig;
pub use crate::performance_monitor::{EventType, OperationTimer, PerformanceMonitor};
pub use grid::animation::EasingType;

// Import the heartbeat service module
pub mod heartbeat;
pub use heartbeat::HeartbeatService;

// Import window events module with unified hook management
pub mod window_events;
pub use window_events::{cleanup_hooks, setup_window_events, WindowEventConfig};

// Coverage threshold: percentage of cell area that must be covered by window
// to consider the window as occupying that cell (0.0 to 1.0)
const COVERAGE_THRESHOLD: f32 = 0.3; // 30% coverage required

// Dynamic grid configuration
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct GridConfig {
    pub rows: usize,
    pub cols: usize,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            rows: 8, // Default grid size
            cols: 12,
        }
    }
}

impl GridConfig {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }

    pub fn cell_count(&self) -> usize {
        self.rows * self.cols
    }
}

// Animation and Tweening System
// #[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
// pub enum EasingType {
//     Linear,        // Constant speed
//     EaseIn,        // Slow start, fast end
//     EaseOut,       // Fast start, slow end
//     EaseInOut,     // Slow start and end, fast middle
//     Bounce,        // Bouncing effect at the end
//     Elastic,       // Elastic/spring effect
//     Back,          // Slight overshoot then settle
// }

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

    pub fn get_current_rect(&self) -> RECT {
        if self.completed {
            return self.target_rect;
        }

        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            return self.target_rect;
        }

        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased_progress = self.apply_easing(progress);

        RECT {
            left: self.lerp(self.start_rect.left, self.target_rect.left, eased_progress),
            top: self.lerp(self.start_rect.top, self.target_rect.top, eased_progress),
            right: self.lerp(
                self.start_rect.right,
                self.target_rect.right,
                eased_progress,
            ),
            bottom: self.lerp(
                self.start_rect.bottom,
                self.target_rect.bottom,
                eased_progress,
            ),
        }
    }

    pub fn is_completed(&self) -> bool {
        self.completed || self.start_time.elapsed() >= self.duration
    }

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

    fn lerp(&self, start: i32, end: i32, t: f32) -> i32 {
        (start as f32 + (end - start) as f32 * t) as i32
    }

    pub fn apply_easing(&self, t: f32) -> f32 {
        match self.easing {
            EasingType::Linear => t,
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
        }
    }
}

// Grid Layout for transferring complete grid states
#[derive(Clone, Debug)]
pub struct GridLayout {
    pub name: String,
    pub config: GridConfig,
    pub virtual_grid: Vec<Vec<Option<HWND>>>,
    pub monitor_grids: Vec<MonitorGridLayout>,
    pub created_at: Instant,
}

#[derive(Clone, Debug)]
pub struct MonitorGridLayout {
    pub monitor_id: usize,
    pub config: GridConfig,
    pub grid: Vec<Vec<Option<HWND>>>,
}

impl GridLayout {
    pub fn new(name: String) -> Self {
        Self::new_with_config(name, GridConfig::default())
    }

    pub fn new_with_config(name: String, config: GridConfig) -> Self {
        let virtual_grid = vec![vec![None; config.cols]; config.rows];
        Self {
            name,
            config,
            virtual_grid,
            monitor_grids: Vec::new(),
            created_at: Instant::now(),
        }
    }

    pub fn from_current_state(tracker: &WindowTracker, name: String) -> Self {
        let mut layout = Self::new_with_config(name, tracker.config.clone());

        // Extract virtual grid layout
        for row in 0..tracker.config.rows {
            for col in 0..tracker.config.cols {
                if let CellState::Occupied(hwnd) = tracker.grid[row][col] {
                    layout.virtual_grid[row][col] = Some(hwnd);
                }
            }
        }

        // Extract monitor grid layouts
        for monitor_grid in &tracker.monitor_grids {
            let mut monitor_layout = MonitorGridLayout {
                monitor_id: monitor_grid.monitor_id,
                config: monitor_grid.config.clone(),
                grid: vec![vec![None; monitor_grid.config.cols]; monitor_grid.config.rows],
            };

            for row in 0..monitor_grid.config.rows {
                for col in 0..monitor_grid.config.cols {
                    if let CellState::Occupied(hwnd) = monitor_grid.grid[row][col] {
                        monitor_layout.grid[row][col] = Some(hwnd);
                    }
                }
            }

            layout.monitor_grids.push(monitor_layout);
        }

        layout
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellState {
    Empty,          // No window (on-screen area)
    Occupied(HWND), // Window present
    OffScreen,      // Off-screen area (outside actual monitor bounds)
}

#[derive(Clone, Debug)]
pub struct MonitorGrid {
    pub monitor_id: usize,
    pub monitor_rect: (i32, i32, i32, i32), // (left, top, right, bottom)
    pub config: GridConfig,
    pub grid: Vec<Vec<CellState>>,
}

impl MonitorGrid {
    pub fn new(monitor_id: usize, monitor_rect: RECT) -> Self {
        Self::new_with_config(monitor_id, monitor_rect, GridConfig::default())
    }

    pub fn new_with_config(monitor_id: usize, monitor_rect: RECT, config: GridConfig) -> Self {
        let grid = vec![vec![CellState::Empty; config.cols]; config.rows];
        Self {
            monitor_id,
            monitor_rect: (
                monitor_rect.left,
                monitor_rect.top,
                monitor_rect.right,
                monitor_rect.bottom,
            ),
            config,
            grid,
        }
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates
        if rect.left < -30000
            || rect.top < -30000
            || rect.right < rect.left
            || rect.bottom < rect.top
        {
            return cells;
        }

        let (left, top, right, bottom) = self.monitor_rect;

        // Check if window intersects with this monitor
        if rect.right <= left || rect.left >= right || rect.bottom <= top || rect.top >= bottom {
            return cells; // Window is not on this monitor
        }

        let cell_width = (right - left) / self.config.cols as i32;
        let cell_height = (bottom - top) / self.config.rows as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return cells;
        }

        // Calculate potential range of cells that might be affected
        let start_col = ((rect.left.max(left) - left) / cell_width).max(0) as usize;
        let end_col =
            ((rect.right.min(right) - left) / cell_width).min(self.config.cols as i32 - 1) as usize;
        let start_row = ((rect.top.max(top) - top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom.min(bottom) - top) / cell_height)
            .min(self.config.rows as i32 - 1) as usize;

        // Check coverage for each potentially affected cell
        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < self.config.rows && col < self.config.cols {
                    // Calculate the exact bounds of this grid cell
                    let cell_rect = RECT {
                        left: left + (col as i32 * cell_width),
                        top: top + (row as i32 * cell_height),
                        right: left + ((col + 1) as i32 * cell_width),
                        bottom: top + ((row + 1) as i32 * cell_height),
                    };

                    // Only include cell if window meets coverage threshold
                    if meets_coverage_threshold(rect, &cell_rect) {
                        cells.push((row, col));
                    }
                }
            }
        }

        cells
    }

    pub fn update_grid(&mut self, windows: &DashMap<HWND, WindowInfo>) {
        // Reset grid to empty
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                self.grid[row][col] = CellState::Empty;
            }
        }

        // Place windows on the grid
        for entry in windows {
            let (hwnd, window_info) = entry.pair();
            let grid_cells = self.window_to_grid_cells(&window_info.rect);
            for (row, col) in grid_cells {
                if row < self.config.rows && col < self.config.cols {
                    self.grid[row][col] = CellState::Occupied(*hwnd);
                }
            }
        }
    }

    pub fn print_grid(&self) {
        // Column headers
        print!("    ");
        for col in 0..self.config.cols {
            print!(" {:2}", col);
        }
        println!();

        for row in 0..self.config.rows {
            print!("{:2}: ", row);
            for col in 0..self.config.cols {
                match self.grid[row][col] {
                    CellState::Empty => print!(" . "),
                    CellState::Occupied(hwnd) => {
                        // Show last 2 digits of HWND for compactness
                        let hwnd_u64 = hwnd as u64;
                        let display_val = (hwnd_u64 % 100) as u8;
                        print!("{:2X} ", display_val);
                    }
                    CellState::OffScreen => print!(" - "),
                }
            }
            println!();
        }
    }
}

#[derive(Clone)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub rect: RECT,
    pub grid_cells: Vec<(usize, usize)>, // For virtual grid
    pub monitor_cells: HashMap<usize, Vec<(usize, usize)>>, // For individual monitor grids
}

pub struct WindowTracker {
    pub windows: DashMap<HWND, WindowInfo>, // Lock-free concurrent HashMap
    pub monitor_rect: RECT,                 // Virtual screen rect
    pub config: GridConfig,                 // Dynamic grid configuration
    pub grid: Vec<Vec<CellState>>,          // Virtual grid (dynamic)
    pub monitor_grids: Vec<MonitorGrid>,    // Individual monitor grids
    pub enum_counter: AtomicUsize,          // Atomic counter for lock-free access
    pub active_animations: DashMap<HWND, WindowAnimation>, // Lock-free animations
    pub saved_layouts: DashMap<String, GridLayout>, // Lock-free layouts
    pub event_callbacks: Vec<WindowEventCallbackBox>, // Event callbacks
}

impl WindowTracker {
    pub fn new() -> Self {
        Self::new_with_config(GridConfig::default())
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
        };

        // Initialize individual monitor grids
        tracker.initialize_monitor_grids();
        tracker
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
    fn trigger_window_created(&self, hwnd: HWND, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_created(hwnd, window_info);
        }
    }

    fn trigger_window_destroyed(&self, hwnd: HWND) {
        for callback in &self.event_callbacks {
            callback.on_window_destroyed(hwnd);
        }
    }

    fn trigger_window_moved(&self, hwnd: HWND, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_moved(hwnd, window_info);
        }
    }

    fn trigger_window_activated(&self, hwnd: HWND, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_activated(hwnd, window_info);
        }
    }

    fn trigger_window_minimized(&self, hwnd: HWND) {
        for callback in &self.event_callbacks {
            callback.on_window_minimized(hwnd);
        }
    }

    fn trigger_window_restored(&self, hwnd: HWND, window_info: &WindowInfo) {
        for callback in &self.event_callbacks {
            callback.on_window_restored(hwnd, window_info);
        }
    }

    pub fn get_window_title(hwnd: HWND) -> String {
        unsafe {
            let mut buffer = [0u16; 256];
            let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
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

    pub fn get_window_rect(hwnd: HWND) -> Option<RECT> {
        unsafe {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) != 0 {
                Some(rect)
            } else {
                None
            }
        }
    }

    pub fn is_manageable_window(hwnd: HWND) -> bool {
        unsafe {
            let title = Self::get_window_title(hwnd);

            // Debug: Print details for client-related windows
            // if title.contains("Client") || title.contains("cargo") || title.contains("grid") {
            //     println!("🔍 DEBUG: Checking client-related window: '{}'", title);
            //     println!("   HWND: {:?}", hwnd);
            //     println!("   IsWindow: {}", IsWindow(hwnd));
            //     println!("   IsWindowVisible: {}", IsWindowVisible(hwnd));
            //     println!("   IsIconic: {}", IsIconic(hwnd));
            //     let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            //     println!("   ExStyle: 0x{:X}", ex_style);
            //     println!("   WS_EX_TOOLWINDOW: {}", (ex_style & WS_EX_TOOLWINDOW) != 0);
            //     println!("   WS_EX_APPWINDOW: {}", (ex_style & WS_EX_APPWINDOW) != 0);
            // }

            if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
                return false;
            }

            // Skip minimized windows
            if IsIconic(hwnd) != 0 {
                return false;
            }

            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

            // Skip tool windows unless they have app window flag
            if (ex_style & WS_EX_TOOLWINDOW) != 0 && (ex_style & WS_EX_APPWINDOW) == 0 {
                if title.contains("Client") || title.contains("cargo") || title.contains("grid") {
                    println!("   ❌ Filtered out due to WS_EX_TOOLWINDOW");
                }
                return false;
            }

            if title.is_empty() {
                return false;
            }

            // Skip system windows
            if title.contains("Program Manager")
                || title.contains("Task Switching")
                || title.contains("Windows Input Experience")
            {
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
                    if meets_coverage_threshold(rect, &cell_rect) {
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

        // Place windows on the grid
        for entry in &self.windows {
            let (hwnd, window_info) = entry.pair();
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

    pub fn add_window(&mut self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let title = Self::get_window_title(hwnd);
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);

            let window_info = WindowInfo {
                hwnd,
                title,
                rect,
                grid_cells,
                monitor_cells,
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

    pub fn remove_window(&mut self, hwnd: HWND) -> bool {
        if self.windows.remove(&hwnd).is_some() {
            self.update_grid();
            self.update_monitor_grids();

            // Trigger callback
            self.trigger_window_destroyed(hwnd);

            return true;
        }
        false
    }

    pub fn update_window(&mut self, hwnd: HWND) -> bool {
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
                        print!("## ");
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

    pub fn print_grid_only(&self) {
        println!();
        println!("Grid ({} windows):", self.windows.len());

        // Print column headers
        print!("   ");
        for col in 0..self.config.cols {
            print!("{:2} ", col);
        }
        println!();

        // Print grid rows
        for row in 0..self.config.rows {
            print!("{:2} ", row);
            for col in 0..self.config.cols {
                match self.grid[row][col] {
                    CellState::Occupied(_hwnd) => {
                        print!("## ");
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
    }

    pub fn print_all_grids(&self) {
        // Print virtual grid
        println!();
        println!("=== VIRTUAL GRID (All Monitors Combined) ===");
        self.print_grid_only();

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
                            print!("## ");
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
        hwnd: HWND,
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

    pub fn update_animations(&mut self) -> Vec<HWND> {
        let mut completed_animations = Vec::new();

        // Collect keys that need to be processed
        let animation_keys: Vec<HWND> = self
            .active_animations
            .iter()
            .map(|entry| *entry.key())
            .collect();

        for hwnd in animation_keys {
            if let Some(animation_entry) = self.active_animations.get_mut(&hwnd) {
                if animation_entry.is_completed() {
                    completed_animations.push(hwnd);
                } else {
                    let current_rect = animation_entry.get_current_rect();
                    // Move window to current animation position
                    unsafe {
                        use winapi::um::winuser::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
                        SetWindowPos(
                            hwnd,
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
        let layout = GridLayout::from_current_state(self, name.clone());
        self.saved_layouts.insert(name.clone(), layout);
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
        hwnd: HWND,
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
            if IsWindow(hwnd) == 0 {
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
                    hwnd,
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
        hwnd: HWND,
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
        hwnd: HWND,
        target_row: usize,
        target_col: usize,
        monitor_id: usize,
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
}

// Move/resize detection state (per window)
pub struct MoveResizeState {
    pub last_event: Instant,
    pub in_progress: bool,
}

// Move/resize tracker (shared across threads)
pub struct MoveResizeTracker {
    pub states: Arc<DashMap<isize, MoveResizeState>>, // Use Arc for sharing
    pub timeout: Duration,
}

impl MoveResizeTracker {
    pub fn new(timeout: Duration, states: Arc<DashMap<isize, MoveResizeState>>) -> Arc<Self> {
        Arc::new(Self {
            states: states.clone(),
            timeout,
        })
    }

    pub fn update_event(
        producer: &mut Prod<Arc<HeapRb<(isize, bool)>>>,
        states: &Arc<DashMap<isize, MoveResizeState>>,
        hwnd: HWND,
    ) {
        let hwnd_val = hwnd as isize;
        println!(
            "[MoveResizeTracker::update_event] Called for HWND={:?}",
            hwnd
        ); // DEBUG LOG
        let mut entry = states.entry(hwnd_val).or_insert(MoveResizeState {
            last_event: Instant::now(),
            in_progress: false,
        });
        entry.last_event = Instant::now();
        if !entry.in_progress {
            println!(
                "[MoveResizeTracker] Detected move/resize START for HWND={:?}",
                hwnd
            );
            entry.in_progress = true;
            let _ = producer.try_push((hwnd_val, true));
        }
    }
}

// Event callback system for WindowTracker
pub trait WindowEventCallback: Send + Sync {
    fn on_window_created(&self, hwnd: HWND, window_info: &WindowInfo);
    fn on_window_destroyed(&self, hwnd: HWND);
    fn on_window_moved(&self, hwnd: HWND, window_info: &WindowInfo);
    fn on_window_activated(&self, hwnd: HWND, window_info: &WindowInfo);
    fn on_window_minimized(&self, hwnd: HWND);
    fn on_window_restored(&self, hwnd: HWND, window_info: &WindowInfo);
    // New: Move/Resize start/stop events
    fn on_window_move_resize_start(&self, hwnd: HWND, window_info: &WindowInfo) {}
    fn on_window_move_resize_stop(&self, hwnd: HWND, window_info: &WindowInfo) {}
}

// Box wrapper for dynamic dispatch
pub type WindowEventCallbackBox = Box<dyn WindowEventCallback>;

// Helper function to calculate intersection area between two rectangles
fn calculate_intersection_area(rect1: &RECT, rect2: &RECT) -> i32 {
    let left = rect1.left.max(rect2.left);
    let top = rect1.top.max(rect2.top);
    let right = rect1.right.min(rect2.right);
    let bottom = rect1.bottom.min(rect2.bottom);

    if left < right && top < bottom {
        (right - left) * (bottom - top)
    } else {
        0
    }
}

// Helper function to check if window coverage of a cell meets the threshold
fn meets_coverage_threshold(window_rect: &RECT, cell_rect: &RECT) -> bool {
    let intersection_area = calculate_intersection_area(window_rect, cell_rect);
    let cell_area = (cell_rect.right - cell_rect.left) * (cell_rect.bottom - cell_rect.top);

    if cell_area <= 0 {
        return false;
    }

    let coverage_ratio = intersection_area as f32 / cell_area as f32;
    coverage_ratio >= COVERAGE_THRESHOLD
}

// iceoryx2 IPC integration for command and control
pub mod ipc;
/// Protocol definitions and message types for IPC communication
pub mod ipc_protocol;

// Client module for real-time grid reconstruction and monitoring
pub mod ipc_client;
pub use ipc_client::GridClient;

// Server module for IPC server functionality
pub mod ipc_server;
pub use crate::ipc_server::start_server;

// Window enumeration callback function
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    let counter = tracker.enum_counter.fetch_add(1, Ordering::SeqCst) + 1;

    if WindowTracker::is_manageable_window(hwnd) {
        // let title = WindowTracker::get_window_title(hwnd);
        // println!("Checking window #{}: {}", counter,
        //     if title.is_empty() { "<No Title>" } else { &title });
        // println!("  -> Adding manageable window: {}", title);
        if tracker.add_window(hwnd) {
            // println!("  -> Added successfully");
        } else {
            // println!("  -> Failed to add window");
        }
    }

    1 // Continue enumeration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_tracker_creation() {
        let tracker = WindowTracker::new();
        assert_eq!(tracker.windows.len(), 0);
        assert_eq!(tracker.enum_counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_grid_config() {
        let config = GridConfig::default();
        assert_eq!(config.rows, 8);
        assert_eq!(config.cols, 12);
        assert_eq!(config.cell_count(), 96);

        let custom_config = GridConfig::new(4, 4);
        assert_eq!(custom_config.rows, 4);
        assert_eq!(custom_config.cols, 4);
        assert_eq!(custom_config.cell_count(), 16);
    }
}

pub struct WindowEventSystem {
    pub move_resize_tracker: Arc<MoveResizeTracker>,
    pub tracker: Arc<Mutex<WindowTracker>>, // Reference to main tracker for callbacks
    pub producer: Arc<Mutex<Prod<Arc<HeapRb<(isize, bool)>>>>>,
    pub consumer: Cons<Arc<HeapRb<(isize, bool)>>>,
    pub states: Arc<DashMap<isize, MoveResizeState>>,
    // Optional event callback for IPC publishing (GridEvent)
    pub event_callback: Option<Box<dyn Fn(crate::ipc_protocol::GridEvent) + Send + Sync>>,
}

impl WindowEventSystem {
    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Self {
        let rb = Arc::new(HeapRb::<(isize, bool)>::new(256));
        let producer = Arc::new(Mutex::new(Prod::new(rb.clone())));
        let consumer = Cons::new(rb.clone());
        let states = Arc::new(DashMap::new());
        let move_resize_tracker =
            MoveResizeTracker::new(Duration::from_millis(200), states.clone());
        // Spawn the background thread for move/resize stop detection, passing a reference to the producer
        let states_ref = states.clone();
        let producer_thread = producer.clone();
        let timeout = Duration::from_millis(200);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let now = Instant::now();
            let mut to_stop = Vec::new();
            for mut entry in states_ref.iter_mut() {
                if entry.in_progress && now.duration_since(entry.last_event) > timeout {
                    println!(
                        "[MoveResizeTracker] Detected move/resize STOP for HWND={:?}",
                        *entry.key()
                    );
                    entry.in_progress = false;
                    to_stop.push(*entry.key());
                }
            }
            for hwnd_val in to_stop {
                if let Ok(mut prod) = producer_thread.lock() {
                    let _ = prod.try_push((hwnd_val, false));
                }
            }
        });
        Self {
            move_resize_tracker,
            tracker,
            producer,
            consumer,
            states,
            event_callback: None,
        }
    }

    // Set the event callback for IPC publishing
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(crate::ipc_protocol::GridEvent) + Send + Sync + 'static,
    {
        self.event_callback = Some(Box::new(callback));
    }

    // Call this periodically from the main thread/event loop
    pub fn poll_move_resize_events(&mut self) {
        while let Some((hwnd_val, is_start)) = self.consumer.try_pop() {
            println!(
                "[DEBUG][poll_move_resize_events] Popped from ringbuf: hwnd_val={:?}, is_start={}",
                hwnd_val, is_start
            );
            let hwnd = hwnd_val as HWND;
            let tracker = self.tracker.lock().unwrap();
            if let Some(window_info) = tracker.windows.get(&hwnd) {
                println!(
                    "[DEBUG][poll_move_resize_events] Found window_info for HWND={:?}, title={}",
                    hwnd, window_info.title
                );
                for callback in &tracker.event_callbacks {
                    if is_start {
                        println!(
                            "[WindowEventSystem] on_window_move_resize_start: HWND={:?}, title={}",
                            hwnd, window_info.title
                        );
                        callback.on_window_move_resize_start(hwnd, &*window_info);
                    } else {
                        println!(
                            "[WindowEventSystem] on_window_move_resize_stop: HWND={:?}, title={}",
                            hwnd, window_info.title
                        );
                        callback.on_window_move_resize_stop(hwnd, &*window_info);
                    }
                }
                // Publish to IPC if callback is set
                if let Some(ref cb) = self.event_callback {
                    use crate::ipc_protocol::GridEvent;
                    // Fill in real values for the event fields
                    let title = window_info.title.clone();
                    let (row, col) = window_info.grid_cells.get(0).cloned().unwrap_or((0, 0));
                    let grid_top_left_row = row;
                    let grid_top_left_col = col;
                    let grid_bottom_right_row = row;
                    let grid_bottom_right_col = col;
                    let real_x = window_info.rect.left;
                    let real_y = window_info.rect.top;
                    let real_width = (window_info.rect.right - window_info.rect.left).max(0) as u32;
                    let real_height =
                        (window_info.rect.bottom - window_info.rect.top).max(0) as u32;
                    let monitor_id = 0; // TODO: fill with real monitor id if available
                    let event = if is_start {
                        println!("[DEBUG][poll_move_resize_events] Creating GridEvent::WindowMoveStart for HWND={:?}", hwnd);
                        GridEvent::WindowMoveStart {
                            hwnd: hwnd as u64,
                            title,
                            current_row: row,
                            current_col: col,
                            grid_top_left_row,
                            grid_top_left_col,
                            grid_bottom_right_row,
                            grid_bottom_right_col,
                            real_x,
                            real_y,
                            real_width,
                            real_height,
                            monitor_id,
                        }
                    } else {
                        println!("[DEBUG][poll_move_resize_events] Creating GridEvent::WindowMoveStop for HWND={:?}", hwnd);
                        GridEvent::WindowMoveStop {
                            hwnd: hwnd as u64,
                            title,
                            final_row: row,
                            final_col: col,
                            grid_top_left_row,
                            grid_top_left_col,
                            grid_bottom_right_row,
                            grid_bottom_right_col,
                            real_x,
                            real_y,
                            real_width,
                            real_height,
                            monitor_id,
                        }
                    };
                    println!("[DEBUG][poll_move_resize_events] Invoking event_callback for HWND={:?}, is_start={}", hwnd, is_start);
                    cb(event);
                }
            } else {
                println!("[WindowEventSystem] Event for unknown HWND={:?}", hwnd);
            };
        }
    }
}
