use crate::ipc::{self, WindowCommand};
use crate::GridConfig;
use iceoryx2::prelude::*;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::service::ipc::Service;
use std::collections::HashMap;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use winapi::um::winuser::{EnumDisplayMonitors};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientCellState {
    Empty,           // No window (on-screen area)
    Occupied(u64),   // Window present (HWND as u64 for thread safety)
    OffScreen,       // Off-screen area (outside actual monitor bounds)
}

#[derive(Clone, Debug)]
pub struct ClientWindowInfo {
    pub hwnd: u64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub virtual_row_start: u32,
    pub virtual_col_start: u32,
    pub virtual_row_end: u32,
    pub virtual_col_end: u32,
    pub monitor_id: u32,
    pub monitor_row_start: u32,
    pub monitor_col_start: u32,
    pub monitor_row_end: u32,
    pub monitor_col_end: u32,
    pub title_len: u32,
}

impl From<ipc::WindowDetails> for ClientWindowInfo {
    fn from(details: ipc::WindowDetails) -> Self {
        Self {
            hwnd: details.hwnd,
            x: details.x,
            y: details.y,
            width: details.width,
            height: details.height,
            virtual_row_start: details.virtual_row_start,
            virtual_col_start: details.virtual_col_start,
            virtual_row_end: details.virtual_row_end,
            virtual_col_end: details.virtual_col_end,
            monitor_id: details.monitor_id,
            monitor_row_start: details.monitor_row_start,
            monitor_col_start: details.monitor_col_start,
            monitor_row_end: details.monitor_row_end,
            monitor_col_end: details.monitor_col_end,
            title_len: details.title_len,
        }
    }
}

pub struct GridClient {
    // Configuration
    config: GridConfig,
    
    // IPC components - only keep what we need for sending commands
    command_publisher: Publisher<Service, ipc::WindowCommand, ()>,
      // Local grid state
    windows: Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
    virtual_grid: Arc<Mutex<Vec<Vec<ClientCellState>>>>,
    
    // Monitor information - store complete monitor grids  
    monitors: Arc<Mutex<Vec<MonitorGridInfo>>>,
    
    // Control flags
    auto_display: Arc<Mutex<bool>>,
    running: Arc<Mutex<bool>>,
}

#[derive(Clone, Debug)]
pub struct MonitorGridInfo {    pub monitor_id: u32,  
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub grid: Vec<Vec<Option<u64>>>,
}

impl GridClient {
    /// Request grid configuration from server before creating client
    fn request_grid_config_from_server() -> Result<GridConfig, Box<dyn std::error::Error>> {
        // For now, return a default config
        // TODO: Implement actual IPC request to server
        println!("⚙️ Using default grid configuration (TODO: implement server request)");
        Ok(GridConfig::new(4, 6)) // Default 4x6 grid
    }
    
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let node = NodeBuilder::new().create::<Service>()?;

        // Create command publisher
        let command_service = node
            .service_builder(&ServiceName::new(ipc::GRID_COMMANDS_SERVICE)?)
            .publish_subscribe::<ipc::WindowCommand>()
            .open()?;
        let command_publisher = command_service.publisher_builder().create()?;

        // First, get the grid configuration from the server
        let config = Self::request_grid_config_from_server()?;
        
        // Now initialize with the dynamic config
        let virtual_grid = vec![vec![ClientCellState::Empty; config.cols]; config.rows];
        
        let client = Self {
            config,
            command_publisher,
            windows: Arc::new(Mutex::new(HashMap::new())),
            virtual_grid: Arc::new(Mutex::new(virtual_grid)),
            monitors: Arc::new(Mutex::new(Vec::new())),
            auto_display: Arc::new(Mutex::new(true)),
            running: Arc::new(Mutex::new(true)),
        };

        println!("✅ Client initialized with grid size: {}x{}", client.config.rows, client.config.cols);

        // Initialize grid with off-screen areas marked
        client.initialize_client_grid()?;

