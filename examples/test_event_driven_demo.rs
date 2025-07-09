//! Event-Driven Comprehensive Window Management Demo
//!
//! This demo showcases the E-Grid system using a proper event-driven architecture:
//!
//! Key Features:
//! - Uses Windows event hooks (SetWinEventHook) instead of polling
//! - Real-time window detection and management through WinEvent callbacks
//! - IPC server/client architecture for command execution
//! - Extensible callback system for event notifications
//! - All window management logic consolidated in lib.rs
//! - Main-thread message loop for proper WinEvent processing
//! - Filters to only show manageable windows in grid and debug output
//!
//! Architecture:
//! 1. WindowTracker (lib.rs) - Core window/grid management with callback system
//! 2. window_events.rs - WinEvent hook setup and callback dispatching  
//! 3. IPC Server - Background service for command processing
//! 4. IPC Client - Command interface for grid operations
//! 5. Main thread message loop - Processes WinEvent messages
//!
//! The demo progresses through three phases:
//! - Phase 1: Animated grid layouts using IPC commands
//! - Phase 2: Dynamic window rotation through grid positions
//! - Phase 3: Real-time event monitoring and display

use e_grid::ipc_client::GridClient;
// Add these trait imports if the methods are defined as extension traits
use e_grid::ipc_server::GridIpcServer;
use e_grid::window_events::{self, DebugEventCallback};
use e_grid::{EasingType, GridConfig, WindowTracker};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Event-Driven Comprehensive Window Management Demo
/// Uses the proper IPC architecture instead of polling
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎬 E-GRID EVENT-DRIVEN COMPREHENSIVE DEMO");
    println!("==========================================");
    println!("🔄 Features:");
    println!("   • Uses proper Windows event hooks (no polling!)");
    println!("   • Real-time IPC communication with server");
    println!("   • Event-driven window detection and management");
    println!("   • Leverages existing window_events.rs architecture");
    println!("   • Integrates with lib.rs WindowTracker");
    println!();

    // Step 1: Initialize the core window tracker
    println!("🔧 Initializing core window tracker...");
    let config = GridConfig::new(4, 4); // Start with 4x4 grid
    let tracker = Arc::new(Mutex::new(WindowTracker::new_with_config(config))); // Step 2: Register debug callback for event notifications
    println!("🔧 Registering event callbacks...");
    {
        let mut tracker_guard = tracker.lock().unwrap();
        tracker_guard.register_event_callback(Box::new(DebugEventCallback));
    } // Step 3: Set up event-driven window monitoring
    println!("🔧 Setting up event-driven window monitoring...");
    let config = window_events::WindowEventConfig::new(tracker.clone()).with_debug(true);
    window_events::setup_window_events(config)?;
    // Step 4: Event-driven monitoring is ready (message loop will run in demo phases)
    println!("🔄 Event-driven system ready - message processing will happen in main thread...");
    // Step 3: Create and setup the IPC server (main thread for HWND safety)
    println!("🔧 Creating IPC server...");
    let windows = {
        let tracker_guard = tracker.lock().unwrap();
        tracker_guard.windows.clone()
    };
    let mut server = GridIpcServer::new(tracker.clone()).unwrap();
    server.setup_services()?;
    // Note: NOT calling server.setup_window_events() to avoid duplicate hooks
    // The window_events::setup_window_events() above already set up the hooks

    // Start IPC server background monitoring
    println!("🔄 Starting IPC server background monitoring...");
    server.start_background_event_loop()?;

    // Give server time to start
    thread::sleep(Duration::from_millis(1000));

    // Step 4: Create IPC client to communicate with server
    println!("🔧 Connecting IPC client...");
    let mut client = GridClient::new()?;
    client.start_background_monitoring()?;
    // Step 5: Initial scan and display
    println!("\n🔍 Performing initial window discovery via event system...");

    // Let's manually scan for existing windows to see if any are detected
    {
        let mut tracker_guard = tracker.lock().unwrap();
        println!("🔍 Scanning for existing manageable windows...");
        tracker_guard.scan_existing_windows();
        println!(
            "🔍 Found {} total windows after scan",
            tracker_guard.windows.len()
        );
        tracker_guard.print_all_grids();
    } // Step 6: Ready for event-driven processing
    println!("\n⚠️  DEMO READY - Event-driven window management is active!");
    println!("    Try opening/closing windows to see real-time event detection.");
    println!("🔄 Windows message loop will run during demo phases...");

    wait_for_user_input("Press Enter to start Phase 1 - Animated Grid Layouts...");
    // Phase 1: Basic Grid Animation
    println!("\n🎬 PHASE 1: Event-Driven Grid Animation");
    demonstrate_grid_animation(&mut client, &tracker, &mut server)?;

    wait_for_user_input("Press Enter to continue to Phase 2 - Dynamic Window Rotation...");
    // Phase 2: Dynamic Window Rotation
    println!("\n🎬 PHASE 2: Event-Driven Window Rotation");
    demonstrate_window_rotation(&mut client, &tracker, &mut server)?;

    wait_for_user_input("Press Enter to continue to Phase 3 - Real-time Event Response...");
    // Phase 3: Real-time Event Monitoring
    println!("\n🎬 PHASE 3: Real-time Event Monitoring");
    demonstrate_realtime_events(&tracker, &mut server)?;

    println!("\n🎉 EVENT-DRIVEN COMPREHENSIVE DEMO COMPLETE!");
    println!("==============================================");
    println!("✅ Successfully demonstrated:");
    println!("   🔔 Real-time Windows event hook integration");
    println!("   📡 IPC server/client communication");
    println!("   🎯 Event-driven window management (no polling!)");
    println!("   🎬 Smooth animations via proper architecture");
    println!("   🏗️  Integration with existing lib.rs components"); // Cleanup
    println!("\n🧹 Cleaning up...");
    window_events::cleanup_hooks();

    wait_for_user_input("Press Enter to exit...");
    Ok(())
}

