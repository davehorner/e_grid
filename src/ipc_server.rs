use crate::GridConfig;
use crate::{WindowTracker, ipc};
use iceoryx2::prelude::*;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::service::ipc::Service;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::ptr;
use winapi::shared::windef::HWND;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::{
    SetWinEventHook, UnhookWinEvent, GetSystemMetrics,
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND,
    WINEVENT_OUTOFCONTEXT, OBJID_WINDOW, CHILDID_SELF, SM_CXSCREEN, SM_CYSCREEN
};
use crate::ipc::GridResponse;
// Global state for the IPC server instance - needed for WinEvent callbacks
static mut GLOBAL_IPC_SERVER: Option<*mut GridIpcServer> = None;

/// Dedicated IPC Server for E-Grid system
/// Manages all server-side IPC communications including:
/// - Publishing window events (create, move, destroy)
/// - Publishing individual window details for real-time updates
/// - Processing client commands (window assignment, grid requests)
/// - Publishing responses to client requests
pub struct GridIpcServer {
    // Core window tracker
    tracker: Arc<Mutex<WindowTracker>>,
    config: GridConfig,    // IPC Publishers
    event_publisher: Option<Publisher<Service, ipc::WindowEvent, ()>>,
    response_publisher: Option<Publisher<Service, ipc::WindowResponse, ()>>,
    window_details_publisher: Option<Publisher<Service, ipc::WindowDetails, ()>>,
    focus_publisher: Option<Publisher<Service, ipc::WindowFocusEvent, ()>>,  // NEW: Focus events
    layout_publisher: Option<Publisher<Service, ipc::GridLayoutMessage, ()>>,
    cell_assignment_publisher: Option<Publisher<Service, ipc::GridCellAssignment, ()>>,
    animation_status_publisher: Option<Publisher<Service, ipc::AnimationStatus, ()>>,
    
    // IPC Subscribers
    command_subscriber: Option<Subscriber<Service, ipc::WindowCommand, ()>>,
    layout_subscriber: Option<Subscriber<Service, ipc::GridLayoutMessage, ()>>,
    cell_assignment_subscriber: Option<Subscriber<Service, ipc::GridCellAssignment, ()>>,
    animation_subscriber: Option<Subscriber<Service, ipc::AnimationCommand, ()>>,
    
    // Server state
    is_running: bool,
    event_listeners: Vec<Box<dyn Fn(&ipc::GridEvent) + Send + Sync>>,
      // WinEvent hooks
    event_hooks: Vec<winapi::shared::windef::HWINEVENTHOOK>,
    
    // Focus tracking (NEW: for DEFOCUSED events)
    last_focused_window: Option<HWND>,
}

impl GridIpcServer {
    /// Create a new IPC server instance
    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Result<Self, Box<dyn std::error::Error>> {    
                // Get the config from the tracker once during initialization
        let config = {
            let tracker_guard = tracker.lock().unwrap();
            tracker_guard.config.clone()
        };
            Ok(Self {
            tracker,
            config,            event_publisher: None,
            response_publisher: None,
            window_details_publisher: None,
            focus_publisher: None,  // NEW: Focus events
            layout_publisher: None,
            cell_assignment_publisher: None,
            animation_status_publisher: None,
            command_subscriber: None,
            layout_subscriber: None,
            cell_assignment_subscriber: None,
            animation_subscriber: None,            is_running: false,
            event_listeners: Vec::new(),
            event_hooks: Vec::new(),
            last_focused_window: None,  // NEW: Initialize focus tracking
        })
    }

    /// Initialize all IPC services
    pub fn setup_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Setting up E-Grid IPC server services...");
        
        let node = NodeBuilder::new().create::<Service>()?;

