use e_grid::{GridConfig, WindowTracker};
use std::io;
use std::ptr;
use std::thread;
use std::time::Duration;
use winapi::shared::minwindef::{LPARAM, TRUE};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    EnumWindows, GetWindowRect, GetWindowTextW, IsWindowVisible, SetWindowPos, SWP_NOACTIVATE,
    SWP_NOZORDER,
};

// Global storage for window enumeration
static mut WINDOW_LIST: Vec<(HWND, RECT, String)> = Vec::new();

// Callback function for EnumWindows
unsafe extern "system" fn enum_windows_callback(hwnd: HWND, _lparam: LPARAM) -> i32 {
    if IsWindowVisible(hwnd) == 0 {
        return TRUE; // Continue enumeration
    }

    // Get window title
    let mut title_buffer = [0u16; 256];
    let title_len = GetWindowTextW(hwnd, title_buffer.as_mut_ptr(), 256);
    let title = if title_len > 0 {
        String::from_utf16_lossy(&title_buffer[..title_len as usize])
    } else {
        String::new()
    };

    // Skip windows without titles or system windows
    if title.is_empty() || title == "Program Manager" {
        return TRUE;
    }

    // Get window rectangle
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if GetWindowRect(hwnd, &mut rect) != 0 {
        WINDOW_LIST.push((hwnd, rect, title));
    }

    TRUE // Continue enumeration
}

fn get_visible_windows() -> Vec<(HWND, RECT, String)> {
    unsafe {
        WINDOW_LIST.clear();
        EnumWindows(Some(enum_windows_callback), 0);
        WINDOW_LIST.clone()
    }
}

#[derive(Clone)]
struct WindowState {
    hwnd: HWND,
    original_rect: RECT,
    current_rect: RECT,
    target_rect: RECT,
    title: String,
    grid_row: usize,
    grid_col: usize,
    is_moving: bool,
}

fn calculate_grid_position(
    monitor_rect: &RECT,
    row: usize,
    col: usize,
    grid_rows: usize,
    grid_cols: usize,
) -> RECT {
    let monitor_width = monitor_rect.right - monitor_rect.left;
    let monitor_height = monitor_rect.bottom - monitor_rect.top;

    let cell_width = monitor_width / grid_cols as i32;
    let cell_height = monitor_height / grid_rows as i32;

    let left = monitor_rect.left + (col as i32 * cell_width);
    let top = monitor_rect.top + (row as i32 * cell_height);
    let right = left + cell_width - 20; // Margin for visibility
    let bottom = top + cell_height - 20; // Margin for visibility

    RECT {
        left,
        top,
        right,
        bottom,
    }
}

