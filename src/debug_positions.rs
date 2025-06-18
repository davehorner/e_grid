use e_grid::WindowTracker;
use winapi::um::winuser::*;

fn main() {
    println!("Window Position Debug Tool");
    println!("=========================");

    let mut tracker = WindowTracker::new();
    let (left, top, width, height) = tracker.get_monitor_info();
    println!(
        "Virtual Screen: {}x{} px (from {}, {} to {}, {})",
        width,
        height,
        left,
        top,
        left + width,
        top + height
    );

    println!(
        "Primary Monitor: {}x{} px",
        unsafe { GetSystemMetrics(SM_CXSCREEN) },
        unsafe { GetSystemMetrics(SM_CYSCREEN) }
    );

    println!("\nScanning windows with detailed position info...");
    tracker.scan_existing_windows();

    println!("\nDetailed Window Analysis:");
    println!("{}", "=".repeat(80));

    for (hwnd, window_info) in &tracker.windows {
        let rect = &window_info.rect;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        // Determine which monitor area the window is in
        let monitor_area = if rect.left >= 5120 {
            "SECOND MONITOR"
        } else if rect.right <= 5120 {
            "PRIMARY MONITOR"
        } else {
            "SPANS MONITORS"
        };

        println!("Window: {}", window_info.title);
        println!("  HWND: {:?}", hwnd);
        println!(
            "  Position: ({}, {}) to ({}, {})",
            rect.left, rect.top, rect.right, rect.bottom
        );
        println!("  Size: {}x{}", width, height);
        println!("  Monitor: {}", monitor_area);
        println!(
            "  Grid cells: {} cells -> {:?}",
            window_info.grid_cells.len(),
            if window_info.grid_cells.len() <= 6 {
                &window_info.grid_cells
            } else {
                &window_info.grid_cells[..6]
            }
        );
        println!();
    }

    println!("Grid visualization:");
    tracker.print_grid();

    println!("\nTo test second monitor detection:");
    println!("1. Move a window to your second monitor (right side, x > 5120)");
    println!("2. Press Enter to re-scan");

    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();

    println!("\nRe-scanning after moving window...");
    tracker.scan_existing_windows();

    println!("\nUpdated window positions:");
    for (hwnd, window_info) in &tracker.windows {
        let rect = &window_info.rect;
        if rect.left >= 5120 {
            println!(
                "SECOND MONITOR WINDOW: {} at ({}, {})",
                window_info.title, rect.left, rect.top
            );
        }
    }

    tracker.print_grid();
}