        // Setup event publishing service
        let event_service = node
            .service_builder(&ServiceName::new(ipc::GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<ipc::WindowEvent>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.event_publisher = Some(event_service.publisher_builder().create()?);

        // Setup response publishing service
        let response_service = node
            .service_builder(&ServiceName::new(ipc::GRID_RESPONSE_SERVICE)?)
            .publish_subscribe::<ipc::WindowResponse>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.response_publisher = Some(response_service.publisher_builder().create()?);        // Setup window details publishing service
        let window_details_service = node
            .service_builder(&ServiceName::new(ipc::GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<ipc::WindowDetails>()
            .max_publishers(8)
            .max_subscribers(8)
            .history_size(32)  // Keep more messages in history
            .subscriber_max_buffer_size(64)  // Larger buffer for subscribers
            .open_or_create()?;        self.window_details_publisher = Some(window_details_service.publisher_builder().create()?);

        // Setup focus events publishing service (NEW: for e_midi integration)
        let focus_service = node
            .service_builder(&ServiceName::new(ipc::GRID_FOCUS_EVENTS_SERVICE)?)
            .publish_subscribe::<ipc::WindowFocusEvent>()
            .max_publishers(8)
            .max_subscribers(8)
            .history_size(16)  // Keep recent focus events in history
            .subscriber_max_buffer_size(32)  // Buffer for focus event subscribers
            .open_or_create()?;
        self.focus_publisher = Some(focus_service.publisher_builder().create()?);

        // Setup command subscription service
        let command_service = node
            .service_builder(&ServiceName::new(ipc::GRID_COMMANDS_SERVICE)?)
            .publish_subscribe::<ipc::WindowCommand>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;        self.command_subscriber = Some(command_service.subscriber_builder().create()?);

        // Setup grid layout services
        let layout_service = node
            .service_builder(&ServiceName::new(ipc::GRID_LAYOUT_SERVICE)?)
            .publish_subscribe::<ipc::GridLayoutMessage>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        
        self.layout_publisher = Some(layout_service.publisher_builder().create()?);
        self.layout_subscriber = Some(layout_service.subscriber_builder().create()?);
        
        // Setup cell assignment services
        let cell_assignment_service = node
            .service_builder(&ServiceName::new(ipc::GRID_CELL_ASSIGNMENTS_SERVICE)?)
            .publish_subscribe::<ipc::GridCellAssignment>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        
        self.cell_assignment_publisher = Some(cell_assignment_service.publisher_builder().create()?);
        self.cell_assignment_subscriber = Some(cell_assignment_service.subscriber_builder().create()?);

        // Setup animation services
        let animation_service = node
            .service_builder(&ServiceName::new(ipc::ANIMATION_COMMANDS_SERVICE)?)
            .publish_subscribe::<ipc::AnimationCommand>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        
        self.animation_subscriber = Some(animation_service.subscriber_builder().create()?);
        
        // Setup animation status service
        let animation_status_service = node
            .service_builder(&ServiceName::new(ipc::ANIMATION_STATUS_SERVICE)?)
            .publish_subscribe::<ipc::AnimationStatus>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        
        self.animation_status_publisher = Some(animation_status_service.publisher_builder().create()?);        println!("✅ E-Grid IPC server services initialized successfully");
        println!("   📡 Event service: {}", ipc::GRID_EVENTS_SERVICE);
        println!("   📨 Command service: {}", ipc::GRID_COMMANDS_SERVICE);
        println!("   📤 Response service: {}", ipc::GRID_RESPONSE_SERVICE);
        println!("   📋 Window details service: {}", ipc::GRID_WINDOW_DETAILS_SERVICE);
        println!("   🎯 Focus events service: {}", ipc::GRID_FOCUS_EVENTS_SERVICE);  // NEW
        println!("   🗂️  Grid layout service: {}", ipc::GRID_LAYOUT_SERVICE);
        println!("   📍 Cell assignment service: {}", ipc::GRID_CELL_ASSIGNMENTS_SERVICE);
        println!("   🎬 Animation service: {}", ipc::ANIMATION_COMMANDS_SERVICE);
        println!("   📊 Animation status service: {}", ipc::ANIMATION_STATUS_SERVICE);

        self.is_running = true;
        Ok(())
    }

    /// Start the server event loop in the current thread
    pub fn run_event_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Starting E-Grid IPC server event loop...");
        
        while self.is_running {
            // Process incoming commands from clients
            self.process_commands()?;
            
            // Small delay to prevent busy waiting
            thread::sleep(Duration::from_millis(10));
        }
        
        println!("🛑 E-Grid IPC server event loop stopped");
        Ok(())
    }    /// Start the server event loop in a background thread
    /// Note: This is a simplified version that doesn't use actual background threading
    /// due to HWND thread safety constraints
    pub fn start_background_event_loop(&self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.is_running {
            return Err("Server services not initialized. Call setup_services() first.".into());
        }

        println!("🔄 E-Grid IPC server background monitoring enabled");
        println!("📨 Server will process commands in the main event loop");
        
        Ok(())
    }

    /// Process incoming commands from clients
    pub fn process_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut commands_to_process: Vec<ipc::WindowCommand> = Vec::new();
        
        // Collect all incoming commands
        if let Some(ref mut subscriber) = self.command_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let command = *sample;
                commands_to_process.push(command);
            }
        }

        // Process each command
        for command in commands_to_process {
            println!("📨 Received command: {:?}", command);
            
            // Convert to high-level command and process
            let grid_command = Self::window_command_to_grid_command(&command);
            let response = self.handle_command(grid_command)?;
            
            // Send response
            self.send_response(response)?;
        }

