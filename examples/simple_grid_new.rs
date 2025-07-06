use e_grid::{window_events, WindowTracker};
use std::ptr;
use std::sync::{Arc, Mutex};
use winapi::um::winuser::*;

fn main() {
    println!("Starting Simple Grid Tracker with Real-time Window Events...");

    // Initialize window tracker
    let mut tracker = WindowTracker::new();

    let (left, top, width, height) = tracker.get_monitor_info();
    println!(
        "Monitor area: {}x{} px (at {}, {})",
        width, height, left, top
    );

    println!("Scanning existing windows...");
    let start_time = std::time::Instant::now();
    tracker.scan_existing_windows();
    let scan_duration = start_time.elapsed();
    println!("Window scan completed in {:?}", scan_duration);

    println!("Found {} windows", tracker.windows.len());

    if tracker.windows.is_empty() {
        println!("No manageable windows found. This might indicate an issue.");
        println!("Press Enter to continue anyway...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
    }

    println!("Displaying initial grid...");
    tracker.print_all_grids();
    println!("Initial grid displayed successfully!");

    println!("Creating tracker arc...");
    let tracker_arc = Arc::new(Mutex::new(tracker));
    println!("Tracker arc created successfully!");

    println!("Setting up real-time window event tracking...");
    let config = window_events::WindowEventConfig::new(tracker_arc.clone()).with_debug(true);
    match window_events::setup_window_events(config) {
        Ok(()) => {
            println!("✅ Window event hooks set up successfully!");
            println!("🔄 Starting real-time event monitoring...");
            println!("📢 INSTRUCTIONS:");
            println!("   • Try opening/closing/moving windows to see real-time events!");
            println!("   • Move windows between monitors to test multi-monitor support!");
            println!("   • Watch for 🔔 WINDOW EVENT notifications");
            println!("   • Type 'g' and press Enter to print just the grid");
            println!("   • Press Ctrl+C to exit");
            println!();

            // Simple message loop - WinEvent hooks work in background
            unsafe {
                let mut msg = std::mem::zeroed::<MSG>();
                let mut message_count = 0;

                println!("🚀 Event monitoring is now active!");
                println!(
                    "   The program will automatically detect window changes via WinEvent hooks."
                );
                println!("   No polling needed - events are processed in real-time!");
                println!();

                loop {
                    let result = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);

                    if result == 0 {
                        // WM_QUIT received
                        println!("Received quit message, exiting...");
                        break;
                    } else if result == -1 {
                        // Error occurred
                        println!("Error in message loop, exiting...");
                        break;
                    }

                    message_count += 1;

                    // Show periodic status (but not too verbose)
                    if message_count % 10000 == 0 {
                        println!(
                            "� Message loop active ({} messages processed)",
                            message_count
                        );
                        println!("   Hooks are running - try creating/moving windows!");
                    }

                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to set up window event hooks: {}", e);
            println!("This is required for real-time window tracking.");
            println!("Please ensure you have the proper permissions and try again.");

            // Clean up any partial setup
            window_events::cleanup_hooks();

            println!("Press Enter to exit...");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
        }
    }

    println!("Cleaning up...");
    window_events::cleanup_hooks();
    println!("Shutting down...");
}
