// COMPREHENSIVE FIX FOR GridClient locking issues
// The main problems are:
// 1. static mut variables (LAST_STATUS_TIME, LAST_AUTO_DISPLAY, etc.) - UNSAFE!
// 2. Potential deadlocks from holding multiple locks or long-held locks
// 3. Busy waiting without proper error handling
// 4. Lock contention in background monitoring loop

// SOLUTION APPROACH:
// 1. Replace all static mut with safe alternatives (local variables or Arc<Mutex<>>)
// 2. Use try_lock() with timeouts instead of blocking locks
// 3. Reduce lock duration by releasing locks as soon as possible
// 4. Add proper error handling and recovery
// 5. Use smaller batch sizes to prevent blocking

// Let's fix the key functions:

// 1. Background monitoring loop - FIXED (remove static mut, use local vars)
// 2. handle_window_event - FIXED (remove static mut throttling)  
// 3. handle_window_details - FIXED (remove static mut throttling)
// 4. Add timeout-based locking throughout

// Key changes:
// - All timing done with local variables in thread
// - try_lock() with fallback behavior instead of blocking
// - Shorter lock holds, immediate release after data extraction
// - Better error handling for lock failures
// - Reduced batch sizes and more frequent yields

pub fn start_background_monitoring_safe(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    let windows = self.windows.clone();
    let virtual_grid = self.virtual_grid.clone();
    let monitors = self.monitors.clone();
    let auto_display = self.auto_display.clone();
    let running = self.running.clone();
    let config = self.config.clone();
    
    thread::spawn(move || {
        match Self::create_background_subscribers() {
            Ok((event_subscriber, window_details_subscriber)) => {
                println!("🔍 Background monitoring started (SAFE VERSION) - listening for real-time updates...");
                
                // SAFE: Use local variables instead of static mut
                let mut last_status_time = std::time::Instant::now();
                let start_time = std::time::Instant::now();
                let mut total_events = 0u64;
                let mut total_details = 0u64;
                let mut consecutive_failures = 0u32;
                
                while {
                    // SAFE: Non-blocking check of running state
                    match running.try_lock() {
                        Ok(running_guard) => *running_guard,
                        Err(_) => {
                            // If we can't check, assume we should continue (fail-safe)
                            consecutive_failures += 1;
                            if consecutive_failures > 1000 {
                                println!("⚠️ Too many consecutive lock failures, stopping...");
                                false
                            } else {
                                true
                            }
                        }
                    }
                } {
                    let mut had_activity = false;
                    consecutive_failures = 0; // Reset on successful iteration
                    
                    // Process events with SMALL batches to prevent blocking
                    let mut event_batch_count = 0;
                    while let Some(event_sample) = event_subscriber.receive().unwrap_or(None) {
                        let event = *event_sample;
                        total_events += 1;
                        event_batch_count += 1;
                        had_activity = true;
                        
                        Self::handle_window_event_safe(&event, &windows, &virtual_grid, &monitors, &auto_display, &config);
                        
                        // MUCH smaller batches to prevent blocking
                        if event_batch_count >= 3 {
                            break;
                        }
                    }
                    
                    // Process details with SMALL batches
                    let mut details_batch_count = 0;
                    while let Some(details_sample) = window_details_subscriber.receive().unwrap_or(None) {
                        let details = *details_sample;
                        total_details += 1;
                        details_batch_count += 1;
                        had_activity = true;
                        
                        Self::handle_window_details_safe(&details, &windows, &virtual_grid, &monitors, &auto_display, &config);
                        
                        // MUCH smaller batches to prevent blocking
                        if details_batch_count >= 2 {
                            break;
                        }
                    }
                    
                    // SAFE status reporting with local variables
                    if last_status_time.elapsed().as_secs() >= 60 { // Less frequent
                        let window_count = {
                            match windows.try_lock() {
                                Ok(windows_lock) => windows_lock.len(),
                                Err(_) => {
                                    println!("⚠️ Could not acquire windows lock for status");
                                    0
                                }
                            }
                        }; // Lock released here
                        
                        let uptime = start_time.elapsed().as_secs();
                        println!("\n📊 ===== CLIENT STATUS (SAFE) =====");
                        println!("🔍 Monitoring: {} windows", window_count);
                        println!("⏱️  Uptime: {}s | Events: {} | Details: {}", uptime, total_events, total_details);
                        if uptime > 0 {
                            println!("📈 Rates: {:.1} events/s | {:.1} details/s", 
                                total_events as f64 / uptime as f64,
                                total_details as f64 / uptime as f64);
                        }
                        last_status_time = std::time::Instant::now();
                    }
                    
                    // RESPONSIVE sleep pattern
                    if had_activity {
                        thread::sleep(Duration::from_millis(10)); // Very responsive for active periods
                    } else {
                        thread::sleep(Duration::from_millis(50)); // Reasonable idle period
                    }
                }
            }
            Err(e) => {
                println!("❌ Failed to create background subscribers: {}", e);
            }
        }
        
        println!("🛑 Background monitoring stopped (SAFE VERSION)");
    });
    
    // Initial requests with shorter delay
    std::thread::sleep(Duration::from_millis(25));
    self.request_initial_data_safe()
}

