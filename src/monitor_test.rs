use winapi::um::winuser::*;

fn main() {
    unsafe {
        println!("=== Monitor Detection Test ===");
        
        // Old method (single monitor work area)
        let mut work_area = winapi::shared::windef::RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work_area as *mut _ as *mut _, 0);
        
        println!("Primary Monitor Work Area:");
        println!("  Left: {}, Top: {}", work_area.left, work_area.top);
        println!("  Right: {}, Bottom: {}", work_area.right, work_area.bottom);
        println!("  Size: {}x{} px", 
            work_area.right - work_area.left, 
            work_area.bottom - work_area.top);
        
        // New method (virtual screen - all monitors)
        let virtual_left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let virtual_top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let virtual_width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let virtual_height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        
        println!("\nVirtual Screen (All Monitors):");
        println!("  Left: {}, Top: {}", virtual_left, virtual_top);
        println!("  Right: {}, Bottom: {}", virtual_left + virtual_width, virtual_top + virtual_height);
        println!("  Size: {}x{} px", virtual_width, virtual_height);
        
        // Additional monitor info
        let monitor_count = GetSystemMetrics(SM_CMONITORS);
        println!("\nMonitor Count: {}", monitor_count);
        
        let primary_width = GetSystemMetrics(SM_CXSCREEN);
        let primary_height = GetSystemMetrics(SM_CYSCREEN);
        println!("Primary Monitor: {}x{} px", primary_width, primary_height);
    }
}
