use e_grid::ipc::{self, WindowEvent, WindowDetails, WindowFocusEvent};
use iceoryx2::prelude::*;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::service::ipc::Service;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use winapi::shared::windef::HWND;
use winapi::um::winuser::{
    GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    IsWindow, IsWindowVisible
};

/// Demo server for focus tracking examples
/// This server simulates focus events and publishes them via IPC for the focus tracking demos
pub struct FocusDemoServer {
    // IPC Publishers
    event_publisher: Publisher<Service, WindowEvent, ()>,
    details_publisher: Publisher<Service, WindowDetails, ()>,
    focus_publisher: Publisher<Service, WindowFocusEvent, ()>,
    
    // State tracking
    current_focus: Arc<Mutex<Option<HWND>>>,
    tracked_windows: Arc<Mutex<HashSet<u64>>>,
    
    // Demo control
    running: Arc<Mutex<bool>>,
}

impl FocusDemoServer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        println!("🚀 Starting Focus Demo Server...");
        
        // Create IPC node
        let node = NodeBuilder::new().create::<Service>()?;
        
        // Create event service and publisher
        println!("📡 Setting up window events IPC service...");
        let event_service = node
            .service_builder(&ServiceName::new(ipc::GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowEvent>()
            .create()?;
        let event_publisher = event_service.publisher_builder().create()?;
        
        // Create window details service and publisher
        println!("📊 Setting up window details IPC service...");
        let details_service = node
            .service_builder(&ServiceName::new(ipc::GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<WindowDetails>()
            .create()?;
        let details_publisher = details_service.publisher_builder().create()?;
        
        // Create focus events service and publisher
        println!("🎯 Setting up focus events IPC service...");
        let focus_service = node
            .service_builder(&ServiceName::new(ipc::GRID_FOCUS_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowFocusEvent>()
            .create()?;
        let focus_publisher = focus_service.publisher_builder().create()?;
        
        Ok(Self {
            event_publisher,
            details_publisher,
            focus_publisher,
            current_focus: Arc::new(Mutex::new(None)),
            tracked_windows: Arc::new(Mutex::new(HashSet::new())),
            running: Arc::new(Mutex::new(true)),
        })
    }    pub fn start(&mut self, external_running: Arc<Mutex<bool>>) -> Result<(), Box<dyn std::error::Error>> {
        println!("✅ Focus Demo Server started successfully!");
        println!("🔍 Monitoring focus changes...");
        println!("💡 Start a focus tracking example in another terminal to see events");
        println!("⌨️  Press Ctrl+C to stop\n");
        
        let mut last_focus: Option<HWND> = None;
        let mut check_count = 0u32;
        
        // Single-threaded monitoring loop - check both internal and external running flags
        while *self.running.lock().unwrap() && *external_running.lock().unwrap() {
            check_count += 1;
            
            // Get currently focused window
            let current_hwnd = unsafe { GetForegroundWindow() };
            
            // Check if focus changed
            if current_hwnd != last_focus.unwrap_or(std::ptr::null_mut()) {
                // Handle focus change
                self.handle_focus_change(
                    last_focus,
                    if current_hwnd.is_null() { None } else { Some(current_hwnd) },
                );
                
                last_focus = if current_hwnd.is_null() { None } else { Some(current_hwnd) };
            }
            
            // Print status every 30 seconds
            if check_count % 300 == 0 {
                let window_count = self.tracked_windows.lock().unwrap().len();
                let focused = self.current_focus.lock().unwrap();
                println!("📈 Server Status: {} windows tracked, focus: {:?}", 
                         window_count, 
                         focused.map(|h| h as u64).unwrap_or(0));
            }
            
            // Check every 100ms for responsive focus tracking
            thread::sleep(Duration::from_millis(100));
        }
        
        println!("🛑 Focus monitoring stopped");
        Ok(())
    }
    
    fn handle_focus_change(
        &mut self,
        old_focus: Option<HWND>,
        new_focus: Option<HWND>,
    ) {
        // Update current focus
        *self.current_focus.lock().unwrap() = new_focus;        
        // Handle defocus event
        if let Some(old_hwnd) = old_focus {
            if !old_hwnd.is_null() {
                self.send_focus_event(old_hwnd, 1); // 1 = DEFOCUSED
            }
        }
        
        // Handle focus event
        if let Some(new_hwnd) = new_focus {
            if !new_hwnd.is_null() && unsafe { IsWindow(new_hwnd) != 0 && IsWindowVisible(new_hwnd) != 0 } {
                // Track this window
                let hwnd_u64 = new_hwnd as u64;
                self.tracked_windows.lock().unwrap().insert(hwnd_u64);
                
                // Send window event
                self.send_window_event(new_hwnd, 0); // 0 = CREATED/FOCUSED
                
                // Send window details
                self.send_window_details(new_hwnd);
                
                // Send focus event
                self.send_focus_event(new_hwnd, 0); // 0 = FOCUSED
            }
        }
    }
    
    fn send_window_event(&mut self, hwnd: HWND, event_type: u32) {        let event = WindowEvent {
            event_type: event_type as u8,
            hwnd: hwnd as u64,
            row: 0, // Simplified for demo
            col: 0,
            old_row: 0,
            old_col: 0,
            timestamp: Self::get_timestamp(),
            total_windows: 1,
            occupied_cells: 1,
        };
        
        if let Err(e) = self.event_publisher.send_copy(event) {
            eprintln!("Failed to send window event: {:?}", e);
        }
    }
    
    fn send_window_details(&mut self, hwnd: HWND) {        let details = WindowDetails {
            hwnd: hwnd as u64,
            x: 0, // Simplified for demo
            y: 0,
            width: 800,
            height: 600,
            virtual_row_start: 0,
            virtual_col_start: 0,
            virtual_row_end: 1,
            virtual_col_end: 1,
            monitor_id: 0,
            monitor_row_start: 0,
            monitor_col_start: 0,
            monitor_row_end: 1,
            monitor_col_end: 1,
            title_len: 0,
        };
        
        if let Err(e) = self.details_publisher.send_copy(details) {
            eprintln!("Failed to send window details: {:?}", e);
        }
    }
    
    fn send_focus_event(&mut self, hwnd: HWND, event_type: u32) {        let window_title = Self::get_window_title(hwnd);
        let process_id = Self::get_process_id(hwnd);
        
        let focus_event = WindowFocusEvent {
            event_type: event_type as u8,
            hwnd: hwnd as u64,
            process_id,
            timestamp: Self::get_timestamp(),
            app_name_hash: Self::hash_string(&format!("Process_{}", process_id)),
            window_title_hash: Self::hash_string(&window_title),
            reserved: [0; 2],
        };
        
        let event_name = if event_type == 0 { "FOCUSED" } else { "DEFOCUSED" };
        println!("🎯 {} Window: {} (PID: {}) Title: '{}' Hash: 0x{:x}", 
                 event_name, hwnd as u64, process_id, 
                 if window_title.len() > 30 { &window_title[..30] } else { &window_title },
                 focus_event.app_name_hash);
        
        if let Err(e) = self.focus_publisher.send_copy(focus_event) {
            eprintln!("Failed to send focus event: {:?}", e);
        }
    }
    
    fn get_window_title(hwnd: HWND) -> String {
        unsafe {
            let mut buffer: [u16; 256] = [0; 256];
            let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if len > 0 {
                String::from_utf16_lossy(&buffer[..len as usize])
            } else {
                "(No Title)".to_string()
            }
        }
    }
    
    fn get_process_id(hwnd: HWND) -> u32 {
        unsafe {
            let mut process_id: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut process_id);
            process_id
        }
    }
    
    fn get_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    fn hash_string(s: &str) -> u64 {
        // Simple hash function for demo purposes
        let mut hash = 0u64;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }
    
    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
        println!("🛑 Stopping focus demo server...");
    }
}

impl Drop for FocusDemoServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 e_grid Focus Demo Server");
    println!("===========================");
    println!("This server provides focus events for the focus tracking examples.");
    println!("Run this first, then run any of the focus tracking examples.\n");
    
    let mut server = FocusDemoServer::new()?;
    
    // Handle Ctrl+C gracefully
    let running = Arc::new(Mutex::new(true));
    let running_clone = running.clone();
    
    ctrlc::set_handler(move || {
        println!("\n🛑 Received Ctrl+C, shutting down server...");
        *running_clone.lock().unwrap() = false;
    })?;
    
    server.start(running)?;
    
    println!("👋 Focus demo server stopped");
    Ok(())
}