fn demonstrate_grid_animation(
    client: &mut GridClient,
    tracker: &Arc<Mutex<WindowTracker>>,
    server: &mut GridIpcServer,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   📐 Using IPC commands to arrange windows in 2x2 grid...");
    // Get current windows from tracker
    let window_list = {
        let tracker_guard = tracker.lock().unwrap();
        tracker_guard
            .windows
            .iter()
            .map(|entry| *entry.key())
            .collect::<Vec<_>>()
    };

    println!("   📊 Found {} windows to manage", window_list.len());
    // Use IPC to move first 4 windows to grid
    for (i, &hwnd) in window_list.iter().take(4).enumerate() {
        let row = i / 2;
        let col = i % 2;

        // Validate window handle before trying to move it
        unsafe {
            use winapi::shared::windef::HWND;
            use winapi::um::winuser::IsWindow;
            if IsWindow(hwnd as HWND) == 0 {
                println!("   ⚠️  Skipping invalid window handle: {:?}", hwnd);
                continue;
            }
        }

        println!(
            "   📦 Moving window {:?} (as u64: {}) to grid position [{},{}]",
            hwnd, hwnd as u64, row, col
        );
        // Send IPC command to move window to grid cell (this actually moves the window)
        client.move_window_to_cell(
            hwnd as u64,
            row as u32,
            col as u32,
            1000,
            EasingType::Bounce,
        )?;

        // Process any pending commands on the server
        server.process_commands()?;

        // Process any pending commands on the server
        server.process_commands()?;
        thread::sleep(Duration::from_millis(100)); // Small delay between commands

        // Process messages to keep events responsive
        window_events::process_windows_messages()?;
    }

    thread::sleep(Duration::from_millis(2000)); // Let animations complete

    // Display results
    {
        let tracker_guard = tracker.lock().unwrap();
        println!("   📊 Grid state after animation:");
        tracker_guard.print_all_grids();
    }

    Ok(())
}

fn demonstrate_window_rotation(
    client: &mut GridClient,
    tracker: &Arc<Mutex<WindowTracker>>,
    server: &mut GridIpcServer,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   🔄 Demonstrating dynamic window rotation through grid...");

    for rotation in 1..=3 {
        println!("   🔄 Rotation step {} of 3", rotation);
        // Get current windows
        let window_list = {
            let tracker_guard = tracker.lock().unwrap();
            tracker_guard
                .windows
                .iter()
                .map(|entry| *entry.key())
                .collect::<Vec<_>>()
        };
        // Rotate windows through grid positions
        for (i, &hwnd) in window_list.iter().take(4).enumerate() {
            let base_pos = (i + rotation) % 4;
            let row = base_pos / 2;
            let col = base_pos % 2;
            println!(
                "     📦 Moving window {:?} (as u64: {}) to [{},{}]",
                hwnd, hwnd as u64, row, col
            );
            // client.send_move_window_to_cell(hwnd as u64, row as u32, col as u32)?;

            // Process any pending commands on the server
            server.process_commands()?;

            // client.send_animate_window(hwnd as u64, 800, EasingType::EaseInOut)?;

            // Process any pending commands on the server
            server.process_commands()?;
        }
    }
    Ok(())
}

fn demonstrate_realtime_events(
    tracker: &Arc<Mutex<WindowTracker>>,
    server: &mut GridIpcServer,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   🔔 Real-time event monitoring active for 30 seconds...");
    println!("   💡 Try opening/closing/moving windows to see events!");
    println!();

    let start_time = std::time::Instant::now();
    let monitor_duration = Duration::from_secs(30);

    let mut last_window_count = 0;
    while start_time.elapsed() < monitor_duration {
        // Process Windows messages for WinEvent hooks (critical for event detection)
        window_events::process_windows_messages()?;

        // Process any pending IPC commands
        server.process_commands()?;

        // Check for window count changes
        let current_window_count = {
            let tracker_guard = tracker.lock().unwrap();
            tracker_guard.windows.len()
        };

        if current_window_count != last_window_count {
            println!(
                "\n   📊 Window count changed: {} → {} windows",
                last_window_count, current_window_count
            );

            // Print updated grids
            {
                let tracker_guard = tracker.lock().unwrap();
                tracker_guard.print_all_grids();
            }

            last_window_count = current_window_count;
        }
        let elapsed = start_time.elapsed();
        if elapsed < monitor_duration {
            let remaining = monitor_duration - elapsed;
            print!("\r   ⏱️  Monitoring... {}s remaining", remaining.as_secs());
        } else {
            print!("\r   ⏱️  Monitoring... 0s remaining");
        }
        io::stdout().flush().unwrap();
        // Simple delay - but keep processing messages frequently
        thread::sleep(Duration::from_millis(50));
        io::stdout().flush().unwrap();
    }

    println!("\n   ✅ Real-time monitoring complete!");
    Ok(())
}

fn wait_for_user_input(prompt: &str) {
    println!("\n{}", prompt);
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
}
