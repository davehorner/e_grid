use e_grid::{GridClient, GridClientResult};
use std::thread;
use std::time::Duration;

/// Simple focus event demonstration
/// This example shows the basic usage of GridClient's focus callback feature
fn main() -> GridClientResult<()> {
    println!("🎯 Simple Focus Event Demo");
    println!("==========================");
    println!("This demo shows basic window focus event tracking.\n");

    // Create grid client
    println!("🔧 Initializing GridClient...");
    let mut grid_client = GridClient::new()?;

    // Set up a simple focus callback
    println!("📋 Registering focus callback...");
    grid_client.set_focus_callback(|focus_event| {
        let event_name = if focus_event.event_type == 0 {
            "🟢 FOCUSED"
        } else {
            "🔴 DEFOCUSED"
        };

        println!(
            "{} - Window: {} (PID: {}) at timestamp: {}",
            event_name, focus_event.hwnd, focus_event.process_id, focus_event.timestamp
        );

        // Show application hash for identification
        if focus_event.app_name_hash != 0 {
            println!("   📱 App Hash: 0x{:x}", focus_event.app_name_hash);
        }

        // Show window title hash if available
        if focus_event.window_title_hash != 0 {
            println!("   🪟 Title Hash: 0x{:x}", focus_event.window_title_hash);
        }

        println!("   ─────────────────────────────");
    })?;

    // Start monitoring
    println!("📡 Starting focus event monitoring...");
    grid_client.start_background_monitoring().map_err(|e| {
        e_grid::GridClientError::InitializationError(format!("Failed to start monitoring: {}", e))
    })?;

    println!("✅ Focus monitoring active!");
    println!();
    println!("💡 Instructions:");
    println!("   - Click on different windows to see FOCUS events");
    println!("   - Click away from windows to see DEFOCUS events");
    println!("   - Try switching between different applications");
    println!("   - Notice the different app hashes for different programs");
    println!();
    println!("🔍 Watching for focus events... (Press Ctrl+C to exit)");
    println!("=====================================================\n");

    // Keep the demo running
    let mut seconds = 0;
    loop {
        thread::sleep(Duration::from_secs(1));
        seconds += 1;

        // Print a status update every 30 seconds
        if seconds % 30 == 0 {
            println!(
                "📊 Demo has been running for {} seconds - focus different windows to see events",
                seconds
            );
        }
    }
}
