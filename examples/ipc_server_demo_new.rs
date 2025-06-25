use e_grid::{ipc_server, WindowTracker};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
    let windows = {
         let tracker_guard = tracker.lock().unwrap();
        tracker_guard.windows.clone()
    };
    let mut ipc_server = ipc_server::GridIpcServer::new(tracker.clone(),Arc::new(windows))?;
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
    }

    // Give the server a moment to be ready
    thread::sleep(Duration::from_millis(500));

    // Publish initial window details for any connected clients
    println!("\n📤 Publishing initial window state...");
    if let Err(e) = ipc_server.publish_all_window_details() {
        println!("⚠️ Failed to publish initial window details: {}", e);
    } else {
        println!("✅ Initial window state published successfully");
    }

    println!("\n✅ IPC server is now running with integrated WinEvent monitoring!");
    println!("  📨 Client commands (GetWindowList, GetGridState, AssignWindow, etc.)");
    println!("  🔔 Real-time window events (create, move, destroy) via WinEvents");
    println!("  📤 Automatic publishing of updates to connected clients");
    println!();
    println!("📊 Server Statistics:");
    if let Ok(tracker) = tracker.lock() {
        println!("  Windows tracked: {}", tracker.windows.len());
        println!(
            "  Grid size: {}x{}",
            ipc_server.get_config().rows,
            ipc_server.get_config().cols
        );
        println!("  Monitors: {}", tracker.monitor_grids.len());
    }
    println!();
    println!("🎯 To test the server:");
    println!("  1. Run the client demo: cargo run --bin grid_client_demo");
    println!("  2. Move windows around to see real-time updates");
    println!("  3. Use client commands to assign windows to grid cells");
    println!();
    println!("Press Ctrl+C to stop the server...");

    // Keep the main thread alive with responsive command processing
    // WinEvents will trigger callbacks automatically for real-time updates
    let mut iteration = 0;
    loop {
        // Process commands frequently for responsiveness
        if let Err(e) = ipc_server.process_commands() {
            println!("❌ Error processing commands: {}", e);
        }

        thread::sleep(Duration::from_millis(100));

        // Print status every 30 seconds - just for monitoring, no polling
        if iteration % 300 == 0 && iteration > 0 {
            // 300 * 100ms = 30 seconds
            println!("\n📊 Server Status Update #{}", iteration / 300);
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
                    for (i, entry) in tracker.windows.iter().take(5).enumerate() {
                        let (_hwnd, window) = entry.pair();
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
        }

        iteration += 1;
    }
}
