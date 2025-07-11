use e_grid::window_tracker::WindowTracker;
use e_grid::EasingType;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use winapi::shared::windef::RECT;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Animate & Rotate All Windows in a 4x4 Grid Demo (WindowTracker-based)");

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

    println!("Animating {} windows into a 4x4 grid...", hwnds.len());

    // Animate windows into the grid
    for (i, hwnd) in hwnds.iter().enumerate() {
        let (row, col) = positions[i];
        if let Some(target_rect) = tracker.primary_monitor_cell_to_rect(row, col) {
            if let Err(e) = tracker.start_window_animation(
                *hwnd,
                target_rect,
                Duration::from_millis(700),
                EasingType::EaseInOut,
            ) {
                println!("⚠️ Failed to animate window 0x{:X}: {}", hwnd, e);
            }
        }
    }
    // Let the animations play out
    for _ in 0..15 {
        tracker.update_animations();
        thread::sleep(Duration::from_millis(50));
    }

    thread::sleep(Duration::from_secs(1));

    // Rotate: shift each window to the next grid cell (wrap around)
    println!("Animating rotation of windows in the grid...");

    // Number of rotations needed to return to original positions
    let num_rotations = hwnds.len();
    let animation_duration = Duration::from_millis(700);
    let animation_steps = (animation_duration.as_millis() / 50) as usize; // 50ms per step

    let mut rotated = hwnds.clone();
    for _ in 0..num_rotations {
        rotated.rotate_right(1);

        for (i, hwnd) in rotated.iter().enumerate() {
            let (row, col) = positions[i];
            if let Some(target_rect) = tracker.primary_monitor_cell_to_rect(row, col) {
                if let Err(e) = tracker.start_window_animation(
                    *hwnd,
                    target_rect,
                    animation_duration,
                    EasingType::EaseInOut,
                ) {
                    println!("⚠️ Failed to animate window 0x{:X}: {}", hwnd, e);
                }
            }
        }
        for _ in 0..animation_steps {
            tracker.update_animations();
            thread::sleep(Duration::from_millis(50));
        }
    }

    thread::sleep(Duration::from_secs(1));

    // Restore original positions with animation
    println!("Restoring windows to original positions (animated)...");
    for hwnd in &hwnds {
        if let Some(rect) = original_rects.get(hwnd) {
            if let Err(e) = tracker.start_window_animation(
                *hwnd,
                *rect,
                Duration::from_millis(700),
                EasingType::EaseInOut,
            ) {
                println!("⚠️ Failed to animate window 0x{:X}: {}", hwnd, e);
            }
        }
    }
    for _ in 0..15 {
        tracker.update_animations();
        thread::sleep(Duration::from_millis(50));
    }

    println!("✅ Animated demo complete!");
    Ok(())
}
