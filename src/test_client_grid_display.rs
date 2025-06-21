use e_grid::{GridClient, GridConfig};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 Testing client grid display...");

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

    // Start monitoring
    println!("🔍 Starting background monitoring...");
    client.start_background_monitoring()?;

    // Wait a moment for initial data
    thread::sleep(Duration::from_secs(2));

    // Display current grid state
    println!("\n🎯 === CLIENT GRID DISPLAY TEST ===");
    client.display_current_grid();

    // Wait a bit more to see real-time updates
    println!("\n⏱️  Waiting for real-time updates...");
    thread::sleep(Duration::from_secs(3));

    // Display again to see any changes
    println!("\n🔄 === UPDATED CLIENT GRID DISPLAY ===");
    client.display_current_grid();

    println!("\n✅ Client test completed successfully");
    Ok(())
}
