use e_grid::{EasingType, WindowAnimation, WindowTracker};
use rand::Rng;
use std::collections::HashMap;
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

fn calculate_grid_position(
    window_index: usize,
    grid_rows: usize,
    grid_cols: usize,
    monitor_rect: RECT,
) -> RECT {
    let row = window_index / grid_cols; // Dynamic grid columns
    let col = window_index % grid_cols; // Dynamic grid columns

    if row >= grid_rows {
        // If we exceed the grid, place in the last available cell
        let row = grid_rows - 1;
        let col = grid_cols - 1;
    }

    let grid_width = monitor_rect.right - monitor_rect.left;
    let grid_height = monitor_rect.bottom - monitor_rect.top;
    let cell_width = grid_width / grid_cols as i32;
    let cell_height = grid_height / grid_rows as i32;

    // Add padding to prevent windows from touching edges/each other
    let padding = 20;
    let window_width = cell_width - (padding * 2);
    let window_height = cell_height - (padding * 2);

    RECT {
        left: monitor_rect.left + (col as i32 * cell_width) + padding,
        top: monitor_rect.top + (row as i32 * cell_height) + padding,
        right: monitor_rect.left + (col as i32 * cell_width) + padding + window_width,
        bottom: monitor_rect.top + (row as i32 * cell_height) + padding + window_height,
    }
}

fn determine_optimal_grid_size(window_count: usize) -> (usize, usize) {
    // Choose grid size based on number of windows
    match window_count {
        1..=4 => (2, 2),   // 2x2 grid for 1-4 windows
        5..=6 => (2, 3),   // 2x3 grid for 5-6 windows
        7..=9 => (3, 3),   // 3x3 grid for 7-9 windows
        10..=12 => (3, 4), // 3x4 grid for 10-12 windows
        13..=16 => (4, 4), // 4x4 grid for 13-16 windows
        17..=20 => (4, 5), // 4x5 grid for 17-20 windows        21..=24 => (4, 6),  // 4x6 grid for 21-24 windows
        _ => {
            // For more windows, use a larger grid
            (8, 12) // Default grid size
        }
    }
}

