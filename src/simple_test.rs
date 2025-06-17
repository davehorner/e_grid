use std::collections::HashMap;
use std::ptr;
use winapi::shared::minwindef::{LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::libloaderapi::{GetModuleHandleW, GetProcAddress};
use winapi::um::winuser::*;

fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buffer = [0u16; 256];
        let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if len > 0 {
            String::from_utf16_lossy(&buffer[..len as usize])
        } else {
            String::new()
        }
    }
}

fn get_window_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) != 0 {
            Some(rect)
        } else {
            None
        }
    }
}

fn is_manageable_window(hwnd: HWND) -> bool {
    unsafe {
        if IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
            return false;
        }

        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        
        // Skip tool windows, but include app windows
        if (ex_style & WS_EX_TOOLWINDOW) != 0 && (ex_style & WS_EX_APPWINDOW) == 0 {
            return false;
        }

        // Must have a title
        let title = get_window_title(hwnd);
        if title.is_empty() {
            return false;
        }

        // Skip certain system windows
        if title.contains("Program Manager") || title.contains("Task Switching") {
            return false;
        }

        true
    }
}

unsafe extern "system" fn enum_windows_proc(hwnd: HWND, _lparam: LPARAM) -> i32 {
    if is_manageable_window(hwnd) {
        let title = get_window_title(hwnd);
        if let Some(rect) = get_window_rect(hwnd) {
            println!("Window: {} - ({}, {}) {}x{}", 
                title, 
                rect.left, rect.top, 
                rect.right - rect.left, 
                rect.bottom - rect.top
            );
        }
    }
    1 // Continue enumeration
}

fn main() {
    println!("Starting simple window test...");
    
    unsafe {
        println!("Calling EnumWindows...");
        let result = EnumWindows(Some(enum_windows_proc), 0);
        println!("EnumWindows result: {}", result);
    }
    
    println!("Test completed. Press Enter to exit...");
    let mut input = String::new();
    match std::io::stdin().read_line(&mut input) {
        Ok(_) => println!("Input received: {}", input.trim()),
        Err(e) => println!("Error reading input: {}", e),
    }
}
