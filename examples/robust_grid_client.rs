use std::time::Duration;
use std::thread;

/// Robust example demonstrating proper error handling and focus callback integration
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 GridClient Robust Implementation Example");
    println!("==========================================");

    // Create a grid client with proper error handling
    let mut grid_client = match e_grid::GridClient::new() {
        Ok(client) => {
            println!("✅ GridClient created successfully");
            client
        }
        Err(e) => {
            eprintln!("❌ Failed to create GridClient: {}", e);
            return Err(e.into());
        }
    };
    
    // Demonstrate coordinate validation
    println!("\n🔍 Testing coordinate validation...");
    let config = grid_client.get_config();
    println!("   Grid size: {}x{}", config.rows, config.cols);
    
    // Test valid coordinates
    match e_grid::validate_grid_coordinates(1, 2, config.rows as u32, config.cols as u32) {
        Ok(_) => println!("   ✅ Valid coordinates (1, 2)"),
        Err(e) => println!("   ❌ Coordinate validation failed: {}", e),
    }
    
    // Test invalid coordinates
    match e_grid::validate_grid_coordinates(10, 20, config.rows as u32, config.cols as u32) {
        Ok(_) => println!("   ⚠️  Invalid coordinates were accepted!"),
        Err(e) => println!("   ✅ Invalid coordinates properly rejected: {}", e),
    }

    // Register a robust focus callback with error handling
    println!("\n🎯 Registering focus callback...");
    let callback_result = grid_client.set_focus_callback(|focus_event| {
        let event_type = if focus_event.is_focused { "FOCUSED" } else { "DEFOCUSED" };
        let app_name = String::from_utf8_lossy(
            &focus_event.app_name[..focus_event.app_name_len.min(256) as usize]
        );
        
        println!("🎵 [FOCUS CALLBACK] {} - Window: {} - App: '{}'", 
            event_type, focus_event.hwnd, app_name);
            
        // In real e_midi integration, this would:
        match focus_event.is_focused {
            true => {
                println!("   🎶 Starting music for app: {}", app_name);
                // - Look up or assign a song for this app
                // - Start/resume MIDI playback
                // - Update spatial audio based on window position
            }
            false => {
                println!("   🔇 Pausing music for app: {}", app_name);
                // - Pause current MIDI playback
                // - Save playback position for later resume
            }
        }
    });
    
    match callback_result {
        Ok(_) => println!("   ✅ Focus callback registered successfully"),
        Err(e) => {
            eprintln!("   ❌ Failed to register focus callback: {}", e);
            return Err(e.into());
        }
    }

    // Start background monitoring with error handling
    println!("\n📡 Starting background monitoring...");
    match grid_client.start_background_monitoring() {
        Ok(_) => println!("   ✅ Background monitoring started"),
        Err(e) => {
            eprintln!("   ❌ Failed to start background monitoring: {}", e);
            return Err(e.into());
        }
    }

    // Demonstrate safe window assignment with validation
    println!("\n🏠 Testing window assignment with validation...");
    let test_hwnd = 12345u64;
    
    // Test valid assignment
    match grid_client.assign_window_to_virtual_cell(test_hwnd, 1, 2) {
        Ok(_) => println!("   ✅ Window assignment command sent successfully"),
        Err(e) => println!("   ⚠️  Window assignment failed: {}", e),
    }
    
    // Test invalid assignment (should fail validation)
    match grid_client.assign_window_to_virtual_cell(test_hwnd, 100, 200) {
        Ok(_) => println!("   ⚠️  Invalid assignment was accepted!"),
        Err(e) => println!("   ✅ Invalid assignment properly rejected: {}", e),
    }

    // Show current configuration
    println!("\n⚙️  Current Configuration:");
    println!("   Grid Size: {}x{}", config.rows, config.cols);
    println!("   Focus Callback: {}", 
        if grid_client.has_focus_callback() { "Registered" } else { "Not registered" });
    println!("   Auto Display: {}", 
        if grid_client.is_auto_display_enabled() { "Enabled" } else { "Disabled" });

    println!("\n📻 Listening for window focus events and grid updates...");
    println!("💡 Focus different windows to see the callback in action");
    println!("💡 Move or resize windows to see grid updates");
    println!("⌨️  Press Ctrl+C to exit\n");
    
    // Keep the example running with periodic status updates
    let mut iteration = 0u32;
    loop {
        thread::sleep(Duration::from_secs(10));
        iteration += 1;
        
        if iteration % 6 == 0 { // Every minute
            println!("📊 Status: Running for {} minutes - Focus callback: {}",
                iteration / 6,
                if grid_client.has_focus_callback() { "Active" } else { "Inactive" }
            );
        }
    }
}
