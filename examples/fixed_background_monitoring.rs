// Fixed background monitoring function without static mut variables
    pub fn start_background_monitoring(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let windows = self.windows.clone();
        let virtual_grid = self.virtual_grid.clone();
        let monitors = self.monitors.clone();
        let auto_display = self.auto_display.clone();
        let running = self.running.clone();
        let config = self.config.clone();
        
        thread::spawn(move || {
            match Self::create_background_subscribers() {
                Ok((event_subscriber, window_details_subscriber)) => {
                    println!("🔍 Background monitoring started - listening for real-time updates...");
                    
                    // Use local variables instead of unsafe static mut
                    let mut last_status_time = std::time::Instant::now();
                    let start_time = std::time::Instant::now();
                    let mut total_events = 0u64;
                    let mut total_details = 0u64;
                    
                    while {
                        match running.try_lock() {
                            Ok(running_guard) => *running_guard,
                            Err(_) => {
                                println!("⚠️ Could not check running status, continuing...");
                                true
                            }
                        }
                    } {
                        let mut had_activity = false;
                        
                        // Process window events (non-blocking)
                        let mut event_batch_count = 0;
                        while let Some(event_sample) = event_subscriber.receive().unwrap_or(None) {
                            let event = *event_sample;
                            total_events += 1;
                            event_batch_count += 1;
                            had_activity = true;
                            
                            Self::handle_window_event(&event, &windows, &virtual_grid, &monitors, &auto_display, &config);
                            
                            if event_batch_count >= 5 { // Smaller batches
                                break;
                            }
                        }
                        
                        // Process window details (non-blocking)
                        let mut details_batch_count = 0;
                        while let Some(details_sample) = window_details_subscriber.receive().unwrap_or(None) {
                            let details = *details_sample;
                            total_details += 1;
                            details_batch_count += 1;
                            had_activity = true;
                            
                            Self::handle_window_details(&details, &windows, &virtual_grid, &monitors, &auto_display, &config);
                            
                            if details_batch_count >= 3 { // Smaller batches
                                break;
                            }
                        }
                        
                        // Status reporting (safe)
                        if last_status_time.elapsed().as_secs() >= 45 {
                            let window_count = {
                                match windows.try_lock() {
                                    Ok(windows_lock) => windows_lock.len(),
                                    Err(_) => 0
                                }
                            };
                            
                            let uptime = start_time.elapsed().as_secs();
                            println!("\n📊 ===== CLIENT STATUS =====");
                            println!("🔍 Monitoring: {} windows", window_count);
                            println!("⏱️  Uptime: {}s | Events: {} | Details: {}", uptime, total_events, total_details);
                            if uptime > 0 {
                                println!("📈 Rates: {:.1} events/s | {:.1} details/s", 
                                    total_events as f64 / uptime as f64,
                                    total_details as f64 / uptime as f64);
                            }
                            last_status_time = std::time::Instant::now();
                        }
                        
                        // Adaptive sleep
                        if had_activity {
                            thread::sleep(Duration::from_millis(25)); // Very responsive
                        } else {
                            thread::sleep(Duration::from_millis(100)); // Less aggressive when idle
                        }
                    }
                }
                Err(e) => {
                    println!("❌ Failed to create background subscribers: {}", e);
                }
            }
            
            println!("🛑 Background monitoring stopped");
        });
        
        // Request initial data
        std::thread::sleep(Duration::from_millis(50));
        println!("📡 Requesting initial window data from server...");
        match self.request_window_list() {
            Ok(_) => println!("✅ Window list request sent"),
            Err(e) => println!("❌ Failed to send window list request: {}", e),
        }
        match self.request_grid_state() {
            Ok(_) => println!("✅ Grid state request sent"),
            Err(e) => println!("❌ Failed to send grid state request: {}", e),
        }
        
        Ok(())
    }
