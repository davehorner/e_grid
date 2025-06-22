use e_grid::{EasingType, GridConfig, WindowAnimation, WindowTracker};
use std::collections::HashMap;
use std::io;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};
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

// Safe string truncation that respects UTF-8 character boundaries
fn truncate_string_safe_bytes(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        s.chars().take(max_chars).collect::<String>() + "..."
    }
}

#[derive(Clone)]
struct ManagedWindow {
    hwnd: HWND,
    original_rect: RECT,
    current_rect: RECT,
    target_rect: RECT,
    title: String,
    grid_row: Option<usize>, // None if not currently in grid
    grid_col: Option<usize>, // None if not currently in grid
    is_animating: bool,
    animation: Option<WindowAnimation>,
    in_grid: bool,      // Whether this window is currently displayed in the grid
    last_seen: Instant, // For tracking if window still exists
}

impl ManagedWindow {
    fn new(hwnd: HWND, rect: RECT, title: String) -> Self {
        Self {
            hwnd,
            original_rect: rect,
            current_rect: rect,
            target_rect: rect,
            title,
            grid_row: None,
            grid_col: None,
            is_animating: false,
            animation: None,
            in_grid: false,
            last_seen: Instant::now(),
        }
    }

    fn is_in_grid(&self) -> bool {
        self.in_grid && self.grid_row.is_some() && self.grid_col.is_some()
    }

    fn remove_from_grid(&mut self) {
        self.in_grid = false;
        self.grid_row = None;
        self.grid_col = None;
    }

    fn assign_to_grid(&mut self, row: usize, col: usize) {
        self.in_grid = true;
        self.grid_row = Some(row);
        self.grid_col = Some(col);
    }
}

struct WindowManager {
    windows: HashMap<HWND, ManagedWindow>,
    monitor_rect: RECT,
    next_rotation_index: usize, // For rotating windows in/out of grid
}

impl WindowManager {
    fn new(monitor_rect: RECT) -> Self {
        Self {
            windows: HashMap::new(),
            monitor_rect,
            next_rotation_index: 0, // Start rotation at index 0
        }
    }

    fn scan_and_update_windows(&mut self) {
        let current_windows = get_visible_windows();
        let now = Instant::now();

        // Update existing windows and add new ones
        for (hwnd, rect, title) in current_windows {
            if let Some(window) = self.windows.get_mut(&hwnd) {
                // Update existing window
                window.last_seen = now;
                if !window.is_animating && !window.in_grid {
                    // Update original position if not in grid and not animating
                    window.original_rect = rect;
                    window.current_rect = rect;
                }
            } else {
                // New window discovered
                println!(
                    "🆕 New window discovered: '{}'",
                    &truncate_string_safe_bytes(&title, 40)
                );
                let new_window = ManagedWindow::new(hwnd, rect, title);
                self.windows.insert(hwnd, new_window);
            }
        }

        // Remove windows that no longer exist (haven't been seen for 5 seconds)
        let stale_cutoff = Duration::from_secs(5);
        let stale_windows: Vec<HWND> = self
            .windows
            .iter()
            .filter(|(_, window)| now.duration_since(window.last_seen) > stale_cutoff)
            .map(|(&hwnd, _)| hwnd)
            .collect();

        for hwnd in stale_windows {
            if let Some(window) = self.windows.remove(&hwnd) {
                println!("🗑️  Removed stale window: '{}'", window.title);
            }
        }
    }
    fn get_windows_for_grid(&self, grid_size: usize) -> Vec<HWND> {
        // Get all available windows, prioritizing those already in grid
        let in_grid: Vec<HWND> = self
            .windows
            .iter()
            .filter(|(_, w)| w.in_grid)
            .map(|(&hwnd, _)| hwnd)
            .collect();

        let mut not_in_grid: Vec<HWND> = self
            .windows
            .iter()
            .filter(|(_, w)| !w.in_grid)
            .map(|(&hwnd, _)| hwnd)
            .collect();

        // Sort not_in_grid to have a consistent rotation order
        not_in_grid.sort_by(|&a, &b| {
            let title_a = &self.windows[&a].title;
            let title_b = &self.windows[&b].title;
            title_a.cmp(title_b)
        });

        // For initial layout or when no windows are in grid, start from rotation index
        if in_grid.is_empty() {
            let mut selected = Vec::new();
            if !not_in_grid.is_empty() {
                for i in 0..grid_size.min(not_in_grid.len()) {
                    let index = (self.next_rotation_index + i) % not_in_grid.len();
                    selected.push(not_in_grid[index]);
                }
            }
            return selected;
        }

        // For subsequent layouts, keep existing windows in grid and fill remaining slots
        let mut selected = Vec::new();

        // Add currently in-grid windows first (up to grid capacity)
        selected.extend(in_grid.iter().take(grid_size));

        // Fill remaining slots with rotated windows
        let remaining_slots = grid_size.saturating_sub(selected.len());

        if remaining_slots > 0 && !not_in_grid.is_empty() {
            // Rotate through available windows
            for i in 0..remaining_slots {
                let index = (self.next_rotation_index + i) % not_in_grid.len();
                selected.push(not_in_grid[index]);
            }
        }

        selected.truncate(grid_size);
        selected
    }

