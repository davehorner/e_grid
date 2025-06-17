use crate::WindowTracker;
use iceoryx2::prelude::*;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Service names for iceoryx2 communication  
pub const GRID_EVENTS_SERVICE: &str = "e_grid_events";
pub const GRID_COMMANDS_SERVICE: &str = "e_grid_commands";
pub const GRID_RESPONSE_SERVICE: &str = "e_grid_responses";

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
    pub command_type: u8, // 0=move_window, 1=get_state, 2=get_windows
    pub hwnd: u64,
    pub target_row: u32,
    pub target_col: u32,
}

impl Default for WindowCommand {
    fn default() -> Self {
        Self {
            command_type: 0,
            hwnd: 0,
            target_row: 0,
            target_col: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq,ZeroCopySend)]
#[repr(C)]
pub struct WindowResponse {
    pub response_type: u8, // 0=success, 1=error, 2=window_list
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
    
    is_running: bool,
}

impl GridIpcManager {
    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self { 
            tracker,
            event_listeners: Vec::new(),
            node: None,
            event_publisher: None,
            command_subscriber: None,
            response_publisher: None,
            is_running: false,
        })
    }    pub fn setup_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Setting up iceoryx2 IPC services...");
        
        // Create iceoryx2 node
        let node = NodeBuilder::new().create::<ipc::Service>()?;
        
        // Setup event publishing service
        let event_service = node
            .service_builder(&ServiceName::new(GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowEvent>()
            .open_or_create()?;
        
        self.event_publisher = Some(event_service.publisher_builder().create()?);
        
        // Setup command subscription service
        let command_service = node
            .service_builder(&ServiceName::new(GRID_COMMANDS_SERVICE)?)
            .publish_subscribe::<WindowCommand>()
            .open_or_create()?;
        
        self.command_subscriber = Some(command_service.subscriber_builder().create()?);
        
        // Setup response publishing service
        let response_service = node
            .service_builder(&ServiceName::new(GRID_RESPONSE_SERVICE)?)
            .publish_subscribe::<WindowResponse>()
            .open_or_create()?;
        
        self.response_publisher = Some(response_service.publisher_builder().create()?);
        
        // Store the node
        self.node = Some(node);
        
        println!("✅ iceoryx2 IPC services initialized successfully");
        println!("   📡 Event service: {}", GRID_EVENTS_SERVICE);
        println!("   📨 Command service: {}", GRID_COMMANDS_SERVICE);
        println!("   📤 Response service: {}", GRID_RESPONSE_SERVICE);

        self.is_running = true;
        Ok(())
    }    pub fn publish_event(&mut self, event: GridEvent) -> Result<(), Box<dyn std::error::Error>> {
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

    pub fn handle_command(&self, command: GridCommand) -> Result<GridResponse, Box<dyn std::error::Error>> {
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
            }
            GridCommand::GetWindowList => {
                if let Ok(tracker) = self.tracker.lock() {
                    let windows: Vec<WindowInfo> = tracker.windows.iter()
                        .map(|(hwnd, window)| {
                            let (row, col) = if !window.grid_cells.is_empty() {
                                (window.grid_cells[0].0, window.grid_cells[0].1)
                            } else {
                                (0, 0)
                            };
                            
                            WindowInfo {
                                hwnd: *hwnd as u64,
                                title: window.title.clone(),
                                x: window.rect.left,
                                y: window.rect.top,
                                width: window.rect.right - window.rect.left,
                                height: window.rect.bottom - window.rect.top,
                                row,
                                col,
                            }
                        })
                        .collect();
                    
                    Ok(GridResponse::WindowList { windows })
                } else {
                    Ok(GridResponse::Error("Failed to access window tracker".to_string()))
                }
            }
            GridCommand::MoveWindowToCell { hwnd, target_row, target_col } => {
                println!("🎯 Request to move window {} to cell ({}, {})", hwnd, target_row, target_col);
                
                // TODO: Implement actual window movement using Windows API
                match self.move_window_to_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!("Failed to move window: {}", e))),
                }
            }
        }
    }

    fn move_window_to_cell(&self, hwnd: u64, target_row: usize, target_col: usize) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement actual window movement logic
        // This would involve:
        // 1. Calculate target position based on grid dimensions
        // 2. Use Windows API to move the window
        // 3. Update the internal tracking
        
        println!("🔧 TODO: Implement window movement for HWND {} to ({}, {})", hwnd, target_row, target_col);
        Ok(())
    }

    fn count_occupied_cells(&self, tracker: &WindowTracker) -> usize {
        let mut occupied = std::collections::HashSet::new();
        for window in tracker.windows.values() {
            for &(row, col) in &window.grid_cells {
                occupied.insert((row, col));
            }
        }
        occupied.len()
    }    // Conversion functions between high-level and zero-copy types
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
            _ => GridCommand::GetGridState, // Default fallback
        }
    }    fn grid_response_to_window_response(response: &GridResponse) -> WindowResponse {
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
    }

    pub fn publish_grid_state_changed(&mut self, total_windows: usize, occupied_cells: usize) -> Result<(), Box<dyn std::error::Error>> {
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
}
