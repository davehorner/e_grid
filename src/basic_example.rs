use e_grid::WindowTracker;

fn main() {
    println!("Basic Grid Example");
    println!("==================");
    
    // Create a new window tracker
    let mut tracker = WindowTracker::new();
    
    // Get monitor information
    let (left, top, width, height) = tracker.get_monitor_info();
    println!("Virtual Screen: {}x{} px (at {}, {})", width, height, left, top);
    
    // Scan for existing windows
    println!("\nScanning for windows...");
    tracker.scan_existing_windows();
    
    // Display the grid
    tracker.print_grid();
    
    println!("Press Enter to exit...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
}