    fn animate_to_grid_layout(
        &mut self,
        grid_rows: usize,
        grid_cols: usize,
        phase_name: &str,
        animation_duration_ms: u64,
        easing: EasingType,
    ) {
        println!(
            "\n🎬 {}: Animating to {}x{} grid",
            phase_name, grid_rows, grid_cols
        );

        let grid_size = grid_rows * grid_cols;
        let selected_windows = self.get_windows_for_grid(grid_size);

        println!("   📐 Grid capacity: {} cells", grid_size);
        println!("   📊 Total windows available: {}", self.windows.len());
        println!(
            "   🎯 Windows selected for grid: {}",
            selected_windows.len()
        );
        println!("   🔄 Current rotation index: {}", self.next_rotation_index);
        println!(
            "   ⏱️  Animation: {}ms with {:?} easing",
            animation_duration_ms, easing
        );
        // First, animate windows currently in grid but not selected back to original positions
        let windows_to_remove: Vec<HWND> = self
            .windows
            .iter()
            .filter(|(hwnd, w)| w.in_grid && !selected_windows.contains(hwnd))
            .map(|(&hwnd, _)| hwnd)
            .collect();

        for &hwnd in &windows_to_remove {
            let (original_rect, title) = {
                if let Some(window) = self.windows.get(&hwnd) {
                    (window.original_rect, window.title.clone())
                } else {
                    continue;
                }
            };
            println!(
                "  📤 Sending '{}' back to original position",
                &truncate_string_safe_bytes(&title, 25)
            );

            self.animate_window_to_position(
                hwnd,
                original_rect,
                animation_duration_ms,
                easing.clone(),
            );

            if let Some(window) = self.windows.get_mut(&hwnd) {
                window.remove_from_grid();
            }
        } // Then, animate selected windows to grid positions
        println!("   🎯 Selected windows for grid:");
        for (i, &hwnd) in selected_windows.iter().enumerate() {
            if let Some(window) = self.windows.get(&hwnd) {
                println!(
                    "      {}. '{}'",
                    i + 1,
                    &truncate_string_safe_bytes(&window.title, 30)
                );
            }
        }

        for (i, &hwnd) in selected_windows.iter().enumerate() {
            let target_row = i / grid_cols;
            let target_col = i % grid_cols;

            let target_rect =
                self.calculate_grid_position(target_row, target_col, grid_rows, grid_cols);

            if let Some(window) = self.windows.get(&hwnd) {
                println!(
                    "  📦 Window {}: '{}' → Cell [{},{}]",
                    i + 1,
                    &truncate_string_safe_bytes(&window.title, 25),
                    target_row,
                    target_col
                );
            }

            if let Some(window) = self.windows.get_mut(&hwnd) {
                window.assign_to_grid(target_row, target_col);
            }

            self.animate_window_to_position(
                hwnd,
                target_rect,
                animation_duration_ms,
                easing.clone(),
            );
        }

        // Update rotation index for next time (based on non-grid windows)
        let available_for_rotation = self.windows.iter().filter(|(_, w)| !w.in_grid).count();

        let old_index = self.next_rotation_index;
        if available_for_rotation > 0 {
            self.next_rotation_index =
                (self.next_rotation_index + selected_windows.len()) % available_for_rotation;
        }

        println!(
            "   🔄 Rotation index: {} → {} (available: {})",
            old_index, self.next_rotation_index, available_for_rotation
        );

        self.wait_for_animations(animation_duration_ms);
        println!("   ✅ Layout transition complete!");
    }

