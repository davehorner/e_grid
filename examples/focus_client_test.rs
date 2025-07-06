use e_grid::ipc_client::GridClient;
use e_grid::ipc_protocol;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 E-Grid Focus Event Test Client");
    println!("===================================");
    println!("This client will only show focus/defocus events");
    println!("Move focus between windows to see events appear");
    println!("Press Ctrl+C to stop");
    println!();

    // Create the grid client
    let mut client = GridClient::new()?;

    // Set up a focus callback to explicitly handle focus events
    client.set_focus_callback(Box::new(|focus_event: ipc_protocol::WindowFocusEvent| {
        let event_type = if focus_event.event_type == 0 {
            "FOCUSED"
        } else {
            "DEFOCUSED"
        };
        let timestamp = focus_event.timestamp;

        println!(
            "🎯 [{}] Window {} (PID: {}) at timestamp: {}",
            event_type, focus_event.hwnd, focus_event.process_id, timestamp
        );
    }))?;

    // Start background monitoring for real-time updates
    client.start_background_monitoring()?;

    println!("✅ Connected to E-Grid server");
    println!("🔍 Focus event monitoring started!");
    println!("🪟 Switch between windows to see focus events...");
    println!();

    // Keep the client running and processing focus events
    loop {
        thread::sleep(Duration::from_secs(1));
        print!("."); // Show activity
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
    }
}
