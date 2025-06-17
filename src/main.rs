use crossterm::{
    cursor, execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use std::collections::HashMap;
use std::io::{self, Write};
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
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

#[derive(Clone)]
struct Monitor {
    rect: RECT,
    grid: [[Option<HWND>; GRID_COLS]; GRID_ROWS],
}

struct WindowTracker {
    monitors: Vec<Monitor>,
    windows: HashMap<HWND, WindowInfo>,
    shell_hook_id: UINT,
}

impl WindowTracker {
    fn new() -> Self {
        Self {
            monitors: Vec::new(),
            windows: HashMap::new(),
            shell_hook_id: 0,
        }
    }

    fn initialize_monitors(&mut self) {
        // Get primary monitor for simplicity
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        
        unsafe {
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0);
        }

        let monitor = Monitor {
            rect,
            grid: [[None; GRID_COLS]; GRID_ROWS],
        };
        
        self.monitors.push(monitor);
    }

    fn get_window_title(hwnd: HWND) -> String {
        unsafe {
            let mut buffer = [0u16; 256];
            let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if len > 0 {
                String::from_utf16_lossy(&buffer[..len as usize])
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
    }    fn is_manageable_window(hwnd: HWND) -> bool {
        unsafe {
            if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
                return false;
            }

            let _style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            
            // Skip tool windows, but include app windows
            if (ex_style & WS_EX_TOOLWINDOW) != 0 && (ex_style & WS_EX_APPWINDOW) == 0 {
                return false;
            }

            // Must have a title
            let title = Self::get_window_title(hwnd);
            if title.is_empty() {
                return false;
            }

            // Skip certain system windows
            if title.contains("Program Manager") || title.contains("Task Switching") {
                return false;
            }

            true
        }
    }

    fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        if self.monitors.is_empty() {
            return Vec::new();
        }

        let monitor = &self.monitors[0];
        let mut cells = Vec::new();

        let cell_width = (monitor.rect.right - monitor.rect.left) / GRID_COLS as i32;
        let cell_height = (monitor.rect.bottom - monitor.rect.top) / GRID_ROWS as i32;

        // Calculate which cells this window covers
        let start_col = ((rect.left - monitor.rect.left) / cell_width).max(0) as usize;
        let end_col = ((rect.right - monitor.rect.left) / cell_width).min(GRID_COLS as i32 - 1) as usize;
        let start_row = ((rect.top - monitor.rect.top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom - monitor.rect.top) / cell_height).min(GRID_ROWS as i32 - 1) as usize;

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
        if self.monitors.is_empty() {
            return;
        }

        // Clear the grid
        self.monitors[0].grid = [[None; GRID_COLS]; GRID_ROWS];

        // Update grid with current windows
        for (hwnd, window_info) in &self.windows {
            for (row, col) in &window_info.grid_cells {
                if *row < GRID_ROWS && *col < GRID_COLS {
                    self.monitors[0].grid[*row][*col] = Some(*hwnd);
                }
            }
        }
    }

    fn add_window(&mut self, hwnd: HWND) {
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
        }
    }

    fn remove_window(&mut self, hwnd: HWND) {
        self.windows.remove(&hwnd);
        self.update_grid();
    }    fn update_window(&mut self, hwnd: HWND) {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let grid_cells = self.window_to_grid_cells(&rect);
            if let Some(window_info) = self.windows.get_mut(&hwnd) {
                window_info.rect = rect;
                window_info.grid_cells = grid_cells;
                self.update_grid();
            }
        }
    }

    fn print_grid(&self) -> io::Result<()> {
        let mut stdout = io::stdout();
        
        queue!(stdout, cursor::MoveTo(0, 0))?;
        queue!(stdout, Clear(ClearType::All))?;
        
        // Print header
        queue!(stdout, SetForegroundColor(Color::Cyan))?;
        queue!(stdout, Print(format!("Window Grid Tracker - {}x{} Grid\n", GRID_ROWS, GRID_COLS)))?;
        queue!(stdout, Print("═".repeat(50)))?;
        queue!(stdout, Print("\n"))?;
        queue!(stdout, ResetColor)?;

        if self.monitors.is_empty() {
            queue!(stdout, Print("No monitors detected\n"))?;
            stdout.flush()?;
            return Ok(());
        }

        let monitor = &self.monitors[0];
        
        // Print column numbers
        queue!(stdout, Print("   "))?;
        for col in 0..GRID_COLS {
            queue!(stdout, Print(format!("{:2} ", col)))?;
        }
        queue!(stdout, Print("\n"))?;

        // Print grid
        for row in 0..GRID_ROWS {
            queue!(stdout, Print(format!("{:2} ", row)))?;
            
            for col in 0..GRID_COLS {
                match monitor.grid[row][col] {
                    Some(hwnd) => {
                        // Get a color based on the HWND value
                        let color_index = (hwnd as usize) % 6;
                        let color = match color_index {
                            0 => Color::Red,
                            1 => Color::Green,
                            2 => Color::Blue,
                            3 => Color::Yellow,
                            4 => Color::Magenta,
                            _ => Color::White,
                        };
                        queue!(stdout, SetBackgroundColor(color))?;
                        queue!(stdout, SetForegroundColor(Color::Black))?;
                        queue!(stdout, Print("██ "))?;
                        queue!(stdout, ResetColor)?;
                    }
                    None => {
                        queue!(stdout, Print("·· "))?;
                    }
                }
            }
            queue!(stdout, Print("\n"))?;
        }

        // Print window list
        queue!(stdout, Print("\n"))?;
        queue!(stdout, SetForegroundColor(Color::Yellow))?;
        queue!(stdout, Print("Active Windows:\n"))?;
        queue!(stdout, ResetColor)?;
        
        for (i, (hwnd, window_info)) in self.windows.iter().enumerate() {
            if i < 10 { // Limit display to avoid clutter
                let color_index = (*hwnd as usize) % 6;
                let color = match color_index {
                    0 => Color::Red,
                    1 => Color::Green,
                    2 => Color::Blue,
                    3 => Color::Yellow,
                    4 => Color::Magenta,
                    _ => Color::White,
                };
                
                queue!(stdout, SetForegroundColor(color))?;
                queue!(stdout, Print("██ "))?;
                queue!(stdout, ResetColor)?;
                queue!(stdout, Print(format!("{} (cells: {})\n", 
                    window_info.title, 
                    window_info.grid_cells.len())))?;
            }
        }

        stdout.flush()?;
        Ok(())
    }

    fn scan_existing_windows(&mut self) {
        unsafe {
            EnumWindows(Some(enum_windows_proc), self as *mut _ as LPARAM);
        }
    }
}