    fn calculate_grid_position(
        &self,
        row: usize,
        col: usize,
        grid_rows: usize,
        grid_cols: usize,
    ) -> RECT {
        let monitor_width = self.monitor_rect.right - self.monitor_rect.left;
        let monitor_height = self.monitor_rect.bottom - self.monitor_rect.top;

        let cell_width = monitor_width / grid_cols as i32;
        let cell_height = monitor_height / grid_rows as i32;

        let left = self.monitor_rect.left + (col as i32 * cell_width);
        let top = self.monitor_rect.top + (row as i32 * cell_height);
        let right = left + cell_width - 30; // Margin for visibility
        let bottom = top + cell_height - 30; // Margin for visibility
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    fn animate_window_to_position(
        &mut self,
        hwnd: HWND,
        target_rect: RECT,
        duration_ms: u64,
        easing: EasingType,
    ) {
        if let Some(window) = self.windows.get_mut(&hwnd) {
            println!(
                "   🎯 Animating window '{}' to position [{},{} {}x{}]",
                &truncate_string_safe_bytes(&window.title, 20),
                target_rect.left,
                target_rect.top,
                target_rect.right - target_rect.left,
                target_rect.bottom - target_rect.top
            );

            window.target_rect = target_rect;
            window.animation = Some(WindowAnimation::new(
                window.hwnd,
                window.current_rect,
                target_rect,
                Duration::from_millis(duration_ms),
                easing,
            ));
            window.is_animating = true;
        }
    }

    fn wait_for_animations(&mut self, duration_ms: u64) {
        let start_time = Instant::now();
        let duration = Duration::from_millis(duration_ms);

        println!("   🎬 Running animation loop...");

        while start_time.elapsed() < duration {
            let mut all_complete = true;

            for window in self.windows.values_mut() {
                if window.is_animating {
                    if let Some(ref mut animation) = window.animation {
                        let current_rect = animation.get_current_rect();

                        // Move the window to current animation position
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

                        if animation.completed {
                            window.is_animating = false;
                            window.animation = None;
                        } else {
                            all_complete = false;
                        }
                    }
                }
            }

            if all_complete {
                break;
            }

            thread::sleep(Duration::from_millis(16)); // ~60 FPS
        }

        // Ensure all animations are complete
        for window in self.windows.values_mut() {
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
    }
    fn rotate_grid_windows(
        &mut self,
        grid_rows: usize,
        grid_cols: usize,
        rotation_steps: usize,
        step_duration_ms: u64,
    ) {
        let grid_size = grid_rows * grid_cols;

        for step in 0..rotation_steps {
            println!(
                "\n🔄 ROTATION STEP {} of {} (scanning for new windows)",
                step + 1,
                rotation_steps
            );

            // Scan for new windows before each rotation
            self.scan_and_update_windows();

            // Get all windows sorted by title for consistent ordering
            let mut all_windows: Vec<HWND> = self.windows.keys().cloned().collect();
            all_windows.sort_by(|&a, &b| {
                let title_a = &self.windows[&a].title;
                let title_b = &self.windows[&b].title;
                title_a.cmp(title_b)
            });

            if all_windows.len() < grid_size {
                println!(
                    "   ⚠️  Only {} windows available for {}-cell grid",
                    all_windows.len(),
                    grid_size
                );
            }

            // Calculate how many windows to rotate (at least half the grid, but not more than available)
            let num_to_rotate = (grid_size / 2).max(1).min(all_windows.len());

            // Get current windows in grid
            let current_in_grid: Vec<HWND> = self
                .windows
                .iter()
                .filter(|(_, w)| w.is_in_grid())
                .map(|(&hwnd, _)| hwnd)
                .collect();

            println!(
                "   📊 Available windows: {}, Grid size: {}, Will rotate: {}",
                all_windows.len(),
                grid_size,
                num_to_rotate
            );

            // Step 1: Remove some current grid windows
            let windows_to_remove = current_in_grid
                .iter()
                .take(num_to_rotate)
                .cloned()
                .collect::<Vec<_>>();

            for &hwnd in &windows_to_remove {
                let (original_rect, title) = {
                    if let Some(window) = self.windows.get(&hwnd) {
                        (window.original_rect, window.title.clone())
                    } else {
                        continue;
                    }
                };

                println!(
                    "  📤 Rotating out: '{}'",
                    &truncate_string_safe_bytes(&title, 20)
                );

                self.animate_window_to_position(
                    hwnd,
                    original_rect,
                    step_duration_ms / 2,
                    EasingType::EaseOut,
                );

                if let Some(window) = self.windows.get_mut(&hwnd) {
                    window.remove_from_grid();
                }
            }

            // Wait for exit animations
            self.wait_for_animations(step_duration_ms / 2);

            // Step 2: Add new windows to fill the grid, cycling through all available windows
            let start_index = (self.next_rotation_index + step * num_to_rotate) % all_windows.len();

            for i in 0..grid_size {
                let window_index = (start_index + i) % all_windows.len();
                let hwnd = all_windows[window_index];

                // Calculate grid position in row-major order: [0,0], [0,1], [1,0], [1,1], etc.
                let row = i / grid_cols;
                let col = i % grid_cols;

                let target_rect = self.calculate_grid_position(row, col, grid_rows, grid_cols);

                if let Some(window) = self.windows.get(&hwnd) {
                    println!(
                        "  📥 Placing: '{}' → Cell [{},{}]",
                        &truncate_string_safe_bytes(&window.title, 20),
                        row,
                        col
                    );
                }

                if let Some(window) = self.windows.get_mut(&hwnd) {
                    window.assign_to_grid(row, col);
                }

                self.animate_window_to_position(
                    hwnd,
                    target_rect,
                    step_duration_ms,
                    EasingType::EaseIn,
                );
            }

            // Update rotation index for next step
            self.next_rotation_index =
                (self.next_rotation_index + num_to_rotate) % all_windows.len();

            // Wait for enter animations
            self.wait_for_animations(step_duration_ms);

            println!(
                "   ✅ Rotation step {} complete (next index: {})",
                step + 1,
                self.next_rotation_index
            );

            if step < rotation_steps - 1 {
                thread::sleep(Duration::from_millis(800)); // Pause between rotations
            }
        }
    }

    fn find_available_grid_slot(
        &self,
        grid_rows: usize,
        grid_cols: usize,
    ) -> Option<(usize, usize)> {
        for row in 0..grid_rows {
            for col in 0..grid_cols {
                let slot_occupied = self
                    .windows
                    .values()
                    .any(|w| w.is_in_grid() && w.grid_row == Some(row) && w.grid_col == Some(col));

                if !slot_occupied {
                    return Some((row, col));
                }
            }
        }
        None
    }

    fn restore_all_windows(&mut self, animation_duration_ms: u64) {
        println!("\n🏠 RESTORING ALL WINDOWS TO ORIGINAL POSITIONS");
        println!("   📊 Restoring {} windows", self.windows.len());
        println!(
            "   ⏱️  Animation: {}ms with EaseInOut easing",
            animation_duration_ms
        );

        for (i, (_hwnd, window)) in self.windows.iter().enumerate() {
            println!(
                "  📦 Restoring window {}: '{}'",
                i + 1,
                &truncate_string_safe_bytes(&window.title, 30)
            );
        }
        // Animate all windows back to original positions
        let windows_to_restore: Vec<(HWND, RECT)> = self
            .windows
            .iter()
            .map(|(&hwnd, window)| (hwnd, window.original_rect))
            .collect();

        for (hwnd, original_rect) in windows_to_restore {
            self.animate_window_to_position(
                hwnd,
                original_rect,
                animation_duration_ms,
                EasingType::EaseInOut,
            );
        }

        // Remove all from grid
        for window in self.windows.values_mut() {
            window.remove_from_grid();
        }

        self.wait_for_animations(animation_duration_ms);
        println!("   ✅ All windows restored to original positions");
    }
    fn print_status(&self, phase: &str) {
        println!("\n📊 {} STATUS:", phase);
        println!("   Total windows tracked: {}", self.windows.len());

        let in_grid_count = self.windows.values().filter(|w| w.is_in_grid()).count();
        let animating_count = self.windows.values().filter(|w| w.is_animating).count();
        let available_for_rotation = self.windows.values().filter(|w| !w.in_grid).count();

        println!("   Windows in grid: {}", in_grid_count);
        println!(
            "   Windows available for rotation: {}",
            available_for_rotation
        );
        println!("   Windows animating: {}", animating_count);
        println!("   Next rotation index: {}", self.next_rotation_index);
    }

    fn progressive_grid_fill(
        &mut self,
        grid_rows: usize,
        grid_cols: usize,
        phase_name: &str,
        step_duration_ms: u64,
    ) {
        println!(
            "\n🎬 {}: Progressive fill of {}x{} grid",
            phase_name, grid_rows, grid_cols
        );

        let grid_size = grid_rows * grid_cols;
        let all_windows: Vec<HWND> = self.windows.keys().cloned().collect();

        println!("   📐 Grid capacity: {} cells", grid_size);
        println!("   📊 Total windows available: {}", all_windows.len());
        println!("   ⏱️  Step duration: {}ms each", step_duration_ms);

        // Start with empty grid - make sure all windows are out of grid
        for window in self.windows.values_mut() {
            window.remove_from_grid();
        }

        println!("   🏁 Starting with empty grid...");

        // Progressive fill: add one window at a time, sliding others to make room
        for step in 0..grid_size.min(all_windows.len()) {
            println!(
                "\n   🔄 FILL STEP {} of {}",
                step + 1,
                grid_size.min(all_windows.len())
            );

            // Get the next window to add to grid
            let next_window_idx = (self.next_rotation_index + step) % all_windows.len();
            let next_window = all_windows[next_window_idx];

            // Shift all current grid windows to make room
            self.shift_grid_windows_and_add_new(
                grid_rows,
                grid_cols,
                next_window,
                step_duration_ms,
            );

            // Small pause between steps to see the progression
            thread::sleep(Duration::from_millis(300));
        }

        // Update rotation index for next time
        self.next_rotation_index =
            (self.next_rotation_index + grid_size) % all_windows.len().max(1);

        println!("   ✅ Progressive grid fill complete!");
    }

    fn shift_grid_windows_and_add_new(
        &mut self,
        grid_rows: usize,
        grid_cols: usize,
        new_window: HWND,
        duration_ms: u64,
    ) {
        let grid_size = grid_rows * grid_cols;

        // Get current windows in grid (in order of their positions)
        let mut current_grid_windows: Vec<(HWND, usize, usize)> = self
            .windows
            .iter()
            .filter_map(|(&hwnd, window)| {
                if window.is_in_grid() {
                    Some((hwnd, window.grid_row.unwrap(), window.grid_col.unwrap()))
                } else {
                    None
                }
            })
            .collect();

        // Sort by position (row-major order)
        current_grid_windows.sort_by(|a, b| {
            let pos_a = a.1 * grid_cols + a.2;
            let pos_b = b.1 * grid_cols + b.2;
            pos_a.cmp(&pos_b)
        });

        if let Some(window) = self.windows.get(&new_window) {
            println!(
                "      ➕ Adding: '{}'",
                &truncate_string_safe_bytes(&window.title, 25)
            );
        }

        // If grid is full, remove the last window
        if current_grid_windows.len() >= grid_size {
            if let Some((last_window, _, _)) = current_grid_windows.pop() {
                if let Some(window) = self.windows.get(&last_window) {
                    println!(
                        "      ➖ Removing: '{}'",
                        &truncate_string_safe_bytes(&window.title, 25)
                    );

                    let original_rect = window.original_rect;
                    self.animate_window_to_position(
                        last_window,
                        original_rect,
                        duration_ms,
                        EasingType::EaseOut,
                    );
                }

                if let Some(window) = self.windows.get_mut(&last_window) {
                    window.remove_from_grid();
                }
            }
        }

        // Shift all existing windows one position forward
        for (i, (hwnd, _, _)) in current_grid_windows.iter().enumerate() {
            let new_pos = i + 1; // Shift forward by 1
            let new_row = new_pos / grid_cols;
            let new_col = new_pos % grid_cols;

            let target_rect = self.calculate_grid_position(new_row, new_col, grid_rows, grid_cols);

            if let Some(window) = self.windows.get_mut(hwnd) {
                window.assign_to_grid(new_row, new_col);
            }

            self.animate_window_to_position(*hwnd, target_rect, duration_ms, EasingType::EaseInOut);

            if let Some(window) = self.windows.get(hwnd) {
                println!(
                    "      🔄 Shifting: '{}' → Cell [{},{}]",
                    &truncate_string_safe_bytes(&window.title, 20),
                    new_row,
                    new_col
                );
            }
        }

        // Add new window at position [0,0]
        let target_rect = self.calculate_grid_position(0, 0, grid_rows, grid_cols);

        if let Some(window) = self.windows.get_mut(&new_window) {
            window.assign_to_grid(0, 0);
        }

        self.animate_window_to_position(new_window, target_rect, duration_ms, EasingType::EaseIn);

        // Wait for all animations to complete
        self.wait_for_animations(duration_ms);
    }
    fn wait_for_user_input_with_scanning(&mut self, prompt: &str) {
        println!("\n{}", prompt);
        println!("    🔍 Try opening a new window now - it will be detected automatically!");

        // Scan for new windows while waiting
        for i in 0..10 {
            // Check 10 times over 5 seconds
            let initial_count = self.windows.len();
            self.scan_and_update_windows();
            let new_count = self.windows.len();

            if new_count > initial_count {
                println!(
                    "   🆕 Detected {} new window(s)! Total: {}",
                    new_count - initial_count,
                    new_count
                );
                self.print_status(&format!("UPDATED - ITERATION {}", i + 1));
            }

            thread::sleep(Duration::from_millis(500));
        }

        println!("\n   ⏩ Press Enter to continue...");
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
    }
}

fn main() {
    println!("🎬 E-GRID COMPREHENSIVE WINDOW MANAGEMENT DEMO");
    println!("===============================================");
    println!("🔄 Features:");
    println!("   • ALL windows are managed and rotated through layouts");
    println!("   • Windows rotate in/out of grid as needed");
    println!("   • New windows discovered and added automatically");
    println!("   • Non-grid windows return to original positions");
    println!("   • Smooth 60 FPS animations with multiple easing functions");
    println!("   • Real-time window discovery and management\n");

    // Get monitor bounds
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
        "🖥️  Using monitor bounds: [{},{} {}x{}]",
        monitor_rect.left,
        monitor_rect.top,
        monitor_rect.right - monitor_rect.left,
        monitor_rect.bottom - monitor_rect.top
    );

    // Initialize window manager
    let mut window_manager = WindowManager::new(monitor_rect);

    // Initial window discovery
    println!("\n🔍 Performing initial window discovery...");
    window_manager.scan_and_update_windows();
    window_manager.print_status("INITIAL");
    println!("\n⚠️  DEMO READY - This will manage ALL your windows!");
    window_manager.wait_for_user_input_with_scanning(
        "    Press Enter to start the comprehensive window management demo...",
    ); // Phase 1: Start with 2x2 Grid with immediate layout (for testing)
    println!("\n🔄 Starting Phase 1 with periodic window scanning...");
    window_manager.animate_to_grid_layout(2, 2, "PHASE 1", 1200, EasingType::Bounce);
    window_manager.print_status("PHASE 1");

    // Scan for new windows after initial layout
    println!("\n🔍 Scanning for any new windows that appeared during layout...");
    window_manager.scan_and_update_windows();
    window_manager.print_status("PHASE 1 - AFTER SCAN");
    window_manager
        .wait_for_user_input_with_scanning("⏸️  Press Enter to see window rotation in 2x2 grid...");

    window_manager.rotate_grid_windows(2, 2, 5, 800);

    println!("\n⏸️  Press Enter to expand to 4x4 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    // Phase 2: Progressive 4x4 Grid fill
    window_manager.progressive_grid_fill(4, 4, "PHASE 2", 800);
    window_manager.print_status("PHASE 2");

    println!("\n⏸️  Press Enter to see window rotation in 4x4 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    window_manager.rotate_grid_windows(4, 4, 4, 800);

    println!("\n⏸️  Press Enter to expand to 6x6 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Phase 3: 6x6 Grid (larger to show more windows)
    window_manager.animate_to_grid_layout(6, 6, "PHASE 3", 2000, EasingType::Back);
    window_manager.print_status("PHASE 3");

    println!("\n⏸️  Press Enter to see extensive window rotation in 6x6 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    window_manager.rotate_grid_windows(6, 6, 6, 600);

    println!("\n⏸️  Press Enter to return to 3x3 grid...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Phase 4: 3x3 Grid
    window_manager.animate_to_grid_layout(3, 3, "PHASE 4", 1000, EasingType::EaseInOut);
    window_manager.print_status("PHASE 4");

    println!("\n⏸️  Press Enter to restore all windows to original positions...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    // Final: Restore all windows
    window_manager.restore_all_windows(2500);
    window_manager.print_status("FINAL");

    println!("\n🎉 COMPREHENSIVE WINDOW MANAGEMENT DEMO COMPLETE!");
    println!("==================================================");
    println!("✅ Successfully demonstrated:");
    println!("   🔄 Dynamic window rotation through grid layouts");
    println!("   🆕 Real-time new window detection and integration");
    println!("   📊 ALL windows managed (not just a subset)");
    println!("   🏠 Smart return-to-original positioning");
    println!("   🎬 Smooth animations with multiple easing functions");
    println!("   ⚡ 60 FPS real-time rendering");
    println!("   🧠 Intelligent window selection and rotation algorithms");
    println!("   📐 Multiple grid sizes with seamless transitions");
    println!("\n🚀 Your E-Grid system now handles comprehensive window management!");

    println!("\nPress Enter to exit...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
}
