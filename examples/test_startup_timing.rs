use e_grid::{GridClient, ipc_client::ClientCellState};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing startup timing for monitor and window data availability...");

    // Create client
    let mut client = match GridClient::new() {
        Ok(client) => {
            println!("✅ Client created successfully");
            client
        }
        Err(e) => {
            println!("❌ Failed to create client: {}", e);
            return Err(Box::new(e));
        }
    };

    // Measure startup time
    let start_time = Instant::now();
    
    // Start services with auto-server option (new feature!)
    println!("🔄 Starting services with auto-server enabled...");
    client.start_services_with_server(true)?;
    
    let startup_time = start_time.elapsed();
    println!("⏱️ Startup completed in: {:.2}s", startup_time.as_secs_f64());

    // Check data availability immediately after startup
    let monitor_data = client.get_monitor_data();
    let window_data = client.get_window_data();
    
    println!("\n📊 Data availability after startup:");
    println!("  🖥️ Monitors: {} found", monitor_data.len());
    println!("  🪟 Windows: {} found", window_data.len());
    
    // Display each monitor grid to verify they have window data
    for monitor in monitor_data.iter() {
        let mut window_count = 0;
        for row in &monitor.grid {
            for cell in row {
                if cell.is_some() {
                    window_count += 1;
                }
            }
        }
        println!("  📺 Monitor {}: {} windows in grid", monitor.monitor_id, window_count);
    }

    // Display virtual grid state
    let virtual_grid_state = client.get_virtual_grid_state();
    let mut virtual_window_count = 0;
    for row in &virtual_grid_state {
        for cell in row {
            if let ClientCellState::Occupied(_) = cell {
                virtual_window_count += 1;
            }
        }
    }
    println!("  🌐 Virtual Grid: {} cells occupied", virtual_window_count);

    if startup_time.as_secs_f64() <= 3.0 {
        println!("\n✅ STARTUP TIMING TEST PASSED - Completed in {:.2}s (≤ 3s)", startup_time.as_secs_f64());
    } else {
        println!("\n❌ STARTUP TIMING TEST FAILED - Took {:.2}s (> 3s)", startup_time.as_secs_f64());
    }

    if monitor_data.len() > 0 && window_data.len() > 0 {
        println!("✅ DATA AVAILABILITY TEST PASSED - Both monitors and windows available");
    } else {
        println!("❌ DATA AVAILABILITY TEST FAILED - Missing data (monitors: {}, windows: {})", 
                 monitor_data.len(), window_data.len());
    }

    // Check if monitor grids are populated
    let total_monitor_windows: usize = monitor_data.iter()
        .map(|m| m.grid.iter().flat_map(|row| row.iter()).filter(|cell| cell.is_some()).count())
        .sum();
    
    if total_monitor_windows > 0 {
        println!("✅ MONITOR GRID TEST PASSED - {} windows found in monitor grids", total_monitor_windows);
    } else {
        println!("❌ MONITOR GRID TEST FAILED - No windows found in monitor grids");
    }

    println!("\n🎯 Test Summary:");
    println!("  ⏱️ Startup Time: {:.2}s", startup_time.as_secs_f64());
    println!("  📊 Monitors: {}", monitor_data.len());
    println!("  🪟 Windows: {}", window_data.len());
    println!("  📺 Monitor Grid Windows: {}", total_monitor_windows);
    println!("  🌐 Virtual Grid Windows: {}", virtual_window_count);

    println!("\n💡 New auto-server feature:");
    println!("  🔧 start_services() - Backward compatible, no auto-start");
    println!("  🚀 start_services_with_server(true) - Auto-starts server if needed");
    println!("  📝 This test used auto-start to ensure server availability");

    Ok(())
}
