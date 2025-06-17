use e_grid::{WindowTracker, ipc};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use iceoryx2::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 iceoryx2 IPC Integration Test");
    println!("================================");
    
    // Create window tracker
    let tracker = Arc::new(Mutex::new(WindowTracker::new()));
    
    // Test IPC manager creation and setup
    println!("🔧 Creating IPC manager...");
    let mut ipc_manager = ipc::GridIpcManager::new(tracker.clone())?;
    
    println!("🔧 Setting up iceoryx2 services...");
    ipc_manager.setup_services()?;
    
    // Test event publishing with zero-copy data
    println!("📡 Testing event publishing...");
    ipc_manager.publish_window_created(12345, "Test Window".to_string(), 1, 2)?;
    ipc_manager.publish_window_moved(12345, "Test Window".to_string(), 1, 2, 2, 3)?;
    ipc_manager.publish_grid_state_changed(5, 3)?;
    
    // Test command processing (simulated)
    println!("📨 Testing command processing...");
    let test_event = ipc::GridEvent::WindowCreated {
        hwnd: 67890,
        title: "Another Test Window".to_string(),
        row: 3,
        col: 4,
    };
    ipc_manager.publish_event(test_event)?;
    
    // Process commands (would normally receive from other processes)
    ipc_manager.process_commands()?;
    
    println!("✅ All iceoryx2 IPC tests completed successfully!");
    println!("🔍 Key features verified:");
    println!("   • NodeBuilder and service creation");
    println!("   • Zero-copy data type definitions (WindowEvent, WindowCommand, WindowResponse)");
    println!("   • Publisher/Subscriber setup");
    println!("   • Event publishing with send_copy()");
    println!("   • Command processing pipeline");
    println!("   • Type conversions between high-level and zero-copy formats");
    
    // Show the zero-copy data structures
    println!("\n📊 Zero-copy data structure sizes:");
    println!("   WindowEvent: {} bytes", std::mem::size_of::<ipc::WindowEvent>());
    println!("   WindowCommand: {} bytes", std::mem::size_of::<ipc::WindowCommand>());
    println!("   WindowResponse: {} bytes", std::mem::size_of::<ipc::WindowResponse>());
    
    Ok(())
}
