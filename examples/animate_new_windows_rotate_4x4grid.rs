use crossterm::event::{self, Event, KeyCode};
use ctrlc;
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
    let grid_hwnds = Arc::new(Mutex::new(Vec::<u64>::new()));
    let original_rects = Arc::new(Mutex::new(HashMap::<u64, RECT>::new()));
    let known_hwnds = Arc::new(Mutex::new(HashSet::<u64>::new()));

    // Ensure tracker is initialized and monitors are available
    {
        let mut tracker_guard = tracker.lock().unwrap();
        tracker_guard.scan_existing_windows();
        for hwnd in tracker_guard.windows.iter() {
            known_hwnds.lock().unwrap().insert(*hwnd.key());
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

    let event_callback = move |event: e_grid::ipc_protocol::GridEvent| {
        println!("🟢 [DEBUG] WinEvent callback invoked: {:?}", event);

        if let e_grid::ipc_protocol::GridEvent::WindowCreated { hwnd, .. } = event {
            println!("🆕 Window created: HWND 0x{:X}", hwnd);
            let mut known = known_hwnds_clone.lock().unwrap();
            if known.contains(&hwnd) {
                println!("🟡 [DEBUG] HWND 0x{:X} already known, skipping.", hwnd);
                return;
            }
            known.insert(hwnd);

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
                        original_rects_clone.lock().unwrap().insert(hwnd, *rect);
                    } else if let Some(rect) = WindowTracker::get_window_rect(hwnd) {
                        // fallback if not in tracker.windows
                        println!(
                            "🔍 [CHANNEL] Fallback: Saving original rect for HWND 0x{:X}: left={}, top={}, right={}, bottom={}",
                            hwnd, rect.left, rect.top, rect.right, rect.bottom
                        );
                        original_rects_clone.lock().unwrap().insert(hwnd, rect);
                    } else {
                        println!("🔴 [CHANNEL] No rect found for HWND 0x{:X}", hwnd);
                    }

                    println!("🔍 [CHANNEL] Adding HWND 0x{:X} to grid_hwnds", hwnd);
                    grid_hwnds_clone.lock().unwrap().push(hwnd);
                    let grid_hwnds_now = grid_hwnds_clone.lock().unwrap().clone();
                    println!("🔍 [CHANNEL] Current grid_hwnds: {:?}", grid_hwnds_now);

                    // Monitor info and animation
                    let (monitor_rect, rows, cols) = {
                        let monitor = tracker.monitor_grids.iter()
                            .find(|m| m.monitor_id == 1)
                            .unwrap_or(&tracker.monitor_grids[1]);
                        (monitor.monitor_rect, monitor.config.rows, monitor.config.cols)
                    };

                    let grid_size = grid_hwnds_now.len().next_power_of_two().min(rows.min(cols));
                    println!("🔍 [CHANNEL] Grid size for animation: {}", grid_size);

                    let mut positions: Vec<RECT> = Vec::new();

                    for idx in 0..grid_hwnds_now.len() {
                        let row = idx / grid_size;
                        let col = idx % grid_size;
                        let cell_width = (monitor_rect.right - monitor_rect.left) / grid_size as i32;
                        let cell_height = (monitor_rect.bottom - monitor_rect.top) / grid_size as i32;
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

                    println!("🔍 [CHANNEL] Issuing animation commands...");
                    for (hwnd, rect) in grid_hwnds_now.iter().zip(&positions) {
                        println!("Animating HWND 0x{:X} to rect ({},{})-({},{}) on monitor_id=1", hwnd, rect.left, rect.top, rect.right, rect.bottom);
                        let res = tracker.start_window_animation(
                            *hwnd,
                            *rect,
                            Duration::from_millis(800),
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
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            })).unwrap();
        }
    };

    // Setup WinEvent hooks with our event callback
    let mut config = WindowEventConfig::new(tracker.clone(), e_grid::EventDispatchMode::Open);
    config = config.with_event_callback(event_callback);
    e_grid::window_events::setup_window_events(config)?;

    // Enable raw mode so key events are captured immediately
    crossterm::terminal::enable_raw_mode()?;

    println!("Waiting for new windows... (Press Ctrl+C, q, x, or Esc to exit)");

    // Main loop: rotate grid windows and handle exit keys
    run_message_loop(|| {
        // Check for exit keys (q, x, Esc) without waiting for Enter
        let mut exit_requested = false;
        if let Ok(true) = event::poll(std::time::Duration::from_millis(0)) {
            if let Ok(Event::Key(key_event)) = event::read() {
                match key_event.code {
                    KeyCode::Char('q') | KeyCode::Char('x') => {
                        println!("Exit key pressed (q/x) - exiting...");
                        let _ = crossterm::terminal::disable_raw_mode();
                        exit_requested = true;
                    }
                    KeyCode::Esc => {
                        println!("Escape key pressed - exiting...");
                        let _ = crossterm::terminal::disable_raw_mode();
                        exit_requested = true;
                    }
                    _ => {}
                }
            }
        }

        if exit_requested || !running.load(Ordering::SeqCst) {
            if let Err(e) = crossterm::terminal::disable_raw_mode() {
                println!("Failed to disable raw mode: {}", e);
            }
            // On exit, restore original positions for new windows
            let (grid_hwnds_vec, original_rects_map) = {
                let grid_hwnds = grid_hwnds.lock().unwrap();
                let original_rects = original_rects.lock().unwrap();
                (grid_hwnds.clone(), original_rects.clone())
            };
            let mut tracker_guard = tracker.lock().unwrap();
            println!("Restoring new windows to original positions...");
            for hwnd in &grid_hwnds_vec {
                if let Some(rect) = original_rects_map.get(hwnd) {
                    let _ = tracker_guard.start_window_animation(
                        *hwnd,
                        *rect,
                        Duration::from_millis(700),
                        EasingType::EaseInOut,
                    );
                }
            }
            for _ in 0..15 {
                let _ = tracker_guard.update_animations();
                thread::sleep(Duration::from_millis(50));
            }
            // Cleanup WinEvent hooks to avoid lingering background threads
            if win_event_cleanup.load(Ordering::SeqCst) {
                e_grid::window_events::cleanup_hooks();
                win_event_cleanup.store(false, Ordering::SeqCst);
            }
            println!("✅ Demo complete!");
            return false;
        }

        // Rotate grid windows if more than 1
        let (do_rotate, grid_hwnds_vec) = {
            let grid_hwnds = grid_hwnds.lock().unwrap();
            (grid_hwnds.len() > 1, grid_hwnds.clone())
        };
        if do_rotate {
            let grid_size = grid_hwnds_vec.len().next_power_of_two().min(rows.min(cols));
            let mut positions: Vec<RECT> = Vec::new();
            for idx in 0..grid_hwnds_vec.len() {
                let row = idx / grid_size;
                let col = idx % grid_size;
                let cell_width = (monitor_rect.right - monitor_rect.left) / grid_size as i32;
                let cell_height = (monitor_rect.bottom - monitor_rect.top) / grid_size as i32;
                positions.push(RECT {
                    left: monitor_rect.left + col as i32 * cell_width,
                    top: monitor_rect.top + row as i32 * cell_height,
                    right: monitor_rect.left + (col as i32 + 1) * cell_width,
                    bottom: monitor_rect.top + (row as i32 + 1) * cell_height,
                });
            }
            // Rotate the vector (need to update the shared state)
            {
                let mut grid_hwnds = grid_hwnds.lock().unwrap();
                grid_hwnds.rotate_right(1);
            }
            // Use the rotated vector for animation
            let grid_hwnds_rotated = {
                let grid_hwnds = grid_hwnds.lock().unwrap();
                grid_hwnds.clone()
            };
            let mut tracker_guard = tracker.lock().unwrap();
            for (hwnd, rect) in grid_hwnds_rotated.iter().zip(&positions) {
                let _ = tracker_guard.start_window_animation(
                    *hwnd,
                    *rect,
                    Duration::from_millis(700),
                    EasingType::EaseInOut,
                );
            }
            for _ in 0..14 {
                let _ = tracker_guard.update_animations();
                thread::sleep(Duration::from_millis(50));
            }
        }

        thread::sleep(Duration::from_millis(500));
        true
    })?;

    // Disable raw mode on exit (in case not already disabled)
    let _ = crossterm::terminal::disable_raw_mode();

    // Ensure WinEvent hooks are cleaned up if not already
    e_grid::window_events::cleanup_hooks();

    Ok(())
}
