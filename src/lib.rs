use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::ptr;
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::*;

// Additional constants for WinEvent hooks
const EVENT_OBJECT_CREATE: u32 = 0x8000;
const EVENT_OBJECT_DESTROY: u32 = 0x8001;
const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_SYSTEM_MINIMIZESTART: u32 = 0x0016;
const EVENT_SYSTEM_MINIMIZEEND: u32 = 0x0017;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const OBJID_WINDOW: i32 = 0x00000000;
const CHILDID_SELF: i32 = 0;

// Grid configuration
pub const GRID_ROWS: usize = 8;
pub const GRID_COLS: usize = 12;

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

        let start_col = ((rect.left.max(left) - left) / cell_width).max(0) as usize;
        let end_col = ((rect.right.min(right) - left) / cell_width).min(GRID_COLS as i32 - 1) as usize;
        let start_row = ((rect.top.max(top) - top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom.min(bottom) - top) / cell_height).min(GRID_ROWS as i32 - 1) as usize;

        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < GRID_ROWS && col < GRID_COLS {
                    cells.push((row, col));
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
            if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
                return false;
            }

            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            
            // Skip tool windows unless they have app window flag
            if (ex_style & WS_EX_TOOLWINDOW) != 0 && (ex_style & WS_EX_APPWINDOW) == 0 {
                return false;
            }

            let title = Self::get_window_title(hwnd);
            if title.is_empty() {
                return false;
            }

            // Skip system windows
            if title.contains("Program Manager") 
                || title.contains("Task Switching")
                || title.contains("Windows Input Experience") {
                return false;
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

        let start_col = ((rect.left - self.monitor_rect.left) / cell_width).max(0) as usize;
        let end_col = ((rect.right - self.monitor_rect.left) / cell_width).min(GRID_COLS as i32 - 1) as usize;
        let start_row = ((rect.top - self.monitor_rect.top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom - self.monitor_rect.top) / cell_height).min(GRID_ROWS as i32 - 1) as usize;

        // Additional bounds checking
        if start_col >= GRID_COLS || start_row >= GRID_ROWS {
            return cells;
        }

        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < GRID_ROWS && col < GRID_COLS {
                    cells.push((row, col));
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
}

// Windows event hook integration using SetWinEventHook
pub mod window_events {
    use super::*;
    use std::ptr;
    use winapi::um::errhandlingapi::GetLastError;

    // Global state for event hooks - necessary due to Windows API callback constraints
    // These are accessed only from the main thread and properly synchronized
    // SAFETY: These statics are only accessed from a single thread (main thread)
    // and proper cleanup is ensured through the cleanup_hooks function
    static mut WINDOW_TRACKER: Option<Arc<Mutex<WindowTracker>>> = None;
    static mut EVENT_HOOKS: Vec<winapi::shared::windef::HWINEVENTHOOK> = Vec::new();

    // WinEvent hook procedure
    pub unsafe extern "system" fn win_event_proc(
        _h_winevent_hook: winapi::shared::windef::HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        id_object: i32,
        id_child: i32,
        _dw_event_thread: u32,
        _dw_ms_event_time: u32,
    ) {
        // Only process window-level events (not child objects)
        if id_object != OBJID_WINDOW || id_child != CHILDID_SELF {
            return;
        }

        // Skip if window handle is null
        if hwnd.is_null() {
            return;
        }

        // Safe access to static without creating references
        let tracker_opt = ptr::addr_of!(WINDOW_TRACKER).read();
        if let Some(tracker_arc) = tracker_opt {
            if let Ok(mut tracker) = tracker_arc.try_lock() {
                let window_title = WindowTracker::get_window_title(hwnd);
                let event_name = match event {
                    EVENT_OBJECT_CREATE => "CREATED",
                    EVENT_OBJECT_DESTROY => "DESTROYED", 
                    EVENT_OBJECT_LOCATIONCHANGE => "MOVED/RESIZED",
                    EVENT_SYSTEM_FOREGROUND => "ACTIVATED",
                    EVENT_SYSTEM_MINIMIZESTART => "MINIMIZED",
                    EVENT_SYSTEM_MINIMIZEEND => "RESTORED",
                    _ => "OTHER"
                };

                println!("🔔 WINDOW EVENT RECEIVED!");
                println!("   Event: {} | Window: {}", 
                    event_name,
                    if window_title.is_empty() { "<No Title>" } else { &window_title }
                );

                match event {
                    EVENT_OBJECT_CREATE => {
                        println!("   → Checking if window is manageable...");
                        if WindowTracker::is_manageable_window(hwnd) {
                            println!("   → Window IS manageable, adding to tracker...");
                            // Small delay to ensure window is fully initialized
                            std::thread::sleep(std::time::Duration::from_millis(100));
                            if tracker.add_window(hwnd) {
                                println!("   ✅ Window created and added: {}", window_title);
                                println!("   📊 Updating grid displays...");
                                tracker.print_all_grids();
                            } else {
                                println!("   ❌ Failed to add window");
                            }
                        } else {
                            println!("   → Window is NOT manageable, ignoring");
                        }
                    }
                    EVENT_OBJECT_DESTROY => {
                        println!("   → Removing window from tracker...");
                        if tracker.remove_window(hwnd) {
                            println!("   ✅ Window destroyed and removed");
                            println!("   📊 Updating grid displays...");
                            tracker.print_all_grids();
                        } else {
                            println!("   → Window was not being tracked");
                        }
                    }
                    EVENT_OBJECT_LOCATIONCHANGE | EVENT_SYSTEM_FOREGROUND => {
                        println!("   → Checking if window is manageable...");
                        if WindowTracker::is_manageable_window(hwnd) {
                            println!("   → Window IS manageable, updating position...");
                            if tracker.update_window(hwnd) {
                                println!("   ✅ Window updated: {}", window_title);
                                println!("   📊 Updating grid displays...");
                                tracker.print_all_grids();
                            } else {
                                println!("   → No significant position change detected");
                            }
                        } else {
                            println!("   → Window is NOT manageable, ignoring");
                        }
                    }
                    EVENT_SYSTEM_MINIMIZESTART => {
                        println!("   → Window minimized, removing from grid...");
                        if tracker.remove_window(hwnd) {
                            println!("   ✅ Minimized window removed from grid");
                            println!("   📊 Updating grid displays...");
                            tracker.print_all_grids();
                        }
                    }
                    EVENT_SYSTEM_MINIMIZEEND => {
                        println!("   → Window restored, checking if should be tracked...");
                        if WindowTracker::is_manageable_window(hwnd) {
                            if tracker.add_window(hwnd) {
                                println!("   ✅ Restored window added back to grid");
                                println!("   📊 Updating grid displays...");
                                tracker.print_all_grids();
                            }
                        }
                    }
                    _ => {
                        println!("   → Unhandled event type: {}", event);
                    }
                }
                println!(); // Add blank line for readability
            }
        }
    }

    pub fn setup_window_events(tracker: Arc<Mutex<WindowTracker>>) -> Result<(), String> {
        unsafe {
            // Use raw pointer access to avoid static mut ref warnings
            ptr::addr_of_mut!(WINDOW_TRACKER).write(Some(tracker.clone()));
            ptr::addr_of_mut!(EVENT_HOOKS).write(Vec::new());

            println!("🔧 Setting up WinEvent hooks...");

            // Set up hooks for different window events
            let events_to_hook = [
                (EVENT_OBJECT_CREATE, "Window Creation"),
                (EVENT_OBJECT_DESTROY, "Window Destruction"), 
                (EVENT_OBJECT_LOCATIONCHANGE, "Window Move/Resize"),
                (EVENT_SYSTEM_FOREGROUND, "Window Activation"),
                (EVENT_SYSTEM_MINIMIZESTART, "Window Minimize"),
                (EVENT_SYSTEM_MINIMIZEEND, "Window Restore"),
            ];

            for (event, description) in &events_to_hook {
                let hook = SetWinEventHook(
                    *event,
                    *event,
                    ptr::null_mut(), // No specific module
                    Some(win_event_proc),
                    0, // All processes
                    0, // All threads
                    WINEVENT_OUTOFCONTEXT, // Out-of-context (more reliable)
                );

                if hook.is_null() {
                    let error = GetLastError();
                    println!("❌ Failed to set up hook for {}: error {}", description, error);
                } else {
                    // Safely add hook to static vector
                    let hooks_ptr = ptr::addr_of_mut!(EVENT_HOOKS);
                    (*hooks_ptr).push(hook);
                    println!("✅ Successfully set up hook for {}", description);
                }
            }

            // Check if we have any hooks
            let hooks_len = ptr::addr_of!(EVENT_HOOKS).read().len();
            if hooks_len == 0 {
                return Err("Failed to set up any event hooks".to_string());
            }

            println!("🚀 Successfully set up {} WinEvent hooks!", hooks_len);
            println!("📢 Now listening for real-time window events across all monitors!");
            println!();

            Ok(())
        }
    }

    pub fn cleanup_hooks() {
        unsafe {
            // Use raw pointer iteration to avoid warnings
            let hooks_ptr = ptr::addr_of!(EVENT_HOOKS);
            for hook in &(*hooks_ptr) {
                UnhookWinEvent(*hook);
            }
            ptr::addr_of_mut!(EVENT_HOOKS).write(Vec::new());
            println!("🧹 Cleaned up all event hooks");
        }
    }
}

// Window enumeration callback function
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    tracker.enum_counter += 1;
    
    let title = WindowTracker::get_window_title(hwnd);
    println!("Checking window #{}: {}", tracker.enum_counter, 
        if title.is_empty() { "<No Title>" } else { &title });

    if WindowTracker::is_manageable_window(hwnd) {
        println!("  -> Adding manageable window: {}", title);
        if tracker.add_window(hwnd) {
            println!("  -> Added successfully");
        } else {
            println!("  -> Failed to add window");
        }
    } else {
        println!("  -> Skipping non-manageable window");
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
