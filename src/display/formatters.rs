// Display formatting utilities
// Moved from the grid_display module in lib.rs

/// Format a window handle for display (last 2 digits in hex)
pub fn format_hwnd_display(hwnd: u64) -> String {
    let display_val = (hwnd % 100) as u8;
    format!("{:2X}", display_val)
}

/// Print column headers for a grid
pub fn print_column_headers(cols: usize) {
    print!("    ");
    for col in 0..cols {
        print!(" {:2}", col);
    }
    println!();
}

/// Print row prefix for grid rows
pub fn print_row_prefix(row: usize) {
    print!("{:2}: ", row);
}

/// Print virtual monitor header with bounds and resolution
pub fn print_virtual_monitor_header(
    virtual_monitor_id: usize,
    bounds: (i32, i32, i32, i32), // (left, top, right, bottom)
    window_count: usize,
) {
    let (left, top, right, bottom) = bounds;
    let width = right - left;
    let height = bottom - top;

    println!();
    println!(
        "=== VIRTUAL MONITOR {} GRID (All Monitors Combined) ===",
        virtual_monitor_id
    );
    println!(
        "Virtual bounds: ({}, {}) to ({}, {}) - Resolution: {}x{}",
        left, top, right, bottom, width, height
    );
    println!("Windows tracked: {}", window_count);
}

/// Print monitor grid header with bounds and resolution
pub fn print_monitor_header(
    monitor_id: usize,
    bounds: (i32, i32, i32, i32), // (left, top, right, bottom)
    window_count: usize,
) {
    let (left, top, right, bottom) = bounds;
    let width = right - left;
    let height = bottom - top;

    println!();
    println!("=== MONITOR {} GRID ===", monitor_id);
    println!(
        "Monitor bounds: ({}, {}) to ({}, {}) - Resolution: {}x{}",
        left, top, right, bottom, width, height
    );
    println!("Windows in this monitor: {}", window_count);
}

/// Print an empty grid cell
pub fn print_empty_cell() {
    print!(" . ");
}

/// Print off-screen grid cell
pub fn print_offscreen_cell() {
    print!(" - ");
}
