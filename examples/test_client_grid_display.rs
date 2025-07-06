use e_grid::GridClient;

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

    // Start services (automatically starts monitoring and populates monitor data)
    println!("� Starting services...");
    client.start_services()?;

    // Display current grid state (monitor data is now available)
    println!("\n🎯 === CLIENT GRID DISPLAY TEST ===");
    client.display_current_grid();

    println!("\n✅ Client test completed successfully");
    Ok(())
}
