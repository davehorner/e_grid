use e_grid::window_tracker::WindowTracker;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use winapi::shared::windef::RECT;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Move & Rotate All Windows in a 4x4 Grid Demo (WindowTracker-based)");

    // Create and enumerate windows
    let mut tracker = WindowTracker::new();
    tracker.set_grid_size(4, 4);
    tracker.scan_existing_windows();

    // Collect manageable, visible, non-minimized windows
    let mut hwnds: Vec<u64> = tracker
        .windows
        .iter()
        .filter(|entry| {
            let info = entry.value();
            info.is_visible && !info.is_minimized && WindowTracker::is_manageable_window(info.hwnd)
        })
        .map(|entry| *entry.key())
        .collect();

    if hwnds.is_empty() {
        println!("No windows to move!");
        return Ok(());
    }

    // Save original positions
    let mut original_rects: HashMap<u64, RECT> = HashMap::new();
    for hwnd in &hwnds {
        if let Some(rect) = WindowTracker::get_window_rect(*hwnd) {
            original_rects.insert(*hwnd, rect);
        }
    }

    // 4x4 grid
    let grid_rows = 4;
    let grid_cols = 4;
    let mut positions: Vec<(usize, usize)> = Vec::new();
    for row in 0..grid_rows {
        for col in 0..grid_cols {
            positions.push((row, col));
        }
    }

    // Only move as many windows as fit in the grid
    let count = hwnds.len().min(positions.len());
    hwnds.truncate(count);

    println!("Moving {} windows into a 4x4 grid...", hwnds.len());

    // Move windows into the grid
    for (i, hwnd) in hwnds.iter().enumerate() {
        let (row, col) = positions[i];
        if let Err(e) = tracker.move_window_to_cell(*hwnd, row, col) {
            println!("⚠️ Failed to move window 0x{:X}: {}", hwnd, e);
        }
    }

    thread::sleep(Duration::from_secs(2));

    // Rotate: shift each window to the next grid cell (wrap around)
    println!("Rotating windows in the grid...");
    let mut rotated = hwnds.clone();
    rotated.rotate_right(1);

    for (i, hwnd) in rotated.iter().enumerate() {
        let (row, col) = positions[i];
        if let Err(e) = tracker.move_window_to_cell(*hwnd, row, col) {
            println!("⚠️ Failed to move window 0x{:X}: {}", hwnd, e);
        }
    }

    thread::sleep(Duration::from_secs(2));

    // Restore original positions
    println!("Restoring windows to original positions...");
    for hwnd in &hwnds {
        if let Some(rect) = original_rects.get(hwnd) {
            tracker.move_window_to_rect(*hwnd, *rect)?;
        }
    }

    println!("✅ Demo complete!");
    Ok(())
}
