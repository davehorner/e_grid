use e_grid::{WindowTracker, EasingType, WindowAnimation};
use std::time::Duration;
use std::collections::HashMap;
use std::thread;
use std::ptr;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{EnumWindows, GetWindowRect, IsWindowVisible, GetWindowTextW, SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE};
use winapi::shared::minwindef::{LPARAM, TRUE};
use rand::Rng;

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
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
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

fn calculate_4x4_grid_position(index: usize, monitor_rect: RECT) -> RECT {
    let row = index / 4;
    let col = index % 4;
    
    let width = (monitor_rect.right - monitor_rect.left) / 4;
    let height = (monitor_rect.bottom - monitor_rect.top) / 4;
    
    RECT {
        left: monitor_rect.left + (col as i32 * width),
        top: monitor_rect.top + (row as i32 * height),
        right: monitor_rect.left + ((col + 1) as i32 * width),
        bottom: monitor_rect.top + ((row + 1) as i32 * height),
    }
}

fn main() {
    println!("🧪 Testing E-Grid with Real Window Animation...");
    
    // Initialize window tracker
    let mut tracker = WindowTracker::new();
    println!("✅ WindowTracker created successfully");
    
    // Get all visible windows and save their original positions
    let windows = get_visible_windows();
    let mut original_positions: HashMap<HWND, RECT> = HashMap::new();
    
    println!("📋 Found {} visible windows:", windows.len());
    for (i, (hwnd, rect, title)) in windows.iter().enumerate() {
        if i < 16 { // Only use first 16 windows for 4x4 grid
            original_positions.insert(*hwnd, *rect);
            println!("  {}. {} [{},{} {}x{}]", 
                i + 1, 
                if title.len() > 30 { &title[..30] } else { title },
                rect.left, rect.top,
                rect.right - rect.left, rect.bottom - rect.top
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
        let (left, top, right, bottom) = tracker.monitor_grids[1].monitor_rect;
        RECT { left, top, right, bottom }
    } else {
        let (left, top, right, bottom) = tracker.monitor_grids[0].monitor_rect;
        RECT { left, top, right, bottom }
    };
    
    println!("🖥️ Using monitor bounds: [{},{} {}x{}]", 
        monitor_1_rect.left, monitor_1_rect.top,
        monitor_1_rect.right - monitor_1_rect.left,
        monitor_1_rect.bottom - monitor_1_rect.top
    );
    
    // Phase 1: Move windows to 4x4 grid with random animations
    println!("\n🎬 Phase 1: Moving windows to 4x4 grid with random animations...");
    
    let easing_types = [
        EasingType::Linear,
        EasingType::EaseIn,
        EasingType::EaseOut,
        EasingType::EaseInOut,
        EasingType::Bounce,
        EasingType::Elastic,
        EasingType::Back,
    ];
    
    let mut rng = rand::thread_rng();    let mut animations = Vec::new();
    let windows_list: Vec<_> = windows.iter().take(16).collect(); // Keep reference to original list
    
    for (i, (hwnd, original_rect)) in original_positions.iter().enumerate() {
        if i >= 16 { break; } // Only 16 windows for 4x4 grid
        
        let title = &windows_list[i].2; // Get title from original list
        let grid_rect = calculate_4x4_grid_position(i, monitor_1_rect);
        let duration_ms = rng.gen_range(1000..=3000); // 1-3 seconds
        let easing_type = easing_types[rng.gen_range(0..easing_types.len())];
        
        println!("  Moving '{}' to position [{},{}] with {:?} easing for {}ms", 
            if title.len() > 20 { &title[..20] } else { title },
            grid_rect.left, grid_rect.top,
            easing_type, duration_ms
        );
        
        // Create animation
        let animation = WindowAnimation::new(
            *hwnd,
            *original_rect,
            grid_rect,
            Duration::from_millis(duration_ms),
            easing_type
        );
        
        animations.push(animation);
        
        // Start the animation by moving window to start position and then animate
        tracker.start_window_animation(*hwnd, grid_rect, Duration::from_millis(duration_ms), easing_type)
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
                        animation.hwnd,
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
        
        println!("  Restoring window to [{},{}] with {}ms animation", 
            original_rect.left, original_rect.top, duration_ms);
        
        // Create return animation
        let mut current_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe {
            GetWindowRect(*hwnd, &mut current_rect);
        }
        
        let return_animation = WindowAnimation::new(
            *hwnd,
            current_rect,
            *original_rect,
            Duration::from_millis(duration_ms),
            easing_type
        );
        
        // Animate back to original position
        let animation_start = std::time::Instant::now();
        while !return_animation.is_completed() && animation_start.elapsed().as_millis() < duration_ms as u128 + 100 {
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
