use std::collections::HashMap;
use std::ptr;
use std::time::{Duration, Instant};
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::*;

// Grid configuration
pub const GRID_ROWS: usize = 8;
pub const GRID_COLS: usize = 12;

// Coverage threshold: percentage of cell area that must be covered by window
// to consider the window as occupying that cell (0.0 to 1.0)
const COVERAGE_THRESHOLD: f32 = 0.3; // 30% coverage required

// Animation and Tweening System
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EasingType {
    Linear,        // Constant speed
    EaseIn,        // Slow start, fast end
    EaseOut,       // Fast start, slow end  
    EaseInOut,     // Slow start and end, fast middle
    Bounce,        // Bouncing effect at the end
    Elastic,       // Elastic/spring effect
    Back,          // Slight overshoot then settle
}

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
    pub fn new(hwnd: HWND, start_rect: RECT, target_rect: RECT, duration: Duration, easing: EasingType) -> Self {
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
            right: self.lerp(self.start_rect.right, self.target_rect.right, eased_progress),
            bottom: self.lerp(self.start_rect.bottom, self.target_rect.bottom, eased_progress),
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
            },
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
            },
            EasingType::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    -((2.0_f32).powf(10.0 * (t - 1.0)) * ((t - 1.0 - s) * (2.0 * std::f32::consts::PI) / p).sin())
                }
            },
            EasingType::Back => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            },
        }
    }
}

// Grid Layout for transferring complete grid states
#[derive(Clone, Debug)]
pub struct GridLayout {
    pub name: String,
    pub virtual_grid: [[Option<HWND>; GRID_COLS]; GRID_ROWS],
    pub monitor_grids: Vec<MonitorGridLayout>,
    pub created_at: Instant,
}

#[derive(Clone, Debug)]
pub struct MonitorGridLayout {
    pub monitor_id: usize,
    pub grid: [[Option<HWND>; GRID_COLS]; GRID_ROWS],
}

impl GridLayout {
    pub fn new(name: String) -> Self {
        Self {
            name,
            virtual_grid: [[None; GRID_COLS]; GRID_ROWS],
            monitor_grids: Vec::new(),
            created_at: Instant::now(),
        }
    }
    
