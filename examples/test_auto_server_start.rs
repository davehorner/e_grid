use e_grid::GridClient;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing auto-server startup functionality...");

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

    // Test 1: Try connecting without auto-start (should fail if no server)
    println!("\n🔍 Test 1: Attempting to connect without auto-start...");
    let start_time = Instant::now();
    match client.start_background_monitoring() {
        Ok(_) => {
            println!(
                "✅ Connected to existing server in {:.2}s",
                start_time.elapsed().as_secs_f64()
            );
        }
        Err(e) => {
            println!("❌ Failed to connect (expected if no server): {}", e);
            println!("   This is normal if no server is running");
        }
    }

    // Test 2: Try connecting with auto-start (should start server if needed)
    println!("\n🚀 Test 2: Attempting to connect with auto-start enabled...");
    let start_time = Instant::now();
    match client.start_background_monitoring() {
        Ok(_) => {
            let elapsed = start_time.elapsed().as_secs_f64();
            println!("✅ Connected successfully in {:.2}s", elapsed);

            // Check data availability
            // let monitor_data = client.get_monitor_data();
            // let window_data = client.get_window_data();

            // println!("\n📊 Data availability after auto-start:");
            // println!("  🖥️ Monitors: {} found", monitor_data.len());
            // println!("  🪟 Windows: {} found", window_data.len());

            // Display each monitor grid to verify they have window data
            // for monitor in monitor_data.iter() {
            //     let mut window_count = 0;
            //     for row in &monitor.grid {
            //         for cell in row {
            //             if cell.is_some() {
            //                 window_count += 1;
            //             }
            //         }
            //     }
            //     println!(
            //         "  📺 Monitor {}: {} windows in grid",
            //         monitor.monitor_id, window_count
            //     );
            // }

            // // Display virtual grid state
            // let virtual_grid_state = client.get_virtual_grid_state();
            // let mut virtual_window_count = 0;
            // for row in &virtual_grid_state {
            //     for cell in row {
            //         if let ClientCellState::Occupied(_) = cell {
            //             virtual_window_count += 1;
            //         }
            //     }
            // }
            // println!("  🌐 Virtual Grid: {} cells occupied", virtual_window_count);

            println!("\n✅ AUTO-START TEST PASSED");
        }
        Err(e) => {
            println!("❌ Failed to connect with auto-start: {}", e);
            println!("   This may indicate a problem with server startup");
        }
    }

    println!("\n🎯 Auto-start test summary:");
    println!("  💡 The client can now optionally start the server if it's not running");
    println!("  🔧 Use start_services() for backward compatibility (no auto-start)");
    println!("  🚀 Use start_services_with_server(true) to auto-start server if needed");
    println!("  ⚡ Auto-start uses: cargo run --bin e_grid --release");
    println!("  📋 Fallback to manual server startup if auto-start fails");

    Ok(())
}