        Ok(client)
    }    pub fn request_grid_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let command = WindowCommand {
            command_type: 8, // New command type for GetGridConfig
            hwnd: 0,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
            layout_id: 1001, // Use layout_id as command identifier
            animation_duration_ms: 0,
            easing_type: 0,
        };
        self.command_publisher.send_copy(command)?;
        println!("🔧 Requested grid configuration from server...");
        Ok(())
    }
    fn process_responses(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement response subscriber for config updates
        // For now this is a placeholder
        Ok(())
    }    pub fn wait_for_config(&mut self, timeout_ms: u64) -> Result<GridConfig, Box<dyn std::error::Error>> {
        // For now, just return the current config
        // TODO: Implement actual waiting for server response
        Ok(self.config.clone())
    }
    
    fn initialize_client_grid(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Get virtual screen dimensions
        let virtual_rect = Self::get_virtual_screen_rect();
        let actual_monitors = Self::get_actual_monitor_bounds();
        
        println!("🔍 DEBUG: Client virtual screen: {}x{} at ({},{})", 
            virtual_rect.2 - virtual_rect.0, virtual_rect.3 - virtual_rect.1,
            virtual_rect.0, virtual_rect.1);
        println!("🔍 DEBUG: Client found {} monitors", actual_monitors.len());
        for (i, monitor) in actual_monitors.iter().enumerate() {
            println!("   Monitor {}: {}x{} at ({},{})", i, 
                monitor.2 - monitor.0, monitor.3 - monitor.1,
                monitor.0, monitor.1);
        }
        
        // Store monitor information for display
        if let Ok(mut monitors_lock) = self.monitors.lock() {
            monitors_lock.clear();
            for (i, monitor) in actual_monitors.iter().enumerate() {
                let monitor_info = MonitorGridInfo {
                    monitor_id: i as u32,
                    width: monitor.2 - monitor.0,
                    height: monitor.3 - monitor.1,
                    x: monitor.0,
                    y: monitor.1,
                    grid: vec![vec![None; self.config.cols]; self.config.rows],
                };
                monitors_lock.push(monitor_info);
            }
            println!("🔍 DEBUG: Stored {} monitor grid structures", monitors_lock.len());
        }
        
        let cell_width = (virtual_rect.2 - virtual_rect.0) / self.config.cols as i32;
        let cell_height = (virtual_rect.3 - virtual_rect.1) / self.config.rows as i32;
        
        if let Ok(mut grid) = self.virtual_grid.lock() {
            // Initialize all cells based on whether they're on an actual monitor  
            for row in 0..self.config.rows {
                for col in 0..self.config.cols {
                    let cell_left = virtual_rect.0 + (col as i32 * cell_width);
                    let cell_top = virtual_rect.1 + (row as i32 * cell_height);
                    let cell_right = cell_left + cell_width;
                    let cell_bottom = cell_top + cell_height;
                    
                    // Check if this cell overlaps with any actual monitor
                    let mut is_on_screen = false;
                    for monitor_rect in &actual_monitors {
                        if cell_left < monitor_rect.2 && cell_right > monitor_rect.0 &&
                           cell_top < monitor_rect.3 && cell_bottom > monitor_rect.1 {
                            is_on_screen = true;
                            break;
                        }
                    }
                      grid[row][col] = if is_on_screen {
                        ClientCellState::Empty
                    } else {
                        ClientCellState::OffScreen
                    };
                }
            }
        }
        
        Ok(())
    }

    fn get_virtual_screen_rect() -> (i32, i32, i32, i32) {
        unsafe {
            use winapi::um::winuser::{GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, 
                                      SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN};
            let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
            (left, top, left + width, top + height)
        }
    }

    fn get_actual_monitor_bounds() -> Vec<(i32, i32, i32, i32)> {
        let mut monitors = Vec::new();
        
        unsafe {
            // Enumerate all monitors
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
            
            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(monitor_enum_proc),
                &mut monitors as *mut Vec<(i32, i32, i32, i32)> as winapi::shared::minwindef::LPARAM,
            );
        }
        
        monitors
    }    pub fn start_background_monitoring(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let windows = self.windows.clone();
        let virtual_grid = self.virtual_grid.clone();
        let monitors = self.monitors.clone();
        let auto_display = self.auto_display.clone();
        let running = self.running.clone();
        let config = self.config.clone(); // Clone the config for the background thread
        
        thread::spawn(move || {
            // Create new node and subscribers for the background thread
            match Self::create_background_subscribers() {
                Ok((event_subscriber, window_details_subscriber)) => {
                    println!("🔍 Background monitoring started - listening for real-time updates...");
                    
                    while *running.lock().unwrap() {                        let mut events_received = 0;
                        let mut details_received = 0;
                        
                        // Process window events
                        while let Some(event_sample) = event_subscriber.receive().unwrap_or(None) {
                            let event = *event_sample;
                            events_received += 1;
                            Self::handle_window_event(&event, &windows, &virtual_grid, &monitors, &auto_display, &config);
                        }                        // Process window details updates
                        let mut batch_count = 0;
                        while let Some(details_sample) = window_details_subscriber.receive().unwrap_or(None) {
                            let details = *details_sample;
                            details_received += 1;
                            batch_count += 1;
                            
                            println!("\n🔍 DEBUG: Received window details #{} (batch #{}): HWND {} - {}", 
                                details_received, batch_count, details.hwnd, 
                                if details.title_len > 0 { "with title" } else { "no title" });
                            
                            // Get window title using Windows API
                            let title = Self::get_window_title(details.hwnd as winapi::shared::windef::HWND);
                            
                            println!("   📋 Title: '{}'", title.chars().take(50).collect::<String>());
                            println!("   📐 Position: ({}, {}) Size: {}x{}", details.x, details.y, details.width, details.height);
                            println!("   🎯 Virtual Grid: ({}, {}) to ({}, {})", 
                                details.virtual_row_start, details.virtual_col_start,
                                details.virtual_row_end, details.virtual_col_end);
                            println!("   🖥️  Monitor {}: ({}, {}) to ({}, {})", 
                                details.monitor_id,
                                details.monitor_row_start, details.monitor_col_start,
                                details.monitor_row_end, details.monitor_col_end);
                            
                            Self::handle_window_details(&details, &windows, &virtual_grid, &monitors, &auto_display, &config);
                            
                            // Print current window count and brief grid after each window
                            let current_window_count = windows.lock().unwrap().len();
                            println!("   📊 Total windows cached: {}", current_window_count);
                            
                            // Sleep briefly to make output readable
                            thread::sleep(Duration::from_millis(200));
                        }
                        
                        if batch_count > 0 {
                            println!("📦 Processed batch of {} window details messages", batch_count);
                        }
                        
                        // Print periodic status if we're receiving data
                        static mut LAST_STATUS_TIME: std::time::Instant = unsafe { std::mem::zeroed() };
                        static mut STATUS_INITIALIZED: bool = false;
                        
                        unsafe {
                            if !STATUS_INITIALIZED {
                                LAST_STATUS_TIME = std::time::Instant::now();
                                STATUS_INITIALIZED = true;
                            }
                              if LAST_STATUS_TIME.elapsed().as_secs() > 10 {
                                let windows_lock = windows.lock().unwrap();
                                let window_count = windows_lock.len();
                                println!("\n� ===== CLIENT STATUS UPDATE =====");
                                println!("�🔍 Client monitoring status: {} windows cached", window_count);
                                  // Print list of all cached windows
                                if window_count > 0 {
                                    println!("📄 Current window list:");
                                    for (i, (hwnd, window_info)) in windows_lock.iter().enumerate() {
                                        let title = Self::get_window_title(*hwnd as winapi::shared::windef::HWND);
                                        println!("  {}. HWND {} - '{}' at ({}, {}) size {}x{}", 
                                            i + 1, hwnd, 
                                            title.chars().take(40).collect::<String>(),
                                            window_info.x, window_info.y, window_info.width, window_info.height);
                                    }
                                      // Also show current grid state
                                    println!("\n📊 Current Client Grid State:");
                                    Self::display_virtual_grid(&virtual_grid, &windows, &config);
                                }
                                
                                LAST_STATUS_TIME = std::time::Instant::now();
                                println!("================================\n");                            }
                        }
                        
                        // Small sleep to prevent busy waiting and make output readable
                        thread::sleep(Duration::from_millis(50));
                    }
                }
                Err(e) => {
                    println!("❌ Failed to create background subscribers: {}", e);
                }
            }
            
            println!("🛑 Background monitoring stopped");
        });
        
        // Automatically request initial window list after a short delay to let monitoring start
        println!("🔍 DEBUG: About to sleep before requesting initial data...");
        std::thread::sleep(Duration::from_millis(100));
        
        // Request initial data using the existing public methods
        println!("📡 Requesting initial window data from server...");
        match self.request_window_list() {
            Ok(_) => println!("✅ Window list request sent"),
            Err(e) => println!("❌ Failed to send window list request: {}", e),
        }
        match self.request_grid_state() {
            Ok(_) => println!("✅ Grid state request sent"),
            Err(e) => println!("❌ Failed to send grid state request: {}", e),
        }
        println!("📡 Initial data requests completed");
        
        Ok(())
    }
  

    fn create_background_subscribers() -> Result<(Subscriber<Service, ipc::WindowEvent, ()>, Subscriber<Service, ipc::WindowDetails, ()>), Box<dyn std::error::Error>> {
        let node = NodeBuilder::new().create::<Service>()?;

        // Create event subscriber
        let event_service = node
            .service_builder(&ServiceName::new(ipc::GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<ipc::WindowEvent>()
            .open()?;
        let event_subscriber = event_service.subscriber_builder().create()?;

        // Create window details subscriber
        let window_details_service = node
            .service_builder(&ServiceName::new(ipc::GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<ipc::WindowDetails>()
            .open()?;
        let window_details_subscriber = window_details_service.subscriber_builder().create()?;        Ok((event_subscriber, window_details_subscriber))
    }    fn handle_window_event(
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
            0 => { // Window created - we'll get window details shortly
                println!("   🆕 New window {} created, waiting for details...", event.hwnd);
            }
            1 => { // Window destroyed
                Self::remove_window_from_client(event.hwnd, windows, virtual_grid, monitors);
                println!("   🗑️  Removed window {} from client state", event.hwnd);
            }
            2 => { // Window moved - we'll get updated window details shortly
                println!("   🔄 Window {} moved, waiting for updated details...", event.hwnd);
            }
            _ => {}
        }
        
        // Only display grid for significant events and not too frequently
        static mut LAST_EVENT_DISPLAY: std::time::Instant = unsafe { std::mem::zeroed() };
        static mut EVENT_DISPLAY_INITIALIZED: bool = false;
        
        unsafe {
            if !EVENT_DISPLAY_INITIALIZED {
                LAST_EVENT_DISPLAY = std::time::Instant::now();
                EVENT_DISPLAY_INITIALIZED = true;
            }
            
            if *auto_display.lock().unwrap() &&               (event.event_type == 0 || event.event_type == 1) && // Only for create/destroy
               LAST_EVENT_DISPLAY.elapsed().as_millis() > 500 { // Max twice per second
                println!("   📊 Displaying grid after {} event...", event_name);
                Self::display_virtual_grid(&virtual_grid, &windows, &config);
                LAST_EVENT_DISPLAY = std::time::Instant::now();
            }
        }
    }
      /// Remove a window from all client state
    fn remove_window_from_client(
        hwnd: u64,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
    ) {
        // Remove from window cache
        if let Ok(mut windows_lock) = windows.lock() {
            windows_lock.remove(&hwnd);
        }
        
        // Remove from virtual grid
        if let Ok(mut grid) = virtual_grid.lock() {
            for row in 0..grid.len() {
                for col in 0..grid[row].len() {
                    if let ClientCellState::Occupied(existing_hwnd) = grid[row][col] {
                        if existing_hwnd == hwnd {
                            grid[row][col] = ClientCellState::Empty;
                        }
                    }
                }
            }
        }
          // Remove from monitor grids
        if let Ok(mut monitors_lock) = monitors.lock() {
            for monitor in monitors_lock.iter_mut() {
                for row in 0..monitor.grid.len() {
                    for col in 0..monitor.grid[row].len() {
                        if monitor.grid[row][col] == Some(hwnd) {
                            monitor.grid[row][col] = None;
                        }
                    }
                }
            }
        }
    }
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
        
        // Update local window cache
        if let Ok(mut windows_lock) = windows.lock() {
            let window_info = ClientWindowInfo::from(*details);
            windows_lock.insert(details.hwnd, window_info);
        }
        
        // Update virtual grid
        Self::update_virtual_grid(&details, &virtual_grid);
          // Update monitor grids
        Self::update_monitor_grids(&details, &monitors, config);// Auto-display grid if enabled (but not too frequently)
        if *auto_display.lock().unwrap() {
            static mut LAST_AUTO_DISPLAY: std::time::Instant = unsafe { std::mem::zeroed() };
            static mut AUTO_DISPLAY_INITIALIZED: bool = false;
            
            unsafe {
                if !AUTO_DISPLAY_INITIALIZED {
                    LAST_AUTO_DISPLAY = std::time::Instant::now();
                    AUTO_DISPLAY_INITIALIZED = true;
                }
                  // Only auto-display if it's been at least 1 second since last display
                if LAST_AUTO_DISPLAY.elapsed().as_millis() > 1000 {
                    println!("   🔄 Auto-displaying updated grid...");
                    Self::display_virtual_grid(&virtual_grid, &windows, config);
                    println!("   ─────────────────────────────────────");
                    LAST_AUTO_DISPLAY = std::time::Instant::now();
                } else {
                    println!("   ⏳ Auto-display throttled (last update {} ms ago)", 
                        LAST_AUTO_DISPLAY.elapsed().as_millis());
                }
            }
        }
    }    
      fn update_virtual_grid(
        details: &ipc::WindowDetails,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
    ) {
        if let Ok(mut grid) = virtual_grid.lock() {
            let hwnd = details.hwnd;
            
            // Clear previous positions for this window
            for row in 0..grid.len() {
                for col in 0..grid[row].len() {
                    if let ClientCellState::Occupied(existing_hwnd) = grid[row][col] {
                        if existing_hwnd == hwnd {
                            grid[row][col] = ClientCellState::Empty;
                        }
                    }
                }
            }
            
            // Set new positions
            for row in details.virtual_row_start..=details.virtual_row_end {
                for col in details.virtual_col_start..=details.virtual_col_end {
                    if row < grid.len() as u32 && col < grid[0].len() as u32 {
                        // Only update if it's not an off-screen cell
                        if let ClientCellState::OffScreen = grid[row as usize][col as usize] {
                            // Don't overwrite off-screen markers
                        } else {
                            grid[row as usize][col as usize] = ClientCellState::Occupied(hwnd);
                        }
                    }
                }
            }
        }
    }    
      fn display_virtual_grid(
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        config: &GridConfig,
    ) {
        println!("\n🔥 REAL-TIME GRID UPDATE:");
        println!("{}", "=".repeat(60));
        
        let window_count = windows.lock().unwrap().len();
        println!("Client Grid Viewer - {}x{} Grid ({} windows)", config.rows, config.cols, window_count);
        println!("{}", "=".repeat(60));

        // Print column headers
        print!("   ");
        for col in 0..config.cols {
            print!("{:2} ", col);
        }
        println!();

        // Print grid
        if let Ok(grid) = virtual_grid.lock() {
            for row in 0..config.rows {
                print!("{:2} ", row);
                
                for col in 0..config.cols {
                    match grid[row][col] {
                        ClientCellState::Occupied(_hwnd) => print!("## "),
                        ClientCellState::Empty => print!(".. "),
                        ClientCellState::OffScreen => print!("XX "),
                    }
                }
                println!();
            }
        }
        
        println!("{}", "=".repeat(60));
        println!();
    }
      fn update_monitor_grids(
        details: &ipc::WindowDetails,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
        config: &GridConfig,
    ) {        
        if let Ok(mut monitors_lock) = monitors.lock() {
            // Ensure we have enough monitor grid entries
            let current_len = monitors_lock.len();
            while monitors_lock.len() <= details.monitor_id as usize {
                let new_id = monitors_lock.len() as u32;
                monitors_lock.push(MonitorGridInfo {
                    monitor_id: new_id,
                    width: 0,
                    height: 0, 
                    x: 0,
                    y: 0,
                    grid: vec![vec![None; config.cols]; config.rows],
                });
            }
            
            let monitor = &mut monitors_lock[details.monitor_id as usize];
            
            // Clear previous positions for this window in this monitor
            for row in 0..monitor.grid.len() {
                for col in 0..monitor.grid[row].len() {
                    if monitor.grid[row][col] == Some(details.hwnd) {
                        monitor.grid[row][col] = None;
                    }
                }
            }
            
            // Set new positions in monitor grid
            for row in details.monitor_row_start..=details.monitor_row_end {
                for col in details.monitor_col_start..=details.monitor_col_end {
                    if row < config.rows as u32 && col < config.cols as u32 {
                        monitor.grid[row as usize][col as usize] = Some(details.hwnd);
                    }
                }
            }
            
            // Update monitor dimensions if we can infer them from window position
            // This is approximate - ideally we'd get actual monitor info from the server
            let window_right = details.x + details.width;
            let window_bottom = details.y + details.height; 
            if window_right > monitor.width {
                monitor.width = window_right;
            }
            if window_bottom > monitor.height {
                monitor.height = window_bottom;
            }        
        }
    }
      fn display_complete_grid(
        &self,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
    ) {
        let window_count = match windows.try_lock() {
            Ok(windows_lock) => windows_lock.len(),
            Err(_) => {
                println!("⚠️ Windows cache locked, skipping grid display");
                return;
            }
        };
        
        // Get actual virtual screen dimensions like the server
        let virtual_rect = Self::get_virtual_screen_rect();
        let virtual_width = virtual_rect.2 - virtual_rect.0;
        let virtual_height = virtual_rect.3 - virtual_rect.1;
        
        println!("\n============================================================");
        println!("Window Grid Tracker - {}x{} Grid ({} windows)", self.config.rows, self.config.cols, window_count);
        println!("Monitor: {}x{} px", virtual_width, virtual_height);
        println!("============================================================");        // Print column headers
        print!("    ");
        for col in 0..self.config.cols {
            print!("{:2} ", col);
        }
        println!();
          // Print virtual grid with window representation (like server)
        if let Ok(grid) = virtual_grid.try_lock() {
            for row in 0..self.config.rows {
                print!(" {}: ", row);
                
                for col in 0..self.config.cols {
                    match grid[row][col] {
                        ClientCellState::Occupied(hwnd) => {
                            // Display last 2 decimal digits of HWND in hex format like the server
                            let display_val = (hwnd % 100) as u8;
                            if display_val == 0 {
                                print!("XX ");
                            } else {
                                print!("{:2X} ", display_val);
                            }
                        }
                        ClientCellState::Empty => print!(".. "),
                        ClientCellState::OffScreen => print!("XX "),
                    }
                }
                println!();
            }}
        
        println!();
        
        // Print monitor grids
        match monitors.try_lock() {
            Ok(monitors_lock) => {
                if !monitors_lock.is_empty() {
                    println!("\n🖥️ Monitor Grids:");
                    for (i, monitor) in monitors_lock.iter().enumerate() {
                        if monitor.width > 0 || monitor.height > 0 {
                            println!("  Monitor {}: {}x{}", i, monitor.width, monitor.height);
                        }
                        // Print column headers
                        print!("      ");
                        for col in 0..self.config.cols {
                            print!("{:2} ", col);
                        }
                        println!();
                        
                        // Print monitor grid
                        for row in 0..self.config.rows {
                            print!(" {}:  ", row);
                            
                            for col in 0..self.config.cols {
                                match monitor.grid[row][col] {                                    
                                    Some(hwnd) => {
                                        // Display last 2 decimal digits of HWND in hex format like server
                                        let display_val = (hwnd % 100) as u8;
                                        if display_val == 0 {
                                            print!(" X ");
                                        } else {
                                            print!("{:2X} ", display_val);
                                        }
                                    }
                                    None => print!(" . "),
                                }
                            }
                            println!();                        
                        }
                    }
                }
            }
            Err(_) => {
                println!("⚠️ Monitor grids locked, skipping monitor grid display");
            }
        }
        
        println!();
    }

    pub fn send_command(&mut self, command: ipc::WindowCommand) -> Result<(), Box<dyn std::error::Error>> {
        self.command_publisher.send_copy(command)?;
        Ok(())
    }
      pub fn request_window_list(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let command = ipc::WindowCommand {
            command_type: 2, // GetWindowList
            hwnd: 0,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
            layout_id: 0,
            animation_duration_ms: 0,
            easing_type: 0,
        };
        self.send_command(command)
    }
    
    pub fn request_grid_state(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let command = ipc::WindowCommand {
            command_type: 1, // GetGridState
            hwnd: 0,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
            layout_id: 0,
            animation_duration_ms: 0,
            easing_type: 0,
        };
        self.send_command(command)
    }
      pub fn assign_window_to_virtual_cell(&mut self, hwnd: u64, row: u32, col: u32) -> Result<(), Box<dyn std::error::Error>> {
        let command = ipc::WindowCommand {
            command_type: 3, // AssignToVirtualCell
            hwnd,
            target_row: row,
            target_col: col,
            monitor_id: 0,
            layout_id: 0,
            animation_duration_ms: 0,
            easing_type: 0,
        };
        self.send_command(command)
    }
    
    pub fn assign_window_to_monitor_cell(&mut self, hwnd: u64, row: u32, col: u32, monitor_id: u32) -> Result<(), Box<dyn std::error::Error>> {
        let command = ipc::WindowCommand {
            command_type: 4, // AssignToMonitorCell
            hwnd,
            target_row: row,
            target_col: col,
            monitor_id,
            layout_id: 0,
            animation_duration_ms: 0,
            easing_type: 0,
        };
        self.send_command(command)
    }
      pub fn display_current_grid(&self) {
        self.display_complete_grid(&self.virtual_grid, &self.windows, &self.monitors);
    }
    
    pub fn display_window_list(&self) {
        println!("\n📋 Current Windows:");
        println!("{}", "-".repeat(80));
        
        if let Ok(windows) = self.windows.lock() {
            if windows.is_empty() {
                println!("   (No windows currently tracked)");
            } else {
                for (i, (hwnd, info)) in windows.iter().enumerate() {
                    println!("   [{}] HWND: {} | Position: ({}, {}) | Size: {}x{}", 
                        i + 1, hwnd, info.x, info.y, info.width, info.height);
                    println!("       Virtual: ({},{}) to ({},{}) | Monitor {}: ({},{}) to ({},{})",
                        info.virtual_row_start, info.virtual_col_start,
                        info.virtual_row_end, info.virtual_col_end,
                        info.monitor_id,
                        info.monitor_row_start, info.monitor_col_start,
                        info.monitor_row_end, info.monitor_col_end);
                }
            }
        }
        println!("{}", "-".repeat(80));
    }
    
    pub fn set_auto_display(&self, enabled: bool) {
        *self.auto_display.lock().unwrap() = enabled;
        println!("🔄 Auto-display {}", if enabled { "enabled" } else { "disabled" });    
    }
    
    pub fn is_auto_display_enabled(&self) -> bool {
        *self.auto_display.lock().unwrap()
    }
    
    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
        println!("🛑 Stopping grid client...");
    }
    
    /// Get window title using Windows API
    fn get_window_title(hwnd: winapi::shared::windef::HWND) -> String {
        use winapi::um::winuser::{GetWindowTextW, GetWindowTextLengthW};
        
        unsafe {
            let length = GetWindowTextLengthW(hwnd);
            if length == 0 {
                return String::new();
            }
            
            let mut buffer: Vec<u16> = vec![0; (length + 1) as usize];
            let result = GetWindowTextW(hwnd, buffer.as_mut_ptr(), length + 1);
              if result > 0 {
                buffer.truncate(result as usize);
                String::from_utf16_lossy(&buffer)
            } else {
                String::new()
            }
        }
    }
}

impl Drop for GridClient {
    fn drop(&mut self) {
        self.stop();
    }
}

impl GridClient {
    /// Get the current grid configuration
    pub fn get_config(&self) -> &GridConfig {
        &self.config
    }
}
