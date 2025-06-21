use crate::ipc::{self, WindowCommand};
use crate::GridConfig;
use crate::grid_client_errors::{GridClientError, GridClientResult, RetryConfig, 
                                retry_with_backoff, validate_grid_coordinates, 
                                safe_arc_lock};
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

#[derive(Debug)]
enum MonitoringResult {
    ServerDisconnected,
    Shutdown,
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
    
    // NEW: Focus event handling for e_midi integration
    focus_callback: Arc<Mutex<Option<Box<dyn Fn(ipc::WindowFocusEvent) + Send + Sync>>>>,
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
    fn request_grid_config_from_server() -> GridClientResult<GridConfig> {
        // For now, return the same default config as the server
        // TODO: Implement actual IPC request to server for dynamic configuration
        println!("⚙️ Using server default grid configuration (TODO: implement server request)");
        Ok(GridConfig::default()) // Use same default as server (8x12)
    }
    
    pub fn new() -> GridClientResult<Self> {
        let node = NodeBuilder::new()
            .create::<Service>()
            .map_err(|e| GridClientError::IpcError(format!("Failed to create IPC node: {}", e)))?;        // Create command publisher with retry logic
        let retry_config = RetryConfig::default();
        let command_publisher = retry_with_backoff(|| -> Result<_, Box<dyn std::error::Error>> {
            let command_service = node
                .service_builder(&ServiceName::new(ipc::GRID_COMMANDS_SERVICE).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                .publish_subscribe::<ipc::WindowCommand>()
                .open().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(command_service.publisher_builder().create().map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
        }, &retry_config)
        .map_err(|e| GridClientError::IpcError(format!("Failed to create command publisher: {:?}", e)))?;// First, get the grid configuration from the server
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
            focus_callback: Arc::new(Mutex::new(None)),
        };

        println!("✅ Client initialized with grid size: {}x{}", client.config.rows, client.config.cols);

        // Initialize grid with off-screen areas marked
        client.initialize_client_grid()
            .map_err(|e| GridClientError::InitializationError(format!("Grid initialization failed: {}", e)))?;

        Ok(client)
    }
    
    
    pub fn request_grid_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
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
      fn initialize_client_grid(&self) -> GridClientResult<()> {
        // Get virtual screen dimensions
        let virtual_rect = Self::get_virtual_screen_rect();
        let actual_monitors = Self::get_actual_monitor_bounds()
            .map_err(|e| GridClientError::MonitorError(format!("Failed to get monitor bounds: {}", e)))?;
        
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
        {
            let mut monitors_lock = safe_arc_lock(&self.monitors, "monitors initialization")?;
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
        
        {
            let mut grid = safe_arc_lock(&self.virtual_grid, "virtual grid initialization")?;
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
    }    fn get_actual_monitor_bounds() -> GridClientResult<Vec<(i32, i32, i32, i32)>> {
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
            
            let result = EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(monitor_enum_proc),
                &mut monitors as *mut Vec<(i32, i32, i32, i32)> as winapi::shared::minwindef::LPARAM,
            );
            
            if result == 0 {
                return Err(GridClientError::MonitorError(
                    "Failed to enumerate display monitors".to_string()
                ));
            }
        }
        
        if monitors.is_empty() {
            return Err(GridClientError::MonitorError(
                "No monitors detected".to_string()
            ));
        }        
        Ok(monitors)
    }

    pub fn start_background_monitoring(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let windows = self.windows.clone();
        let virtual_grid = self.virtual_grid.clone();
        let monitors = self.monitors.clone();
        let auto_display = self.auto_display.clone();
        let running = self.running.clone();
        let focus_callback = self.focus_callback.clone();
        let config = self.config.clone(); // Clone the config for the background thread
        
        thread::spawn(move || {
            let mut last_connection_attempt = std::time::Instant::now();
            let mut connection_retry_count = 0;
            let max_retries = 10;
            let retry_delay = Duration::from_secs(2);
            
            while *running.lock().unwrap() {
                // Try to create/recreate connection to server
                match Self::create_background_subscribers() {
                    Ok((event_subscriber, window_details_subscriber, focus_subscriber, heartbeat_subscriber)) => {
                        if connection_retry_count > 0 {
                            println!("✅ Successfully reconnected to e_grid server (attempt {})", connection_retry_count + 1);
                        } else {
                            println!("🔍 Background monitoring started - listening for real-time updates + focus events...");
                        }
                        connection_retry_count = 0; // Reset retry count on successful connection
                          // Main monitoring loop - process events while connected
                        let monitoring_result = Self::run_monitoring_loop(
                            &event_subscriber,
                            &window_details_subscriber, 
                            &focus_subscriber,
                            &heartbeat_subscriber,
                            &windows,
                            &virtual_grid,
                            &monitors,
                            &auto_display,
                            &running,
                            &focus_callback,
                            &config
                        );
                        
                        match monitoring_result {
                            MonitoringResult::ServerDisconnected => {
                                println!("⚠️ Lost connection to e_grid server - attempting to reconnect...");
                                connection_retry_count = 0; // Start fresh retry sequence
                            },
                            MonitoringResult::Shutdown => {
                                println!("🛑 Monitoring shutdown requested");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        connection_retry_count += 1;
                        if connection_retry_count == 1 {
                            println!("❌ Failed to connect to e_grid server: {}", e);
                            println!("🔄 Will retry connection every {} seconds...", retry_delay.as_secs());
                        } else if connection_retry_count <= max_retries {
                            println!("🔄 Reconnection attempt {} failed, retrying in {} seconds...", 
                                connection_retry_count, retry_delay.as_secs());
                        } else {
                            println!("💀 Max reconnection attempts ({}) exceeded. Monitoring suspended.", max_retries);
                            println!("   Please ensure the e_grid server is running and restart the client.");
                            break;
                        }
                        
                        // Wait before retrying
                        thread::sleep(retry_delay);
                        last_connection_attempt = std::time::Instant::now();
                    }
                }
            }
            
            println!("🛑 Background monitoring stopped");
        });
        
        // Wait a moment for initial connection attempt before requesting data
        thread::sleep(Duration::from_millis(500));
        
        // Request initial data
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

    fn run_monitoring_loop(        event_subscriber: &Subscriber<Service, ipc::WindowEvent, ()>,
        window_details_subscriber: &Subscriber<Service, ipc::WindowDetails, ()>,
        focus_subscriber: &Subscriber<Service, ipc::WindowFocusEvent, ()>,
        heartbeat_subscriber: &Subscriber<Service, ipc::HeartbeatMessage, ()>,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
        auto_display: &Arc<Mutex<bool>>,
        running: &Arc<Mutex<bool>>,
        focus_callback: &Arc<Mutex<Option<Box<dyn Fn(ipc::WindowFocusEvent) + Send + Sync>>>>,
        config: &GridConfig,
    ) -> MonitoringResult {
        let mut consecutive_empty_cycles = 0;
        let max_empty_cycles = 200; // If no data for 200 cycles (10+ seconds), assume disconnection
        
        loop {
            if !*running.lock().unwrap() {
                return MonitoringResult::Shutdown;
            }
            
            let mut _events_received = 0;
            let mut _details_received = 0;
            let mut _focus_events_received = 0;
            let mut had_activity = false;              // Process window events (real-time, process all available)
            while let Some(event_sample) = event_subscriber.receive().unwrap_or(None) {
                let event = *event_sample;
                _events_received += 1;
                had_activity = true;
                
                Self::handle_window_event(&event, windows, virtual_grid, monitors, auto_display, config);
            }
            
            // Process focus events (real-time, process all available)
            while let Some(focus_sample) = focus_subscriber.receive().unwrap_or(None) {
                let focus_event = *focus_sample;
                _focus_events_received += 1;
                had_activity = true;
                
                Self::handle_focus_event(&focus_event, focus_callback);
            }
            
            // Process window details updates (real-time, process all available)
            while let Some(details_sample) = window_details_subscriber.receive().unwrap_or(None) {
                let details = *details_sample;
                _details_received += 1;
                had_activity = true;
                
                Self::handle_window_details(&details, windows, virtual_grid, monitors, auto_display, config);
            }              // Process heartbeat messages to keep connection alive
            while let Some(heartbeat_sample) = heartbeat_subscriber.receive().unwrap_or(None) {
                let heartbeat = *heartbeat_sample;
                had_activity = true; // Reset disconnect counter on heartbeat
                
                // Check for shutdown heartbeat (iteration = 0)
                if heartbeat.server_iteration == 0 {
                    println!("💓 Received shutdown heartbeat from server - server is gracefully shutting down");
                    return MonitoringResult::ServerDisconnected;
                }
                // No need to log every normal heartbeat, just reset the timer
            }
            
            // Connection health monitoring
            if had_activity {
                consecutive_empty_cycles = 0; // Reset counter on activity
            } else {
                consecutive_empty_cycles += 1;
                if consecutive_empty_cycles >= max_empty_cycles {
                    println!("⚠️ No data received for {} cycles - server may have disconnected", consecutive_empty_cycles);
                    return MonitoringResult::ServerDisconnected;
                }
            }
            
            // Reduced status frequency - only during low activity periods
            static mut LAST_STATUS_TIME: std::time::Instant = unsafe { std::mem::zeroed() };
            static mut STATUS_INITIALIZED: bool = false;
            
            unsafe {
                if !STATUS_INITIALIZED {
                    LAST_STATUS_TIME = std::time::Instant::now();
                    STATUS_INITIALIZED = true;
                }
                
                if LAST_STATUS_TIME.elapsed().as_secs() > 30 && !had_activity { // Only during idle periods
                    let window_count = {
                        let windows_lock = windows.lock().unwrap();
                        windows_lock.len()
                    }; // Release lock immediately
                    
                    println!("\n🔥 ===== CLIENT STATUS (IDLE) =====");
                    println!("🔍 Monitoring: {} windows", window_count);
                    println!("🎯 Focus events: enabled");
                    println!("📡 Server connection: healthy");
                    LAST_STATUS_TIME = std::time::Instant::now();
                }
            }
            
            // Much shorter sleep for real-time responsiveness
            if had_activity {
                thread::sleep(Duration::from_millis(10)); // Very responsive during activity
            } else {
                thread::sleep(Duration::from_millis(50)); // Still responsive during idle
            }
        }
    }

    /// Get the current grid configuration
    pub fn get_config(&self) -> &GridConfig {
        &self.config
    }
    
    /// Start IPC services for client communication
    pub fn start_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Services are already started in new(), this is for compatibility
        println!("✅ IPC client services started");
        Ok(())
    }
      /// Assign window to virtual grid (alias for existing method)
    pub fn assign_window_to_virtual_grid(&mut self, hwnd: u64, row: u32, col: u32) -> GridClientResult<()> {
        self.assign_window_to_virtual_cell(hwnd, row, col)
    }
    
    /// Animate window with specified duration and easing
    pub fn animate_window(&mut self, hwnd: u64, duration_ms: u32, easing: crate::EasingType) -> Result<(), Box<dyn std::error::Error>> {
        use crate::ipc::WindowCommand;
        
        let easing_type = match easing {
            crate::EasingType::Linear => 0,
            crate::EasingType::EaseIn => 1,
            crate::EasingType::EaseOut => 2,
            crate::EasingType::EaseInOut => 3,
            crate::EasingType::Bounce => 4,
            crate::EasingType::Elastic => 5,
            crate::EasingType::Back => 6,
        };
        
        let command = WindowCommand {
            command_type: 9, // Animation command
            hwnd,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
            layout_id: 0,
            animation_duration_ms: duration_ms,
            easing_type,
        };
        
        self.command_publisher.send_copy(command)?;
        println!("🎬 Animation command sent for window {}", hwnd);
        Ok(())
    }
      /// Move window to a specific grid cell (actually moves the window)
    pub fn move_window_to_cell(&mut self, hwnd: u64, row: u32, col: u32) -> Result<(), Box<dyn std::error::Error>> {
        use crate::ipc::WindowCommand;
        
        let command = WindowCommand {
            command_type: 0, // MoveWindowToCell
            hwnd,
            target_row: row,
            target_col: col,
            monitor_id: 0,
            layout_id: 0,
            animation_duration_ms: 0,
            easing_type: 0,
        };
        
        self.send_command(command)?;
        Ok(())
    }    
    /// Register a callback to be called when window focus events occur
    /// This enables e_midi integration by allowing it to listen for focus changes
    pub fn set_focus_callback<F>(&mut self, callback: F) -> GridClientResult<()>
    where
        F: Fn(ipc::WindowFocusEvent) + Send + Sync + 'static,
    {
        let mut focus_callback_lock = safe_arc_lock(&self.focus_callback, "focus callback registration")?;
        *focus_callback_lock = Some(Box::new(callback));
        println!("🎯 Focus callback registered for e_midi integration");
        Ok(())
    }

    /// Remove the focus callback
    pub fn clear_focus_callback(&mut self) -> GridClientResult<()> {
        let mut focus_callback_lock = safe_arc_lock(&self.focus_callback, "focus callback clearing")?;
        *focus_callback_lock = None;
        println!("🎯 Focus callback cleared");
        Ok(())
    }

    /// Check if a focus callback is currently registered
    pub fn has_focus_callback(&self) -> bool {
        if let Ok(focus_callback_lock) = self.focus_callback.lock() {
            focus_callback_lock.is_some()
        } else {
            false        }
    }



    fn create_background_subscribers() -> Result<(Subscriber<Service, ipc::WindowEvent, ()>, Subscriber<Service, ipc::WindowDetails, ()>, Subscriber<Service, ipc::WindowFocusEvent, ()>, Subscriber<Service, ipc::HeartbeatMessage, ()>), Box<dyn std::error::Error>> {
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
        let window_details_subscriber = window_details_service.subscriber_builder().create()?;

        // Create focus events subscriber for e_midi integration
        let focus_service = node
            .service_builder(&ServiceName::new(ipc::GRID_FOCUS_EVENTS_SERVICE)?)
            .publish_subscribe::<ipc::WindowFocusEvent>()
            .open()?;
        let focus_subscriber = focus_service.subscriber_builder().create()?;

        // Create heartbeat subscriber for connection monitoring
        let heartbeat_service = node
            .service_builder(&ServiceName::new(ipc::GRID_HEARTBEAT_SERVICE)?)
            .publish_subscribe::<ipc::HeartbeatMessage>()
            .open()?;
        let heartbeat_subscriber = heartbeat_service.subscriber_builder().create()?;

        Ok((event_subscriber, window_details_subscriber, focus_subscriber, heartbeat_subscriber))
    }
    
    fn handle_window_event(
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
                }            }
        }
    }    /// Handle focus events for e_midi integration
    fn handle_focus_event(
        focus_event: &ipc::WindowFocusEvent,
        focus_callback: &Arc<Mutex<Option<Box<dyn Fn(ipc::WindowFocusEvent) + Send + Sync>>>>,
    ) {
        // Invoke the callback if one is registered (real-time, no logging)
        match safe_arc_lock(focus_callback, "focus event callback") {
            Ok(callback_lock) => {
                if let Some(ref callback) = *callback_lock {
                    callback(*focus_event);
                } else {
                    // Only log when no callback is registered (debugging)
                    let event_type = if focus_event.event_type == 0 { "FOCUSED" } else { "DEFOCUSED" };
                    println!("🎯 [FOCUS EVENT] {} window {} (no callback)", event_type, focus_event.hwnd);
                }
            }
            Err(e) => {
                println!("⚠️ Failed to acquire focus callback lock: {}", e);
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
    }    fn display_virtual_grid(
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        config: &GridConfig,
    ) {
        let window_count = match windows.try_lock() {
            Ok(windows_lock) => windows_lock.len(),
            Err(_) => 0,
        };
        
        // Convert client grid to server grid format for display
        if let Ok(grid) = virtual_grid.try_lock() {
            let server_grid: Vec<Vec<crate::CellState>> = grid.iter().map(|row| {
                row.iter().map(|cell| {
                    match cell {
                        ClientCellState::Empty => crate::CellState::Empty,
                        ClientCellState::Occupied(hwnd) => crate::CellState::Occupied(*hwnd as crate::HWND),
                        ClientCellState::OffScreen => crate::CellState::OffScreen,
                    }
                }).collect()
            }).collect();
              println!("\n🔥 REAL-TIME GRID UPDATE:");
            
            // Get virtual screen bounds for display
            let virtual_rect = Self::get_virtual_screen_rect();
            let bounds = ((virtual_rect.0, virtual_rect.1), (virtual_rect.2, virtual_rect.3));
            
            // Use the centralized display function for consistency with server
            crate::grid_display::display_grid(
                &server_grid,
                config,
                window_count,
                &crate::grid_display::GridDisplayConfig::default(),
                Some("Client Grid Viewer"),
                None,
                Some(bounds),
            );
        }
        
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
    }    fn display_complete_grid(
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
        
        // Convert client grid to server grid format for display
        if let Ok(grid) = virtual_grid.try_lock() {
            let server_grid: Vec<Vec<crate::CellState>> = grid.iter().map(|row| {
                row.iter().map(|cell| {
                    match cell {
                        ClientCellState::Empty => crate::CellState::Empty,
                        ClientCellState::Occupied(hwnd) => crate::CellState::Occupied(*hwnd as crate::HWND),
                        ClientCellState::OffScreen => crate::CellState::OffScreen,
                    }
                }).collect()
            }).collect();
              // Use the centralized display function for consistency with server
            crate::grid_display::display_grid(
                &server_grid,
                &self.config,
                window_count,
                &crate::grid_display::GridDisplayConfig::default(),
                None,
                Some((virtual_width, virtual_height)),
                Some(((virtual_rect.0, virtual_rect.1), (virtual_rect.2, virtual_rect.3))),
            );
        }
        
        // Display monitor grids using the centralized function like the server
        match monitors.try_lock() {
            Ok(monitors_lock) => {
                if !monitors_lock.is_empty() {
                    println!("\n🖥️ Monitor Grids:");
                    
                    for monitor in monitors_lock.iter() {
                        println!("  Monitor {}: {}x{}", monitor.monitor_id, monitor.width, monitor.height);
                        println!();
                        
                        // Convert monitor grid to server format
                        let mut server_monitor_grid = vec![vec![crate::CellState::Empty; self.config.cols]; self.config.rows];
                        for row in 0..self.config.rows {
                            for col in 0..self.config.cols {
                                if row < monitor.grid.len() && col < monitor.grid[row].len() {
                                    server_monitor_grid[row][col] = match monitor.grid[row][col] {
                                        Some(hwnd) => crate::CellState::Occupied(hwnd as crate::HWND),
                                        None => crate::CellState::Empty,
                                    };
                                }
                            }
                        }
                          // Use centralized display for monitor grids
                        let monitor_title = format!("Monitor {} Grid", monitor.monitor_id);
                        let monitor_bounds = ((monitor.x, monitor.y), (monitor.x + monitor.width, monitor.y + monitor.height));
                        crate::grid_display::display_grid(
                            &server_monitor_grid,
                            &self.config,
                            0, // Monitor grids don't track window count separately
                            &crate::grid_display::GridDisplayConfig::default(),
                            Some(&monitor_title),
                            Some((monitor.width, monitor.height)),
                            Some(monitor_bounds),
                        );
                    }
                }
            }
            Err(_) => {
                println!("⚠️ Monitor grids locked, skipping monitor grid display");
            }
        }
    }
    
    pub fn send_command(&mut self, command: ipc::WindowCommand) -> GridClientResult<()> {
        self.command_publisher.send_copy(command)
            .map(|_| ()) // Ignore the returned size, just return ()
            .map_err(|e| GridClientError::IpcError(format!("Failed to send command: {:?}", e)))
    }      
    
    pub fn request_window_list(&mut self) -> GridClientResult<()> {
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
    
      pub fn request_grid_state(&mut self) -> GridClientResult<()> {
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
    
    
    pub fn assign_window_to_virtual_cell(&mut self, hwnd: u64, row: u32, col: u32) -> GridClientResult<()> {
        // Validate coordinates
        validate_grid_coordinates(row, col, self.config.rows as u32, self.config.cols as u32)?;
        
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
            .map_err(|e| GridClientError::IpcError(format!("Failed to assign window to virtual cell: {}", e)))
    }
    
    pub fn assign_window_to_monitor_cell(&mut self, hwnd: u64, row: u32, col: u32, monitor_id: u32) -> GridClientResult<()> {
        // Validate coordinates
        validate_grid_coordinates(row, col, self.config.rows as u32, self.config.cols as u32)?;
        
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
            .map_err(|e| GridClientError::IpcError(format!("Failed to assign window to monitor cell: {}", e)))
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
            let result = GetWindowTextW(hwnd, buffer.as_mut_ptr(), length + 1);            if result > 0 {
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_client_errors::*;

    #[test]
    fn test_coordinate_validation_integration() {
        // Test that our validation function works with actual grid sizes
        let config = GridConfig::new(4, 6);
        
        // Valid coordinates
        assert!(validate_grid_coordinates(0, 0, config.rows as u32, config.cols as u32).is_ok());
        assert!(validate_grid_coordinates(3, 5, config.rows as u32, config.cols as u32).is_ok());
        
        // Invalid coordinates
        assert!(validate_grid_coordinates(4, 0, config.rows as u32, config.cols as u32).is_err());
        assert!(validate_grid_coordinates(0, 6, config.rows as u32, config.cols as u32).is_err());
    }

    #[test]
    fn test_monitor_grid_info_creation() {
        let monitor_info = MonitorGridInfo {
            monitor_id: 0,
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
            grid: vec![vec![None; 6]; 4],
        };
        
        assert_eq!(monitor_info.monitor_id, 0);
        assert_eq!(monitor_info.width, 1920);
        assert_eq!(monitor_info.height, 1080);
        assert_eq!(monitor_info.grid.len(), 4);
        assert_eq!(monitor_info.grid[0].len(), 6);
    }
}
