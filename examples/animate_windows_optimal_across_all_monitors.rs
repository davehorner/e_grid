//! Animate new windows on Monitor 1 in a dynamically optimal grid layout.
//!
//! - Listens for new window creation events and animates them into a grid.
//! - The grid layout is recalculated to minimize unused cells and keep the grid as square as possible.
//! - Windows are assigned to grid cells in row-major order.
//! - Uses lock-free DashMap/DashSet for window tracking and original positions.
//! - Handles graceful shutdown and restores windows to their original positions on exit.
//! - Supports rotating window positions within the grid.
//!
//! Usage:
//!   - Run the program. Open new windows to see them animated into the grid.
//!   - Press Ctrl+C, q, x, or Esc to exit and restore window positions.

use crossterm::event::{self, Event, KeyCode};
use ctrlc;
use dashmap::{DashMap, DashSet};
use e_grid::window_events::{run_message_loop, WindowEventConfig};
use e_grid::window_tracker::WindowTracker;
use e_grid::EasingType;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use winapi::shared::windef::RECT;

/// Returns the optimal grid size (rows, cols) for n windows.
/// The grid will be as square as possible and will not grow until all cells are filled.
///
/// # Arguments
/// * `n` - Number of windows to arrange.
/// * `max_rows` - Maximum allowed rows.
/// * `max_cols` - Maximum allowed columns.
///
/// # Returns
/// (rows, cols) tuple for the grid.
fn optimal_grid(n: usize, max_rows: usize, max_cols: usize) -> (usize, usize) {
    if n == 1 {
        return (1, 1);
    }
    let mut best = (1, n);
    let mut min_unused = usize::MAX;
    for rows in 1..=max_rows {
        for cols in 1..=max_cols {
            if rows * cols < n {
                continue;
            }
            let unused = rows * cols - n;
            let aspect = (rows as isize - cols as isize).abs();
            // Prefer more square grids, but minimize unused cells
            if unused < min_unused
                || (unused == min_unused && aspect < (best.0 as isize - best.1 as isize).abs())
            {
                best = (rows, cols);
                min_unused = unused;
            }
        }
    }
    best
}

