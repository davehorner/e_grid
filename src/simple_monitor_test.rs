use winapi::um::winuser::{EnumDisplayMonitors, GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN};
use std::ptr;

fn main() {
    println!("🔍 MONITOR DETECTION TEST");
    println!("========================");
    
    // Get virtual screen information
    let virtual_rect = get_virtual_screen_rect();
    println!("🖥️ Virtual Screen: {}x{} at ({},{})", 
        virtual_rect.2 - virtual_rect.0, virtual_rect.3 - virtual_rect.1,
        virtual_rect.0, virtual_rect.1);
    
    // Get actual monitor bounds
    match get_actual_monitor_bounds() {
        Ok(monitors) => {
            println!("📺 Found {} monitors:", monitors.len());
            for (i, monitor) in monitors.iter().enumerate() {
                println!("   Monitor {}: {}x{} at ({},{})", 
                    i, 
                    monitor.2 - monitor.0, monitor.3 - monitor.1,
                    monitor.0, monitor.1);
            }
            
            // Test grid calculation
            let grid_rows = 8;
            let grid_cols = 12;
            println!("\n🔢 Grid Calculation Test ({}x{}):", grid_rows, grid_cols);
            
            let cell_width = (virtual_rect.2 - virtual_rect.0) / grid_cols;
            let cell_height = (virtual_rect.3 - virtual_rect.1) / grid_rows;
            
            println!("   Cell size: {}x{}", cell_width, cell_height);
            
            // Test a few grid positions
            for row in [0, 2, 4] {
                for col in [0, 3, 6, 9] {
                    let cell_left = virtual_rect.0 + (col * cell_width);
                    let cell_top = virtual_rect.1 + (row * cell_height);
                    let cell_right = cell_left + cell_width;
                    let cell_bottom = cell_top + cell_height;
                    
                    // Check which monitor this cell is on
                    let mut monitor_id = None;
                    for (i, monitor_rect) in monitors.iter().enumerate() {
                        if cell_left < monitor_rect.2 && cell_right > monitor_rect.0 &&
                           cell_top < monitor_rect.3 && cell_bottom > monitor_rect.1 {
                            monitor_id = Some(i);
                            break;
                        }
                    }
                    
                    match monitor_id {
                        Some(id) => println!("   Cell ({},{}) -> Monitor {} at ({},{})", row, col, id, cell_left, cell_top),
                        None => println!("   Cell ({},{}) -> OFF-SCREEN at ({},{})", row, col, cell_left, cell_top),
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Failed to get monitor bounds: {}", e);
        }
    }
}

fn get_virtual_screen_rect() -> (i32, i32, i32, i32) {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        (left, top, left + width, top + height)
    }
}

fn get_actual_monitor_bounds() -> Result<Vec<(i32, i32, i32, i32)>, Box<dyn std::error::Error>> {
    let mut monitors = Vec::new();
    
    unsafe {
        extern "system" fn monitor_enum_proc(
            _hmonitor: winapi::shared::windef::HMONITOR,
            _hdc: winapi::shared::windef::HDC,
            rect: *mut winapi::shared::windef::RECT,
            data: winapi::shared::minwindef::LPARAM,
        ) -> i32 {
            unsafe {
                let monitors = &mut *(data as *mut Vec<(i32, i32, i32, i32)>);
                let r = *rect;
                monitors.push((r.left, r.top, r.right, r.bottom));
            }
            1 // Continue enumeration
        }
        
        let result = EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(monitor_enum_proc),
            &mut monitors as *mut Vec<(i32, i32, i32, i32)> as winapi::shared::minwindef::LPARAM,
        );
        
        if result == 0 {
            return Err("Failed to enumerate display monitors".into());
        }
    }
    
    if monitors.is_empty() {
        return Err("No monitors detected".into());
    }
    
    Ok(monitors)
}
