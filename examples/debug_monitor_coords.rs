use e_grid::GridClient;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging for debug output
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    println!("🔍 DEBUG: Testing Monitor Coordinate Calculation");
    println!("==============================================");

    // Create a grid client to test monitor detection
    let mut client = GridClient::new()?;

    // Start background monitoring briefly
    client.start_background_monitoring()?;

    // Wait a moment for initialization
    std::thread::sleep(Duration::from_millis(1000));

    // Display the current grid to see monitor information
    println!("\n📊 Current Grid State:");
    client.display_current_grid();

    println!("\n📋 Current Window List:");
    client.display_window_list();

    // Stop the client
    client.shutdown();

    println!("\n✅ Debug test completed");
    Ok(())
}
