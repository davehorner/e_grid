use std::time::Duration;
use std::thread;

/// Example demonstrating how e_midi can use the GridClient focus callback API
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎵 e_midi Focus Integration Example");
    println!("===================================");

    // Note: This would be in e_midi's integration code
    // Here we simulate how e_midi would use the GridClient
    
    // Create a grid client (this would be done by e_midi's integration manager)
    let mut grid_client = e_grid::GridClient::new()?;
    
    // Register a focus callback that maps window focus to music
    grid_client.set_focus_callback(|focus_event| {
        if focus_event.is_focused {
            println!("🎵 Window {} gained focus - starting music for app '{}'", 
                focus_event.hwnd, 
                String::from_utf8_lossy(&focus_event.app_name[..focus_event.app_name_len as usize])
            );
            // In real e_midi, this would trigger:
            // - Look up or assign a song for this app
            // - Start/resume MIDI playback
            // - Update spatial audio based on window position
        } else {
            println!("🔇 Window {} lost focus - pausing music", focus_event.hwnd);
            // In real e_midi, this would trigger:
            // - Pause current MIDI playback
            // - Save playback position
        }
    })?;

    // Start background monitoring to receive focus events
    grid_client.start_background_monitoring()?;
    
    println!("\n📻 Listening for window focus events...");
    println!("💡 Focus a different window to see the callback in action");
    println!("⌨️  Press Ctrl+C to exit\n");
    
    // Keep the example running
    loop {
        thread::sleep(Duration::from_secs(1));
        
        // In a real application, e_midi would be doing other work here
        // like processing MIDI events, updating UI, etc.
    }
}
