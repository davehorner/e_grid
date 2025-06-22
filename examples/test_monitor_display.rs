use e_grid::WindowTracker;

fn main() {
    println!("Testing monitor grid display fix...");

    let mut tracker = WindowTracker::new();
    println!("Initializing tracker and scanning windows...");
    tracker.scan_existing_windows();

    println!("Displaying all grids including monitor grids...");
    tracker.print_all_grids();

    println!("Test completed. Monitor 1 should now display HWNDs properly.");
}
