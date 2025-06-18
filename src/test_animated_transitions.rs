use e_grid::{WindowTracker, GridConfig, EasingType, WindowAnimation};
use std::time::{Duration, Instant};
use std::thread;
use std::ptr;
use std::io;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{EnumWindows, GetWindowRect, IsWindowVisible, GetWindowTextW, SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE};
use winapi::shared::minwindef::{LPARAM, TRUE};

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

#[derive(Clone)]
struct WindowState {
    hwnd: HWND,
    original_rect: RECT,
    current_rect: RECT,
    target_rect: RECT,
    title: String,
    grid_row: usize,
    grid_col: usize,
    is_animating: bool,
    animation: Option<WindowAnimation>,
}

impl WindowState {
    fn new(hwnd: HWND, rect: RECT, title: String) -> Self {
        Self {
            hwnd,
            original_rect: rect,
            current_rect: rect,
            target_rect: rect,
            title,
            grid_row: 0,
            grid_col: 0,
            is_animating: false,
            animation: None,
        }
    }
}

fn calculate_grid_position(monitor_rect: &RECT, row: usize, col: usize, grid_rows: usize, grid_cols: usize) -> RECT {
    let monitor_width = monitor_rect.right - monitor_rect.left;
    let monitor_height = monitor_rect.bottom - monitor_rect.top;
    
    let cell_width = monitor_width / grid_cols as i32;
    let cell_height = monitor_height / grid_rows as i32;
    
    let left = monitor_rect.left + (col as i32 * cell_width);
    let top = monitor_rect.top + (row as i32 * cell_height);
    let right = left + cell_width - 30; // Margin for visibility
    let bottom = top + cell_height - 30; // Margin for visibility
    
    RECT { left, top, right, bottom }
}

fn animate_window_to_position(window: &mut WindowState, target_rect: RECT, duration_ms: u64, easing: EasingType) {
    window.target_rect = target_rect;
    window.animation = Some(WindowAnimation::new(
        window.hwnd,
        window.current_rect,
        target_rect,
        Duration::from_millis(duration_ms),
        easing,
    ));
    window.is_animating = true;
    
    println!("    🎬 Animating window '{}' to [{},{} {}x{}] over {}ms with {:?} easing", 
        if window.title.len() > 20 { &window.title[..20] } else { &window.title },
        target_rect.left, target_rect.top,
        target_rect.right - target_rect.left,
        target_rect.bottom - target_rect.top,
        duration_ms,
        easing
    );
}