        Ok(())
    }

    /// Handle a grid command and return a response
    pub fn handle_command(&mut self, command: ipc::GridCommand) -> Result<ipc::GridResponse, Box<dyn std::error::Error>> {
        match command {            ipc::GridCommand::GetGridConfig => {
                self.send_response(GridResponse::GridConfig(self.config.clone()))?;
                Ok(GridResponse::GridConfig(self.config.clone()))
            }
            ipc::GridCommand::GetGridState => {
                if let Ok(tracker) = self.tracker.lock() {
                    let total_windows = tracker.windows.len();
                    let occupied_cells = self.count_occupied_cells(&tracker);
                    
                    let mut grid_summary = format!("Grid: {} windows, {} occupied cells\n", total_windows, occupied_cells);
                    grid_summary.push_str("Windows:\n");
                      for entry in tracker.windows.iter().take(10) {
                        let (hwnd, window) = entry.pair();
                        let title = if window.title.len() > 30 {
                            format!("{}...", &window.title[..30])
                        } else {
                            window.title.clone()
                        };
                        grid_summary.push_str(&format!("  HWND {:?}: {}\n", hwnd, title));
                    }
                    
                    if tracker.windows.len() > 10 {
                        grid_summary.push_str(&format!("  ... and {} more windows\n", tracker.windows.len() - 10));
                    }
                    
                    println!("📊 Grid state: {} windows, {} occupied cells", total_windows, occupied_cells);
                    
                    Ok(ipc::GridResponse::GridState {
                        total_windows,
                        occupied_cells,
                        grid_summary,
                    })
                } else {
                    Ok(ipc::GridResponse::Error("Failed to access window tracker".to_string()))
                }
            }

            ipc::GridCommand::GetWindowList => {                let hwnd_list = if let Ok(tracker) = self.tracker.lock() {
                    tracker.windows.iter()
                        .map(|entry| *entry.key() as u64)
                        .collect::<Vec<u64>>()
                } else {
                    return Ok(ipc::GridResponse::Error("Failed to access window tracker".to_string()));
                };
                
                println!("📋 GetWindowList request - publishing details for {} windows", hwnd_list.len());
                
                // Publish individual window details for all windows
                if let Err(e) = self.publish_all_window_details() {
                    println!("⚠️ Failed to publish window details: {}", e);
                }
                
                Ok(ipc::GridResponse::Success)
            }

            ipc::GridCommand::MoveWindowToCell { hwnd, target_row, target_col } => {
                println!("🎯 Request to move window {} to cell ({}, {})", hwnd, target_row, target_col);
                match self.move_window_to_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(ipc::GridResponse::Success),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to move window: {}", e))),
                }
            }

            ipc::GridCommand::AssignWindowToVirtualCell { hwnd, target_row, target_col } => {
                println!("🏷️ Request to assign window {} to virtual grid cell ({}, {})", hwnd, target_row, target_col);
                match self.assign_window_to_virtual_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(ipc::GridResponse::Success),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to assign window to virtual cell: {}", e))),
                }
            }            ipc::GridCommand::AssignWindowToMonitorCell { hwnd, target_row, target_col, monitor_id } => {
                println!("🖥️ Request to assign window {} to monitor {} cell ({}, {})", hwnd, monitor_id, target_row, target_col);
                match self.assign_window_to_monitor_cell(hwnd, target_row, target_col, monitor_id) {
                    Ok(_) => Ok(ipc::GridResponse::Success),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to assign window to monitor cell: {}", e))),
                }
            }            ipc::GridCommand::ApplyGridLayout { layout_name, duration_ms, easing_type } => {
                println!("🎨 Request to apply saved layout '{}'", layout_name);
                match self.apply_saved_layout(&layout_name, duration_ms, easing_type) {
                    Ok(count) => Ok(ipc::GridResponse::Success),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to apply grid layout: {}", e))),
                }
            }

            ipc::GridCommand::SaveCurrentLayout { layout_name } => {
                println!("💾 Request to save current layout as '{}'", layout_name);
                match self.save_current_layout(layout_name) {
                    Ok(_) => Ok(ipc::GridResponse::Success),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to save layout: {}", e))),
                }
            }            ipc::GridCommand::GetSavedLayouts => {
                println!("📂 Request to get saved layouts");
                match self.get_saved_layouts() {
                    Ok(layout_names) => Ok(ipc::GridResponse::SavedLayouts { layout_names }),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to get saved layouts: {}", e))),
                }
            }ipc::GridCommand::StartAnimation { hwnd, target_x, target_y, target_width, target_height, duration_ms, easing_type } => {
                println!("🎬 Request to start animation for window {}", hwnd);
                let target_rect = winapi::shared::windef::RECT {
                    left: target_x,
                    top: target_y,
                    right: target_x + target_width as i32,
                    bottom: target_y + target_height as i32,
                };
                match self.start_window_animation(hwnd, target_rect, duration_ms, easing_type) {
                    Ok(_) => Ok(ipc::GridResponse::Success),
                    Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to start animation: {}", e))),
                }
            }            ipc::GridCommand::GetAnimationStatus { hwnd } => {
                println!("📊 Request to get animation status for window {}", hwnd);
                if hwnd == 0 {
                    // Get status for all windows
                    if let Ok(tracker) = self.tracker.lock() {
                        let mut statuses = Vec::new();                        for entry in &tracker.active_animations {
                            let (window_hwnd, animation) = entry.pair();
                            let progress = animation.get_progress();
                            statuses.push((*window_hwnd as u64, true, progress));
                        }
                        Ok(ipc::GridResponse::AnimationStatus { statuses })
                    } else {
                        Ok(ipc::GridResponse::Error("Failed to get animation status".to_string()))
                    }
                } else {
                    // Get status for specific window
                    match self.get_animation_status(hwnd) {
                        Ok(status) => {
                            let statuses = if let Some(animation) = status {
                                vec![(hwnd, true, animation.get_progress())]
                            } else {
                                vec![(hwnd, false, 0.0)]
                            };
                            Ok(ipc::GridResponse::AnimationStatus { statuses })
                        },
                        Err(e) => Ok(ipc::GridResponse::Error(format!("Failed to get animation status: {}", e))),
                    }
                }
            }
        }
    }

    /// Publish a window event to all connected clients
    pub fn publish_event(&mut self, event: ipc::GridEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Convert high-level event to zero-copy format
        let window_event = self.grid_event_to_window_event(&event);
        
        // Publish via iceoryx2
        if let Some(ref mut publisher) = self.event_publisher {
            publisher.send_copy(window_event)?;
            println!("📡 Published event: {:?}", event);
        }

        // Notify local listeners
        for listener in &self.event_listeners {
            listener(&event);
        }

        Ok(())
    }

    /// Publish details for a specific window
    pub fn publish_window_details(&mut self, hwnd: HWND) -> Result<(), Box<dyn std::error::Error>> {
        // Use try_lock to avoid blocking if the tracker is locked elsewhere.
        if let Ok(tracker) = self.tracker.try_lock() {
            if let Some(window_info) = tracker.windows.get(&hwnd) {
                // Create the details first (immutable borrow)
                let details = self.window_info_to_details(hwnd, &*window_info);
                
                // Then publish (mutable borrow)
                if let Some(ref mut publisher) = self.window_details_publisher {
                    publisher.send_copy(details)?;
                    println!("📤 Published window details for HWND {:?}", hwnd);
                }
            } else {
                println!("⚠️ No WindowInfo found for HWND {:?}", hwnd);
            }
        } else {            println!("⚠️ Could not acquire tracker lock for HWND {:?}", hwnd);
        }
        Ok(())
    }

    /// Publish details for all current windows
    pub fn publish_all_window_details(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Get a snapshot of windows to avoid holding the lock during publishing
        let windows_snapshot = if let Ok(tracker) = self.tracker.lock() {
            println!("📤 Publishing details for {} windows (already filtered by is_manageable_window)...", tracker.windows.len());
            tracker.windows.clone()
        } else {
            println!("❌ Failed to lock window tracker");
            return Err("Failed to lock window tracker".into());
        };
        
        let total_window_count = windows_snapshot.len();
        let mut published_count = 0;
        let mut failed_count = 0;          for entry in &windows_snapshot {
            let (hwnd, window_info) = entry.pair();
            // No additional filtering - windows in tracker are already pre-filtered by is_manageable_window
            // This ensures client and server see the same set of windows
            
            // Create details without holding tracker lock to avoid deadlock
            let details = self.window_info_to_details(*hwnd, &*window_info);
            
            // Publish the details
            if let Some(ref mut publisher) = self.window_details_publisher {
                match publisher.send_copy(details) {
                    Ok(_) => {
                        published_count += 1;                        // Print all published windows to verify they're all being sent
                        println!("   ✅ Published window {} (#{}/{}): '{}'", 
                            *hwnd as u64, published_count, total_window_count, 
                            window_info.title.chars().take(40).collect::<String>());
                        
                        // Small delay to prevent overwhelming the IPC system
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(e) => {
                        println!("   ❌ Failed to publish window {}: {}", *hwnd as u64, e);
                        failed_count += 1;
                        // Continue with other windows instead of failing completely
                    }
                }
            } else {
                println!("⚠️ Window details publisher not available");
                return Err("Window details publisher not available".into());
            }
        }
          println!("✅ Successfully published details for {}/{} windows (failed: {})", 
            published_count, total_window_count, failed_count);
        Ok(())
    }
      /// Publish focus event for window focus tracking (NEW: for e_midi integration)
    pub fn publish_focus_event(&mut self, hwnd: HWND, event_type: u8) {
        // Get window information for the focus event
        let process_id = unsafe {
            let mut process_id: u32 = 0;
            winapi::um::winuser::GetWindowThreadProcessId(hwnd, &mut process_id);
            process_id
        };
        
        // Get window title for hashing
        let window_title = unsafe {
            let mut buffer: [u16; 256] = [0; 256];
            let len = winapi::um::winuser::GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            if len > 0 {
                String::from_utf16_lossy(&buffer[..len as usize])
            } else {
                "(No Title)".to_string()
            }
        };
        
        // Calculate hashes before borrowing publisher
        let app_name_hash = self.hash_string(&format!("Process_{}", process_id));
        let window_title_hash = self.hash_string(&window_title);
        
        if let Some(ref mut publisher) = self.focus_publisher {
            // Create focus event
            let focus_event = ipc::WindowFocusEvent {
                event_type,
                hwnd: hwnd as u64,
                process_id,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                app_name_hash,
                window_title_hash,
                reserved: [0; 2],
            };
            
            // Publish the focus event
            if let Err(e) = publisher.send_copy(focus_event) {
                println!("❌ Failed to publish focus event: {:?}", e);
            } else {
                let event_name = if event_type == 0 { "FOCUSED" } else { "DEFOCUSED" };
                println!("🎯 Published {} event: HWND {} (PID: {}) Title: '{}'", 
                         event_name, hwnd as u64, process_id, 
                         if window_title.len() > 30 { &window_title[..30] } else { &window_title });
            }
        } else {
            println!("⚠️ Focus publisher not available");
        }
    }
    
    /// Simple hash function for strings  
    fn hash_string(&self, s: &str) -> u64 {
        let mut hash = 0u64;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }/// Create window details without holding the tracker lock to avoid deadlocks
    fn create_window_details_safe(&self, hwnd: HWND, window_info: &crate::WindowInfo) -> ipc::WindowDetails {
        let rect = &window_info.rect;
        
        // Get screen dimensions for proper grid calculation
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        // Calculate proper virtual grid position based on actual screen dimensions
        let cell_width = screen_width / self.config.cols as i32;
        let cell_height = screen_height / self.config.rows as i32;
        
        let virtual_row = if cell_height > 0 && rect.top >= 0 {
            ((rect.top / cell_height).max(0).min(self.config.rows as i32 - 1)) as u32
        } else {
            0
        };
        
        let virtual_col = if cell_width > 0 && rect.left >= 0 {
            ((rect.left / cell_width).max(0).min(self.config.cols as i32 - 1)) as u32
        } else {
            0
        };
        
        // Calculate end positions based on window size
        let virtual_row_end = if cell_height > 0 && rect.bottom > rect.top {
            ((rect.bottom / cell_height).max(virtual_row as i32).min(self.config.rows as i32)) as u32
        } else {
            virtual_row + 1
        };
        
        let virtual_col_end = if cell_width > 0 && rect.right > rect.left {
            ((rect.right / cell_width).max(virtual_col as i32).min(self.config.cols as i32)) as u32
        } else {
            virtual_col + 1
        };
        
        ipc::WindowDetails {
            hwnd: hwnd as u64,
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            
            // Virtual grid positions - proper calculation based on screen dimensions
            virtual_row_start: virtual_row,
            virtual_col_start: virtual_col,
            virtual_row_end: virtual_row_end,
            virtual_col_end: virtual_col_end,
            
            // Monitor positions - use same as virtual for now (could be improved later)
            monitor_id: 0,
            monitor_row_start: virtual_row,
            monitor_col_start: virtual_col,
            monitor_row_end: virtual_row_end,
            monitor_col_end: virtual_col_end,
            
            // Title length for validation
            title_len: window_info.title.len().min(255) as u32,
        }
    }

    /// Add an event listener for local event handling
    pub fn add_event_listener<F>(&mut self, listener: F)
    where
        F: Fn(&ipc::GridEvent) + Send + Sync + 'static,
    {
        self.event_listeners.push(Box::new(listener));
    }

    /// Stop the server
    pub fn stop(&mut self) {
        self.is_running = false;
        println!("🛑 E-Grid IPC server stopped");
    }

    /// Get the current grid configuration
    pub fn get_config(&self) -> &GridConfig {
        &self.config
    }

    // Convenience methods for publishing specific events
    pub fn publish_window_created(&mut self, hwnd: u64, title: String, row: usize, col: usize) -> Result<(), Box<dyn std::error::Error>> {
        let event = ipc::GridEvent::WindowCreated { hwnd, title, row, col };
        self.publish_event(event)
    }

    pub fn publish_window_destroyed(&mut self, hwnd: u64, title: String) -> Result<(), Box<dyn std::error::Error>> {
        let event = ipc::GridEvent::WindowDestroyed { hwnd, title };
        self.publish_event(event)
    }

    pub fn publish_window_moved(&mut self, hwnd: u64, title: String, old_row: usize, old_col: usize, new_row: usize, new_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        let event = ipc::GridEvent::WindowMoved { hwnd, title, old_row, old_col, new_row, new_col };
        self.publish_event(event)
    }

    pub fn publish_grid_state_changed(&mut self, total_windows: usize, occupied_cells: usize) -> Result<(), Box<dyn std::error::Error>> {
        let event = ipc::GridEvent::GridStateChanged {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            total_windows,
            occupied_cells,
        };
        self.publish_event(event)
    }    // Private helper methods
    fn send_response(&mut self, response: ipc::GridResponse) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut publisher) = self.response_publisher {
            let window_response = Self::grid_response_to_window_response(&response);
            publisher.send_copy(window_response)?;
            println!("📤 Sent response: {:?}", response);
        }
        Ok(())
    }    fn count_occupied_cells(&self, tracker: &WindowTracker) -> usize {
        let mut occupied = std::collections::HashSet::new();
        for entry in &tracker.windows {
            let (_, window) = entry.pair();
            for &(row, col) in &window.grid_cells {
                occupied.insert((row, col));
            }
        }
        occupied.len()
    }/// Move a window to a specific grid cell
    pub fn move_window_to_cell(&mut self, hwnd: u64, target_row: usize, target_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 IPC Server: Converting hwnd {} to HWND pointer", hwnd);
        let hwnd_ptr = hwnd as winapi::shared::windef::HWND;
        println!("🔧 IPC Server: HWND pointer = {:?}", hwnd_ptr);
        
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.move_window_to_cell(hwnd_ptr, target_row, target_col)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Assign a window to a virtual grid cell
    pub fn assign_window_to_virtual_cell(&mut self, hwnd: u64, target_row: usize, target_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.assign_window_to_virtual_cell(hwnd as winapi::shared::windef::HWND, target_row, target_col)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Assign a window to a monitor-specific grid cell
    pub fn assign_window_to_monitor_cell(&mut self, hwnd: u64, target_row: usize, target_col: usize, monitor_id: usize) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.assign_window_to_monitor_cell(hwnd as winapi::shared::windef::HWND, target_row, target_col, monitor_id)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Apply a saved layout by name
    pub fn apply_saved_layout(&mut self, layout_name: &str, duration_ms: u32, easing_type: crate::EasingType) -> Result<usize, Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            if let Some(layout) = tracker.get_saved_layout(layout_name) {
                let duration = std::time::Duration::from_millis(duration_ms as u64);
                tracker.apply_grid_layout(&layout, duration, easing_type)
                    .map_err(|e| e.into())
            } else {
                Err(format!("Layout '{}' not found", layout_name).into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Save the current layout with a given name
    pub fn save_current_layout(&mut self, layout_name: String) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.save_current_layout(layout_name);
            Ok(())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Get all saved layout names
    pub fn get_saved_layouts(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {        if let Ok(tracker) = self.tracker.lock() {
            Ok(tracker.list_saved_layouts())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Start animation for a specific window
    pub fn start_window_animation(&mut self, hwnd: u64, target_rect: winapi::shared::windef::RECT, duration_ms: u32, easing_type: crate::EasingType) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            let duration = std::time::Duration::from_millis(duration_ms as u64);
            tracker.start_window_animation(hwnd as winapi::shared::windef::HWND, target_rect, duration, easing_type)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Stop animation for a specific window
    pub fn stop_window_animation(&mut self, hwnd: u64) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.active_animations.remove(&(hwnd as winapi::shared::windef::HWND));
            Ok(())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Get animation status for a specific window
    pub fn get_animation_status(&self, hwnd: u64) -> Result<Option<crate::WindowAnimation>, Box<dyn std::error::Error>> {        if let Ok(tracker) = self.tracker.lock() {
            Ok(tracker.active_animations.get(&(hwnd as winapi::shared::windef::HWND)).map(|anim_ref| anim_ref.clone()))
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    // Conversion helper methods
    fn grid_event_to_window_event(&self, event: &ipc::GridEvent) -> ipc::WindowEvent {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match event {
            ipc::GridEvent::WindowCreated { hwnd, row, col, .. } => ipc::WindowEvent {
                event_type: 0,
                hwnd: *hwnd,
                row: *row as u32,
                col: *col as u32,
                timestamp,
                ..Default::default()
            },
            ipc::GridEvent::WindowDestroyed { hwnd, .. } => ipc::WindowEvent {
                event_type: 1,
                hwnd: *hwnd,
                timestamp,
                ..Default::default()
            },
            ipc::GridEvent::WindowMoved { hwnd, old_row, old_col, new_row, new_col, .. } => ipc::WindowEvent {
                event_type: 2,
                hwnd: *hwnd,
                old_row: *old_row as u32,
                old_col: *old_col as u32,
                row: *new_row as u32,
                col: *new_col as u32,
                timestamp,
                ..Default::default()
            },
            ipc::GridEvent::GridStateChanged { timestamp, total_windows, occupied_cells } => ipc::WindowEvent {
                event_type: 3,
                timestamp: *timestamp,
                total_windows: *total_windows as u32,
                occupied_cells: *occupied_cells as u32,
                ..Default::default()
            },
        }
    }

    fn window_command_to_grid_command(command: &ipc::WindowCommand) -> ipc::GridCommand {
        match command.command_type {
            0 => ipc::GridCommand::MoveWindowToCell {
                hwnd: command.hwnd,
                target_row: command.target_row as usize,
                target_col: command.target_col as usize,
            },
            1 => ipc::GridCommand::GetGridState,
            2 => ipc::GridCommand::GetWindowList,
            5 => ipc::GridCommand::AssignWindowToVirtualCell {
                hwnd: command.hwnd,
                target_row: command.target_row as usize,
                target_col: command.target_col as usize,
            },
            6 => ipc::GridCommand::AssignWindowToMonitorCell {
                hwnd: command.hwnd,
                target_row: command.target_row as usize,
                target_col: command.target_col as usize,
                monitor_id: command.monitor_id as usize,
            },
            _ => ipc::GridCommand::GetGridState, // Default fallback
        }
    }    fn grid_response_to_window_response(response: &ipc::GridResponse) -> ipc::WindowResponse {
        match response {
            ipc::GridResponse::Success => ipc::WindowResponse {
                response_type: 0,
                error_code: 0,
                ..Default::default()
            },
            ipc::GridResponse::Error(_) => ipc::WindowResponse {
                response_type: 1,
                error_code: 1,
                ..Default::default()
            },
            ipc::GridResponse::WindowList { windows } => ipc::WindowResponse {
                response_type: 2,
                window_count: windows.len() as u32,
                ..Default::default()
            },
            ipc::GridResponse::GridState { total_windows, occupied_cells, .. } => ipc::WindowResponse {
                response_type: 3,
                error_code: 0,
                window_count: *total_windows as u32,
                data: [*occupied_cells as u64, 0, 0, 0],
            },            ipc::GridResponse::SavedLayouts { layout_names } => ipc::WindowResponse {
                response_type: 4,
                error_code: 0,
                window_count: layout_names.len() as u32,
                ..Default::default()
            },            ipc::GridResponse::AnimationStatus { statuses } => ipc::WindowResponse {
                response_type: 5,
                error_code: 0,
                window_count: statuses.len() as u32,
                data: if let Some((hwnd, is_active, progress)) = statuses.first() {
                    [*hwnd, if *is_active { 1 } else { 0 }, (*progress * 1000.0) as u64, 0]
                } else {
                    [0, 0, 0, 0]
                },
                ..Default::default()
            },
            ipc::GridResponse::GridConfig(config) => ipc::WindowResponse {
                response_type: 6, // Grid config response type
                error_code: 0,
                window_count: 0, // Not used for config
                data: [config.rows as u64, config.cols as u64, 0, 0],
                ..Default::default()
            },
        }
    }

    fn window_info_to_details(&self, hwnd: HWND, window_info: &crate::WindowInfo) -> ipc::WindowDetails {
        let rect = &window_info.rect;
        
        // Get virtual grid positions
        let (virtual_start_row, virtual_start_col, virtual_end_row, virtual_end_col) = 
            if let Ok(tracker) = self.tracker.lock() {
                let cells = tracker.window_to_grid_cells(rect);
                if cells.is_empty() {
                    (0, 0, 0, 0)
                } else {
                    let min_row = cells.iter().map(|(r, _)| *r).min().unwrap_or(0) as u32;
                    let max_row = cells.iter().map(|(r, _)| *r).max().unwrap_or(0) as u32;
                    let min_col = cells.iter().map(|(_, c)| *c).min().unwrap_or(0) as u32;
                    let max_col = cells.iter().map(|(_, c)| *c).max().unwrap_or(0) as u32;
                    (min_row, min_col, max_row, max_col)
                }
            } else {
                (0, 0, 0, 0)
            };

        // Find which monitor this window is primarily on and get monitor grid positions
        let (monitor_id, monitor_start_row, monitor_start_col, monitor_end_row, monitor_end_col) = 
            if let Ok(tracker) = self.tracker.lock() {
                // Find the monitor that contains the center of the window
                let center_x = rect.left + (rect.right - rect.left) / 2;
                let center_y = rect.top + (rect.bottom - rect.top) / 2;
                
                let mut found_monitor = None;
                
                for (i, monitor) in tracker.monitor_grids.iter().enumerate() {
                    let (left, top, right, bottom) = monitor.monitor_rect;
                    if center_x >= left && center_x < right && center_y >= top && center_y < bottom {
                        // Get grid positions within this monitor
                        let cells = monitor.window_to_grid_cells(rect);
                        if cells.is_empty() {
                            found_monitor = Some((i as u32, 0, 0, 0, 0));
                        } else {
                            let min_row = cells.iter().map(|(r, _)| *r).min().unwrap_or(0) as u32;
                            let max_row = cells.iter().map(|(r, _)| *r).max().unwrap_or(0) as u32;
                            let min_col = cells.iter().map(|(_, c)| *c).min().unwrap_or(0) as u32;
                            let max_col = cells.iter().map(|(_, c)| *c).max().unwrap_or(0) as u32;
                            found_monitor = Some((i as u32, min_row, min_col, max_row, max_col));
                        }
                        break;
                    }
                }
                
                found_monitor.unwrap_or((0, 0, 0, 0, 0))
            } else {
                (0, 0, 0, 0, 0)
            };

        ipc::WindowDetails {
            hwnd: hwnd as u64,
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            virtual_row_start: virtual_start_row,
            virtual_col_start: virtual_start_col,
            virtual_row_end: virtual_end_row,
            virtual_col_end: virtual_end_col,
            monitor_id,
            monitor_row_start: monitor_start_row,
            monitor_col_start: monitor_start_col,
            monitor_row_end: monitor_end_row,
            monitor_col_end: monitor_end_col,
            title_len: window_info.title.len().min(255) as u32,
        }
    }
}

// WinEvent hook procedure for the IPC server
unsafe extern "system" fn server_win_event_proc(
    _h_winevent_hook: winapi::shared::windef::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _dw_event_thread: u32,
    _dw_ms_event_time: u32,
) {
    // Only process window-level events
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF || hwnd.is_null() {
        return;
    }

    // Get the global server instance
    if let Some(server_ptr) = GLOBAL_IPC_SERVER {
        let server = &mut *server_ptr;
        server.handle_window_event(event, hwnd);
    }
}

impl GridIpcServer {    /// Handle a window event from WinEvent hook - MINIMAL PROCESSING ONLY
    fn handle_window_event(&mut self, event: u32, hwnd: HWND) {
        // DO NOTHING EXPENSIVE IN WINEVENT CALLBACK
        // Just print a simple message - all actual processing happens in main loop
        
        let event_name = match event {
            EVENT_OBJECT_CREATE => "CREATED",
            EVENT_OBJECT_DESTROY => "DESTROYED", 
            EVENT_OBJECT_LOCATIONCHANGE => "MOVED/RESIZED",
            EVENT_SYSTEM_FOREGROUND => "FOCUSED",  // Changed from ACTIVATED
            EVENT_SYSTEM_MINIMIZESTART => "MINIMIZED",
            EVENT_SYSTEM_MINIMIZEEND => "RESTORED",
            _ => "UNKNOWN",
        };
        
        // Only do minimal logging - no lock acquisition, no publishing, no window operations
        println!("🔔 WinEvent: {} - HWND: {}", event_name, hwnd as u64);
          // NEW: For focus events, we can do minimal publishing since it's lightweight
        if event == EVENT_SYSTEM_FOREGROUND {
            // Send DEFOCUSED event for previous window if it exists
            if let Some(prev_hwnd) = self.last_focused_window {
                if prev_hwnd != hwnd && !prev_hwnd.is_null() {
                    self.publish_focus_event(prev_hwnd, 1);  // 1 = DEFOCUSED
                }
            }
            
            // Update the last focused window
            self.last_focused_window = Some(hwnd);
            
            // Send FOCUSED event for current window
            self.publish_focus_event(hwnd, 0);  // 0 = FOCUSED
        }
        
        // That's it! All actual processing happens in the main loop periodic updates
    }

    /// Setup WinEvent hooks for real-time window monitoring
    pub fn setup_window_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            // Set the global server pointer for the callback
            GLOBAL_IPC_SERVER = Some(self as *mut GridIpcServer);
            
            println!("🔧 Setting up WinEvent hooks for real-time monitoring...");
            
            let events_to_hook = [
                (EVENT_OBJECT_CREATE, "Window Creation"),
                (EVENT_OBJECT_DESTROY, "Window Destruction"), 
                (EVENT_OBJECT_LOCATIONCHANGE, "Window Move/Resize"),
                (EVENT_SYSTEM_FOREGROUND, "Window Activation"),
                (EVENT_SYSTEM_MINIMIZESTART, "Window Minimize"),
                (EVENT_SYSTEM_MINIMIZEEND, "Window Restore"),
            ];
            
            for (event, description) in &events_to_hook {
                let hook = SetWinEventHook(
                    *event,
                    *event,
                    ptr::null_mut(),
                    Some(server_win_event_proc),
                    0, // All processes
                    0, // All threads
                    WINEVENT_OUTOFCONTEXT,
                );
                
                if hook.is_null() {
                    let error = GetLastError();
                    println!("❌ Failed to set up hook for {}: error {}", description, error);
                } else {
                    self.event_hooks.push(hook);
                    println!("✅ Successfully set up hook for {}", description);
                }
            }
            
            if self.event_hooks.is_empty() {
                return Err("Failed to set up any event hooks".into());
            }
            
            println!("🚀 Successfully set up {} WinEvent hooks!", self.event_hooks.len());
            println!("📢 Now listening for real-time window events and publishing updates!");
        }
          Ok(())
    }
    
    /// Process layout commands from clients
    pub fn process_layout_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut subscriber) = self.layout_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let layout_msg = *sample;
                println!("🗂️ Received layout command: {:?}", layout_msg);
                
                match layout_msg.message_type {
                    0 => { // apply_layout
                        println!("📥 Layout application request received");
                    },
                    1 => { // save_current_layout  
                        let layout_name = format!("layout_{}", layout_msg.layout_id);
                        if let Ok(mut tracker) = self.tracker.lock() {
                            tracker.save_current_layout(layout_name.clone());
                            println!("💾 Saved current layout as '{}'", layout_name);
                        }
                    },
                    2 => { // get_saved_layouts
                        println!("📋 Saved layouts request received");
                    },
                    _ => {
                        println!("⚠️ Unknown layout command type: {}", layout_msg.message_type);
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Process animation commands from clients
    pub fn process_animation_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut subscriber) = self.animation_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let anim_cmd = *sample;
                println!("🎬 Received animation command: {:?}", anim_cmd);
                
                match anim_cmd.command_type {
                    0 => { // start_animation
                        let target_rect = winapi::shared::windef::RECT {
                            left: anim_cmd.target_x,
                            top: anim_cmd.target_y,
                            right: anim_cmd.target_x + anim_cmd.target_width as i32,
                            bottom: anim_cmd.target_y + anim_cmd.target_height as i32,
                        };
                        
                        let easing_type = match anim_cmd.easing_type {
                            0 => crate::EasingType::Linear,
                            1 => crate::EasingType::EaseIn,
                            2 => crate::EasingType::EaseOut,
                            3 => crate::EasingType::EaseInOut,
                            4 => crate::EasingType::Bounce,
                            5 => crate::EasingType::Elastic,
                            6 => crate::EasingType::Back,
                            _ => crate::EasingType::Linear,
                        };
                        
                        if let Ok(mut tracker) = self.tracker.lock() {
                            let duration = std::time::Duration::from_millis(anim_cmd.duration_ms as u64);
                            if let Err(e) = tracker.start_window_animation(
                                anim_cmd.hwnd as winapi::shared::windef::HWND, 
                                target_rect, 
                                duration, 
                                easing_type
                            ) {
                                println!("⚠️ Failed to start animation for window {}: {}", anim_cmd.hwnd, e);
                            }
                        }
                    },
                    1 => { // stop_animation
                        if let Ok(mut tracker) = self.tracker.lock() {
                            if anim_cmd.hwnd == 0 {
                                tracker.active_animations.clear();
                                println!("🛑 Stopped all animations");
                            } else {
                                tracker.active_animations.remove(&(anim_cmd.hwnd as winapi::shared::windef::HWND));
                                println!("🛑 Stopped animation for window {}", anim_cmd.hwnd);
                            }
                        }
                    },
                    4 => { // get_status
                        println!("📊 Animation status request for window {}", anim_cmd.hwnd);
                        // Could publish status here
                    },
                    _ => {
                        println!("⚠️ Unknown animation command type: {}", anim_cmd.command_type);
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Update all active animations
    pub fn update_animations(&mut self) -> Result<Vec<u64>, Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            let completed = tracker.update_animations();
            Ok(completed.into_iter().map(|hwnd| hwnd as u64).collect())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }
    
    /// Cleanup WinEvent hooks
    pub fn cleanup_hooks(&mut self) {
        unsafe {
            for hook in &self.event_hooks {
                UnhookWinEvent(*hook);
            }
            self.event_hooks.clear();
            GLOBAL_IPC_SERVER = None;
            println!("🧹 Cleaned up all WinEvent hooks");
        }
    }
}

impl Drop for GridIpcServer {
    fn drop(&mut self) {
        self.cleanup_hooks();    }
}