/// Main entry point.
/// Sets up event hooks, window tracking, and runs the animation loop.
/// Restores window positions on exit.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🆕 Animate NEW Windows on Monitor 1 in Rotating Grid (WinEvent, runs until Ctrl+C)");

    // Setup Ctrl+C handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let win_event_cleanup = Arc::new(AtomicBool::new(false));
    {
        let running = running.clone();
        let win_event_cleanup = win_event_cleanup.clone();
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
            win_event_cleanup.store(true, Ordering::SeqCst);
        })?;
    }

    // Shared state for new windows and their original rects
    let tracker = Arc::new(Mutex::new(WindowTracker::new()));
    let grid_hwnds = Arc::new(DashMap::<u64, ()>::new()); // <-- DashMap for HWNDs
    let original_rects = Arc::new(DashMap::<u64, RECT>::new());
    let known_hwnds = Arc::new(DashSet::<u64>::new());

    // Ensure tracker is initialized and monitors are available
    {
        let mut tracker_guard = tracker.lock().unwrap();
        tracker_guard.scan_existing_windows();
        for hwnd in tracker_guard.windows.iter() {
            known_hwnds.insert(*hwnd.key());
        }
        if tracker_guard.monitor_grids.len() < 2 {
            println!("Less than 2 monitors detected. Aborting.");
            return Ok(());
        }
    }

    // Monitor info for monitor 1
    let (monitor_rect, rows, cols) = {
        let tracker_guard = tracker.lock().unwrap();
        let monitor = &tracker_guard.monitor_grids[1];
        (
            monitor.monitor_rect,
            monitor.config.rows,
            monitor.config.cols,
        )
    };

    // Channel for lock-free tracker requests
    let (tracker_req_tx, tracker_req_rx) =
        mpsc::channel::<Box<dyn FnOnce(&mut WindowTracker) + Send>>();

    // Spawn a tracker thread to handle requests
    let tracker_thread = {
        let tracker = tracker.clone();
        std::thread::spawn(move || {
            while let Ok(req) = tracker_req_rx.recv() {
                let mut tracker_guard = tracker.lock().unwrap();
                req(&mut *tracker_guard);
            }
        })
    };

    // Event callback for new windows
    let tracker_req_tx_cb = tracker_req_tx.clone();
    let grid_hwnds_clone = grid_hwnds.clone();
    let original_rects_clone = original_rects.clone();
    let known_hwnds_clone = known_hwnds.clone();
    // Track the initial focused HWND at startup
    let initial_focused_hwnd = WindowTracker::get_foreground_window().unwrap_or(0);
    let event_callback = move |event: e_grid::ipc_protocol::GridEvent| {
        println!("🟢 [DEBUG] WinEvent callback invoked: {:?}", event);

        if let e_grid::ipc_protocol::GridEvent::WindowCreated { hwnd, .. } = event {
            println!("🆕 Window created: HWND 0x{:X}", hwnd);
            // --- Lock-free known_hwnds check/insert ---
            if known_hwnds_clone.contains(&hwnd) {
                println!("🟡 [DEBUG] HWND 0x{:X} already known, skipping.", hwnd);
                return;
            }
            known_hwnds_clone.insert(hwnd);

            // --- Lock-free tracker request via channel ---
            let grid_hwnds_clone = grid_hwnds_clone.clone();
            let original_rects_clone = original_rects_clone.clone();
            let tracker_req_tx_cb = tracker_req_tx_cb.clone();

            tracker_req_tx_cb.send(Box::new(move |tracker: &mut WindowTracker| {
                // Scan windows
                println!("🔍 [CHANNEL] Scanning windows before animation for HWND 0x{:X}", hwnd);
                tracker.scan_existing_windows();

                // Foreground and restore
                if let Some(info) = tracker.windows.get(&hwnd) {
                    println!("🔍 [CHANNEL] Window info for HWND 0x{:X}: visible={}, minimized={}, rect={:?}", hwnd, info.is_visible, info.is_minimized, info.window_rect);
                    if info.is_minimized {
                        println!("🔍 [CHANNEL] Restoring minimized window HWND 0x{:X}", hwnd);
                        unsafe {
                            use winapi::um::winuser::{ShowWindow, SW_RESTORE};
                            ShowWindow(hwnd as winapi::shared::windef::HWND, SW_RESTORE);
                        }
                        thread::sleep(Duration::from_millis(200));
                    }
                    println!("🔍 [CHANNEL] Setting foreground window HWND 0x{:X}", hwnd);
                    unsafe {
                        use winapi::um::winuser::SetForegroundWindow;
                        SetForegroundWindow(hwnd as winapi::shared::windef::HWND);
                    }
                }

                // Animation logic
                let (is_visible, is_minimized, title) = if let Some(info) = tracker.windows.get(&hwnd) {
                    (
                        info.is_visible,
                        info.is_minimized,
                        String::from_utf16_lossy(&info.title[..info.title_len as usize]),
                    )
                } else {
                    (false, false, String::new())
                };
                println!("🔍 [CHANNEL] After foreground/minimize: HWND 0x{:X} visible={}, minimized={}", hwnd, is_visible, is_minimized);

                if is_visible && !is_minimized && WindowTracker::is_manageable_window(hwnd) {
                    println!("🆕 [CHANNEL] New window detected: HWND 0x{:X} '{}'", hwnd, title);

                    // --- Fix: Always get and store the rect BEFORE any animation ---
                    if let Some(rect) = tracker.windows.get(&hwnd).map(|info| info.window_rect) {
                        println!(
                            "🔍 [CHANNEL] Saving original rect for HWND 0x{:X}: left={}, top={}, right={}, bottom={}",
                            hwnd, rect.left, rect.top, rect.right, rect.bottom
                        );
                        original_rects_clone.insert(hwnd, *rect); // <-- DashMap, no lock
                    } else if let Some(rect) = WindowTracker::get_window_rect(hwnd) {
                        println!(
                            "🔍 [CHANNEL] Fallback: Saving original rect for HWND 0x{:X}: left={}, top={}, right={}, bottom={}",
                            hwnd, rect.left, rect.top, rect.right, rect.bottom
                        );
                        original_rects_clone.insert(hwnd, rect); // <-- DashMap, no lock
                    } else {
                        println!("🔴 [CHANNEL] No rect found for HWND 0x{:X}", hwnd);
                    }

                    // --- Add HWND to grid_hwnds, then clone in a single lock block ---
                    let grid_hwnds_now = {
                        grid_hwnds_clone.insert(hwnd, ());
                        println!("🔍 [CHANNEL] Adding HWND 0x{:X} to grid_hwnds", hwnd);

                        // --- DashMap: Get current HWNDs as Vec<u64> ---
                        let grid_hwnds_now: Vec<u64> = grid_hwnds_clone
                            .iter()
                            .map(|entry| *entry.key())
                            .filter(|hwnd| *hwnd != initial_focused_hwnd)
                            .collect();
                        // --- Use optimal_grid for layout ---
                        let (monitor_rect, max_rows, max_cols) = {
                            let monitor = tracker.monitor_grids.iter()
                                .find(|m| m.monitor_id == 1)
                                .unwrap_or(&tracker.monitor_grids[1]);
                            (monitor.monitor_rect, monitor.config.rows, monitor.config.cols)
                        };
                        let n = grid_hwnds_now.len();
                        let (rows, cols) = optimal_grid(n, max_rows, max_cols);
                        println!("🔍 [CHANNEL] Optimal grid for {} windows: {} rows x {} cols", n, rows, cols);

                        // --- Assign windows to grid cells (row-major) ---
                        let mut positions: Vec<RECT> = Vec::new();
                        let cell_width = (monitor_rect.right - monitor_rect.left) / cols as i32;
                        let cell_height = (monitor_rect.bottom - monitor_rect.top) / rows as i32;
                        for idx in 0..grid_hwnds_now.len() {
                            let row = idx / cols;
                            let col = idx % cols;
                            let rect = RECT {
                                left: monitor_rect.left + col as i32 * cell_width,
                                top: monitor_rect.top + row as i32 * cell_height,
                                right: monitor_rect.left + (col as i32 + 1) * cell_width,
                                bottom: monitor_rect.top + (row as i32 + 1) * cell_height,
                            };
                            println!(
                                "🔍 [CHANNEL] Target rect for HWND 0x{:X}: left={}, top={}, right={}, bottom={}",
                                grid_hwnds_now[idx], rect.left, rect.top, rect.right, rect.bottom
                            );
                            positions.push(rect);
                        }
                        // --- Debug: Show assigned positions ---
                        for (idx, hwnd) in grid_hwnds_now.iter().enumerate() {
                            let rect = positions[idx];
                            println!(
                                "🔍 [CHANNEL] HWND 0x{:X} assigned to position left={}, top={}, right={}, bottom={}",
                                hwnd, rect.left, rect.top, rect.right, rect.bottom
                            );
                        }

                        // --- Return current HWNDs and their target positions ---
                        (grid_hwnds_now, positions)
                    };

                    // Monitor info and animation
                    let (monitor_rect, rows, cols) = {
                        let monitor = tracker.monitor_grids.iter()
                            .find(|m| m.monitor_id == 1)
                            .unwrap_or(&tracker.monitor_grids[1]);
                        (monitor.monitor_rect, monitor.config.rows, monitor.config.cols)
                    };

                    let grid_size = grid_hwnds_now.0.len().next_power_of_two().min(rows.min(cols));
                    println!("🔍 [CHANNEL] Grid size for animation: {}", grid_size);

                    // --- Use optimal grid for positions ---
                    let n = grid_hwnds_now.0.len();
                    let (opt_rows, opt_cols) = optimal_grid(n, rows, cols);
                    let cell_width = ((monitor_rect.right - monitor_rect.left) as f64 / opt_cols as f64).ceil() as i32;
                    let cell_height = ((monitor_rect.bottom - monitor_rect.top) as f64 / opt_rows as f64).ceil() as i32;
                    let mut positions: Vec<RECT> = Vec::with_capacity(n);

                    for idx in 0..n {
                        let row = idx / opt_cols;
                        let col = idx % opt_cols;
                        let left = monitor_rect.left + col as i32 * cell_width;
                        let top = monitor_rect.top + row as i32 * cell_height;
                        let right = if col + 1 == opt_cols {
                            monitor_rect.right
                        } else {
                            monitor_rect.left + (col as i32 + 1) * cell_width
                        };
                        let bottom = if row + 1 == opt_rows {
                            monitor_rect.bottom
                        } else {
                            monitor_rect.top + (row as i32 + 1) * cell_height
                        };
                        let rect = RECT {
                            left,
                            top,
                            right,
                            bottom,
                        };
                        println!(
                            "🔍 [CHANNEL] Target rect for HWND 0x{:X}: left={}, top={}, right={}, bottom={}",
                            grid_hwnds_now.0[idx], rect.left, rect.top, rect.right, rect.bottom
                        );
                        positions.push(rect);
                    }
                    let animation_duration = Duration::from_millis(800);
                    println!("🔍 [CHANNEL] Issuing animation commands...");
                    let (hwnd_vec, positions_vec) = (&grid_hwnds_now.0, &positions);
                    for (hwnd, rect) in hwnd_vec.iter().zip(positions_vec) {
                        println!("Animating HWND 0x{:X} to rect ({},{})-({},{}) on monitor_id=1", hwnd, rect.left, rect.top, rect.right, rect.bottom);
                        let res = tracker.start_window_animation(
                            *hwnd,
                            *rect,
                            animation_duration.clone(),
                            EasingType::EaseInOut,
                        );
                        if let Err(e) = res {
                            println!("⚠️ Failed to animate window 0x{:X}: {}", hwnd, e);
                        } else {
                            println!("✅ Animation command issued for HWND 0x{:X}", hwnd);
                        }
                    }
                    for step in 0..16 {
                        tracker.update_animations();
                        thread::sleep(Duration::from_millis(animation_duration.as_millis() as u64 / 16));
                    }
                    // Ensure initial focused window stays foreground
                    if initial_focused_hwnd != 0 {
                        unsafe {
                            use winapi::um::winuser::SetForegroundWindow;
                            SetForegroundWindow(initial_focused_hwnd as winapi::shared::windef::HWND);
                        }
                    }
                }
            })).unwrap();
        }
    };

    // Setup WinEvent hooks with our event callback
    let mut config = WindowEventConfig::new(tracker.clone());
    config = config.with_event_callback(event_callback);
    e_grid::window_events::setup_window_events(config)?;

    // Enable raw mode so key events are captured immediately
    crossterm::terminal::enable_raw_mode()?;

    println!("Waiting for new windows... (Press Ctrl+C, q, x, or Esc to exit)");

    // Main loop: animate all windows in optimal grid for each monitor
    run_message_loop(|| {
        // Check for exit keys (q, x, Esc) without waiting for Enter
        let mut exit_requested = false;
        if let Ok(true) = event::poll(std::time::Duration::from_millis(0)) {
            if let Ok(Event::Key(key_event)) = event::read() {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('x') | KeyCode::Esc => {
                        println!("Exit key pressed - exiting...");
                        let _ = crossterm::terminal::disable_raw_mode();
                        exit_requested = true;
                    }
                    _ => {}
                }
            }
        }

        // --- Move exit check to the top of the loop ---
        if exit_requested {
            return false;
        }
        if !running.load(Ordering::SeqCst) {
            if let Err(e) = crossterm::terminal::disable_raw_mode() {
                println!("Failed to disable raw mode: {}", e);
            }
            // On exit, restore original positions for new windows
            let (grid_hwnds_vec, original_rects_map) = {
                // DashMap: clone keys and values into a HashMap for animation
                let grid_hwnds_vec: Vec<u64> =
                    grid_hwnds.iter().map(|entry| *entry.key()).collect();
                let original_rects: HashMap<u64, RECT> = original_rects
                    .iter()
                    .map(|entry| (*entry.key(), *entry.value()))
                    .collect();
                (grid_hwnds_vec, original_rects)
            };
            let mut tracker_guard = tracker.lock().unwrap();
            println!("Restoring new windows to original positions...");
            let animation_duration = Duration::from_millis(800);
            for hwnd in &grid_hwnds_vec {
                if let Some(rect) = original_rects_map.get(hwnd) {
                    let _ = tracker_guard.start_window_animation(
                        *hwnd,
                        *rect,
                        animation_duration.clone(),
                        EasingType::EaseInOut,
                    );
                }
            }
            for _ in 0..16 {
                let _ = tracker_guard.update_animations();
                thread::sleep(Duration::from_millis(
                    animation_duration.as_millis() as u64 / 16,
                ));
            }
            // Cleanup WinEvent hooks to avoid lingering background threads
            if win_event_cleanup.load(Ordering::SeqCst) {
                e_grid::window_events::cleanup_hooks();
                win_event_cleanup.store(false, Ordering::SeqCst);
            }
            println!("✅ Demo complete!");
            return false;
        }

        let mut tracker_guard = tracker.lock().unwrap();
        let all_hwnds: Vec<u64> = tracker_guard
            .windows
            .iter()
            .map(|entry| *entry.key())
            .filter(|hwnd| *hwnd != initial_focused_hwnd)
            .collect();

        // --- Distribute windows based on monitor resolution ---
        let monitor_count = tracker_guard.monitor_grids.len();
        let monitor_areas: Vec<i64> = tracker_guard
            .monitor_grids
            .iter()
            .map(|m| {
                let w = (m.monitor_rect.right - m.monitor_rect.left) as i64;
                let h = (m.monitor_rect.bottom - m.monitor_rect.top) as i64;
                w * h
            })
            .collect();
        let total_area: i64 = monitor_areas.iter().sum();

        // Assign windows proportionally to monitor area
        let mut windows_per_monitor: Vec<usize> = monitor_areas
            .iter()
            .map(|&area| {
                ((area as f64 / total_area as f64) * all_hwnds.len() as f64).round() as usize
            })
            .collect();

        // Adjust for rounding errors so sum matches total windows
        let mut assigned = windows_per_monitor.iter().sum::<usize>();
        while assigned < all_hwnds.len() {
            if let Some((idx, _)) = monitor_areas.iter().enumerate().max_by_key(|(_, &a)| a) {
                windows_per_monitor[idx] += 1;
                assigned += 1;
            }
        }
        while assigned > all_hwnds.len() {
            if let Some((idx, _)) = windows_per_monitor
                .iter()
                .enumerate()
                .filter(|(_, &n)| n > 0)
                .min_by_key(|(idx, _)| monitor_areas[*idx])
            {
                windows_per_monitor[idx] -= 1;
                assigned -= 1;
            }
        }

        // --- Define total_windows and grid_targets ---
        let total_windows = all_hwnds.len();
        let grid_targets: Vec<(u64, RECT)> = Vec::new();

        // 2. For each monitor, compute the optimal grid for its assigned windows
        let mut monitor_grids: Vec<(usize, usize, i32, i32)> = Vec::with_capacity(monitor_count);
        for (monitor_idx, monitor) in tracker_guard.monitor_grids.iter().enumerate() {
            let n = windows_per_monitor[monitor_idx];
            let (rows, cols) = optimal_grid(n, monitor.config.rows, monitor.config.cols);
            let grid_width = monitor.monitor_rect.right - monitor.monitor_rect.left;
            let grid_height = monitor.monitor_rect.bottom - monitor.monitor_rect.top;
            let cell_width = grid_width / cols as i32;
            let cell_height = grid_height / rows as i32;
            monitor_grids.push((rows, cols, cell_width, cell_height));
            println!(
                "Monitor {}: assigned_windows={}, optimal_grid={}x{}, cell_size={}x{}",
                monitor_idx, n, rows, cols, cell_width, cell_height
            );
        }

        // 3. Assign windows to grid cells on each monitor
        let mut hwnd_idx = 0;
        let mut grid_targets: Vec<(u64, RECT)> = Vec::new();

        // Track used cells for each monitor
        let mut monitor_used_cells: Vec<Vec<Vec<bool>>> = monitor_grids
            .iter()
            .map(|(rows, cols, _, _)| vec![vec![false; *cols]; *rows])
            .collect();

        // Map HWNDs to their assigned monitor/cell
        let mut hwnd_assignments: HashMap<u64, (usize, usize, usize)> = HashMap::new();

        // First, assign windows to existing empty cells (row-major order)
        for (monitor_idx, monitor) in tracker_guard.monitor_grids.iter().enumerate() {
            let n = windows_per_monitor[monitor_idx];
            let (rows, cols, cell_width, cell_height) = monitor_grids[monitor_idx];
            let used_cells = &mut monitor_used_cells[monitor_idx];

            for row in 0..rows {
                for col in 0..cols {
                    if hwnd_idx < total_windows {
                        if !used_cells[row][col] {
                            let hwnd = all_hwnds[hwnd_idx];
                            let left = monitor.monitor_rect.left + col as i32 * cell_width;
                            let top = monitor.monitor_rect.top + row as i32 * cell_height;
                            let right = if col + 1 == cols {
                                monitor.monitor_rect.right
                            } else {
                                left + cell_width
                            };
                            let bottom = if row + 1 == rows {
                                monitor.monitor_rect.bottom
                            } else {
                                top + cell_height
                            };
                            println!(
                                "Assign HWND 0x{:X} to monitor {}: cell ({},{}), requested rect=({},{} -> {},{}), cell_size={}x{}",
                                hwnd, monitor_idx, row, col, left, top, right, bottom, right-left, bottom-top
                            );
                            grid_targets.push((
                                hwnd,
                                RECT {
                                    left,
                                    top,
                                    right,
                                    bottom,
                                },
                            ));
                            used_cells[row][col] = true;
                            hwnd_assignments.insert(hwnd, (monitor_idx, row, col));
                            hwnd_idx += 1;
                        }
                    }
                }
            }
        }

        // If there are more windows than empty cells, assign remaining windows by expanding grid as before
        while hwnd_idx < total_windows {
            // Find monitor with largest area
            let monitor_idx = monitor_areas
                .iter()
                .enumerate()
                .max_by_key(|(_, &area)| area)
                .map(|(idx, _)| idx)
                .unwrap_or(0);
            let monitor = &tracker_guard.monitor_grids[monitor_idx];
            let (rows, cols, cell_width, cell_height) = monitor_grids[monitor_idx];
            let used_cells = &mut monitor_used_cells[monitor_idx];

            // Find first unused cell (or add a new row/col if needed)
            let mut found = false;
            for row in 0..rows {
                for col in 0..cols {
                    if !used_cells[row][col] {
                        let hwnd = all_hwnds[hwnd_idx];
                        let left = monitor.monitor_rect.left + col as i32 * cell_width;
                        let top = monitor.monitor_rect.top + row as i32 * cell_height;
                        let right = if col + 1 == cols {
                            monitor.monitor_rect.right
                        } else {
                            left + cell_width
                        };
                        let bottom = if row + 1 == rows {
                            monitor.monitor_rect.bottom
                        } else {
                            top + cell_height
                        };
                        println!(
                            "Assign HWND 0x{:X} to monitor {}: cell ({},{}), requested rect=({},{} -> {},{}), cell_size={}x{}",
                            hwnd, monitor_idx, row, col, left, top, right, bottom, right-left, bottom-top
                        );
                        grid_targets.push((
                            hwnd,
                            RECT {
                                left,
                                top,
                                right,
                                bottom,
                            },
                        ));
                        used_cells[row][col] = true;
                        hwnd_assignments.insert(hwnd, (monitor_idx, row, col));
                        hwnd_idx += 1;
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }
            if !found {
                // No empty cell, just assign to (0,0) of largest monitor
                let hwnd = all_hwnds[hwnd_idx];
                let left = monitor.monitor_rect.left;
                let top = monitor.monitor_rect.top;
                let right = left + cell_width;
                let bottom = top + cell_height;
                println!(
                    "Assign HWND 0x{:X} to monitor {}: cell (0,0), requested rect=({},{} -> {},{}), cell_size={}x{}",
                    hwnd, monitor_idx, left, top, right, bottom, right-left, bottom-top
                );
                grid_targets.push((
                    hwnd,
                    RECT {
                        left,
                        top,
                        right,
                        bottom,
                    },
                ));
                hwnd_assignments.insert(hwnd, (monitor_idx, 0, 0));
                hwnd_idx += 1;
            }
        }

        // Animate only new windows to their assigned grid cell if there is an opening
        let mut already_animated: HashSet<u64> = HashSet::new();
        let animation_duration = Duration::from_millis(800);
        for (hwnd, rect) in grid_targets.iter() {
            if *hwnd != initial_focused_hwnd && !already_animated.contains(hwnd) {
                let _ = tracker_guard.start_window_animation(
                    *hwnd,
                    *rect,
                    animation_duration.clone(),
                    EasingType::EaseInOut,
                );
                already_animated.insert(*hwnd);
            }
        }

        // Animation steps and exit key check
        let animation_duration_ms = animation_duration.as_millis() as u64;
        let frame_interval_ms = animation_duration_ms / 15; // 15 frames for smooth animation
        let animation_steps = (animation_duration_ms / frame_interval_ms).max(1);

        for _ in 0..animation_steps {
            if let Ok(true) = event::poll(std::time::Duration::from_millis(0)) {
                if let Ok(Event::Key(key_event)) = event::read() {
                    match key_event.code {
                        KeyCode::Char('q') | KeyCode::Char('x') | KeyCode::Esc => {
                            println!("Exit key pressed - animating windows back to original positions...");
                            let _ = crossterm::terminal::disable_raw_mode();
                            // Animate all windows (except initial focused) back to their original positions
                            let original_rects: HashMap<u64, RECT> = original_rects
                                .iter()
                                .map(|entry| (*entry.key(), *entry.value()))
                                .collect();
                            for (hwnd, rect) in original_rects.iter() {
                                if *hwnd != initial_focused_hwnd {
                                    let _ = tracker_guard.start_window_animation(
                                        *hwnd,
                                        *rect,
                                        animation_duration.clone(),
                                        {
                                            use rand::prelude::IndexedRandom;
                                            let variants: [e_grid::EasingType; 7] = [
                                                e_grid::EasingType::Linear,
                                                e_grid::EasingType::EaseIn,
                                                e_grid::EasingType::EaseOut,
                                                e_grid::EasingType::EaseInOut,
                                                e_grid::EasingType::Bounce,
                                                e_grid::EasingType::Elastic,
                                                e_grid::EasingType::Back,
                                            ];
                                            *variants.choose(&mut rand::rng()).unwrap()
                                        },
                                    );
                                }
                            }
                            for _ in 0..16 {
                                let _ = tracker_guard.update_animations();
                                thread::sleep(Duration::from_millis(
                                    animation_duration.as_millis() as u64 / 16,
                                ));
                            }
                            // Cleanup WinEvent hooks to avoid lingering background threads
                            if win_event_cleanup.load(Ordering::SeqCst) {
                                e_grid::window_events::cleanup_hooks();
                                win_event_cleanup.store(false, Ordering::SeqCst);
                            }
                            println!("✅ Demo complete!");
                            return false;
                        }
                        _ => {}
                    }
                }
            }
            let _ = tracker_guard.update_animations();
            thread::sleep(Duration::from_millis(frame_interval_ms as u64));
        }

        // After animation, print actual window sizes for debugging
        for (hwnd, rect) in grid_targets.iter() {
            if let Some(info) = tracker_guard.windows.get(hwnd) {
                let actual = info.window_rect;
                println!(
                    "Post-animation HWND 0x{:X}: actual rect=({},{} -> {},{}), size={}x{}, requested size={}x{}",
                    hwnd,
                    actual.left, actual.top, actual.right, actual.bottom,
                    actual.right - actual.left, actual.bottom - actual.top,
                    rect.right - rect.left, rect.bottom - rect.top
                );
            }
        }

        // Ensure initial focused window stays foreground
        if initial_focused_hwnd != 0 {
            unsafe {
                use winapi::um::winuser::SetForegroundWindow;
                SetForegroundWindow(initial_focused_hwnd as winapi::shared::windef::HWND);
            }
        }

        // Initial focused window remains in position, not animated
        //thread::sleep(Duration::from_millis(500));
        true
    })?;

    // Disable raw mode on exit (in case not already disabled)
    let _ = crossterm::terminal::disable_raw_mode();

    // Ensure WinEvent hooks are cleaned up if not already
    e_grid::window_events::cleanup_hooks();

    Ok(())
}
