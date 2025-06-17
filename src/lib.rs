use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
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

#[derive(Clone)]
pub struct WindowInfo {
    pub hwnd: HWND,
    pub title: String,
    pub rect: RECT,
    pub grid_cells: Vec<(usize, usize)>,
}

pub struct WindowTracker {
    pub windows: HashMap<HWND, WindowInfo>,
    pub monitor_rect: RECT,
    pub grid: [[Option<HWND>; GRID_COLS]; GRID_ROWS],
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

        Self {
            windows: HashMap::new(),
            monitor_rect: rect,
            grid: [[None; GRID_COLS]; GRID_ROWS],
            enum_counter: 0,
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
        // Clear grid
        self.grid = [[None; GRID_COLS]; GRID_ROWS];

        // Fill grid with current windows
        for (hwnd, window_info) in &self.windows {
            for (row, col) in &window_info.grid_cells {
                if *row < GRID_ROWS && *col < GRID_COLS {
                    self.grid[*row][*col] = Some(*hwnd);
                }
            }
        }
    }

    pub fn add_window(&mut self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let title = Self::get_window_title(hwnd);
            let grid_cells = self.window_to_grid_cells(&rect);

            let window_info = WindowInfo {
                hwnd,
                title,
                rect,
                grid_cells,
            };

            self.windows.insert(hwnd, window_info);
            self.update_grid();
            return true;
        }
        false
    }

    pub fn remove_window(&mut self, hwnd: HWND) -> bool {
        if self.windows.remove(&hwnd).is_some() {
            self.update_grid();
            return true;
        }
        false
    }

    pub fn update_window(&mut self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let grid_cells = self.window_to_grid_cells(&rect);
            if let Some(window_info) = self.windows.get_mut(&hwnd) {
                window_info.rect = rect;
                window_info.grid_cells = grid_cells;
                self.update_grid();
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

        // Print grid (simplified without ANSI colors)
        for row in 0..GRID_ROWS {
            print!("{:2} ", row);
            
            for col in 0..GRID_COLS {
                match self.grid[row][col] {
                    Some(_hwnd) => {
                        print!("## ");
                    }
                    None => {
                        print!(".. ");
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
                if self.grid[row][col].is_some() {
                    print!("## ");
                } else {
                    print!(".. ");
                }
            }
            println!();
        }
        println!();
    }

    pub fn scan_existing_windows(&mut self) {
        println!("Starting window enumeration...");
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
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    
    // Safety counter to prevent infinite loops
    tracker.enum_counter += 1;
    if tracker.enum_counter > 1000 {
        println!("SAFETY: Stopping enumeration after 1000 windows");
        return 0; // Stop enumeration
    }
    
    let title = WindowTracker::get_window_title(hwnd);
    if tracker.enum_counter <= 100 || tracker.enum_counter % 100 == 0 {
        println!("Checking window #{}: {}", tracker.enum_counter, if title.is_empty() { "<No Title>" } else { &title });
    }
    
    if WindowTracker::is_manageable_window(hwnd) {
        if tracker.enum_counter <= 100 {
            println!("  -> Adding manageable window: {}", title);
        }
        if tracker.add_window(hwnd) {
            if tracker.enum_counter <= 100 {
                println!("  -> Added successfully");
            }
        } else if tracker.enum_counter <= 100 {
            println!("  -> Failed to add");
        }
    } else if tracker.enum_counter <= 100 {
        println!("  -> Skipping non-manageable window");
    }
    
    1 // Continue enumeration
}

// Windows event hook integration using SetWinEventHook
pub mod window_events {
    use super::*;
    use std::ptr;
    use winapi::um::libloaderapi::GetModuleHandleW;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::shared::windef::HHOOK;

    pub static mut WINDOW_TRACKER: Option<Arc<Mutex<WindowTracker>>> = None;
    pub static mut EVENT_HOOKS: Vec<winapi::shared::windef::HWINEVENTHOOK> = Vec::new();

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

        #[allow(static_mut_refs)]
        if let Some(tracker_arc) = &WINDOW_TRACKER {
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
                                println!("   � Updating grid display...");
                                tracker.print_grid();
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
                            println!("   📊 Updating grid display...");
                        } else {
                            println!("   → Window was not being tracked");
                        }
                        tracker.print_grid();
                    }
                    EVENT_OBJECT_LOCATIONCHANGE | EVENT_SYSTEM_FOREGROUND => {
                        println!("   → Checking if window is manageable...");
                        if WindowTracker::is_manageable_window(hwnd) {
                            println!("   → Window IS manageable, updating position...");
                            if tracker.update_window(hwnd) {
                                println!("   ✅ Window updated: {}", window_title);
                                println!("   📊 Updating grid display...");
                            } else {
                                println!("   → No significant position change detected");
                            }
                        } else {
                            println!("   → Window is NOT manageable, ignoring");
                        }
                        tracker.print_grid();
                    }
                    EVENT_SYSTEM_MINIMIZESTART => {
                        println!("   → Window minimized, removing from grid...");
                        if tracker.remove_window(hwnd) {
                            println!("   ✅ Minimized window removed from grid");
                            println!("   📊 Updating grid display...");
                            tracker.print_grid();
                        }
                    }
                    EVENT_SYSTEM_MINIMIZEEND => {
                        println!("   → Window restored, checking if should be tracked...");
                        if WindowTracker::is_manageable_window(hwnd) {
                            if tracker.add_window(hwnd) {
                                println!("   ✅ Restored window added back to grid");
                                println!("   📊 Updating grid display...");
                                tracker.print_grid();
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
            WINDOW_TRACKER = Some(tracker.clone());
            EVENT_HOOKS.clear();

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
                    EVENT_HOOKS.push(hook);
                    println!("✅ Successfully set up hook for {}", description);
                }
            }

            if EVENT_HOOKS.is_empty() {
                return Err("Failed to set up any event hooks".to_string());
            }

            println!("🚀 Successfully set up {} WinEvent hooks!", EVENT_HOOKS.len());
            println!("📢 Now listening for real-time window events across all monitors!");
            println!();

            Ok(())
        }
    }

    pub fn cleanup_hooks() {
        unsafe {
            for hook in &EVENT_HOOKS {
                UnhookWinEvent(*hook);
            }
            EVENT_HOOKS.clear();
            println!("🧹 Cleaned up all event hooks");
        }
    }
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
