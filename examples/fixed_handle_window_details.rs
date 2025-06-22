// Fixed handle_window_details function without static mut variables
    fn handle_window_details(
        details: &ipc::WindowDetails,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
        auto_display: &Arc<Mutex<bool>>,
        config: &GridConfig,
    ) {
        println!("📊 [WINDOW UPDATE] HWND {} at ({}, {}) size {}x{}", 
            details.hwnd, details.x, details.y, details.width, details.height);
        println!("   📍 Virtual Grid: ({}, {}) to ({}, {})", 
            details.virtual_row_start, details.virtual_col_start,
            details.virtual_row_end, details.virtual_col_end);
        println!("   🖥️  Monitor {}: ({}, {}) to ({}, {})", 
            details.monitor_id,
            details.monitor_row_start, details.monitor_col_start,
            details.monitor_row_end, details.monitor_col_end);
        
        // Update local window cache (with timeout to prevent blocking)
        match windows.try_lock() {
            Ok(mut windows_lock) => {
                let window_info = ClientWindowInfo::from(*details);
                windows_lock.insert(details.hwnd, window_info);
            }
            Err(_) => {
                println!("⚠️ Could not acquire windows lock for update");
            }
        }
        
        // Update virtual grid
        Self::update_virtual_grid(&details, &virtual_grid);
        
        // Update monitor grids
        Self::update_monitor_grids(&details, &monitors, config);
        
        // Auto-display grid if enabled (simplified - no throttling to avoid static mut)
        match auto_display.try_lock() {
            Ok(auto_display_guard) => {
                if *auto_display_guard {
                    println!("   🔄 Auto-displaying updated grid...");
                    Self::display_virtual_grid(&virtual_grid, &windows, config);
                    println!("   ─────────────────────────────────────");
                }
            }
            Err(_) => {
                // Skip auto-display if locked to prevent blocking
            }
        }
    }