fn main() {
    println!("🧪 Testing E-Grid with Real Window Animation...");

    // Get all visible windows and save their original positions
    let windows = get_visible_windows();
    let mut original_positions: HashMap<HWND, RECT> = HashMap::new();

    println!("📋 Found {} visible windows:", windows.len());

    // Determine optimal grid size based on number of windows
    let window_count = windows.len().min(24); // Increase limit for better testing
    let (grid_rows, grid_cols) = determine_optimal_grid_size(window_count);
    let grid_config = e_grid::grid::GridConfig::new(grid_rows, grid_cols);
    let mut tracker = WindowTracker::new_with_config(grid_config);
    println!(
        "✅ WindowTracker created with {}x{} dynamic grid",
        grid_rows, grid_cols
    );
    let max_windows = grid_rows * grid_cols;

    println!(
        "🎯 Using {}x{} grid for {} windows",
        grid_rows,
        grid_cols,
        window_count.min(max_windows)
    );

    for (i, (hwnd, rect, title)) in windows.iter().enumerate() {
        if i < max_windows {
            // Use calculated grid capacity
            original_positions.insert(*hwnd, *rect);
            println!(
                "  {}. {} [{},{} {}x{}]",
                i + 1,
                if title.len() > 30 {
                    &title[..30]
                } else {
                    title
                },
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top
            );
        }
    }

    if original_positions.is_empty() {
        println!("❌ No suitable windows found for testing");
        return;
    }

    // Get monitor 1 bounds (assuming it exists)
    if tracker.monitor_grids.is_empty() {
        println!("❌ No monitors found");
        return;
    }

    let monitor_1_rect = if tracker.monitor_grids.len() > 1 {
        let rect = &tracker.monitor_grids[1].monitor_rect;
        RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    } else {
        let rect = &tracker.monitor_grids[0].monitor_rect;
        RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    };

    println!(
        "🖥️ Using monitor bounds: [{},{} {}x{}]",
        monitor_1_rect.left,
        monitor_1_rect.top,
        monitor_1_rect.right - monitor_1_rect.left,
        monitor_1_rect.bottom - monitor_1_rect.top
    ); // Phase 1: Move windows to dynamic grid with random animations
    println!(
        "\n🎬 Phase 1: Moving windows to {}x{} grid with random animations...",
        grid_rows, grid_cols
    );
    println!("📐 Grid Layout Preview:");
    // Dynamic grid preview
    let mut line = String::from("   ┌");
    for col in 0..grid_cols {
        line.push_str("─────");
        if col < grid_cols - 1 {
            line.push('┬');
        }
    }
    line.push('┐');
    println!("{}", line);

    for row in 0..grid_rows {
        let mut line = String::from("   │");
        for col in 0..grid_cols {
            let index = row * grid_cols + col;
            if index < original_positions.len() {
                line.push_str(&format!(" {:2}  │", index + 1));
            } else {
                line.push_str("  -  │");
            }
        }
        println!("{}", line);
        if row < grid_rows - 1 {
            let mut line = String::from("   ├");
            for col in 0..grid_cols {
                line.push_str("─────");
                if col < grid_cols - 1 {
                    line.push('┼');
                }
            }
            line.push('┤');
            println!("{}", line);
        }
    }

    let mut line = String::from("   └");
    for col in 0..grid_cols {
        line.push_str("─────");
        if col < grid_cols - 1 {
            line.push('┴');
        }
    }
    line.push('┘');
    println!("{}", line);
    println!();

    let easing_types = [
        EasingType::Linear,
        EasingType::EaseIn,
        EasingType::EaseOut,
        EasingType::EaseInOut,
        EasingType::Bounce,
        EasingType::Elastic,
        EasingType::Back,
    ];

    let mut rng = rand::thread_rng();
    let mut animations = Vec::new();
    let windows_list: Vec<_> = windows.iter().take(max_windows).collect(); // Use calculated max
    for (i, (hwnd, original_rect)) in original_positions.iter().enumerate() {
        if i >= max_windows {
            break;
        } // Use calculated grid capacity

        let title = &windows_list[i].2; // Get title from original list
        let grid_rect = calculate_grid_position(i, grid_rows, grid_cols, monitor_1_rect);
        let duration_ms = rng.gen_range(1000..=3000); // 1-3 seconds
        let easing_type = easing_types[rng.gen_range(0..easing_types.len())];
        let row = i / grid_cols;
        let col = i % grid_cols;

        println!(
            "  📍 Window {}: '{}' -> Grid[{},{}] at [{},{}] ({:?}, {}ms)",
            i + 1,
            {
                let t = title;
                if t.chars().count() > 20 {
                    t.chars().take(20).collect::<String>()
                } else {
                    t.clone()
                }
            },
            row,
            col,
            grid_rect.left,
            grid_rect.top,
            easing_type,
            duration_ms
        );

        // Create animation
        let animation = WindowAnimation::new(
            *hwnd as u64,
            *original_rect,
            grid_rect,
            Duration::from_millis(duration_ms),
            easing_type,
        );

        animations.push(animation);

        // Start the animation by moving window to start position and then animate
        tracker
            .start_window_animation(
                *hwnd as u64,
                grid_rect,
                Duration::from_millis(duration_ms),
                easing_type,
            )
            .unwrap_or_else(|e| println!("⚠️ Warning: {}", e));
    }

    // Wait for animations to complete
    println!("⏳ Waiting for animations to complete...");
    let mut max_duration = 0;
    for animation in &animations {
        let duration_ms = animation.duration.as_millis() as u64;
        if duration_ms > max_duration {
            max_duration = duration_ms;
        }
    }

    // Animate the windows manually since we're testing without full IPC
    let start_time = std::time::Instant::now();
    while start_time.elapsed().as_millis() < max_duration as u128 + 500 {
        for animation in &animations {
            if !animation.is_completed() {
                let current_rect = animation.get_current_rect();
                unsafe {
                    SetWindowPos(
                        animation.hwnd as HWND,
                        ptr::null_mut(),
                        current_rect.left,
                        current_rect.top,
                        current_rect.right - current_rect.left,
                        current_rect.bottom - current_rect.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }

    println!("✅ Grid animations completed!");

    // Phase 2: Wait a moment, then restore original positions
    println!("\n⏸️ Holding grid formation for 2 seconds...");
    thread::sleep(Duration::from_secs(2));

    println!("🔄 Phase 2: Restoring windows to original positions...");

    for (hwnd, original_rect) in original_positions.iter() {
        let duration_ms = rng.gen_range(800..=1500); // Faster return
        let easing_type = EasingType::EaseInOut; // Smooth return

        println!(
            "  Restoring window to [{},{}] with {}ms animation",
            original_rect.left, original_rect.top, duration_ms
        );

        // Create return animation
        let mut current_rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            GetWindowRect(*hwnd, &mut current_rect);
        }

        let return_animation = WindowAnimation::new(
            *hwnd as u64,
            current_rect,
            *original_rect,
            Duration::from_millis(duration_ms),
            easing_type,
        );

        // Animate back to original position
        let animation_start = std::time::Instant::now();
        while !return_animation.is_completed()
            && animation_start.elapsed().as_millis() < duration_ms as u128 + 100
        {
            let animated_rect = return_animation.get_current_rect();
            unsafe {
                SetWindowPos(
                    *hwnd,
                    ptr::null_mut(),
                    animated_rect.left,
                    animated_rect.top,
                    animated_rect.right - animated_rect.left,
                    animated_rect.bottom - animated_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            thread::sleep(Duration::from_millis(16)); // ~60 FPS
        }
    }

    println!("✅ All windows restored to original positions!");

    println!("\n🎉 Animation test completed successfully!");
    println!("✅ Demonstrated:");
    println!("   📤 Real window position capture");
    println!("   🎯 4x4 grid layout calculation");
    println!("   🎬 Random duration animations (1-3s)");
    println!("   🎨 Random easing functions");
    println!("   🔄 Smooth restoration to original positions");
    println!("   🖥️ Monitor-specific positioning");
}