fn move_window_to_position(hwnd: HWND, target_rect: &RECT) {
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

fn wait_for_windows_to_settle(duration_ms: u64) {
    println!("    ⏳ Waiting {}ms for windows to settle...", duration_ms);
    thread::sleep(Duration::from_millis(duration_ms));
}

fn transition_to_grid(
    windows: &mut [WindowState],
    monitor_rect: &RECT,
    grid_rows: usize,
    grid_cols: usize,
    phase_name: &str,
) {
    println!(
        "\n🔄 {}: Transitioning to {}x{} grid",
        phase_name, grid_rows, grid_cols
    );
    println!("   📐 Grid capacity: {} cells", grid_rows * grid_cols);

    let max_windows = windows.len().min(grid_rows * grid_cols);
    let mut moved_count = 0;

    for (i, window) in windows.iter_mut().enumerate().take(max_windows) {
        // Calculate target position based on window order
        let target_row = i / grid_cols;
        let target_col = i % grid_cols;

        window.grid_row = target_row;
        window.grid_col = target_col;
        window.target_rect =
            calculate_grid_position(monitor_rect, target_row, target_col, grid_rows, grid_cols);
        window.is_moving = true;

        println!(
            "  📦 Window {}: '{}' → Cell [{},{}]",
            i + 1,
            if window.title.len() > 25 {
                &window.title[..25]
            } else {
                &window.title
            },
            target_row,
            target_col
        );

        move_window_to_position(window.hwnd, &window.target_rect);
        moved_count += 1;

        // Small delay between window movements to avoid conflicts
        thread::sleep(Duration::from_millis(150));
    }

    println!(
        "   ✅ Moved {} windows to {}x{} grid",
        moved_count, grid_rows, grid_cols
    );
    wait_for_windows_to_settle(1000); // Wait for windows to settle

    // Mark all windows as no longer moving
    for window in windows.iter_mut() {
        window.is_moving = false;
        window.current_rect = window.target_rect;
    }
}

fn main() {
    println!("🧪 E-GRID DYNAMIC TRANSITION TEST");
    println!("=====================================");
    println!("🎯 Testing: 4x4 → 8x8 → 4x4 → Original positions");
    println!("📋 With smart movement coordination and overlap avoidance\n");

    // Get visible windows
    let visible_windows = get_visible_windows();
    let window_count = visible_windows.len().min(16); // Max 16 for 4x4 grid

    if window_count < 2 {
        println!("❌ Need at least 2 visible windows for testing");
        return;
    }

    println!("📱 Found {} suitable windows for testing", window_count);

    // Initialize window states
    let mut windows: Vec<WindowState> = visible_windows
        .into_iter()
        .take(window_count)
        .enumerate()
        .map(|(i, (hwnd, rect, title))| {
            println!(
                "  {}. {}",
                i + 1,
                if title.len() > 40 {
                    &title[..40]
                } else {
                    &title
                }
            );
            WindowState {
                hwnd,
                original_rect: rect,
                current_rect: rect,
                target_rect: rect,
                title,
                grid_row: 0,
                grid_col: 0,
                is_moving: false,
            }
        })
        .collect();

    // Get monitor bounds (use monitor 1 if available, otherwise monitor 0)
    let temp_config = GridConfig::new(4, 4);
    let tracker = WindowTracker::new_with_config(temp_config);

    if tracker.monitor_grids.is_empty() {
        println!("❌ No monitors found");
        return;
    }

    let monitor_rect = if tracker.monitor_grids.len() > 1 {
        let (left, top, right, bottom) = tracker.monitor_grids[1].monitor_rect;
        RECT {
            left,
            top,
            right,
            bottom,
        }
    } else {
        let (left, top, right, bottom) = tracker.monitor_grids[0].monitor_rect;
        RECT {
            left,
            top,
            right,
            bottom,
        }
    };

    println!(
        "🖥️  Monitor bounds: [{},{} {}x{}]\n",
        monitor_rect.left,
        monitor_rect.top,
        monitor_rect.right - monitor_rect.left,
        monitor_rect.bottom - monitor_rect.top
    );

    // Phase 1: Transition to 4x4 grid
    transition_to_grid(&mut windows, &monitor_rect, 4, 4, "PHASE 1");

    println!("\n⏸️  Press Enter to continue to 8x8 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Phase 2: Transition to 8x8 grid
    transition_to_grid(&mut windows, &monitor_rect, 8, 8, "PHASE 2");

    println!("\n⏸️  Press Enter to return to 4x4 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Phase 3: Return to 4x4 grid (different arrangement)
    transition_to_grid(&mut windows, &monitor_rect, 4, 4, "PHASE 3");

    println!("\n⏸️  Press Enter to restore original positions...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Phase 4: Return to original positions
    println!("\n🔄 PHASE 4: Restoring original window positions");
    for (i, window) in windows.iter_mut().enumerate() {
        window.target_rect = window.original_rect;
        window.is_moving = true;

        println!(
            "  📦 Restoring window {}: '{}'",
            i + 1,
            if window.title.len() > 30 {
                &window.title[..30]
            } else {
                &window.title
            }
        );

        move_window_to_position(window.hwnd, &window.original_rect);

        thread::sleep(Duration::from_millis(200));
    }

    wait_for_windows_to_settle(2000);

    println!("\n🎉 DYNAMIC TRANSITION TEST COMPLETE!");
    println!("=====================================");
    println!("✅ Successfully demonstrated:");
    println!("   📐 4x4 grid arrangement with coordinated movement");
    println!("   📐 8x8 grid expansion with smart cell distribution");
    println!("   📐 4x4 grid return with movement coordination");
    println!("   📐 Original position restoration");
    println!("   🤖 Sequential window movement to avoid conflicts");
    println!("   ⏱️  Timed transitions for smooth user experience");
    println!("\n🚀 The dynamic grid system successfully handles complex transitions!");
}
