use crate::WindowTracker;
use iceoryx2::prelude::*;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use serde::{Deserialize, Serialize};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use winapi::shared::windef::HWND;

// Service names for iceoryx2 communication  
pub const GRID_EVENTS_SERVICE: &str = "e_grid_events";
pub const GRID_COMMANDS_SERVICE: &str = "e_grid_commands";
pub const GRID_RESPONSE_SERVICE: &str = "e_grid_responses";
pub const GRID_WINDOW_LIST_SERVICE: &str = "e_grid_window_list"; // Deprecated - chunked approach
pub const GRID_WINDOW_DETAILS_SERVICE: &str = "e_grid_window_details"; // Individual window details

// Zero-copy compatible data types for iceoryx2
// Using only basic types that work with iceoryx2's zero-copy requirements
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowEvent {
    pub event_type: u8, // 0=created, 1=destroyed, 2=moved, 3=state_changed
    pub hwnd: u64,
    pub row: u32,
    pub col: u32,
    pub old_row: u32,
    pub old_col: u32,
    pub timestamp: u64,
    pub total_windows: u32,
    pub occupied_cells: u32,
}

impl Default for WindowEvent {
    fn default() -> Self {
        Self {
            event_type: 0,
            hwnd: 0,
            row: 0,
            col: 0,
            old_row: 0,
            old_col: 0,
            timestamp: 0,
            total_windows: 0,
            occupied_cells: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq,ZeroCopySend)]
#[repr(C)]
pub struct WindowCommand {
    pub command_type: u8, // 0=move_window, 1=get_state, 2=get_windows, 3=assign_window_virtual, 4=assign_window_monitor
    pub hwnd: u64,
    pub target_row: u32,
    pub target_col: u32,
    pub monitor_id: u32, // Monitor index for per-monitor assignment (ignored for virtual grid)
}

impl Default for WindowCommand {
    fn default() -> Self {
        Self {
            command_type: 0,
            hwnd: 0,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq,ZeroCopySend)]
#[repr(C)]
pub struct WindowResponse {
    pub response_type: u8, // 0=success, 1=error, 2=window_list_count, 3=individual_window
    pub error_code: u32,
    pub window_count: u32,
    pub data: [u64; 4], // Generic data payload
}

impl Default for WindowResponse {
    fn default() -> Self {
        Self {
            response_type: 0,
            error_code: 0,
            window_count: 0,
            data: [0; 4],
        }
    }
}

// Individual window information with position data
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowPositionInfo {
    pub hwnd: u64,
    pub top_left_row: u32,
    pub top_left_col: u32,
    pub bottom_right_row: u32,
    pub bottom_right_col: u32,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

impl Default for WindowPositionInfo {
    fn default() -> Self {
        Self {
            hwnd: 0,
            top_left_row: 0,
            top_left_col: 0,
            bottom_right_row: 0,
            bottom_right_col: 0,
            width: 0,
            height: 0,
            x: 0,
            y: 0,
        }
    }
}

// Zero-copy compatible individual window information for IPC
// Based on the WindowInfo from lib.rs but optimized for IPC
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowDetails {
    pub hwnd: u64,
    pub x: i32,         // Window rectangle coordinates
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub virtual_row_start: u32,    // Top-left grid position in virtual grid
    pub virtual_col_start: u32,
    pub virtual_row_end: u32,      // Bottom-right grid position in virtual grid  
    pub virtual_col_end: u32,
    pub monitor_id: u32,           // Which monitor this window is primarily on
    pub monitor_row_start: u32,    // Top-left grid position in monitor grid
    pub monitor_col_start: u32,
    pub monitor_row_end: u32,      // Bottom-right grid position in monitor grid
    pub monitor_col_end: u32,
    pub title_len: u32,            // Length of title (for separate title transmission)
}

impl Default for WindowDetails {
    fn default() -> Self {
        Self {
            hwnd: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            virtual_row_start: 0,
            virtual_col_start: 0,
            virtual_row_end: 0,
            virtual_col_end: 0,
            monitor_id: 0,
            monitor_row_start: 0,
            monitor_col_start: 0,
            monitor_row_end: 0,
            monitor_col_end: 0,
            title_len: 0,
        }
    }
}

// Higher-level enum types for external API compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GridEvent {
    WindowCreated {
        hwnd: u64,
        title: String,
        row: usize,
        col: usize,
    },
    WindowDestroyed {
        hwnd: u64,
        title: String,
    },
    WindowMoved {
        hwnd: u64,
        title: String,
        old_row: usize,
        old_col: usize,
        new_row: usize,
        new_col: usize,
    },
    GridStateChanged {
        timestamp: u64,
        total_windows: usize,
        occupied_cells: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GridCommand {
    MoveWindowToCell {
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    },
    AssignWindowToVirtualCell {
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    },
    AssignWindowToMonitorCell {
        hwnd: u64,
        target_row: usize,
        target_col: usize,
        monitor_id: usize,
    },
    GetGridState,
    GetWindowList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GridResponse {
    Success,
    Error(String),
    WindowList {
        windows: Vec<WindowInfo>,
    },
    GridState {
        total_windows: usize,
        occupied_cells: usize,
        grid_summary: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub hwnd: u64,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub row: usize,
    pub col: usize,
}

// IPC Manager with full iceoryx2 integration
pub struct GridIpcManager {
    tracker: Arc<Mutex<WindowTracker>>,
    event_listeners: Vec<Box<dyn Fn(&GridEvent) + Send + Sync>>,
    
    // iceoryx2 node
    node: Option<Node<ipc::Service>>,
      // iceoryx2 services
    event_publisher: Option<Publisher<ipc::Service, WindowEvent, ()>>,
    command_subscriber: Option<Subscriber<ipc::Service, WindowCommand, ()>>,
    response_publisher: Option<Publisher<ipc::Service, WindowResponse, ()>>,
    window_details_publisher: Option<Publisher<ipc::Service, WindowDetails, ()>>, // Individual window details
    
    is_running: bool,
}

impl GridIpcManager {    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Result<Self, Box<dyn std::error::Error>> {        Ok(Self { 
            tracker,
            event_listeners: Vec::new(),
            node: None,
            event_publisher: None,
            command_subscriber: None,
            response_publisher: None,
            window_details_publisher: None, // Initialize window details publisher
            is_running: false,
        })
    }pub fn setup_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Setting up iceoryx2 IPC services...");
        
        // Create iceoryx2 node
        let node = NodeBuilder::new().create::<ipc::Service>()?;
          // Setup event publishing service
        let event_service = node
            .service_builder(&ServiceName::new(GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowEvent>()
            .max_publishers(8)  // Increase from default (usually 2)
            .max_subscribers(8) // Increase from default (usually 2)
            .open_or_create()?;
        
        self.event_publisher = Some(event_service.publisher_builder().create()?);
        
        // Setup command subscription service
        let command_service = node
            .service_builder(&ServiceName::new(GRID_COMMANDS_SERVICE)?)
            .publish_subscribe::<WindowCommand>()
            .max_publishers(8)  // Increase from default (usually 2)
            .max_subscribers(8) // Increase from default (usually 2)
            .open_or_create()?;
        
        self.command_subscriber = Some(command_service.subscriber_builder().create()?);
          // Setup response publishing service
        let response_service = node
            .service_builder(&ServiceName::new(GRID_RESPONSE_SERVICE)?)
            .publish_subscribe::<WindowResponse>()
            .max_publishers(8)  // Increase from default (usually 2)
            .max_subscribers(8) // Increase from default (usually 2)
            .open_or_create()?;
          self.response_publisher = Some(response_service.publisher_builder().create()?);
          // Setup window details publishing service 
        let window_details_service = node
            .service_builder(&ServiceName::new(GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<WindowDetails>()
            .max_publishers(8)  // Increase from default (usually 2)
            .max_subscribers(8) // Increase from default (usually 2)
            .open_or_create()?;
        
        self.window_details_publisher = Some(window_details_service.publisher_builder().create()?);
        
        // Store the node
        self.node = Some(node);
        
        println!("✅ iceoryx2 IPC services initialized successfully");
        println!("   📡 Event service: {}", GRID_EVENTS_SERVICE);
        println!("   📨 Command service: {}", GRID_COMMANDS_SERVICE);
        println!("   📤 Response service: {}", GRID_RESPONSE_SERVICE);
        println!("   📋 Window details service: {}", GRID_WINDOW_DETAILS_SERVICE);

        self.is_running = true;
        Ok(())
    }pub fn publish_event(&mut self, event: GridEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Convert high-level event to zero-copy format
        let window_event = self.grid_event_to_window_event(&event);
        
        // Publish via iceoryx2
        if let Some(ref mut publisher) = self.event_publisher {
            publisher.send_copy(window_event)?;
            println!("📡 Published event via iceoryx2: {:?}", event);
        }
        
        // Notify local listeners
        for listener in &self.event_listeners {
            listener(&event);
        }
        
        Ok(())
    }    pub fn process_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut commands_to_process: Vec<WindowCommand> = Vec::new();
        
        // First, collect all incoming commands
        if let Some(ref mut subscriber) = self.command_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let command = *sample;
                commands_to_process.push(command);
            }
        }
        
        // Then process each command
        for command in commands_to_process {
            println!("📨 Received command: {:?}", command);
            
            // Convert to high-level command and process
            let grid_command = Self::window_command_to_grid_command(&command);
            let response = self.handle_command(grid_command)?;
            
            // Send response via iceoryx2
            self.send_response(response)?;
        }
        
        Ok(())
    }fn send_response(&mut self, response: GridResponse) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut publisher) = self.response_publisher {
            let window_response = Self::grid_response_to_window_response(&response);
            publisher.send_copy(window_response)?;
            println!("📤 Sent response via iceoryx2: {:?}", response);
        }
        Ok(())
    }

    pub fn handle_command(&mut self, command: GridCommand) -> Result<GridResponse, Box<dyn std::error::Error>> {
        match command {            GridCommand::GetGridState => {
                if let Ok(tracker) = self.tracker.lock() {
                    let total_windows = tracker.windows.len();
                    let occupied_cells = self.count_occupied_cells(&tracker);
                    
                    // Create a simple grid summary
                    let mut grid_summary = format!("Grid: {} windows, {} occupied cells\n", total_windows, occupied_cells);
                    grid_summary.push_str("Windows:\n");
                    
                    for (hwnd, window) in tracker.windows.iter().take(10) { // Limit to first 10 for brevity
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
                    Ok(GridResponse::GridState {
                        total_windows,
                        occupied_cells,
                        grid_summary,
                    })
                } else {
                    Ok(GridResponse::Error("Failed to access window tracker".to_string()))
                }
            }            GridCommand::GetWindowList => {
                let hwnd_list = if let Ok(tracker) = self.tracker.lock() {
                    // Collect HWNDs for the window list response
                    tracker.windows.keys()
                        .map(|hwnd| *hwnd as u64)
                        .collect::<Vec<u64>>()
                } else {
                    return Ok(GridResponse::Error("Failed to access window tracker".to_string()));
                };
                
                println!("📋 GetWindowList request - publishing details for {} windows", hwnd_list.len());
                
                // Publish individual window details for all windows
                if let Err(e) = self.publish_all_window_details() {
                    println!("⚠️ Failed to publish window details: {}", e);
                }
                
                // Still return a simple response via the regular channel
                Ok(GridResponse::Success)
            }GridCommand::MoveWindowToCell { hwnd, target_row, target_col } => {
                println!("🎯 Request to move window {} to cell ({}, {})", hwnd, target_row, target_col);
                
                // TODO: Implement actual window movement using Windows API
                match self.move_window_to_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!("Failed to move window: {}", e))),
                }
            }            GridCommand::AssignWindowToVirtualCell { hwnd, target_row, target_col } => {
                println!("📍 Request to assign window {} to virtual grid cell ({}, {})", hwnd, target_row, target_col);
                
                match self.assign_window_to_virtual_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!("Failed to assign window to virtual cell: {}", e))),
                }
            }
            GridCommand::AssignWindowToMonitorCell { hwnd, target_row, target_col, monitor_id } => {
                println!("📍 Request to assign window {} to monitor {} cell ({}, {})", hwnd, monitor_id, target_row, target_col);
                
                match self.assign_window_to_monitor_cell(hwnd, target_row, target_col, monitor_id) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!("Failed to assign window to monitor cell: {}", e))),
                }
            }
        }
    }    fn move_window_to_cell(&mut self, hwnd: u64, target_row: usize, target_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement actual window movement logic
        // This would involve:
        // 1. Calculate target position based on grid dimensions
        // 2. Use Windows API to move the window
        // 3. Update the internal tracking
        
        println!("🔧 TODO: Implement window movement for HWND {} to ({}, {})", hwnd, target_row, target_col);
        Ok(())
    }

    // Assignment to virtual grid (coordinates span all monitors)
    fn assign_window_to_virtual_cell(&mut self, hwnd: u64, target_row: usize, target_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            // Find the window in our tracking and save the title first
            let window_title = if let Some(window) = tracker.windows.get(&(hwnd as winapi::shared::windef::HWND)) {
                window.title.clone()
            } else {
                return Err(format!("Window with HWND {} not found in tracker", hwnd).into());
            };
            
            // Now we can safely modify the window
            if let Some(window) = tracker.windows.get_mut(&(hwnd as winapi::shared::windef::HWND)) {
                // Clear existing grid assignments for this window
                window.grid_cells.clear();
                
                // Assign to the new cell
                window.grid_cells.push((target_row, target_col));
                
                println!("✅ Assigned window {} '{}' to virtual grid cell ({}, {})", 
                    hwnd, 
                    if window_title.len() > 30 { 
                        format!("{}...", &window_title[..30]) 
                    } else { 
                        window_title.clone() 
                    }, 
                    target_row, 
                    target_col
                );            }
            
            // Update both virtual and monitor grids
            tracker.update_grid();
            tracker.update_monitor_grids();
              // Release the tracker lock before moving the window
            drop(tracker);
              // Calculate the target position for the virtual grid cell
            match self.calculate_virtual_cell_position(target_row, target_col) {
                Ok((cell_left, cell_top, cell_right, cell_bottom)) => {
                    // Calculate cell dimensions
                    let cell_width = cell_right - cell_left;
                    let cell_height = cell_bottom - cell_top;
                    
                    // Calculate window position and size to fill the cell
                    let window_width = cell_width.max(100);  // Minimum width of 100
                    let window_height = cell_height.max(50); // Minimum height of 50
                    let window_left = cell_left;
                    let window_top = cell_top;
                    
                    println!("🔧 Cell bounds: ({}, {}) to ({}, {}) [{}x{}]", 
                        cell_left, cell_top, cell_right, cell_bottom, cell_width, cell_height);
                    println!("🎯 Window bounds: ({}, {}) [{}x{}]", 
                        window_left, window_top, window_width, window_height);
                    
                    // Move the window to the calculated position
                    if let Err(e) = self.move_window_to_position(hwnd, window_left, window_top, window_width, window_height) {
                        println!("⚠️ Failed to physically move window {}: {}", hwnd, e);
                    } else {
                        println!("🎯 Successfully moved window {} to virtual grid cell ({}, {})", hwnd, target_row, target_col);
                    }
                }
                Err(e) => {
                    println!("⚠️ Failed to calculate position for virtual grid cell ({}, {}): {}", target_row, target_col, e);
                }
            }
            
            // Publish an event about the assignment
            let event = crate::ipc::GridEvent::WindowMoved {
                hwnd,
                title: window_title,
                old_row: 0, // We don't track previous assignment currently
                old_col: 0,
                new_row: target_row,
                new_col: target_col,
            };
            
            // Convert and publish the event
            let window_event = self.grid_event_to_window_event(&event);
            if let Some(ref mut publisher) = self.event_publisher {
                let _ = publisher.send_copy(window_event);
            }
            
            Ok(())
        } else {
            Err("Failed to acquire window tracker lock".into())
        }
    }

    // Assignment to specific monitor grid
    fn assign_window_to_monitor_cell(&mut self, hwnd: u64, target_row: usize, target_col: usize, monitor_id: usize) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            // Find the window in our tracking and save the title first
            let window_title = if let Some(window) = tracker.windows.get(&(hwnd as winapi::shared::windef::HWND)) {
                window.title.clone()
            } else {
                return Err(format!("Window with HWND {} not found in tracker", hwnd).into());
            };
            
            // Check if the monitor exists
            if monitor_id >= tracker.monitor_grids.len() {
                return Err(format!("Monitor {} does not exist. Available monitors: 0-{}", 
                    monitor_id, tracker.monitor_grids.len() - 1).into());
            }
            
            // Now we can safely modify the window
            if let Some(window) = tracker.windows.get_mut(&(hwnd as winapi::shared::windef::HWND)) {
                // Clear existing monitor assignments for this window
                window.monitor_cells.clear();
                
                // Assign to the new monitor cell
                window.monitor_cells.insert(monitor_id, vec![(target_row, target_col)]);
                
                println!("✅ Assigned window {} '{}' to monitor {} grid cell ({}, {})", 
                    hwnd, 
                    if window_title.len() > 30 { 
                        format!("{}...", &window_title[..30]) 
                    } else { 
                        window_title.clone() 
                    }, 
                    monitor_id,
                    target_row, 
                    target_col
                );            }
            
            // Update both virtual and monitor grids
            tracker.update_grid();
            tracker.update_monitor_grids();
            
            // Release the tracker lock before moving the window
            drop(tracker);
              // Calculate the target position for the monitor grid cell
            match self.calculate_monitor_cell_position(target_row, target_col, monitor_id) {
                Ok((cell_left, cell_top, cell_right, cell_bottom)) => {
                    // Calculate cell dimensions
                    let cell_width = cell_right - cell_left;                    let cell_height = cell_bottom - cell_top;
                    
                    // Calculate window position and size to fill the cell
                    let window_width = cell_width.max(100);  // Minimum width of 100
                    let window_height = cell_height.max(50); // Minimum height of 50
                    let window_left = cell_left;
                    let window_top = cell_top;
                    
                    println!("🔧 Monitor {} cell bounds: ({}, {}) to ({}, {}) [{}x{}]", 
                        monitor_id, cell_left, cell_top, cell_right, cell_bottom, cell_width, cell_height);
                    println!("🎯 Window bounds: ({}, {}) [{}x{}]", 
                        window_left, window_top, window_width, window_height);
                    
                    // Move the window to the calculated position
                    if let Err(e) = self.move_window_to_position(hwnd, window_left, window_top, window_width, window_height) {
                        println!("⚠️ Failed to physically move window {}: {}", hwnd, e);
                    } else {
                        println!("🎯 Successfully moved window {} to monitor {} grid cell ({}, {})", hwnd, monitor_id, target_row, target_col);
                    }
                }
                Err(e) => {
                    println!("⚠️ Failed to calculate position for monitor {} grid cell ({}, {}): {}", monitor_id, target_row, target_col, e);
                }
            }
            
            // Publish an event about the assignment
            let event = crate::ipc::GridEvent::WindowMoved {
                hwnd,
                title: window_title,
                old_row: 0, // We don't track previous assignment currently
                old_col: 0,
                new_row: target_row,
                new_col: target_col,
            };
            
            // Convert and publish the event
            let window_event = self.grid_event_to_window_event(&event);
            if let Some(ref mut publisher) = self.event_publisher {
                let _ = publisher.send_copy(window_event);
            }
            
            Ok(())
        } else {
            Err("Failed to acquire window tracker lock".into())
        }
    }

    fn count_occupied_cells(&self, tracker: &WindowTracker) -> usize {
        let mut occupied = std::collections::HashSet::new();
        for window in tracker.windows.values() {
            for &(row, col) in &window.grid_cells {
                occupied.insert((row, col));
            }
        }        occupied.len()
    }

    // Conversion functions between high-level and zero-copy types
    fn grid_event_to_window_event(&self, event: &GridEvent) -> WindowEvent {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        match event {
            GridEvent::WindowCreated { hwnd, row, col, .. } => WindowEvent {
                event_type: 0,
                hwnd: *hwnd,
                row: *row as u32,
                col: *col as u32,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowDestroyed { hwnd, .. } => WindowEvent {
                event_type: 1,
                hwnd: *hwnd,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowMoved { hwnd, old_row, old_col, new_row, new_col, .. } => WindowEvent {
                event_type: 2,
                hwnd: *hwnd,
                old_row: *old_row as u32,
                old_col: *old_col as u32,
                row: *new_row as u32,
                col: *new_col as u32,
                timestamp,
                ..Default::default()
            },
            GridEvent::GridStateChanged { timestamp, total_windows, occupied_cells } => WindowEvent {
                event_type: 3,
                timestamp: *timestamp,
                total_windows: *total_windows as u32,
                occupied_cells: *occupied_cells as u32,
                ..Default::default()
            },
        }
    }

    fn window_command_to_grid_command(command: &WindowCommand) -> GridCommand {
        match command.command_type {
            0 => GridCommand::MoveWindowToCell {
                hwnd: command.hwnd,
                target_row: command.target_row as usize,
                target_col: command.target_col as usize,
            },
            1 => GridCommand::GetGridState,
            2 => GridCommand::GetWindowList,
            3 => GridCommand::AssignWindowToVirtualCell {
                hwnd: command.hwnd,
                target_row: command.target_row as usize,
                target_col: command.target_col as usize,
            },
            4 => GridCommand::AssignWindowToMonitorCell {
                hwnd: command.hwnd,
                target_row: command.target_row as usize,
                target_col: command.target_col as usize,
                monitor_id: command.monitor_id as usize,
            },            _ => GridCommand::GetGridState, // Default fallback
        }
    }

    fn grid_response_to_window_response(response: &GridResponse) -> WindowResponse {
        match response {
            GridResponse::Success => WindowResponse {
                response_type: 0,
                error_code: 0,
                ..Default::default()
            },
            GridResponse::Error(_) => WindowResponse {
                response_type: 1,
                error_code: 1,
                ..Default::default()
            },
            GridResponse::WindowList { windows } => WindowResponse {
                response_type: 2,
                window_count: windows.len() as u32,
                ..Default::default()
            },
            GridResponse::GridState { total_windows, occupied_cells, .. } => WindowResponse {
                response_type: 3, // New response type for grid state
                error_code: 0,
                window_count: *total_windows as u32,
                data: [*occupied_cells as u64, 0, 0, 0], // Pack occupied_cells into first data slot
            },
        }
    }

    pub fn add_event_listener<F>(&mut self, listener: F) 
    where
        F: Fn(&GridEvent) + Send + Sync + 'static,
    {
        self.event_listeners.push(Box::new(listener));
    }

    pub fn run_event_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔄 Starting iceoryx2 IPC event loop...");
        
        while self.is_running {
            // Process any incoming commands
            self.process_commands()?;
            
            // Small delay to prevent busy waiting
            std::thread::sleep(Duration::from_millis(10));
        }
        
        Ok(())
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        println!("🛑 iceoryx2 IPC event loop stopped");
    }

    // Convenience methods for publishing specific events
    pub fn publish_window_created(&mut self, hwnd: u64, title: String, row: usize, col: usize) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::WindowCreated { hwnd, title, row, col };
        self.publish_event(event)
    }

    pub fn publish_window_destroyed(&mut self, hwnd: u64, title: String) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::WindowDestroyed { hwnd, title };
        self.publish_event(event)
    }

    pub fn publish_window_moved(&mut self, hwnd: u64, title: String, old_row: usize, old_col: usize, new_row: usize, new_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::WindowMoved { hwnd, title, old_row, old_col, new_row, new_col };
        self.publish_event(event)
    }    pub fn publish_grid_state_changed(&mut self, total_windows: usize, occupied_cells: usize) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::GridStateChanged {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            total_windows,
            occupied_cells,
        };
        self.publish_event(event)
    }

    // Publish individual window details for real-time client updates
    pub fn publish_window_details(&mut self, hwnd: HWND) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            if let Some(window_info) = tracker.windows.get(&hwnd) {
                let details = self.window_info_to_details(hwnd, window_info);
                  if let Some(ref mut publisher) = self.window_details_publisher {
                    publisher.send_copy(details)?;
                    println!("📤 Published window details for HWND {:?}", hwnd);
                } else {
                    println!("⚠️ Window details publisher not available");
                }
            }
        }
        Ok(())
    }

    // Publish details for all current windows (useful for client initialization)
    pub fn publish_all_window_details(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            let window_count = tracker.windows.len();
            println!("📤 Publishing details for {} windows...", window_count);
            
            for (&hwnd, window_info) in &tracker.windows {
                let details = self.window_info_to_details(hwnd, window_info);
                
                if let Some(ref mut publisher) = self.window_details_publisher {
                    publisher.send_copy(details)?;
                } else {
                    println!("⚠️ Window details publisher not available");
                    break;
                }
            }
            
            println!("✅ Published details for {} windows", window_count);
        }
        Ok(())
    }

    // Calculate position for virtual grid (coordinates span all monitors)
    fn calculate_virtual_cell_position(&self, target_row: usize, target_col: usize) -> Result<(i32, i32, i32, i32), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            // Use the virtual grid (spanning all monitors)
            let (left, top, right, bottom) = (
                tracker.monitor_rect.left,
                tracker.monitor_rect.top,
                tracker.monitor_rect.right,
                tracker.monitor_rect.bottom
            );
            
            let cell_width = (right - left) / crate::GRID_COLS as i32;
            let cell_height = (bottom - top) / crate::GRID_ROWS as i32;
            
            if target_row < crate::GRID_ROWS && target_col < crate::GRID_COLS {
                let cell_left = left + (target_col as i32 * cell_width);
                let cell_top = top + (target_row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;
                
                println!("🔧 Calculated VIRTUAL GRID cell position for ({}, {}): screen coords ({}, {}) to ({}, {})", 
                    target_row, target_col, cell_left, cell_top, cell_right, cell_bottom);
                println!("   Virtual screen bounds: ({}, {}) to ({}, {})", left, top, right, bottom);
                
                Ok((cell_left, cell_top, cell_right, cell_bottom))
            } else {
                Err(format!("Invalid virtual grid coordinates: ({}, {}). Max is ({}, {})", 
                    target_row, target_col, crate::GRID_ROWS - 1, crate::GRID_COLS - 1).into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    // Calculate position for specific monitor grid
    fn calculate_monitor_cell_position(&self, target_row: usize, target_col: usize, monitor_id: usize) -> Result<(i32, i32, i32, i32), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            // Get the specific monitor
            if monitor_id >= tracker.monitor_grids.len() {
                return Err(format!("Monitor {} does not exist. Available monitors: 0-{}", 
                    monitor_id, tracker.monitor_grids.len() - 1).into());
            }
            
            let monitor = &tracker.monitor_grids[monitor_id];
            let (left, top, right, bottom) = monitor.monitor_rect;
            
            let cell_width = (right - left) / crate::GRID_COLS as i32;
            let cell_height = (bottom - top) / crate::GRID_ROWS as i32;
            
            if target_row < crate::GRID_ROWS && target_col < crate::GRID_COLS {
                let cell_left = left + (target_col as i32 * cell_width);
                let cell_top = top + (target_row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;
                
                println!("🔧 Calculated MONITOR {} GRID cell position for ({}, {}): screen coords ({}, {}) to ({}, {})", 
                    monitor_id, target_row, target_col, cell_left, cell_top, cell_right, cell_bottom);
                println!("   Monitor {} bounds: ({}, {}) to ({}, {})", monitor_id, left, top, right, bottom);
                
                Ok((cell_left, cell_top, cell_right, cell_bottom))
            } else {
                Err(format!("Invalid monitor grid coordinates: ({}, {}). Max is ({}, {})", 
                    target_row, target_col, crate::GRID_ROWS - 1, crate::GRID_COLS - 1).into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }    fn move_window_to_position(&self, hwnd: u64, left: i32, top: i32, width: i32, height: i32) -> Result<(), Box<dyn std::error::Error>> {
        use winapi::um::winuser::{SetWindowPos, SWP_NOZORDER, SWP_NOACTIVATE};
        
        let hwnd_handle = hwnd as winapi::shared::windef::HWND;
        
        unsafe {
            let result = SetWindowPos(
                hwnd_handle,
                ptr::null_mut(), // HWND_TOP equivalent (we use SWP_NOZORDER to ignore this)
                left,
                top,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE, // Don't change Z-order, don't activate
            );
            
            if result == 0 {
                Err("Failed to move window".into())
            } else {
                println!("🔧 Moved window {} to position ({}, {}) with size {}x{}", hwnd, left, top, width, height);                // Rescan the grid to update internal tracking after the window move
                if let Ok(mut tracker) = self.tracker.lock() {
                    println!("🔄 Rescanning grid after window movement...");
                    
                    // Update the window's rectangle in our tracking
                    if let Some(new_rect) = crate::WindowTracker::get_window_rect(hwnd_handle) {
                        if let Some(window) = tracker.windows.get_mut(&hwnd_handle) {
                            window.rect = new_rect;
                            println!("   📍 Updated window {} rect to ({}, {}) - ({}, {})", 
                                hwnd, new_rect.left, new_rect.top, new_rect.right, new_rect.bottom);
                        }
                        
                        // Now recalculate grid cells after releasing the mutable reference
                        let grid_cells = tracker.window_to_grid_cells(&new_rect);
                        let monitor_cells = tracker.calculate_monitor_cells(&new_rect);
                        
                        // Update the window with the new grid assignments
                        if let Some(window) = tracker.windows.get_mut(&hwnd_handle) {
                            window.grid_cells = grid_cells;
                            window.monitor_cells = monitor_cells;
                            
                            println!("   🔄 Recalculated grid assignments: {} virtual cells, {} monitor assignments", 
                                window.grid_cells.len(), window.monitor_cells.len());
                        }
                    }
                      // Update both virtual and monitor grids
                    tracker.update_grid();
                    tracker.update_monitor_grids();
                    
                    println!("✅ Grid rescan complete after window movement");
                } else {
                    println!("⚠️ Failed to acquire tracker lock for grid rescan");
                }
                
                Ok(())
            }
        }
    }

    // Helper function to convert WindowInfo to WindowDetails for IPC
    fn window_info_to_details(&self, hwnd: HWND, window_info: &crate::WindowInfo) -> WindowDetails {
        // Calculate grid positions for the window
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
        
        WindowDetails {
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
            title_len: window_info.title.len().min(255) as u32, // Cap at 255 chars
        }
    }
}

// Module definition for iceoryx2 service type
pub mod ipc {
    pub use iceoryx2::service::ipc::Service;
}
