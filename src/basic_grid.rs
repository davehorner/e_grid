use std::collections::HashMap;
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::*;

// Grid configuration
const GRID_ROWS: usize = 8;
const GRID_COLS: usize = 12;

#[derive(Clone)]
struct WindowInfo {
    title: String,
    rect: RECT,
    grid_cells: Vec<(usize, usize)>,
}

struct SimpleTracker {
    windows: HashMap<HWND, WindowInfo>,
    monitor_rect: RECT,
    grid: [[Option<HWND>; GRID_COLS]; GRID_ROWS],
    enum_counter: usize,
}

impl SimpleTracker {
    fn new() -> Self {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };

        unsafe {
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut _ as *mut _, 0);
        }
        Self {
            windows: HashMap::new(),
            monitor_rect: rect,
            grid: [[None; GRID_COLS]; GRID_ROWS],
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
                "<No Title>".to_string()
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

            if (ex_style & WS_EX_TOOLWINDOW) != 0 && (ex_style & WS_EX_APPWINDOW) == 0 {
                return false;
            }

            let title = Self::get_window_title(hwnd);
            if title == "<No Title>" || title.is_empty() {
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
            println!("      Skipping window with invalid coordinates");
            return cells;
        }

        let cell_width = (self.monitor_rect.right - self.monitor_rect.left) / GRID_COLS as i32;
        let cell_height = (self.monitor_rect.bottom - self.monitor_rect.top) / GRID_ROWS as i32;

        if cell_width <= 0 || cell_height <= 0 {
            println!("      Invalid cell dimensions");
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
            println!("      Window outside grid bounds");
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
        println!("    Getting window rect...");
        if let Some(rect) = Self::get_window_rect(hwnd) {
            println!(
                "    Got rect: ({},{}) {}x{}",
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top
            );

            let title = Self::get_window_title(hwnd);
            println!("    Computing grid cells...");
            let grid_cells = self.window_to_grid_cells(&rect);
            println!("    Computed {} grid cells", grid_cells.len());

            println!(
                "Adding window: {} at ({},{}) {}x{} -> {} cells",
                title,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                grid_cells.len()
            );

            let window_info = WindowInfo {
                title,
                rect,
                grid_cells,
            };

            println!("    Inserting into HashMap...");
            self.windows.insert(hwnd, window_info);
            println!("    Updating grid...");
            self.update_grid();
            println!("    Window added successfully");
            return true;
        } else {
            println!("    Failed to get window rect");
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

        // Print grid
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
            println!(
                "## {} ({} cells)",
                window_info.title,
                window_info.grid_cells.len()
            );
        }

        println!();
    }
    fn scan_windows(&mut self) {
        println!("Starting window scan...");
        self.enum_counter = 0; // Reset counter

        unsafe {
            let result = EnumWindows(Some(simple_enum_proc), self as *mut _ as LPARAM);
            println!("EnumWindows returned: {}", result);
        }

        println!(
            "Window scan completed. Found {} windows.",
            self.windows.len()
        );
    }
}

unsafe extern "system" fn simple_enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut SimpleTracker);

    // Safety counter to prevent infinite loops
    tracker.enum_counter += 1;
    if tracker.enum_counter > 1000 {
        println!("SAFETY: Stopping enumeration after 1000 windows");
        return 0; // Stop enumeration
    }

    // Add debug output to see where it might hang
    let title = SimpleTracker::get_window_title(hwnd);
    println!("Checking window #{}: {}", tracker.enum_counter, title);

    if SimpleTracker::is_manageable_window(hwnd) {
        println!("  -> Manageable, adding...");
        if tracker.add_window(hwnd) {
            println!("  -> Added successfully");
        } else {
            println!("  -> Failed to add");
        }
    } else {
        println!("  -> Not manageable, skipping");
    }

    1 // Continue enumeration
}

fn main() {
    println!("Basic Grid Display Test");
    println!("=======================");

    let mut tracker = SimpleTracker::new();

    println!(
        "Monitor area: {}x{} px",
        tracker.monitor_rect.right - tracker.monitor_rect.left,
        tracker.monitor_rect.bottom - tracker.monitor_rect.top
    );

    tracker.scan_windows();
    tracker.print_grid();

    println!("Press Enter to exit...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}
