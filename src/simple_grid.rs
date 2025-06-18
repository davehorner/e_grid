use std::collections::HashMap;
use std::ptr;
use std::sync::{Arc, Mutex};
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::libloaderapi::{GetModuleHandleW, GetProcAddress};
use winapi::um::winuser::*;

// Grid configuration
const GRID_ROWS: usize = 8;
const GRID_COLS: usize = 12;

#[derive(Clone)]
#[allow(dead_code)]
struct WindowInfo {
    hwnd: HWND,
    title: String,
    rect: RECT,
    grid_cells: Vec<(usize, usize)>,
}

struct WindowTracker {
    windows: HashMap<HWND, WindowInfo>,
    monitor_rect: RECT,
    grid: [[Option<HWND>; GRID_COLS]; GRID_ROWS],
    shell_hook_id: UINT,
    enum_counter: usize,
}

impl WindowTracker {
    fn new() -> Self {
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
            shell_hook_id: 0,
            enum_counter: 0,
        }
    }

    fn get_window_title(hwnd: HWND) -> String {
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

    fn get_window_rect(hwnd: HWND) -> Option<RECT> {
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

    fn is_manageable_window(hwnd: HWND) -> bool {
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
                || title.contains("Windows Input Experience")
            {
                return false;
            }

            true
        }
    }
    fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates (like minimized windows)
        if rect.left < -30000
            || rect.top < -30000
            || rect.right < rect.left
            || rect.bottom < rect.top
        {
            return cells;
        }

        let cell_width = (self.monitor_rect.right - self.monitor_rect.left) / GRID_COLS as i32;
        let cell_height = (self.monitor_rect.bottom - self.monitor_rect.top) / GRID_ROWS as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return cells;
        }

        let start_col = ((rect.left - self.monitor_rect.left) / cell_width).max(0) as usize;
        let end_col =
            ((rect.right - self.monitor_rect.left) / cell_width).min(GRID_COLS as i32 - 1) as usize;
        let start_row = ((rect.top - self.monitor_rect.top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom - self.monitor_rect.top) / cell_height)
            .min(GRID_ROWS as i32 - 1) as usize;

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

    fn update_grid(&mut self) {
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
    fn add_window(&mut self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let title = Self::get_window_title(hwnd);
            let grid_cells = self.window_to_grid_cells(&rect);

            println!(
                "    Window rect: ({},{}) {}x{} -> {} cells",
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                grid_cells.len()
            );

            let window_info = WindowInfo {
                hwnd,
                title,
                rect,
                grid_cells,
            };

            self.windows.insert(hwnd, window_info);
            self.update_grid();
            return true;
        } else {
            println!("    Failed to get window rect");
        }
        false
    }

    fn remove_window(&mut self, hwnd: HWND) -> bool {
        if self.windows.remove(&hwnd).is_some() {
            self.update_grid();
            return true;
        }
        false
    }

    fn update_window(&mut self, hwnd: HWND) -> bool {
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

    fn print_grid(&self) {
        println!();
        println!("{}", "=".repeat(60));
        println!(
            "Window Grid Tracker - {}x{} Grid ({} windows)",
            GRID_ROWS,
            GRID_COLS,
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
        println!("Active Windows:");
        println!("{}", "-".repeat(60));

        for (i, (_hwnd, window_info)) in self.windows.iter().enumerate() {
            if i < 15 {
                println!(
                    "## {} ({} cells)",
                    window_info.title,
                    window_info.grid_cells.len()
                );
            }
        }

        if self.windows.len() > 15 {
            println!("... and {} more windows", self.windows.len() - 15);
        }

        println!();
        println!("Grid display completed.");
    }
    fn scan_existing_windows(&mut self) {
        println!("Starting window enumeration...");
        self.enum_counter = 0; // Reset counter
        unsafe {
            let result = EnumWindows(Some(enum_windows_proc), self as *mut _ as LPARAM);
            println!("EnumWindows completed with result: {}", result);
        }
        println!(
            "Window enumeration finished. Found {} windows.",
            self.windows.len()
        );
    }
}

static mut WINDOW_TRACKER: Option<Arc<Mutex<WindowTracker>>> = None;

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);

    // Safety counter to prevent infinite loops
    tracker.enum_counter += 1;
    if tracker.enum_counter > 1000 {
        println!("SAFETY: Stopping enumeration after 1000 windows");
        return 0; // Stop enumeration
    }

    let title = WindowTracker::get_window_title(hwnd);
    println!(
        "Checking window #{}: {}",
        tracker.enum_counter,
        if title.is_empty() {
            "<No Title>"
        } else {
            &title
        }
    );

    if WindowTracker::is_manageable_window(hwnd) {
        println!("  -> Adding manageable window: {}", title);
        if tracker.add_window(hwnd) {
            println!("  -> Added successfully");
        } else {
            println!("  -> Failed to add");
        }
    } else {
        println!("  -> Skipping non-manageable window");
    }

    1 // Continue enumeration
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    #[allow(static_mut_refs)]
    if let Some(tracker_arc) = &WINDOW_TRACKER {
        if let Ok(mut tracker) = tracker_arc.try_lock() {
            match msg {
                WM_CREATE => {
                    // Register for shell hook messages
                    let user32_handle = GetModuleHandleW(b"USER32.DLL\0".as_ptr() as *const u16);
                    if user32_handle.is_null() {
                        println!("Failed to get USER32.DLL handle");
                        return DefWindowProcW(hwnd, msg, wparam, lparam);
                    }

                    let register_shell_hook: Option<unsafe extern "system" fn(HWND) -> i32> =
                        std::mem::transmute(GetProcAddress(
                            user32_handle,
                            b"RegisterShellHookWindow\0".as_ptr() as *const i8,
                        ));

                    if let Some(register_fn) = register_shell_hook {
                        let result = register_fn(hwnd);
                        if result != 0 {
                            println!("Shell hook registered successfully");
                        } else {
                            println!("Shell hook registration failed");
                        }
                    } else {
                        println!("RegisterShellHookWindow function not found - this is normal on newer Windows versions");
                        println!(
                            "Real-time updates may not work, but periodic updates will continue"
                        );
                    }

                    tracker.shell_hook_id =
                        RegisterWindowMessageW(b"SHELLHOOK\0".as_ptr() as *const u16);
                    println!("Shell hook message ID: {}", tracker.shell_hook_id);
                }
                _ if msg == tracker.shell_hook_id => {
                    let window_hwnd = lparam as HWND;
                    let event = wparam & 0x7fff;

                    match event as u32 {
                        1 => {
                            // HSHELL_WINDOWCREATED
                            if WindowTracker::is_manageable_window(window_hwnd) {
                                if tracker.add_window(window_hwnd) {
                                    println!(
                                        "Window created: {}",
                                        WindowTracker::get_window_title(window_hwnd)
                                    );
                                    tracker.print_grid();
                                }
                            }
                        }
                        2 => {
                            // HSHELL_WINDOWDESTROYED
                            if tracker.remove_window(window_hwnd) {
                                println!("Window destroyed");
                                tracker.print_grid();
                            }
                        }
                        4 => {
                            // HSHELL_WINDOWACTIVATED
                            if WindowTracker::is_manageable_window(window_hwnd) {
                                if tracker.update_window(window_hwnd) {
                                    println!(
                                        "Window activated: {}",
                                        WindowTracker::get_window_title(window_hwnd)
                                    );
                                    tracker.print_grid();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                }
                _ => {}
            }
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn main() {
    println!("Starting Simple Grid Tracker...");

    // Initialize window tracker
    let mut tracker = WindowTracker::new();

    println!(
        "Monitor area: {}x{} px",
        tracker.monitor_rect.right - tracker.monitor_rect.left,
        tracker.monitor_rect.bottom - tracker.monitor_rect.top
    );

    println!("Scanning existing windows...");
    let start_time = std::time::Instant::now();
    tracker.scan_existing_windows();
    let scan_duration = start_time.elapsed();
    println!("Window scan completed in {:?}", scan_duration);

    println!("Found {} windows", tracker.windows.len());

    if tracker.windows.is_empty() {
        println!("No manageable windows found. This might indicate an issue.");
        println!("Press Enter to continue anyway...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
    }
    println!("Displaying initial grid...");
    tracker.print_grid();
    println!("Initial grid displayed successfully!");

    println!("Creating tracker arc...");
    let tracker_arc = Arc::new(Mutex::new(tracker));
    println!("Tracker arc created successfully!");

    println!("Setting up Windows message handling...");

    unsafe {
        WINDOW_TRACKER = Some(tracker_arc.clone());

        // Register window class
        let class_name = b"SimpleGridTrackerClass\0";

        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: GetModuleHandleW(ptr::null()),
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr() as *const u16,
        };

        let class_result = RegisterClassW(&wc);
        if class_result == 0 {
            println!("Failed to register window class, error: {}", GetLastError());
            return;
        }
        println!("Window class registered successfully");

        // Create hidden window to receive messages
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr() as *const u16,
            b"Simple Grid Tracker\0".as_ptr() as *const u16,
            0,
            0,
            0,
            0,
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            println!("Failed to create window, error: {}", GetLastError());
            return;
        }

        println!("Message window created successfully");
        println!("Starting event loop...");
        println!("Try opening/closing/moving windows to see the grid update!");
        println!("The grid will also refresh every 10 seconds.");
        println!("Press Ctrl+C to exit.");

        // Message loop with periodic updates
        let mut msg = std::mem::zeroed::<MSG>();
        let mut last_update = std::time::Instant::now();
        let mut update_count = 0;

        loop {
            let result = PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE);

            if result != 0 {
                if msg.message == WM_QUIT {
                    println!("Received quit message, exiting...");
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Periodic update every 10 seconds
            if last_update.elapsed().as_secs() >= 10 {
                if let Ok(mut tracker) = tracker_arc.try_lock() {
                    println!("Performing periodic update #{}", update_count + 1);
                    let mut changed = false;
                    let current_windows: Vec<HWND> = tracker.windows.keys().cloned().collect();

                    for hwnd in current_windows {
                        if WindowTracker::is_manageable_window(hwnd) {
                            if tracker.update_window(hwnd) {
                                changed = true;
                            }
                        } else {
                            if tracker.remove_window(hwnd) {
                                changed = true;
                            }
                        }
                    }

                    if changed {
                        println!("Windows changed, updating grid...");
                    }

                    tracker.print_grid();
                    update_count += 1;
                }
                last_update = std::time::Instant::now();
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        println!("Shutting down...");
    }
}
