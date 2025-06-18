use e_grid::ipc_client::GridClient;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔥 E-Grid Non-Interactive Client Demo - Real-Time Grid Reconstruction");
    println!("=====================================================================");
    println!("This demo will automatically:");
    println!("  📡 Connect to the E-Grid server");
    println!("  📋 Request and display window lists");
    println!("  🔍 Monitor real-time window updates");
    println!("  � Show grid state changes automatically");
    println!("  🎯 Demonstrate grid assignments");
    println!();

    // Create the grid client
    let mut client = GridClient::new()?;

    // Enable auto-display for real-time updates
    client.set_auto_display(true);
    // Start background monitoring for real-time updates
    client.start_background_monitoring()?;

    println!("✅ Connected to E-Grid server");
    println!("🔍 Background monitoring started - real-time updates enabled!");
    println!("📡 Initial window data requested automatically");
    println!();

    // Give some time for initial data to arrive
    thread::sleep(Duration::from_millis(800));

    println!("\n📋 Initial Window List:");
    client.display_window_list();

    println!("\n📊 Initial Grid State:");
    client.display_current_grid();

    // Non-interactive demo loop with automatic actions
    let mut demo_cycle = 0;

    println!("\n🎬 Starting automated demo cycle...");
    println!("💡 Move windows around to see real-time updates!");
    println!("🛑 Press Ctrl+C to stop the demo");
    println!();

    loop {
        demo_cycle += 1;
        println!("🔄 Demo Cycle #{}", demo_cycle);

        // Periodic actions to demonstrate functionality
        match demo_cycle % 6 {
            1 => {
                println!("📤 Requesting fresh window list from server...");
                client.request_window_list()?;
                thread::sleep(Duration::from_millis(300));
                client.display_window_list();
            }
            2 => {
                println!("📊 Displaying current grid state...");
                client.display_current_grid();
            }
            3 => {
                println!("🔄 Requesting grid state from server...");
                client.request_grid_state()?;
                thread::sleep(Duration::from_millis(300));
            }
            4 => {
                println!("� Refreshing all data from server...");
                client.request_window_list()?;
                client.request_grid_state()?;
                thread::sleep(Duration::from_millis(500));
            }
            5 => {
                println!("� Current window summary:");
                client.display_window_list();
            }
            _ => {
                println!("🔍 Monitoring for real-time window changes...");
                demonstrate_auto_assignment(&mut client)?;
            }
        }

        // Wait between demo actions - this gives time for real-time updates to show
        thread::sleep(Duration::from_secs(8));

        // Occasional longer pause to let user observe
        if demo_cycle % 10 == 0 {
            println!("\n⏸️  Pausing to observe real-time updates...");
            println!("   (This is a good time to move windows around!)");
            thread::sleep(Duration::from_secs(5));
        }
    }
}

fn demonstrate_auto_assignment(client: &mut GridClient) -> Result<(), Box<dyn std::error::Error>> {
    // Get current window list to demonstrate assignment
    client.request_window_list()?;
    thread::sleep(Duration::from_millis(200));

    // Try to automatically assign a window to demonstrate grid assignment
    // This is just for demo purposes - in a real application, you'd assign specific windows
    println!("🎯 Demonstrating automatic window assignment...");

    // For demo purposes, we'll just mention what could be done
    // In a real scenario, you'd have specific HWNDs to work with
    println!("   (In a real scenario, windows would be assigned to grid cells here)");
    println!("   Example: client.assign_window_to_virtual_cell(hwnd, 2, 3)");
    println!("   � The server will process assignments and update all connected clients!");

    Ok(())
}