static mut WINDOW_TRACKER: Option<Arc<Mutex<WindowTracker>>> = None;

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    
    if WindowTracker::is_manageable_window(hwnd) {
        tracker.add_window(hwnd);
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
                    let register_shell_hook: Option<unsafe extern "system" fn(HWND) -> i32> =
                        mem::transmute(GetProcAddress(
                            GetModuleHandleW(ptr::null()),
                            b"RegisterShellHookWindow\0".as_ptr() as *const i8,
                        ));
                    
                    if let Some(register_fn) = register_shell_hook {
                        register_fn(hwnd);
                    }
                    
                    tracker.shell_hook_id = RegisterWindowMessageW(
                        "SHELLHOOK\0".encode_utf16().collect::<Vec<u16>>().as_ptr()
                    );
                }
                _ if msg == tracker.shell_hook_id => {
                    let window_hwnd = lparam as HWND;
                    let event = wparam & 0x7fff;                    match event as u32 {
                        1 => { // HSHELL_WINDOWCREATED
                            if WindowTracker::is_manageable_window(window_hwnd) {
                                tracker.add_window(window_hwnd);
                                let _ = tracker.print_grid();
                            }
                        }
                        2 => { // HSHELL_WINDOWDESTROYED
                            tracker.remove_window(window_hwnd);
                            let _ = tracker.print_grid();
                        }
                        4 => { // HSHELL_WINDOWACTIVATED
                            if WindowTracker::is_manageable_window(window_hwnd) {
                                tracker.update_window(window_hwnd);
                                let _ = tracker.print_grid();
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

fn main() -> io::Result<()> {
    println!("Starting Window Grid Tracker...");
    
    // Initialize terminal
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;

    println!("Initializing window tracker...");
    
    // Initialize window tracker
    let mut tracker = WindowTracker::new();
    tracker.initialize_monitors();
    
    println!("Scanning existing windows...");
    tracker.scan_existing_windows();
    
    println!("Found {} windows", tracker.windows.len());
    tracker.print_grid()?;

    let tracker_arc = Arc::new(Mutex::new(tracker));
    
    unsafe {
        WINDOW_TRACKER = Some(tracker_arc.clone());
        
        println!("Registering window class...");
        
        // Register window class
        let class_name: Vec<u16> = "GridTrackerClass\0".encode_utf16().collect();
        
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
            lpszClassName: class_name.as_ptr(),
        };
        
        if RegisterClassW(&wc) == 0 {
            println!("Failed to register window class");
            return Ok(());
        }
        
        println!("Creating message window...");
        
        // Create hidden window to receive messages
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            "Grid Tracker\0".encode_utf16().collect::<Vec<u16>>().as_ptr(),
            0,
            0, 0, 0, 0,
            ptr::null_mut(),
            ptr::null_mut(),
            GetModuleHandleW(ptr::null()),
            ptr::null_mut(),
        );
        
        if hwnd.is_null() {
            println!("Failed to create window");
            return Ok(());
        }
        
        println!("Window created successfully");
        
        // Message loop
        let mut msg = MSG {
            hwnd: ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: winapi::shared::windef::POINT { x: 0, y: 0 },
        };
        
        println!("Grid tracker started. Press Ctrl+C to exit...");
        
        // Periodically update the display
        let mut last_update = std::time::Instant::now();
        let mut loop_count = 0;
        
        loop {
            let result = PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE);
            
            if result != 0 {
                if msg.message == WM_QUIT {
                    println!("Received WM_QUIT, exiting...");
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            
            // Update display every 500ms
            if last_update.elapsed().as_millis() > 500 {
                if let Ok(mut tracker) = tracker_arc.try_lock() {
                    // Re-scan all windows to catch moves/resizes
                    let current_windows: Vec<HWND> = tracker.windows.keys().cloned().collect();
                    for hwnd in current_windows {
                        if WindowTracker::is_manageable_window(hwnd) {
                            tracker.update_window(hwnd);
                        } else {
                            tracker.remove_window(hwnd);
                        }
                    }
                    let _ = tracker.print_grid();
                }
                last_update = std::time::Instant::now();
                loop_count += 1;
                
                if loop_count > 20 { // Exit after 10 seconds for testing
                    println!("Test timeout reached, exiting...");
                    break;
                }
            }
            
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        
        println!("Cleaning up...");
    }
    
    // Cleanup
    terminal::disable_raw_mode()?;
    println!("Grid tracker stopped.");
    Ok(())
}
