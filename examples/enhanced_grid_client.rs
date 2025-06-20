use e_grid::{
    GridClient, GridClientConfig, GridClientError, PerformanceMonitor,
    EventType, OperationTimer
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Comprehensive example showing improved GridClient usage
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Enhanced GridClient Example");
    println!("==============================");

    // 1. Load configuration from environment or use defaults
    let config = GridClientConfig::from_env();
    println!("📋 Configuration loaded:");
    println!("   Grid size: {}x{}", config.grid.rows, config.grid.cols);
    println!("   Auto display: {}", config.display.auto_display);
    println!("   Focus events: {}", config.focus_events.enabled);
    println!("   Debug mode: {}", config.display.show_debug_info);
    
    // Validate configuration
    config.validate()?;
    println!("✅ Configuration validated");

    // 2. Create performance monitor
    let performance_monitor = Arc::new(PerformanceMonitor::new());
    println!("📊 Performance monitoring initialized");

    // 3. Create GridClient with error handling
    let mut grid_client = match GridClient::new() {
        Ok(client) => {
            println!("✅ GridClient created successfully");
            client
        }
        Err(e) => {
            eprintln!("❌ Failed to create GridClient: {}", e);
            return Err(e);
        }
    };

    // 4. Register focus callback for e_midi integration
    if config.focus_events.enabled {
        let perf_monitor_clone = performance_monitor.clone();
        
        grid_client.set_focus_callback(move |focus_event| {
            // Measure focus event processing time
            let _timer = OperationTimer::new(perf_monitor_clone.clone(), EventType::FocusEvent);
            
            let event_type = if focus_event.is_focused { "FOCUSED" } else { "DEFOCUSED" };
            let app_name = String::from_utf8_lossy(
                &focus_event.app_name[..focus_event.app_name_len as usize]
            );
            
            println!("🎯 {} window {} ({})", event_type, focus_event.hwnd, app_name);
            
            // Simulate e_midi processing
            if focus_event.is_focused {
                println!("   🎵 Starting music for app: {}", app_name);
                // In real e_midi: lookup song, start playback, update spatial audio
            } else {
                println!("   🔇 Pausing music for app: {}", app_name);
                // In real e_midi: pause playback, save position
            }
        })?;
        
        println!("✅ Focus callback registered");
    }

    // 5. Start background monitoring with error handling
    match grid_client.start_background_monitoring() {
        Ok(_) => println!("✅ Background monitoring started"),
        Err(e) => {
            eprintln!("❌ Failed to start background monitoring: {}", e);
            return Err(e);
        }
    }

    // 6. Performance monitoring loop
    let perf_monitor_clone = performance_monitor.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            
            let metrics = perf_monitor_clone.get_metrics();
            println!("\n📊 Performance Update:");
            println!("   Events processed: {}", metrics.total_events_processed);
            println!("   Focus events: {}", metrics.total_focus_events_processed);
            println!("   Events/sec: {:.1}", metrics.events_per_second);
            println!("   Avg processing: {:.2?}", metrics.avg_event_processing_time);
            println!("   Active windows: {}", metrics.active_window_count);
            
            if perf_monitor_clone.is_performance_degraded() {
                println!("⚠️  Performance degradation detected!");
                println!("{}", perf_monitor_clone.generate_report());
            }
        }
    });

    // 7. Demonstrate API usage
    println!("\n🎮 API Usage Examples:");
    
    // Example window operations (these would typically come from user input)
    let example_hwnd = 12345u64;
    
    // Move window to grid cell (with error handling)
    match grid_client.move_window_to_cell(example_hwnd, 1, 2) {
        Ok(_) => println!("✅ Move window command sent"),
        Err(e) => println!("⚠️  Move window failed: {}", e),
    }
    
    // Assign window to virtual cell
    match grid_client.assign_window_to_virtual_cell(example_hwnd, 0, 0) {
        Ok(_) => println!("✅ Virtual cell assignment sent"),
        Err(e) => println!("⚠️  Virtual cell assignment failed: {}", e),
    }
    
    // Request current state
    match grid_client.request_window_list() {
        Ok(_) => println!("✅ Window list requested"),
        Err(e) => println!("⚠️  Window list request failed: {}", e),
    }
    
    match grid_client.request_grid_state() {
        Ok(_) => println!("✅ Grid state requested"),
        Err(e) => println!("⚠️  Grid state request failed: {}", e),
    }

    // 8. Main event loop
    println!("\n🔄 Main Event Loop Started");
    println!("💡 Focus different windows to see events");
    println!("⌨️  Press Ctrl+C to exit");
    
    let mut loop_count = 0;
    loop {
        thread::sleep(Duration::from_secs(1));
        loop_count += 1;
        
        // Update performance metrics
        performance_monitor.update_window_count(
            // In real usage, this would come from the actual window count
            (loop_count % 10) as usize
        );
        
        // Periodically display grid (every 10 seconds)
        if loop_count % 10 == 0 {
            println!("\n📋 Current Grid State:");
            grid_client.display_current_grid();
            
            // Show performance report every minute
            if loop_count % 60 == 0 {
                println!("{}", performance_monitor.generate_report());
            }
        }
        
        // Demonstrate configuration access
        if loop_count == 5 {
            let grid_config = grid_client.get_config();
            println!("\n⚙️  Current grid configuration: {}x{}", 
                     grid_config.rows, grid_config.cols);
        }
        
        // Toggle auto-display as demonstration
        if loop_count == 15 {
            grid_client.set_auto_display(false);
            println!("🔄 Auto-display disabled");
        }
        
        if loop_count == 25 {
            grid_client.set_auto_display(true);
            println!("🔄 Auto-display re-enabled");
        }
    }
}

/// Demonstrate error handling patterns
fn demonstrate_error_handling() -> Result<(), GridClientError> {
    println!("\n🛠️  Error Handling Examples:");
    
    // This would typically be called with real coordinates
    let result = e_grid::grid_client_errors::validate_grid_coordinates(10, 10, 8, 12);
    match result {
        Ok(_) => println!("✅ Coordinates valid"),
        Err(e) => {
            println!("❌ Invalid coordinates: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}

/// Demonstrate configuration management
fn demonstrate_configuration() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n⚙️  Configuration Management:");
    
    // Create custom configuration  
    let mut config = GridClientConfig::default();
    config.grid.rows = 6;
    config.grid.cols = 8;
    config.display.auto_display = false;
    config.focus_events.enabled = true;
    config.performance.event_batch_size = 15;
    
    // Save to file
    config.save_to_file("grid_config.json")?;
    println!("✅ Configuration saved to grid_config.json");
    
    // Load from file
    let loaded_config = GridClientConfig::load_from_file("grid_config.json")?;
    println!("✅ Configuration loaded from file");
    println!("   Grid size: {}x{}", loaded_config.grid.rows, loaded_config.grid.cols);
    
    // Validate loaded configuration
    loaded_config.validate()?;
    println!("✅ Loaded configuration validated");
    
    Ok(())
}