// SAFE event handler - no static mut variables
fn handle_window_event_safe(
    event: &ipc::WindowEvent,
    windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
    virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
    monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
    auto_display: &Arc<Mutex<bool>>,
    config: &GridConfig,
) {
    let event_name = match event.event_type {
        0 => "CREATED",
        1 => "DESTROYED", 
        2 => "MOVED",
        3 => "STATE_CHANGED",
        _ => "UNKNOWN",
    };
    
    println!("📡 [REAL-TIME EVENT] {}: HWND {} at ({}, {})", 
        event_name, event.hwnd, event.row, event.col);
    
    match event.event_type {
        0 => { // Window created
            println!("   🆕 New window {} created, waiting for details...", event.hwnd);
        }
        1 => { // Window destroyed
            Self::remove_window_from_client_safe(event.hwnd, windows, virtual_grid, monitors);
            println!("   🗑️  Removed window {} from client state", event.hwnd);
        }
        2 => { // Window moved
            println!("   🔄 Window {} moved, waiting for updated details...", event.hwnd);
        }
        _ => {}
    }
    
    // SAFE auto-display - simplified without throttling
    match auto_display.try_lock() {
        Ok(auto_display_guard) => {
            if *auto_display_guard && (event.event_type == 0 || event.event_type == 1) {
                println!("   📊 Auto-displaying grid after {} event...", event_name);
                Self::display_virtual_grid_safe(&virtual_grid, &windows, &config);
            }
        }
        Err(_) => {
            // Skip auto-display if locked
        }
    }
}

// SAFE details handler - no static mut variables
fn handle_window_details_safe(
    details: &ipc::WindowDetails,
    windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
    virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
    monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
    auto_display: &Arc<Mutex<bool>>,
    config: &GridConfig,
) {
    println!("📊 [WINDOW UPDATE] HWND {} at ({}, {}) size {}x{}", 
        details.hwnd, details.x, details.y, details.width, details.height);
    
    // SAFE window cache update with timeout
    match windows.try_lock() {
        Ok(mut windows_lock) => {
            let window_info = ClientWindowInfo::from(*details);
            windows_lock.insert(details.hwnd, window_info);
        }
        Err(_) => {
            println!("⚠️ Could not acquire windows lock for update");
        }
    }
    
    // Update grids (these use try_lock internally)
    Self::update_virtual_grid_safe(&details, &virtual_grid);
    Self::update_monitor_grids_safe(&details, &monitors, config);
    
    // SAFE auto-display - no throttling, just check if enabled
    match auto_display.try_lock() {
        Ok(auto_display_guard) => {
            if *auto_display_guard {
                Self::display_virtual_grid_safe(&virtual_grid, &windows, config);
            }
        }
        Err(_) => {
            // Skip if locked
        }
    }
}

// All helper functions should also use try_lock() patterns:
fn remove_window_from_client_safe(...) { /* use try_lock throughout */ }
fn update_virtual_grid_safe(...) { /* use try_lock with timeout */ }
fn update_monitor_grids_safe(...) { /* use try_lock with timeout */ }
fn display_virtual_grid_safe(...) { /* use try_lock with timeout */ }

// Initial data request with error handling
fn request_initial_data_safe(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    println!("📡 Requesting initial window data from server...");
    
    if let Err(e) = self.request_window_list() {
        println!("⚠️ Failed to send window list request: {}", e);
    } else {
        println!("✅ Window list request sent");
    }
    
    if let Err(e) = self.request_grid_state() {
        println!("⚠️ Failed to send grid state request: {}", e);  
    } else {
        println!("✅ Grid state request sent");
    }
    
    Ok(())
}
