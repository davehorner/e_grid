use e_grid::ipc_client::GridClient;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎨 Visual Grid Debug - Color-coded HWND Display");
    println!("{}", "=".repeat(80));
    
    // Connect to the server
    let mut client = GridClient::new()?;
    client.start_services()?;
    
    println!("📡 Connected to E-Grid server");

    // Debug: Check monitor and window data availability
    println!("🔍 Debug: Checking data availability after start_services()...");
    let monitor_data = client.get_monitor_data();
    let window_data = client.get_window_data();
    println!("   Monitor count: {}", monitor_data.len());
    println!("   Window count: {}", window_data.len());
    println!("   Has recent data: {}", client.has_recent_data());

    if monitor_data.is_empty() {
        println!("⚠️  No monitor data in get_monitor_data() after start_services()");
        // Try the manual method as a fallback for comparison
        if let Some(monitor_list) = client.get_monitor_list() {
            println!("ℹ️  Manual get_monitor_list() returns {} monitors", monitor_list.monitor_count);
        } else {
            println!("❌ No monitor data available from any method");
            return Err("No monitor data available".into());
        }
    } else {
        println!("✅ Monitor data is available via get_monitor_data()!");
    }
    
    // Get current window data and z-order
    let z_order_map = e_grid::util::get_hwnd_z_order_map();
    
    // Find top 3 windows by z-order (lowest z-order = most topmost)
    let mut z_order_vec: Vec<(u64, usize)> = z_order_map.into_iter().collect();
    z_order_vec.sort_by_key(|(_, z)| *z);
    let top_3: Vec<u64> = z_order_vec.into_iter().take(3).map(|(hwnd, _)| hwnd).collect();
    
    println!("🏆 Top 3 windows by Z-order:");
    for (i, hwnd) in top_3.iter().enumerate() {
        let color = match i {
            0 => "🟢 GREEN (Topmost)",
            1 => "🟡 YELLOW (2nd)",
            2 => "🔴 RED (3rd)",
            _ => "",
        };
        println!("  {}: HWND 0x{:X} {}", i + 1, hwnd, color);
    }
    println!();
    
    // Display individual monitor grids (like print_all_grids)
    display_individual_monitor_grids(&mut client, &top_3)?;
    
    Ok(())
}

fn display_individual_monitor_grids(client: &mut GridClient, top_3: &[u64]) -> Result<(), Box<dyn std::error::Error>> {
    println!("🖥️ INDIVIDUAL MONITOR GRIDS");
    println!("{}", "=".repeat(80));
    
    // Try the monitor list method first (this is working!)
    if let Some(monitor_list) = client.get_monitor_list() {
        println!("✅ Using monitor list from get_monitor_list(): {} monitors", monitor_list.monitor_count);
        
        // Use the monitor list data 
        for i in 0..monitor_list.monitor_count as usize {
            let monitor = &monitor_list.monitors[i];
            
            let monitor_type = match monitor.grid_type {
                e_grid::ipc_protocol::GridType::Physical => "Physical",
                e_grid::ipc_protocol::GridType::Virtual => "Virtual Desktop",
                e_grid::ipc_protocol::GridType::Dynamic => "Dynamic",
            };
            
            println!();
            println!("=== MONITOR {} GRID ({}) ===", monitor.monitor_id, monitor_type);
            println!(
                "Monitor bounds: ({}, {}) to ({}, {})",
                monitor.x, monitor.y,
                monitor.x + monitor.width as i32, monitor.y + monitor.height as i32
            );
            println!(
                "Monitor resolution: {}x{} px",
                monitor.width, monitor.height
            );
            
            let config = client.get_config();
            
            // Print column headers
            print!("   ");
            for col in 0..config.cols {
                print!("{:2} ", col);
            }
            println!();
            
            // Print grid rows with color coding from server data
            for row in 0..config.rows.min(32) {
                print!("{:2} ", row);
                for col in 0..config.cols.min(32) {
                    let server_value = monitor.grid[row][col];
                    match server_value {
                        0 => print!(".. "), // Empty
                        u64::MAX => print!("XX "), // Offscreen
                        hwnd => {
                            let display = format_hwnd_with_color(hwnd, top_3);
                            print!("{} ", display);
                        }
                    }
                }
                println!();
            }
            
            // Count windows in this monitor
            let window_count: usize = (0..config.rows.min(32))
                .flat_map(|row| (0..config.cols.min(32)).map(move |col| monitor.grid[row][col]))
                .filter(|&hwnd| hwnd != 0 && hwnd != u64::MAX)
                .count();
                
            println!("Windows in grid: {}", window_count);
        }
        println!();
        return Ok(());
    }
    
    // Fallback to monitor data method if monitor list is not available
    let monitor_data = client.get_monitor_data();
    println!("📊 Fallback: Using monitor data from get_monitor_data(): {} monitors", monitor_data.len());
    
    if monitor_data.is_empty() {
        println!("❌ No monitor data available from either method");
        return Ok(());
    }
    
    for monitor in &monitor_data {
        println!();
        println!("=== MONITOR {} GRID ===", monitor.monitor_id);
        println!(
            "Monitor bounds: ({}, {}) to ({}, {})",
            monitor.x, monitor.y,
            monitor.x + monitor.width as i32, monitor.y + monitor.height as i32
        );
        
        // Count windows on this monitor
        let window_data = client.get_window_data();
        let windows_on_monitor: usize = window_data.iter()
            .filter(|(_, window_info)| window_info.monitor_id == monitor.monitor_id)
            .count();
            
        println!("Windows on this monitor: {}", windows_on_monitor);
        let config = client.get_config();
        println!(
            "Grid size: {} rows x {} cols ({} cells)",
            config.rows, config.cols, config.rows * config.cols
        );
        println!(
            "Monitor resolution: {}x{} px",
            monitor.width, monitor.height
        );
        
        // Print column headers
        print!("   ");
        for col in 0..config.cols {
            print!("{:2} ", col);
        }
        println!();
        
        // Print grid rows with color coding
        for row in 0..config.rows.min(32) {
            print!("{:2} ", row);
            for col in 0..config.cols.min(32) {
                if let Some(hwnd) = monitor.grid[row][col] {
                    if hwnd == 0 {
                        print!(".. ");
                    } else {
                        let display = format_hwnd_with_color(hwnd, top_3);
                        print!("{} ", display);
                    }
                } else {
                    print!(".. ");
                }
            }
            println!();
        }
    }
    println!();
    
    Ok(())
}

fn format_hwnd_with_color(hwnd: u64, top_3: &[u64]) -> String {
    let hex = format!("{:02X}", hwnd & 0xFF);
    
    // Color code based on z-order ranking
    if let Some(pos) = top_3.iter().position(|&h| h == hwnd) {
        match pos {
            0 => format!("\x1b[92m{}\x1b[0m", hex), // Green (topmost)
            1 => format!("\x1b[93m{}\x1b[0m", hex), // Yellow (2nd)
            2 => format!("\x1b[91m{}\x1b[0m", hex), // Red (3rd)
            _ => hex,
        }
    } else {
        hex
    }
}
