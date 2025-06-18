use e_grid::{ipc_server, WindowTracker};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use winapi::um::winuser::{
    DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 E-Grid IPC Server Demo - Integrated WinEvent Mode");
    println!("====================================================");
    println!("Starting server with integrated WinEvent monitoring:");
    println!("  🔔 Real-time window event detection (create, move, destroy)");
    println!("  📤 Automatic publishing of window details to clients");
    println!("  📨 Processing client commands automatically");
    println!("  🔄 No polling - pure event-driven architecture");
    println!();

    // Create the window tracker
    let mut tracker = WindowTracker::new();
    println!("📊 Initializing window tracking...");
    tracker.scan_existing_windows();
    tracker.print_grid();

    let tracker = Arc::new(Mutex::new(tracker));

    // Create and setup the IPC server
    let mut ipc_server = ipc_server::GridIpcServer::new(tracker.clone())?;
    println!("\n🔧 Setting up IPC server...");
    ipc_server.setup_services()?;

    // Start IPC server monitoring
    println!("\n🔄 Starting IPC server monitoring...");
    ipc_server.start_background_event_loop()?;

    // Setup integrated WinEvent hooks for real-time monitoring
    println!("\n🔗 Setting up integrated WinEvent monitoring...");
    if let Err(e) = ipc_server.setup_window_events() {
        println!("⚠️ Failed to setup WinEvents: {}", e);
        println!("   Continuing without real-time event monitoring...");
    } // Give the server a moment to be ready
    thread::sleep(Duration::from_millis(500));

    // Don't publish initial window details automatically - wait for client requests
    println!("\n⏳ Server ready and waiting for client requests...");

    // Print a summary of what's being tracked
    if let Ok(tracker) = tracker.lock() {
        println!("📊 Server tracking {} windows total", tracker.windows.len());
        for (i, (hwnd, window_info)) in tracker.windows.iter().enumerate() {
            if i < 10 {
                // Show first 10 windows
                println!(
                    "   Window {}: HWND {:02X} - '{}'",
                    i + 1,
                    (*hwnd as u64) % 100,
                    window_info.title.chars().take(30).collect::<String>()
                );
            }
        }
        if tracker.windows.len() > 10 {
            println!("   ... and {} more windows", tracker.windows.len() - 10);
        }
    }

    println!("\n✅ IPC server is now running with integrated WinEvent monitoring!");
    println!("  📨 Client commands (GetWindowList, GetGridState, AssignWindow, etc.)");
    println!("  🔔 Real-time window events (create, move, destroy) via WinEvents");
    println!("  📤 Automatic publishing of updates to connected clients");
    println!();
    println!("📊 Server Statistics:");
    if let Ok(tracker) = tracker.lock() {
        println!("  Windows tracked: {}", tracker.windows.len());
        println!("  Grid size: {}x{}", e_grid::GRID_ROWS, e_grid::GRID_COLS);
        println!("  Monitors: {}", tracker.monitor_grids.len());
    }
    println!();
    println!("🎯 To test the server:");
    println!("  1. Run the client demo: cargo run --bin grid_client_demo");
    println!("  2. Move windows around to see real-time updates");
    println!("  3. Use client commands to assign windows to grid cells");
    println!();
    println!("Press Ctrl+C to stop the server..."); // Keep the main thread alive with responsive command processing
                                                    // WinEvents require a proper Windows message loop to work
    println!("🔄 Starting Windows message loop for WinEvent processing...");
    let mut iteration = 0;
    let mut last_status_time = std::time::Instant::now();

    loop {
        // Process Windows messages for WinEvent hooks
        unsafe {
            let mut msg: MSG = std::mem::zeroed();

            // Process all available messages without blocking
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Show heartbeat every 100 iterations (1 second)
        if iteration % 100 == 0 {
            println!("💓 Server heartbeat - iteration {}", iteration);
        }

        // Process IPC commands frequently for responsiveness
        if iteration % 10 == 0 {  // Only process commands every 100ms to reduce load
            match ipc_server.process_commands() {
                Ok(()) => {
                    // Commands processed successfully (no output unless there were commands)
                }
                Err(e) => {
                    println!("❌ Error processing commands: {}", e);
                }
            }
        }
        // Small sleep to prevent busy waiting while still being responsive
        thread::sleep(Duration::from_millis(10));
        iteration += 1;
        // Update grids periodically (every 2 seconds) to handle changes from WinEvents
        // This is done outside WinEvent callbacks to prevent deadlocks
        if iteration % 200 == 0 {
            // 200 * 10ms = 2 seconds
            println!("🔄 Attempting grid update...");
            match tracker.try_lock() {
                Ok(mut tracker_lock) => {
                    let old_count = tracker_lock.windows.len();
                    
                    // Since WinEvents now do minimal processing, we need to periodically
                    // rescan for windows to catch changes
                    tracker_lock.scan_existing_windows();
                    tracker_lock.update_grid();
                    tracker_lock.update_monitor_grids();
                    
                    let new_count = tracker_lock.windows.len();

                    if old_count != new_count || iteration % 1000 == 0 {
                        // Also print every 10 seconds
                        println!("🔄 Grid updated: {} windows tracked (was {})", new_count, old_count);
                        tracker_lock.print_grid();

                        // Print monitor grids too
                        if !tracker_lock.monitor_grids.is_empty() {
                            println!("🖥️ Monitor Grids:");
                            for (i, monitor) in tracker_lock.monitor_grids.iter().enumerate() {
                                println!(
                                    "  Monitor {}: {}x{}",
                                    i,
                                    monitor.monitor_rect.2 - monitor.monitor_rect.0,
                                    monitor.monitor_rect.3 - monitor.monitor_rect.1
                                );
                                monitor.print_grid();
                            }
                        }
                    }
                }
                Err(_) => {
                    println!("⚠️ Could not acquire tracker lock for grid update");
                }
            }

            // Don't automatically republish all window details - only send updates when requested
            // or when individual windows change (via WinEvents)
        }

        // Print status every 30 seconds - just for monitoring, no polling
        if last_status_time.elapsed().as_secs() >= 30 {
            let status_count = iteration / 3000; // Roughly every 30 seconds
            println!("\n📊 Server Status Update #{}", status_count);
            if let Ok(tracker) = tracker.lock() {
                println!("  🔄 Active windows: {}", tracker.windows.len());

                // Print the current virtual grid
                println!("  📱 Virtual Grid State:");
                tracker.print_grid();

                // Print all monitor grids
                println!(
                    "  🖥️ Monitor Grids ({} monitors):",
                    tracker.monitor_grids.len()
                );
                for (i, monitor) in tracker.monitor_grids.iter().enumerate() {
                    println!(
                        "    Monitor {} ({}x{} at {},{}):",
                        i,
                        monitor.monitor_rect.2 - monitor.monitor_rect.0,
                        monitor.monitor_rect.3 - monitor.monitor_rect.1,
                        monitor.monitor_rect.0,
                        monitor.monitor_rect.1
                    );
                    monitor.print_grid();
                }

                // Show recent window activity
                if !tracker.windows.is_empty() {
                    println!("  📋 Recent windows:");
                    for (i, (_hwnd, window)) in tracker.windows.iter().take(5).enumerate() {
                        let title = if window.title.len() > 40 {
                            format!("{}...", &window.title[..40])
                        } else {
                            window.title.clone()
                        };
                        println!(
                            "    {}. {} [{}x{} at {},{}]",
                            i + 1,
                            title,
                            window.rect.right - window.rect.left,
                            window.rect.bottom - window.rect.top,
                            window.rect.left,
                            window.rect.top
                        );
                    }
                    if tracker.windows.len() > 5 {
                        println!("    ... and {} more", tracker.windows.len() - 5);
                    }
                }
            }
            println!("  🟢 Server running normally - real-time events active");

            // Republish window details periodically to help clients stay in sync
            println!("  📤 Republishing window details...");
            if let Err(e) = ipc_server.publish_all_window_details() {
                println!("  ⚠️ Failed to republish window details: {}", e);
            } else {
                println!("  ✅ Republished window details for connected clients");
            }

            last_status_time = std::time::Instant::now();
        }
    }
}