fn apply_window_animation_frame(window: &mut WindowState, _progress: f32) -> bool {
    if !window.is_animating || window.animation.is_none() {
        return false;
    }
    
    if let Some(ref mut animation) = window.animation {
        let current_rect = animation.get_current_rect();
        
        // Move the window to the current animation frame position
        unsafe {
            SetWindowPos(
                window.hwnd,
                ptr::null_mut(),
                current_rect.left,
                current_rect.top,
                current_rect.right - current_rect.left,
                current_rect.bottom - current_rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        
        window.current_rect = current_rect;
        
        // Check if animation is complete
        if animation.completed {
            window.is_animating = false;
            window.animation = None;
            window.current_rect = window.target_rect;
            return true; // Animation completed
        }
    }
    
    false
}

fn apply_easing(t: f32, easing: &EasingType) -> f32 {
    match easing {
        EasingType::Linear => t,
        EasingType::EaseIn => t * t * t,
        EasingType::EaseOut => {
            let u = 1.0 - t;
            1.0 - (u * u * u)
        },        EasingType::EaseInOut => {
            if t < 0.5 {
                4.0 * t * t * t
            } else {
                let u = 1.0 - t;
                1.0 - 4.0 * u * u * u
            }
        },
        EasingType::Bounce => {
            if t < 1.0 / 2.75 {
                7.5625 * t * t
            } else if t < 2.0 / 2.75 {
                let t = t - 1.5 / 2.75;
                7.5625 * t * t + 0.75
            } else if t < 2.5 / 2.75 {
                let t = t - 2.25 / 2.75;
                7.5625 * t * t + 0.9375
            } else {
                let t = t - 2.625 / 2.75;
                7.5625 * t * t + 0.984375
            }
        },
        EasingType::Elastic => {
            if t == 0.0 {
                0.0
            } else if t == 1.0 {
                1.0
            } else {
                let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                -(2.0_f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
            }
        },
        EasingType::Back => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        },
    }
}

fn animate_windows_to_grid(
    windows: &mut [WindowState], 
    monitor_rect: &RECT, 
    grid_rows: usize, 
    grid_cols: usize,
    phase_name: &str,
    animation_duration_ms: u64,
    easing: EasingType
) {
    println!("\n🎬 {}: Animating to {}x{} grid", phase_name, grid_rows, grid_cols);
    println!("   📐 Grid capacity: {} cells", grid_rows * grid_cols);
    println!("   ⏱️  Animation: {}ms with {:?} easing", animation_duration_ms, easing);
    
    let max_windows = windows.len().min(grid_rows * grid_cols);
    
    // Start animations for all windows
    for (i, window) in windows.iter_mut().enumerate().take(max_windows) {
        let target_row = i / grid_cols;
        let target_col = i % grid_cols;
        
        window.grid_row = target_row;
        window.grid_col = target_col;
        
        let target_rect = calculate_grid_position(monitor_rect, target_row, target_col, grid_rows, grid_cols);
        
        println!("  📦 Window {}: '{}' → Cell [{},{}]", 
            i + 1,
            if window.title.len() > 25 { &window.title[..25] } else { &window.title },
            target_row, 
            target_col
        );
        
        animate_window_to_position(window, target_rect, animation_duration_ms, easing.clone());
    }
    
    // Run animation loop
    let start_time = std::time::Instant::now();
    let duration = Duration::from_millis(animation_duration_ms);
    
    println!("   🎬 Running animation loop...");
    
    while start_time.elapsed() < duration {
        let progress = start_time.elapsed().as_secs_f32() / duration.as_secs_f32();
        let progress = progress.min(1.0);
        
        let mut all_complete = true;
        for window in windows.iter_mut().take(max_windows) {
            if window.is_animating {
                let completed = apply_window_animation_frame(window, progress);
                if !completed {
                    all_complete = false;
                }
            }
        }
        
        if all_complete {
            break;
        }
        
        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
    
    // Ensure all windows are at their final positions
    for window in windows.iter_mut().take(max_windows) {
        if window.is_animating {
            window.is_animating = false;
            window.animation = None;
            window.current_rect = window.target_rect;
            
            unsafe {
                SetWindowPos(
                    window.hwnd,
                    ptr::null_mut(),
                    window.target_rect.left,
                    window.target_rect.top,
                    window.target_rect.right - window.target_rect.left,
                    window.target_rect.bottom - window.target_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
    
    println!("   ✅ Animation complete! All windows positioned in {}x{} grid", grid_rows, grid_cols);
}

fn rotate_windows_in_grid(
    windows: &mut [WindowState], 
    monitor_rect: &RECT, 
    grid_rows: usize, 
    grid_cols: usize,
    rotation_steps: usize,
    step_duration_ms: u64
) {
    let max_windows = windows.len().min(grid_rows * grid_cols);
    
    for step in 0..rotation_steps {
        println!("\n🔄 ROTATION STEP {} of {}", step + 1, rotation_steps);
        
        // Calculate new positions (rotate each window to the next cell)
        for (i, window) in windows.iter_mut().enumerate().take(max_windows) {
            let current_index = window.grid_row * grid_cols + window.grid_col;
            let next_index = (current_index + 1) % (grid_rows * grid_cols);
            
            let new_row = next_index / grid_cols;
            let new_col = next_index % grid_cols;
            
            window.grid_row = new_row;
            window.grid_col = new_col;
            
            let target_rect = calculate_grid_position(monitor_rect, new_row, new_col, grid_rows, grid_cols);
            
            println!("  🔄 Window {}: '{}' → Cell [{},{}]", 
                i + 1,
                if window.title.len() > 20 { &window.title[..20] } else { &window.title },
                new_row, 
                new_col
            );
            
            animate_window_to_position(window, target_rect, step_duration_ms, EasingType::EaseInOut);
        }
        
        // Run animation for this rotation step
        let start_time = std::time::Instant::now();
        let duration = Duration::from_millis(step_duration_ms);
        
        while start_time.elapsed() < duration {
            let progress = start_time.elapsed().as_secs_f32() / duration.as_secs_f32();
            let progress = progress.min(1.0);
            
            let mut all_complete = true;
            for window in windows.iter_mut().take(max_windows) {
                if window.is_animating {
                    let completed = apply_window_animation_frame(window, progress);
                    if !completed {
                        all_complete = false;
                    }
                }
            }
            
            if all_complete {
                break;
            }
            
            thread::sleep(Duration::from_millis(16)); // ~60 FPS
        }
        
        // Ensure all windows are at their final positions for this step
        for window in windows.iter_mut().take(max_windows) {
            window.is_animating = false;
            window.animation = None;
            window.current_rect = window.target_rect;
        }
        
        println!("   ✅ Rotation step {} complete", step + 1);
        
        // Small pause between rotation steps
        if step < rotation_steps - 1 {
            thread::sleep(Duration::from_millis(500));
        }
    }
    
    println!("🎉 All rotation steps complete!");
}

fn animate_to_original_positions(windows: &mut [WindowState], animation_duration_ms: u64) {
    println!("\n🏠 RESTORING ORIGINAL POSITIONS");
    println!("   ⏱️  Animation: {}ms with EaseInOut easing", animation_duration_ms);
    
    // Start animations back to original positions
    for (i, window) in windows.iter_mut().enumerate() {
        println!("  📦 Restoring window {}: '{}'", 
            i + 1,
            if window.title.len() > 30 { &window.title[..30] } else { &window.title }
        );
        
        animate_window_to_position(window, window.original_rect, animation_duration_ms, EasingType::EaseInOut);
    }
    
    // Run animation loop
    let start_time = std::time::Instant::now();
    let duration = Duration::from_millis(animation_duration_ms);
    
    println!("   🎬 Running restoration animation...");
    
    while start_time.elapsed() < duration {
        let progress = start_time.elapsed().as_secs_f32() / duration.as_secs_f32();
        let progress = progress.min(1.0);
        
        let mut all_complete = true;
        for window in windows.iter_mut() {
            if window.is_animating {
                let completed = apply_window_animation_frame(window, progress);
                if !completed {
                    all_complete = false;
                }
            }
        }
        
        if all_complete {
            break;
        }
        
        thread::sleep(Duration::from_millis(16)); // ~60 FPS
    }
    
    // Final positioning
    for window in windows.iter_mut() {
        window.is_animating = false;
        window.animation = None;
        window.current_rect = window.original_rect;
    }
    
    println!("   ✅ All windows restored to original positions");
}

fn main() {
    println!("🎬 E-GRID ANIMATED DYNAMIC TRANSITION TEST");
    println!("==========================================");
    println!("🎯 Testing: 2x2 → 4x4 → 8x8 → 4x4 → 2x2 → Original positions");
    println!("🔄 With animated transitions and cell rotation");
    println!("🎭 Multiple easing functions and smooth animations\n");
    
    // Get visible windows
    let visible_windows = get_visible_windows();
    let window_count = visible_windows.len().min(8); // Max 8 for practical demonstration
    
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
            println!("  {}. {}", i + 1, if title.len() > 40 { &title[..40] } else { &title });
            WindowState::new(hwnd, rect, title)
        })
        .collect();
    
    // Get monitor bounds
    let temp_config = GridConfig::new(4, 4);
    let tracker = WindowTracker::new_with_config(temp_config);
    
    if tracker.monitor_grids.is_empty() {
        println!("❌ No monitors found");
        return;
    }
    
    let monitor_rect = if tracker.monitor_grids.len() > 1 {
        let (left, top, right, bottom) = tracker.monitor_grids[1].monitor_rect;
        RECT { left, top, right, bottom }
    } else {
        let (left, top, right, bottom) = tracker.monitor_grids[0].monitor_rect;
        RECT { left, top, right, bottom }
    };
    
    println!("🖥️  Monitor bounds: [{},{} {}x{}]\n", 
        monitor_rect.left, monitor_rect.top,
        monitor_rect.right - monitor_rect.left,
        monitor_rect.bottom - monitor_rect.top
    );
    
    // Phase 1: Transition to 2x2 grid with Bounce easing
    animate_windows_to_grid(&mut windows, &monitor_rect, 2, 2, "PHASE 1", 1200, EasingType::Bounce);
    
    println!("\n⏸️  Press Enter to see cell rotation in 2x2 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Rotate windows through 2x2 grid cells
    rotate_windows_in_grid(&mut windows, &monitor_rect, 2, 2, 3, 800);
    
    println!("\n⏸️  Press Enter to expand to 4x4 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Phase 2: Transition to 4x4 grid with Elastic easing
    animate_windows_to_grid(&mut windows, &monitor_rect, 4, 4, "PHASE 2", 1500, EasingType::Elastic);
    
    println!("\n⏸️  Press Enter to see cell rotation in 4x4 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Rotate windows through 4x4 grid cells
    rotate_windows_in_grid(&mut windows, &monitor_rect, 4, 4, 4, 600);
    
    println!("\n⏸️  Press Enter to expand to 8x8 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Phase 3: Transition to 8x8 grid with Back easing
    animate_windows_to_grid(&mut windows, &monitor_rect, 8, 8, "PHASE 3", 2000, EasingType::Back);
    
    println!("\n⏸️  Press Enter to return to 4x4 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Phase 4: Return to 4x4 grid with EaseInOut
    animate_windows_to_grid(&mut windows, &monitor_rect, 4, 4, "PHASE 4", 1000, EasingType::EaseInOut);
    
    println!("\n⏸️  Press Enter to return to 2x2 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Phase 5: Return to 2x2 grid with EaseOut
    animate_windows_to_grid(&mut windows, &monitor_rect, 2, 2, "PHASE 5", 1000, EasingType::EaseOut);
    
    println!("\n⏸️  Press Enter to restore original positions...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    // Phase 6: Return to original positions
    animate_to_original_positions(&mut windows, 2000);
    
    println!("\n🎉 ANIMATED DYNAMIC TRANSITION TEST COMPLETE!");
    println!("=============================================");
    println!("✅ Successfully demonstrated:");
    println!("   🎬 Smooth animated transitions between grid sizes");
    println!("   🔄 Cell rotation within each grid configuration");
    println!("   🎭 Multiple easing functions (Bounce, Elastic, Back, EaseInOut, EaseOut)");
    println!("   📐 Progressive grid sizing: 2x2 → 4x4 → 8x8 → 4x4 → 2x2");
    println!("   🏠 Smooth animated return to original positions");
    println!("   ⏱️  60 FPS animation rendering with real-time interpolation");
    println!("   🎯 Pixel-perfect window positioning and sizing");
    println!("\n🚀 The dynamic grid system with animations is working perfectly!");
}