    pub fn from_current_state(tracker: &WindowTracker, name: String) -> Self {
        let mut layout = Self::new(name);
        
        // Extract virtual grid layout
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                if let CellState::Occupied(hwnd) = tracker.grid[row][col] {
                    layout.virtual_grid[row][col] = Some(hwnd);
                }
            }
        }
        
        // Extract monitor grid layouts
        for monitor_grid in &tracker.monitor_grids {
            let mut monitor_layout = MonitorGridLayout {
                monitor_id: monitor_grid.monitor_id,
                grid: [[None; GRID_COLS]; GRID_ROWS],
            };
            
            for row in 0..GRID_ROWS {
                for col in 0..GRID_COLS {
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
    Empty,           // No window (on-screen area)
    Occupied(HWND),  // Window present
    OffScreen,       // Off-screen area (outside actual monitor bounds)
}

#[derive(Clone, Debug)]
pub struct MonitorGrid {
    pub monitor_id: usize,
    pub monitor_rect: (i32, i32, i32, i32), // (left, top, right, bottom)
    pub grid: [[CellState; GRID_COLS]; GRID_ROWS],
}

impl MonitorGrid {
    pub fn new(monitor_id: usize, monitor_rect: RECT) -> Self {
        Self {
            monitor_id,
            monitor_rect: (monitor_rect.left, monitor_rect.top, monitor_rect.right, monitor_rect.bottom),
            grid: [[CellState::Empty; GRID_COLS]; GRID_ROWS],
        }
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates
        if rect.left < -30000 || rect.top < -30000 || 
           rect.right < rect.left || rect.bottom < rect.top {
            return cells;
        }

        let (left, top, right, bottom) = self.monitor_rect;

        // Check if window intersects with this monitor
        if rect.right <= left || rect.left >= right ||
           rect.bottom <= top || rect.top >= bottom {
            return cells; // Window is not on this monitor
        }

        let cell_width = (right - left) / GRID_COLS as i32;
        let cell_height = (bottom - top) / GRID_ROWS as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return cells;
        }

        // Calculate potential range of cells that might be affected
        let start_col = ((rect.left.max(left) - left) / cell_width).max(0) as usize;
        let end_col = ((rect.right.min(right) - left) / cell_width).min(GRID_COLS as i32 - 1) as usize;
        let start_row = ((rect.top.max(top) - top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom.min(bottom) - top) / cell_height).min(GRID_ROWS as i32 - 1) as usize;

        // Check coverage for each potentially affected cell
        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < GRID_ROWS && col < GRID_COLS {
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

    pub fn update_grid(&mut self, windows: &HashMap<HWND, WindowInfo>) {
        // Reset grid to empty
        self.grid = [[CellState::Empty; GRID_COLS]; GRID_ROWS];

        // Place windows on the grid
        for (hwnd, window_info) in windows {
            let grid_cells = self.window_to_grid_cells(&window_info.rect);
            for (row, col) in grid_cells {
                if row < GRID_ROWS && col < GRID_COLS {
                    self.grid[row][col] = CellState::Occupied(*hwnd);
                }
            }
        }
    }

    pub fn print_grid(&self) {
        // Column headers
        print!("    ");
        for col in 0..GRID_COLS {
            print!(" {:2}", col);
        }
        println!();

        for row in 0..GRID_ROWS {
            print!("{:2}: ", row);
            for col in 0..GRID_COLS {
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
    pub windows: HashMap<HWND, WindowInfo>,
    pub monitor_rect: RECT,  // Virtual screen rect
    pub grid: [[CellState; GRID_COLS]; GRID_ROWS], // Virtual grid
    pub monitor_grids: Vec<MonitorGrid>, // Individual monitor grids
    pub enum_counter: usize,
    pub active_animations: HashMap<HWND, WindowAnimation>, // Active window animations
    pub saved_layouts: HashMap<String, GridLayout>, // Saved grid layouts
}

impl WindowTracker {
    pub fn new() -> Self {
        // Get the virtual screen dimensions (all monitors combined)
        let rect = unsafe {
            RECT {
                left: GetSystemMetrics(SM_XVIRTUALSCREEN),
                top: GetSystemMetrics(SM_YVIRTUALSCREEN),
                right: GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN),
                bottom: GetSystemMetrics(SM_YVIRTUALSCREEN) + GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        };

        let mut tracker = Self {
            windows: HashMap::new(),
            monitor_rect: rect,
            grid: [[CellState::Empty; GRID_COLS]; GRID_ROWS],
            monitor_grids: Vec::new(),
            enum_counter: 0,
            active_animations: HashMap::new(),
            saved_layouts: HashMap::new(),
        };

        // Initialize individual monitor grids
        tracker.initialize_monitor_grids();
        tracker
    }

    pub fn initialize_monitor_grids(&mut self) {
        self.monitor_grids.clear();
        let monitors = self.get_actual_monitor_bounds();
        
        for (index, monitor_rect) in monitors.iter().enumerate() {
            let monitor_grid = MonitorGrid::new(index, *monitor_rect);
            self.monitor_grids.push(monitor_grid);
        }
        
        println!("Initialized {} individual monitor grids", self.monitor_grids.len());
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
            if title.contains("Client") || title.contains("cargo") || title.contains("grid") {
                println!("🔍 DEBUG: Checking client-related window: '{}'", title);
                println!("   HWND: {:?}", hwnd);
                println!("   IsWindow: {}", IsWindow(hwnd));
                println!("   IsWindowVisible: {}", IsWindowVisible(hwnd));
                println!("   IsIconic: {}", IsIconic(hwnd));
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                println!("   ExStyle: 0x{:X}", ex_style);
                println!("   WS_EX_TOOLWINDOW: {}", (ex_style & WS_EX_TOOLWINDOW) != 0);
                println!("   WS_EX_APPWINDOW: {}", (ex_style & WS_EX_APPWINDOW) != 0);
            }
            
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
                || title.contains("Windows Input Experience") {
                return false;
            }

            if title.contains("Client") || title.contains("cargo") || title.contains("grid") {
                println!("   ✅ Window passed all filters - will be tracked");
            }

            true
        }
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates (like minimized windows)
        if rect.left < -30000 || rect.top < -30000 || 
           rect.right < rect.left || rect.bottom < rect.top {
            return cells;
        }

        let cell_width = (self.monitor_rect.right - self.monitor_rect.left) / GRID_COLS as i32;
        let cell_height = (self.monitor_rect.bottom - self.monitor_rect.top) / GRID_ROWS as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return cells;
        }

        // Calculate potential range of cells that might be affected
        let start_col = ((rect.left - self.monitor_rect.left) / cell_width).max(0) as usize;
        let end_col = ((rect.right - self.monitor_rect.left) / cell_width).min(GRID_COLS as i32 - 1) as usize;
        let start_row = ((rect.top - self.monitor_rect.top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom - self.monitor_rect.top) / cell_height).min(GRID_ROWS as i32 - 1) as usize;

        // Additional bounds checking
        if start_col >= GRID_COLS || start_row >= GRID_ROWS {
            return cells;
        }

        // Check coverage for each potentially affected cell
        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < GRID_ROWS && col < GRID_COLS {
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
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
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
        for (hwnd, window_info) in &self.windows {
            for (row, col) in &window_info.grid_cells {
                if *row < GRID_ROWS && *col < GRID_COLS {
                    self.grid[*row][*col] = CellState::Occupied(*hwnd);
                }
            }
        }
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

            self.windows.insert(hwnd, window_info);
            self.update_grid();
            self.update_monitor_grids();
            return true;
        }
        false
    }

    pub fn remove_window(&mut self, hwnd: HWND) -> bool {
        if self.windows.remove(&hwnd).is_some() {
            self.update_grid();
            self.update_monitor_grids();
            return true;
        }
        false
    }

    pub fn update_window(&mut self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);
            if let Some(window_info) = self.windows.get_mut(&hwnd) {
                window_info.rect = rect;
                window_info.grid_cells = grid_cells;
                window_info.monitor_cells = monitor_cells;
                self.update_grid();
                self.update_monitor_grids();
                return true;
            }
        }
        false
    }
    
    pub fn print_grid(&self) {
        println!();
        println!("{}", "=".repeat(60));
        println!("Window Grid Tracker - {}x{} Grid ({} windows)", GRID_ROWS, GRID_COLS, self.windows.len());
        println!("Monitor: {}x{} px", 
            self.monitor_rect.right - self.monitor_rect.left,
            self.monitor_rect.bottom - self.monitor_rect.top);
        println!("{}", "=".repeat(60));

        // Print column headers
        print!("   ");
        for col in 0..GRID_COLS {
            print!("{:2} ", col);
        }
        println!();

        // Print grid with different symbols for different states
        for row in 0..GRID_ROWS {
            print!("{:2} ", row);
            
            for col in 0..GRID_COLS {
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
        for col in 0..GRID_COLS {
            print!("{:2} ", col);
        }
        println!();
        
        // Print grid rows
        for row in 0..GRID_ROWS {
            print!("{:2} ", row);
            for col in 0..GRID_COLS {
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
            println!("Monitor bounds: ({}, {}) to ({}, {})", 
                monitor_grid.monitor_rect.0, monitor_grid.monitor_rect.1,
                monitor_grid.monitor_rect.2, monitor_grid.monitor_rect.3);
            
            // Count windows on this monitor
            let mut windows_on_monitor = 0;
            for (_, window_info) in &self.windows {
                if window_info.monitor_cells.contains_key(&monitor_grid.monitor_id) {
                    windows_on_monitor += 1;
                }
            }
            println!("Windows on this monitor: {}", windows_on_monitor);
            
            // Print column headers
            print!("   ");
            for col in 0..GRID_COLS {
                print!("{:2} ", col);
            }
            println!();
            
            // Print grid rows
            for row in 0..GRID_ROWS {
                print!("{:2} ", row);
                for col in 0..GRID_COLS {
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
        
        self.enum_counter = 0; // Reset counter
        unsafe {
            let result = EnumWindows(Some(enum_windows_proc), self as *mut _ as LPARAM);
            println!("EnumWindows completed with result: {}", result);
        }
        println!("Window enumeration finished. Found {} windows.", self.windows.len());
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
        
        let cell_width = (self.monitor_rect.right - self.monitor_rect.left) / GRID_COLS as i32;
        let cell_height = (self.monitor_rect.bottom - self.monitor_rect.top) / GRID_ROWS as i32;
        
        // Initialize all cells based on whether they're on an actual monitor
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                let cell_left = self.monitor_rect.left + (col as i32 * cell_width);
                let cell_top = self.monitor_rect.top + (row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;
                
                // Check if this cell overlaps with any actual monitor
                let mut is_on_screen = false;
                for monitor_rect in &actual_monitors {
                    if cell_left < monitor_rect.right && cell_right > monitor_rect.left &&
                       cell_top < monitor_rect.bottom && cell_bottom > monitor_rect.top {
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
    
    pub fn start_window_animation(&mut self, hwnd: HWND, target_rect: RECT, duration: Duration, easing: EasingType) -> Result<(), String> {
        if let Some(current_rect) = Self::get_window_rect(hwnd) {
            let animation = WindowAnimation::new(hwnd, current_rect, target_rect, duration, easing);
            self.active_animations.insert(hwnd, animation);
            println!("🎬 Started animation for window {:?}: {} -> {} over {:?}", 
                hwnd, 
                format!("({},{},{},{})", current_rect.left, current_rect.top, current_rect.right, current_rect.bottom),
                format!("({},{},{},{})", target_rect.left, target_rect.top, target_rect.right, target_rect.bottom),
                duration
            );
            Ok(())
        } else {
            Err(format!("Failed to get current rect for window {:?}", hwnd))
        }
    }
    
    pub fn update_animations(&mut self) -> Vec<HWND> {
        let mut completed_animations = Vec::new();
        
        for (hwnd, animation) in &mut self.active_animations {
            if animation.is_completed() {
                completed_animations.push(*hwnd);
            } else {
                let current_rect = animation.get_current_rect();
                // Move window to current animation position
                unsafe {
                    use winapi::um::winuser::{SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE};
                    SetWindowPos(
                        *hwnd,
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
        
        // Remove completed animations
        for hwnd in &completed_animations {
            self.active_animations.remove(hwnd);
            println!("🎬 Animation completed for window {:?}", hwnd);
        }
        
        completed_animations
    }
    
    pub fn apply_grid_layout(&mut self, layout: &GridLayout, duration: Duration, easing: EasingType) -> Result<usize, String> {
        let mut animations_started = 0;
        
        // Apply virtual grid layout
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
                if let Some(target_hwnd) = layout.virtual_grid[row][col] {
                    // Calculate target position from grid coordinates
                    if let Some(target_rect) = self.virtual_cell_to_window_rect(row, col) {
                        if self.windows.contains_key(&target_hwnd) {
                            match self.start_window_animation(target_hwnd, target_rect, duration, easing) {
                                Ok(_) => animations_started += 1,
                                Err(e) => println!("⚠️ Failed to start animation for window {:?}: {}", target_hwnd, e),
                            }
                        }
                    }
                }
            }
        }
        
        println!("🎬 Started {} animations for grid layout '{}'", animations_started, layout.name);
        Ok(animations_started)
    }
    
    pub fn save_current_layout(&mut self, name: String) {
        let layout = GridLayout::from_current_state(self, name.clone());
        self.saved_layouts.insert(name.clone(), layout);
        println!("💾 Saved current grid layout as '{}'", name);
    }
    
    pub fn get_saved_layout(&self, name: &str) -> Option<&GridLayout> {
        self.saved_layouts.get(name)
    }
    
    pub fn list_saved_layouts(&self) -> Vec<&String> {
        self.saved_layouts.keys().collect()
    }
    
    fn virtual_cell_to_window_rect(&self, row: usize, col: usize) -> Option<RECT> {
        if row >= GRID_ROWS || col >= GRID_COLS {
            return None;
        }
        
        let grid_width = self.monitor_rect.right - self.monitor_rect.left;
        let grid_height = self.monitor_rect.bottom - self.monitor_rect.top;
        
        let cell_width = grid_width / GRID_COLS as i32;
        let cell_height = grid_height / GRID_ROWS as i32;
        
        let left = self.monitor_rect.left + (col as i32 * cell_width);
        let top = self.monitor_rect.top + (row as i32 * cell_height);
        let right = left + cell_width;
        let bottom = top + cell_height;
        
        Some(RECT { left, top, right, bottom })
    }
    
    /// Move a window to a specific grid cell
    pub fn move_window_to_cell(&mut self, hwnd: HWND, target_row: usize, target_col: usize) -> Result<(), String> {
        if target_row >= GRID_ROWS || target_col >= GRID_COLS {
            return Err(format!("Invalid grid coordinates: ({}, {})", target_row, target_col));
        }
        
        if let Some(target_rect) = self.virtual_cell_to_window_rect(target_row, target_col) {
            unsafe {
                SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    target_rect.left,
                    target_rect.top,
                    target_rect.right - target_rect.left,
                    target_rect.bottom - target_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        
        Ok(())
    }

    /// Assign a window to a virtual grid cell (tracking only, no movement)
    pub fn assign_window_to_virtual_cell(&mut self, hwnd: HWND, target_row: usize, target_col: usize) -> Result<(), String> {
        if target_row >= GRID_ROWS || target_col >= GRID_COLS {
            return Err(format!("Invalid grid coordinates: ({}, {})", target_row, target_col));
        }
        
        // Clear the old position
        for row in 0..GRID_ROWS {
            for col in 0..GRID_COLS {
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
        if let Some(window_info) = self.windows.get_mut(&hwnd) {
            window_info.grid_cells.clear();
            window_info.grid_cells.push((target_row, target_col));
        }
        
        Ok(())
    }

    /// Assign a window to a monitor-specific grid cell
    pub fn assign_window_to_monitor_cell(&mut self, hwnd: HWND, target_row: usize, target_col: usize, monitor_id: usize) -> Result<(), String> {
        if monitor_id >= self.monitor_grids.len() {
            return Err(format!("Invalid monitor ID: {}", monitor_id));
        }
        
        if target_row >= GRID_ROWS || target_col >= GRID_COLS {
            return Err(format!("Invalid monitor grid coordinates: ({}, {}) for monitor {}", target_row, target_col, monitor_id));
        }
        
        // Clear the old position in all monitor grids
        for grid in &mut self.monitor_grids {
            for row in 0..GRID_ROWS {
                for col in 0..GRID_COLS {
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
        if let Some(window_info) = self.windows.get_mut(&hwnd) {
            let cells = vec![(target_row, target_col)];
            window_info.monitor_cells.insert(monitor_id, cells);
        }
        
        Ok(())
    }
}

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

// Windows event hook integration using SetWinEventHook
pub mod window_events;

// iceoryx2 IPC integration for command and control
pub mod ipc;

// Client module for real-time grid reconstruction and monitoring
pub mod ipc_client;

// Server module for IPC server functionality
pub mod ipc_server;

// Window enumeration callback function
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    tracker.enum_counter += 1;
    
    if WindowTracker::is_manageable_window(hwnd) {
        let title = WindowTracker::get_window_title(hwnd);
        println!("Checking window #{}: {}", tracker.enum_counter, 
            if title.is_empty() { "<No Title>" } else { &title });
        println!("  -> Adding manageable window: {}", title);
        if tracker.add_window(hwnd) {
            println!("  -> Added successfully");
        } else {
            println!("  -> Failed to add window");
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
        assert_eq!(tracker.enum_counter, 0);
    }

    #[test]
    fn test_grid_dimensions() {
        assert_eq!(GRID_ROWS, 8);
        assert_eq!(GRID_COLS, 12);
    }
}
