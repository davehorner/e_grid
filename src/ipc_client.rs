use crate::grid_client_errors::{
    retry_with_backoff, safe_arc_lock, validate_grid_coordinates, GridClientError,
    GridClientResult, RetryConfig,
};
use crate::ipc_protocol::{
    HeartbeatMessage, IpcCommand, IpcCommandType, IpcResponse, WindowDetails, WindowEvent,
    WindowFocusEvent, GRID_COMMANDS_SERVICE, GRID_EVENTS_SERVICE, GRID_FOCUS_EVENTS_SERVICE,
    GRID_HEARTBEAT_SERVICE, GRID_RESPONSE_SERVICE, GRID_WINDOW_DETAILS_SERVICE,
};
use crate::GridConfig;
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::atomic::AtomicCell;
use dashmap::DashMap;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use iceoryx2::service::ipc::Service;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
#[derive(Clone, Copy, Debug)]
pub struct GridCell {
    pub state: ClientCellState,
    pub monitor_ids: [u32; 4], // fixed-size for lock-free, or use Option<u32>
    pub monitor_count: usize,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClientCellState {
    Empty,         // No window (on-screen area)
    Occupied(u64), // Window present (HWND as u64 for thread safety)
    OffScreen,     // Off-screen area (outside actual monitor bounds)
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
    // pub title_len: u32,
}

impl From<WindowDetails> for ClientWindowInfo {
    fn from(details: WindowDetails) -> Self {
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
            // title_len: details.title_len,
        }
    }
}

pub struct GridClient {
    // Configuration
    config: GridConfig,
    has_valid_grid_data: Arc<AtomicBool>,
    // IPC components - only keep what we need for sending commands
    command_publisher: Publisher<Service, IpcCommand, ()>,
    window_list_subscriber: Option<Subscriber<Service, crate::ipc_protocol::WindowListMessage, ()>>,
    monitor_list_subscriber: Option<Subscriber<Service, crate::ipc_protocol::MonitorList, ()>>,
    // Local grid state
    // windows: Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,

    // Monitor information - store complete monitor grids
    pub windows: Arc<DashMap<u64, ClientWindowInfo>>,

    // Add the missing monitors field
    pub monitors: Arc<DashMap<u32, MonitorGridInfo>>,

    // Control flags
    auto_display: Arc<AtomicBool>,
    running: Arc<AtomicBool>,

    // NEW: Focus event handling for e_midi integration
    // Lock-free callbacks are not directly possible with trait objects due to Rust's safety model.
    // However, you can use an AtomicPtr or crossbeam's lock-free structures if you ensure the callback's lifetime is 'static.
    // For most use cases, Arc<Mutex<...>> is the idiomatic and safe approach in Rust.
    focus_callback: Arc<Mutex<Option<Box<dyn Fn(WindowFocusEvent) + Send + Sync>>>>,
    window_event_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    move_resize_start_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    move_resize_stop_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    pub virtual_grid: Arc<Vec<AtomicCell<GridCell>>>,
    pub physical_grids: Arc<Vec<AtomicCell<GridCell>>>,
}

#[derive(Clone, Debug)]
pub struct MonitorGridInfo {
    pub monitor_id: u32,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub rows: usize, // Add rows and cols fields
    pub cols: usize,
    pub grid: Vec<Vec<Option<u64>>>,
}

impl GridClient {
    pub fn set_window_event_callback<F>(&mut self, callback: F) -> GridClientResult<()>
    where
        F: Fn(WindowEvent) + Send + Sync + 'static,
    {
        let mut cb_lock = safe_arc_lock(
            &self.window_event_callback,
            "window event callback registration",
        )?;
        *cb_lock = Some(Box::new(callback));
        Ok(())
    }
    pub fn set_move_resize_start_callback<F>(&mut self, callback: F) -> GridClientResult<()>
    where
        F: Fn(WindowEvent) + Send + Sync + 'static,
    {
        let mut cb_lock = safe_arc_lock(
            &self.move_resize_start_callback,
            "move/resize start callback registration",
        )?;
        *cb_lock = Some(Box::new(callback));
        Ok(())
    }

    /// Returns a clone of the current virtual grid state for debugging and inspection
    pub fn get_current_grid(&self) -> Result<Vec<Vec<ClientCellState>>, String> {
        // No locking needed, just clone the atomic values into a grid of ClientCellState
        let grid = self
            .virtual_grid
            .iter()
            .map(|cell| cell.load().state)
            .collect::<Vec<ClientCellState>>();
        // Convert flat Vec to 2D Vec<Vec<ClientCellState>>
        let mut result = Vec::with_capacity(self.config.rows);
        for row in 0..self.config.rows {
            let start = row * self.config.cols;
            let end = start + self.config.cols;
            result.push(grid[start..end].to_vec());
        }
        Ok(result)
    }

    pub fn set_move_resize_stop_callback<F>(&mut self, callback: F) -> GridClientResult<()>
    where
        F: Fn(WindowEvent) + Send + Sync + 'static,
    {
        let mut cb_lock = safe_arc_lock(
            &self.move_resize_stop_callback,
            "move/resize stop callback registration",
        )?;
        *cb_lock = Some(Box::new(callback));
        Ok(())
    }
    /// Request grid configuration from server before creating client
    fn request_grid_config_from_server() -> GridClientResult<GridConfig> {
        // For now, return the same default config as the server
        // TODO: Implement actual IPC request to server for dynamic configuration
        debug!("⚙️ Using server default grid configuration (TODO: implement server request)");
        Ok(GridConfig::default()) // Use same default as server (8x12)
    }
    pub fn new() -> GridClientResult<Self> {
        // Add initial delay to allow server startup
        info!("🔄 Waiting for e_grid server to start IPC services...");
        std::thread::sleep(Duration::from_millis(1000));

        let node = NodeBuilder::new()
            .create::<Service>()
            .map_err(|e| GridClientError::IpcError(format!("Failed to create IPC node: {}", e)))?; // Create command publisher with conservative retry logic
        let retry_config = RetryConfig {
            max_attempts: 5,
            base_delay_ms: 500,      // Start with longer delay
            backoff_multiplier: 1.5, // Slower backoff
        };
        let command_publisher = retry_with_backoff(
            || -> Result<_, Box<dyn std::error::Error>> {
                let command_service = node
                    .service_builder(
                        &ServiceName::new(GRID_COMMANDS_SERVICE)
                            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?,
                    )
                    .publish_subscribe::<IpcCommand>()
                    .open()
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                Ok(command_service
                    .publisher_builder()
                    .create()
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
            },
            &retry_config,
        )
        .map_err(|e| {
            GridClientError::IpcError(format!(
                "Failed to create command publisher: {:?} Check/delete: C:\\Temp\\iceoryx2",
                e
            ))
        })?; // First, get the grid configuration from the server
        let config = Self::request_grid_config_from_server()?;
        let window_list_service = node
            .service_builder(&ServiceName::new(crate::ipc_protocol::GRID_WINDOW_LIST_SERVICE).map_err(|e| {
                    GridClientError::IpcError(format!(
                        "Failed to create window list service: {:?} Check/delete: C:\\Temp\\iceoryx2",
                        e
                    ))
                })?)
            .publish_subscribe::<crate::ipc_protocol::WindowListMessage>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create().map_err(|e| {
                    GridClientError::IpcError(format!(
                        "Failed to create window list service: {:?} Check/delete: C:\\Temp\\iceoryx2",
                        e
                    ))
                })?;
        let window_list_subscriber = Some(window_list_service.subscriber_builder().create().map_err(|e| {
                    GridClientError::IpcError(format!(
                        "Failed to create window list subscriber: {:?} Check/delete: C:\\Temp\\iceoryx2",
                        e
                    ))
                })?);

        // Now initialize with the dynamic config
        let grid_size = (config.rows * config.cols) as usize;
        let virtual_grid = (0..grid_size)
            .map(|_| {
                AtomicCell::new(GridCell {
                    state: ClientCellState::Empty,
                    monitor_ids: [0; 4],
                    monitor_count: 0,
                })
            })
            .collect::<Vec<_>>();
        let physical_grids = (0..grid_size)
            .map(|_| {
                AtomicCell::new(GridCell {
                    state: ClientCellState::Empty,
                    monitor_ids: [0; 4],
                    monitor_count: 0,
                })
            })
            .collect::<Vec<_>>();
        let client = Self {
            config,
            command_publisher,
            has_valid_grid_data: Arc::new(AtomicBool::new(false)),
            windows: Arc::new(DashMap::new()),
            virtual_grid: Arc::new(virtual_grid),
            monitors: Arc::new(DashMap::new()),
            auto_display: Arc::new(AtomicBool::new(true)),
            running: Arc::new(AtomicBool::new(true)),
            focus_callback: Arc::new(Mutex::new(None)),
            window_event_callback: Arc::new(Mutex::new(None)),
            move_resize_start_callback: Arc::new(Mutex::new(None)),
            move_resize_stop_callback: Arc::new(Mutex::new(None)),
            window_list_subscriber,
            monitor_list_subscriber: None,
            physical_grids: Arc::new(physical_grids),
        };

        info!(
            "✅ Client initialized with grid size: {}x{}",
            client.config.rows, client.config.cols
        );

        // Initialize grid with off-screen areas marked
        client.initialize_client_grid().map_err(|e| {
            GridClientError::InitializationError(format!("Grid initialization failed: {}", e))
        })?;
        Ok(client)
    }

    pub fn get_latest_window_list(&mut self) -> Option<crate::ipc_protocol::WindowListMessage> {
        println!("[DEBUG] Checking for latest window list update...");
        if let Some(ref mut subscriber) = self.window_list_subscriber {
            while let Some(sample) = subscriber.receive().ok().flatten() {
                println!(
                    "[DEBUG] Received WindowListMessage: {} windows",
                    sample.windows.len()
                );
                return Some(*sample);
            }
        }
        None
    }

    /// Returns the latest MonitorList (if available) from the most recent WindowListMessage
    pub fn get_monitor_list(&mut self) -> Option<crate::ipc_protocol::MonitorList> {
        if let Some(ref mut subscriber) = self.monitor_list_subscriber {
            while let Some(sample) = subscriber.receive().ok().flatten() {
                if sample.monitor_count > 0 {
                    return Some(sample.clone());
                }
            }
        }
        None
    }

    /// Rebuilds the virtual and physical (monitor) grids from a WindowListMessage
    pub fn rebuild_grids_from_window_list(
        &mut self,
        monitor_list: &crate::ipc_protocol::MonitorList,
        window_list: &crate::ipc_protocol::WindowListMessage,
    ) {
        // Clear current state but preserve structure
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let idx = row * self.config.cols + col;
                self.virtual_grid[idx].store(GridCell {
                    state: ClientCellState::Empty,
                    monitor_ids: [0; 4],
                    monitor_count: 0,
                });
            }
        }
        self.windows.clear();
        self.monitors.clear();

        // Initialize offscreen cells based on monitor bounds (like server does)
        self.initialize_offscreen_cells(monitor_list);

        // Rebuild monitor list from WindowListMessage if present
        for i in 0..monitor_list.monitor_count as usize {
            let m = &monitor_list.monitors[i];
            let rows = self.config.rows;
            let cols = self.config.cols;
            let grid = vec![vec![None; cols]; rows];
            println!("[DEBUG] Creating monitor {}: {}x{} at ({}, {}) - {}x{} grid", 
                     m.monitor_id, m.width, m.height, m.x, m.y, rows, cols);
            self.monitors.insert(
                m.monitor_id,
                MonitorGridInfo {
                    monitor_id: m.monitor_id,
                    width: m.width,
                    height: m.height,
                    x: m.x,
                    y: m.y,
                    rows,
                    cols,
                    grid,
                },
            );
        }

        // Re-populate from window list
        for i in 0..window_list.window_count as usize {
            let w = &window_list.windows[i];
            let info = ClientWindowInfo::from(*w);
            self.windows.insert(w.hwnd, info);

            // Update virtual grid - preserve offscreen cells
            for row in w.virtual_row_start..=w.virtual_row_end {
                for col in w.virtual_col_start..=w.virtual_col_end {
                    if row < self.config.rows as u32 && col < self.config.cols as u32 {
                        let idx = (row as usize) * self.config.cols + (col as usize);
                        let current_cell = self.virtual_grid[idx].load();
                        // Only update if not offscreen
                        if !matches!(current_cell.state, ClientCellState::OffScreen) {
                            self.virtual_grid[idx].store(GridCell {
                                state: ClientCellState::Occupied(w.hwnd),
                                monitor_ids: current_cell.monitor_ids,
                                monitor_count: current_cell.monitor_count,
                            });
                        }
                    }
                }
            }

            // Update monitor grid
            if let Some(mut monitor) = self.monitors.get_mut(&w.monitor_id) {
                for row in w.monitor_row_start..=w.monitor_row_end {
                    for col in w.monitor_col_start..=w.monitor_col_end {
                        if row < self.config.rows as u32 && col < self.config.cols as u32 {
                            let hex_display = (w.hwnd % 100) as u8;
                            println!("[DEBUG] Assigning HWND {} (hex: {:02X}) to monitor {} grid cell [{}, {}]", w.hwnd, hex_display, w.monitor_id, row, col);
                            monitor.grid[row as usize][col as usize] = Some(w.hwnd);
                        }
                    }
                }
            } else {
                println!("[WARNING] Monitor {} not found for window HWND {}", w.monitor_id, w.hwnd);
            }
        }
    }

    /// Print the current virtual grid (all windows, all monitors combined)
    pub fn print_virtual_grid(&self) {
        if !self.has_valid_grid_data.load(std::sync::atomic::Ordering::Relaxed) {
            println!("(Grid not available yet: waiting for data from server)");
            return;
        }
        let window_count = self.windows.len();
        // Arc<Vec<AtomicCell<GridCell>>> does not need locking for read access
        let grid_size = self.config.rows * self.config.cols;
        let mut grid = vec![vec![ClientCellState::Empty; self.config.cols]; self.config.rows];
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let idx = row * self.config.cols + col;
                grid[row][col] = self.virtual_grid[idx].load().state;
            }
        }
        let server_grid: Vec<Vec<crate::CellState>> = grid
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        ClientCellState::Empty => crate::CellState::Empty,
                        ClientCellState::Occupied(hwnd) => crate::CellState::Occupied(*hwnd as u64),
                        ClientCellState::OffScreen => crate::CellState::OffScreen,
                    })
                    .collect()
            })
            .collect();
        println!("\n🔥 VIRTUAL GRID:");
        crate::grid_display::display_grid(
            &server_grid,
            &self.config,
            window_count,
            &crate::grid_display::GridDisplayConfig::default(),
            Some("Virtual Grid"),
            None,
            None,
        );
    }

    /// Print all physical (per-monitor) grids
    pub fn print_physical_grids(&self) {
        if !self.has_valid_grid_data.load(std::sync::atomic::Ordering::Relaxed) {
            println!("(Grid not available yet: waiting for data from server)");
            return;
        }
        if self.monitors.is_empty() {
            println!("(No monitor grids available)");
            return;
        }
        for monitor in self.monitors.iter() {
            let mut server_monitor_grid =
                vec![vec![crate::CellState::Empty; self.config.cols]; self.config.rows];
            
            // Get virtual screen bounds (like server does)
            let virtual_rect = unsafe {
                winapi::shared::windef::RECT {
                    left: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_XVIRTUALSCREEN),
                    top: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_YVIRTUALSCREEN),
                    right: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_XVIRTUALSCREEN) 
                        + winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_CXVIRTUALSCREEN),
                    bottom: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_YVIRTUALSCREEN) 
                        + winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_CYVIRTUALSCREEN),
                }
            };

            let cell_width = (virtual_rect.right - virtual_rect.left) / self.config.cols as i32;
            let cell_height = (virtual_rect.bottom - virtual_rect.top) / self.config.rows as i32;

            let monitor_rect = winapi::shared::windef::RECT {
                left: monitor.x,
                top: monitor.y,
                right: monitor.x + monitor.width,
                bottom: monitor.y + monitor.height,
            };

            for row in 0..self.config.rows {
                for col in 0..self.config.cols {
                    let cell_left = virtual_rect.left + (col as i32 * cell_width);
                    let cell_top = virtual_rect.top + (row as i32 * cell_height);
                    let cell_right = cell_left + cell_width;
                    let cell_bottom = cell_top + cell_height;

                    // Check if this cell overlaps with this monitor
                    let is_on_this_monitor = cell_left < monitor_rect.right
                        && cell_right > monitor_rect.left
                        && cell_top < monitor_rect.bottom
                        && cell_bottom > monitor_rect.top;

                    server_monitor_grid[row][col] = if is_on_this_monitor {
                        match monitor.grid[row][col] {
                            Some(hwnd) => {
                                let hex_display = (hwnd % 100) as u8;
                                println!("[DEBUG] Monitor {} grid cell [{}, {}] has HWND: {} (hex: {:02X})", monitor.monitor_id, row, col, hwnd, hex_display);
                                crate::CellState::Occupied(hwnd)
                            },
                            None => {
                                println!("[DEBUG] Monitor {} grid cell [{}, {}] is empty", monitor.monitor_id, row, col);
                                crate::CellState::Empty
                            },
                        }
                    } else {
                        crate::CellState::OffScreen
                    };
                }
            }
            let monitor_title = format!("Monitor {} Grid", monitor.monitor_id);
            crate::grid_display::display_grid(
                &server_monitor_grid,
                &self.config,
                0,
                &crate::grid_display::GridDisplayConfig::default(),
                Some(&monitor_title),
                Some((monitor.width, monitor.height)),
                None,
            );
        }
    }

    /// Print the physical grid for a single monitor
    pub fn print_physical_grid_for_monitor(&self, monitor: &MonitorGridInfo) {
        let mut server_monitor_grid =
            vec![vec![crate::CellState::Empty; self.config.cols]; self.config.rows];
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if row < monitor.grid.len() && col < monitor.grid[row].len() {
                    server_monitor_grid[row][col] = match monitor.grid[row][col] {
                        Some(hwnd) => crate::CellState::Occupied(hwnd as u64),
                        None => crate::CellState::Empty,
                    };
                }
            }
        }
        let monitor_title = format!("Monitor {} Grid", monitor.monitor_id);
        let monitor_bounds = (
            (monitor.x, monitor.y),
            (monitor.x + monitor.width, monitor.y + monitor.height),
        );
        crate::grid_display::display_grid(
            &server_monitor_grid,
            &self.config,
            0,
            &crate::grid_display::GridDisplayConfig::default(),
            Some(&monitor_title),
            Some((monitor.width, monitor.height)),
            Some(monitor_bounds),
        );
    }

    pub fn request_grid_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let command = IpcCommand {
            command_type: IpcCommandType::GetGridState, // Use GetGridState instead of GetGridConfig
            hwnd: None,
            target_row: None,
            target_col: None,
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.command_publisher.send_copy(command)?;
        debug!("🔧 Requested grid configuration from server...");
        Ok(())
    }

    pub fn wait_for_config(
        &mut self,
        _timeout_ms: u64,
    ) -> Result<GridConfig, Box<dyn std::error::Error>> {
        // For now, just return the current config
        // TODO: Implement actual waiting for server response
        Ok(self.config.clone())
    }
    fn initialize_client_grid(&self) -> GridClientResult<()> {
        // Initialize basic client grid structure - set all as empty initially
        debug!(
            "🔧 Initializing client grid structure {}x{}",
            self.config.rows, self.config.cols
        );

        {
            // Initialize all cells as empty - will be updated to offscreen when monitor data arrives
            for row in 0..self.config.rows {
                for col in 0..self.config.cols {
                    let idx = row * self.config.cols + col;
                    self.virtual_grid[idx].store(GridCell {
                        state: ClientCellState::Empty,
                        monitor_ids: [0; 4],
                        monitor_count: 0,
                    });
                }
            }
        }

        debug!("🔧 Client grid initialized - awaiting server data for monitor layouts");
        Ok(())
    }

    /// Initialize offscreen cells based on monitor bounds (similar to server logic)
    fn initialize_offscreen_cells(&self, monitor_list: &crate::ipc_protocol::MonitorList) {
        // Get virtual screen bounds (like server does)
        let virtual_rect = unsafe {
            winapi::shared::windef::RECT {
                left: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_XVIRTUALSCREEN),
                top: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_YVIRTUALSCREEN),
                right: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_XVIRTUALSCREEN) 
                    + winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_CXVIRTUALSCREEN),
                bottom: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_YVIRTUALSCREEN) 
                    + winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_CYVIRTUALSCREEN),
            }
        };

        let cell_width = (virtual_rect.right - virtual_rect.left) / self.config.cols as i32;
        let cell_height = (virtual_rect.bottom - virtual_rect.top) / self.config.rows as i32;

        // Collect actual monitor bounds from monitor list
        let mut actual_monitors = Vec::new();
        for i in 0..monitor_list.monitor_count as usize {
            let m = &monitor_list.monitors[i];
            actual_monitors.push(winapi::shared::windef::RECT {
                left: m.x,
                top: m.y,
                right: m.x + m.width,
                bottom: m.y + m.height,
            });
        }

        // Initialize all cells based on whether they're on an actual monitor
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let idx = row * self.config.cols + col;
                let current_cell = self.virtual_grid[idx].load();
                
                // Skip if cell is already occupied by a window
                if matches!(current_cell.state, ClientCellState::Occupied(_)) {
                    continue;
                }

                let cell_left = virtual_rect.left + (col as i32 * cell_width);
                let cell_top = virtual_rect.top + (row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;

                // Check if this cell overlaps with any actual monitor
                let mut is_on_screen = false;
                for monitor_rect in &actual_monitors {
                    if cell_left < monitor_rect.right
                        && cell_right > monitor_rect.left
                        && cell_top < monitor_rect.bottom
                        && cell_bottom > monitor_rect.top
                    {
                        is_on_screen = true;
                        break;
                    }
                }

                let new_state = if is_on_screen {
                    ClientCellState::Empty
                } else {
                    ClientCellState::OffScreen
                };

                self.virtual_grid[idx].store(GridCell {
                    state: new_state,
                    monitor_ids: current_cell.monitor_ids,
                    monitor_count: current_cell.monitor_count,
                });
            }
        }
    }

    pub fn start_background_monitoring(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let windows = self.windows.clone();
        let virtual_grid = self.virtual_grid.clone();
        let monitors = self.monitors.clone();
        let auto_display = self.auto_display.clone();
        let running = self.running.clone();
        let focus_callback = self.focus_callback.clone();
        let window_event_callback = self.window_event_callback.clone();
        let move_resize_start_callback = self.move_resize_start_callback.clone();
        let move_resize_stop_callback = self.move_resize_stop_callback.clone();
        let has_valid_grid_data = self.has_valid_grid_data.clone(); // Add the valid grid data flag
        let config = self.config.clone(); // Clone the config for the background thread

        thread::spawn(move || {
            let mut connection_retry_count = 0;
            let max_retries = 5; // Reduced from 10
            let retry_delay = Duration::from_secs(3); // Increased from 2 seconds

            while running.load(std::sync::atomic::Ordering::Relaxed) {
                // Try to create/recreate connection to server
                match Self::create_background_subscribers() {
                    Ok((
                        event_subscriber,
                        window_details_subscriber,
                        focus_subscriber,
                        heartbeat_subscriber,
                        response_subscriber,
                        window_list_subscriber,
                    )) => {
                        if connection_retry_count > 0 {
                            info!(
                                "✅ Successfully reconnected to e_grid server (attempt {})",
                                connection_retry_count + 1
                            );
                        } else {
                            info!("🔍 Background monitoring started - listening for real-time updates + focus events...");
                        }
                        connection_retry_count = 0; // Reset retry count on successful connection
                                                    // Main monitoring loop - process events while connected
                                                    // Create lock-free queues for event passing
                        let window_event_queue = Arc::new(ArrayQueue::new(1024));
                        let window_details_queue = Arc::new(ArrayQueue::new(1024));
                        let focus_event_queue = Arc::new(ArrayQueue::new(1024));

                        let monitoring_result = Self::run_monitoring_loop_with_queues(
                            &event_subscriber,
                            &window_details_subscriber,
                            &focus_subscriber,
                            &heartbeat_subscriber,
                            &response_subscriber,
                            &window_list_subscriber,
                            &windows,
                            &virtual_grid,
                            &monitors,
                            &auto_display,
                            &running,
                            &focus_callback,
                            &window_event_callback,
                            &move_resize_start_callback,
                            &move_resize_stop_callback,
                            &has_valid_grid_data,
                            &config,
                            &window_event_queue,
                            &window_details_queue,
                            &focus_event_queue,
                        );

                        match monitoring_result {
                            MonitoringResult::ServerDisconnected => {
                                warn!("⚠️ Lost connection to e_grid server - attempting to reconnect...");
                                connection_retry_count = 0; // Start fresh retry sequence
                            }
                            MonitoringResult::Shutdown => {
                                debug!("🛑 Monitoring shutdown requested");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        connection_retry_count += 1;
                        if connection_retry_count == 1 {
                            warn!("❌ Failed to connect to e_grid server: {}", e);
                            warn!(
                                "🔄 Will retry connection every {} seconds...",
                                retry_delay.as_secs()
                            );
                        } else if connection_retry_count <= max_retries {
                            debug!(
                                "🔄 Reconnection attempt {} failed, retrying in {} seconds...",
                                connection_retry_count,
                                retry_delay.as_secs()
                            );
                        } else {
                            error!(
                                "💀 Max reconnection attempts ({}) exceeded. Monitoring suspended.",
                                max_retries
                            );
                            error!("   Please ensure the e_grid server is running and restart the client.");
                            break;
                        }

                        // Wait before retrying
                        thread::sleep(retry_delay);
                    }
                }
            }

            debug!("🛑 Background monitoring stopped");
        });

        // Wait a moment for initial connection attempt before requesting data
        thread::sleep(Duration::from_millis(500));

        // Request initial data        debug!("📡 Requesting initial window data from server...");
        match self.request_window_list() {
            Ok(_) => debug!("✅ Window list request sent"),
            Err(e) => warn!("❌ Failed to send window list request: {}", e),
        }
        match self.request_grid_state() {
            Ok(_) => debug!("✅ Grid state request sent"),
            Err(e) => warn!("❌ Failed to send grid state request: {}", e),
        }
        debug!("📡 Initial data requests completed");
        Ok(())
    }

    /// Fetch window and monitor lists using the streaming protocol (Begin, Window/Monitor, End)
    pub fn fetch_window_and_monitor_lists_streaming(
        &mut self,
    ) -> Result<
        (
            Vec<crate::ipc_protocol::WindowDetails>,
            Vec<crate::ipc_protocol::Monitor>,
        ),
        String,
    > {
        use crate::ipc_protocol::{
            MonitorDetailsMessage, StreamControlMessage, StreamMsgType, WindowDetailsMessage,
        };
        // Set up subscribers for streaming topics
        let node = NodeBuilder::new()
            .create::<Service>()
            .map_err(|e| format!("Failed to create node: {e}"))?;
        let win_stream_service = node
            .service_builder(
                &ServiceName::new("e_grid_window_details_stream")
                    .map_err(|e| format!("Failed to create window details stream service: {e}"))?,
            )
            .publish_subscribe::<WindowDetailsMessage>()
            .open()
            .map_err(|e| format!("Failed to open window details stream service: {e}"))?;
        let mon_stream_service = node
            .service_builder(
                &ServiceName::new("e_grid_monitor_details_stream")
                    .map_err(|e| format!("Failed to create monitor details stream service: {e}"))?,
            )
            .publish_subscribe::<MonitorDetailsMessage>()
            .open()
            .map_err(|e| format!("Failed to open monitor details stream service: {e}"))?;
        let ctrl_stream_service = node
            .service_builder(
                &ServiceName::new("e_grid_stream_control")
                    .map_err(|e| format!("Failed to create stream control service: {e}"))?,
            )
            .publish_subscribe::<StreamControlMessage>()
            .open()
            .map_err(|e| format!("Failed to open stream control service: {e}"))?;
        let mut win_sub = win_stream_service
            .subscriber_builder()
            .create()
            .map_err(|e| format!("Failed to create window details stream subscriber: {e}"))?;
        let mut mon_sub = mon_stream_service
            .subscriber_builder()
            .create()
            .map_err(|e| format!("Failed to create monitor details stream subscriber: {e}"))?;
        let mut ctrl_sub = ctrl_stream_service
            .subscriber_builder()
            .create()
            .map_err(|e| format!("Failed to create stream control subscriber: {e}"))?;
        // Request window and monitor lists from server
        self.request_window_list()
            .map_err(|e| format!("Failed to request window list: {e}"))?;
        self.request_monitor_list()
            .map_err(|e| format!("Failed to request monitor list: {e}"))?;
        // Collect streamed window details
        let mut windows = Vec::new();
        let mut monitors = Vec::new();
        let mut in_window_stream = false;
        let mut in_monitor_stream = false;
        let start = std::time::Instant::now();
        while start.elapsed().as_secs() < 2 {
            // Control messages
            while let Some(ctrl) = ctrl_sub.receive().ok().flatten() {
                match ctrl.msg_type {
                    StreamMsgType::Begin => {
                        in_window_stream = true;
                        in_monitor_stream = true;
                    }
                    StreamMsgType::End => {
                        in_window_stream = false;
                        in_monitor_stream = false;
                    }
                    _ => {}
                }
            }
            // Window details
            if in_window_stream {
                while let Some(win_msg) = win_sub.receive().ok().flatten() {
                    if let StreamMsgType::Window = win_msg.msg_type {
                        windows.push(win_msg.details);
                    }
                }
            }
            // Monitor details
            if in_monitor_stream {
                while let Some(mon_msg) = mon_sub.receive().ok().flatten() {
                    if let StreamMsgType::Monitor = mon_msg.msg_type {
                        monitors.push(mon_msg.details);
                    }
                }
            }
            if !in_window_stream
                && !in_monitor_stream
                && (!windows.is_empty() || !monitors.is_empty())
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok((windows, monitors))
    }

    /// Rebuild grids from streamed window and monitor lists
    pub fn rebuild_grids_from_streamed_lists(
        &mut self,
        monitors: &[crate::ipc_protocol::Monitor],
        windows: &[crate::ipc_protocol::WindowDetails],
    ) {
        // Clear current state
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let idx = row * self.config.cols + col;
                self.virtual_grid[idx].store(GridCell {
                    state: ClientCellState::Empty,
                    monitor_ids: [0; 4],
                    monitor_count: 0,
                });
            }
        }
        self.windows.clear();
        self.monitors.clear();

        // Rebuild monitor list from streamed monitors
        for m in monitors {
            self.monitors.insert(
                m.id,
                MonitorGridInfo {
                    monitor_id: m.id,
                    width: m.width,
                    height: m.height,
                    x: m.x,
                    y: m.y,
                    rows: self.config.rows,
                    cols: self.config.cols,
                    grid: vec![vec![None; self.config.cols]; self.config.rows],
                },
            );
        }

        // Populate from streamed window details
        for w in windows {
            let info = ClientWindowInfo::from(*w);
            self.windows.insert(w.hwnd, info);

            // Clamp bounds to grid size
            let v_row_start = w.virtual_row_start.min((self.config.rows - 1) as u32);
            let v_row_end = w.virtual_row_end.min((self.config.rows - 1) as u32);
            let v_col_start = w.virtual_col_start.min((self.config.cols - 1) as u32);
            let v_col_end = w.virtual_col_end.min((self.config.cols - 1) as u32);

            // Update virtual grid: fill all cells in the rectangle
            for row in v_row_start..=v_row_end {
                for col in v_col_start..=v_col_end {
                    let idx = (row as usize) * self.config.cols + (col as usize);
                    if idx < self.virtual_grid.len() {
                        let mut cell = self.virtual_grid[idx].load();
                        cell.state = ClientCellState::Occupied(w.hwnd);
                        self.virtual_grid[idx].store(cell);
                    }
                }
            }

            // Update monitor grid: assign only if window's monitor_id matches this monitor
            if let Some(mut monitor) = self.monitors.get_mut(&w.monitor_id) {
                // Clamp monitor-relative bounds
                let m_row_start = w.monitor_row_start.min((self.config.rows - 1) as u32);
                let m_row_end = w.monitor_row_end.min((self.config.rows - 1) as u32);
                let m_col_start = w.monitor_col_start.min((self.config.cols - 1) as u32);
                let m_col_end = w.monitor_col_end.min((self.config.cols - 1) as u32);

                for row in m_row_start..=m_row_end {
                    for col in m_col_start..=m_col_end {
                        monitor.grid[row as usize][col as usize] = Some(w.hwnd);
                    }
                }
            }
        }
        self.has_valid_grid_data
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    // you can use lock-free ring buffers (e.g., crossbeam::ArrayQueue, heapless::spsc::Queue, or similar).
    // This requires changing your data structures to use these queues for communication.
    // Example: Replace Arc<Mutex<HashMap<...>>> with a lock-free queue for events.

    // For illustration, here's how you might change the function signature to use ring buffers:
    /// Main monitoring loop using lock-free queues for event passing.
    /// This function takes references to all subscribers and shared state, plus
    /// lock-free queues (crossbeam::ArrayQueue) for window events, window details, and focus events.
    fn run_monitoring_loop_with_queues(
        event_subscriber: &Subscriber<Service, WindowEvent, ()>,
        window_details_subscriber: &Subscriber<Service, WindowDetails, ()>,
        focus_subscriber: &Subscriber<Service, WindowFocusEvent, ()>,
        heartbeat_subscriber: &Subscriber<Service, HeartbeatMessage, ()>,
        response_subscriber: &Subscriber<Service, IpcResponse, ()>,
        window_list_subscriber: &Subscriber<Service, crate::ipc_protocol::WindowListMessage, ()>,
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        auto_display: &Arc<AtomicBool>,
        running: &Arc<AtomicBool>,
        focus_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowFocusEvent) + Send + Sync>>>>,
        window_event_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_resize_start_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_resize_stop_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        has_valid_grid_data: &Arc<AtomicBool>,
        config: &GridConfig,
        window_event_queue: &Arc<crossbeam_queue::ArrayQueue<WindowEvent>>,
        window_details_queue: &Arc<crossbeam_queue::ArrayQueue<WindowDetails>>,
        focus_event_queue: &Arc<crossbeam_queue::ArrayQueue<WindowFocusEvent>>,
    ) -> MonitoringResult {
        let mut consecutive_empty_cycles = 0;
        let max_empty_cycles = 200; // If no data for 200 cycles (10+ seconds), assume disconnection

        loop {
            if !running.load(std::sync::atomic::Ordering::Relaxed) {
                return MonitoringResult::Shutdown;
            }

            let mut _events_received = 0;
            let mut _details_received = 0;
            let mut _focus_events_received = 0;
            let mut had_activity = false; // Process window events (real-time, process all available)
            while let Some(event_sample) = event_subscriber.receive().unwrap_or(None) {
                let event = *event_sample;
                _events_received += 1;
                had_activity = true;
                println!(
                    "[DEBUG] Received WindowEvent: type={} hwnd={} row={} col={}",
                    event.event_type, event.hwnd, event.row, event.col
                );
                // Call user callback if set
                if let Ok(cb_lock) = window_event_callback.lock() {
                    if let Some(ref cb) = *cb_lock {
                        cb(event.clone());
                    }
                }
                if let Ok(cb_lock) = move_resize_start_callback.lock() {
                    if let Some(ref cb) = *cb_lock {
                        // Trigger for any start event: 4 (MoveStart), 6 (ResizeStart), 8 (BothStart)
                        if matches!(event.event_type, 4 | 6 | 8) {
                            debug!(
                                "📦 Move/resize start callback triggered for HWND {} (type={})",
                                event.hwnd, event.event_type
                            );
                            cb(event.clone());
                        }
                    }
                }
                if let Ok(cb_lock) = move_resize_stop_callback.lock() {
                    if let Some(ref cb) = *cb_lock {
                        // Trigger for any stop event: 5 (MoveStop), 7 (ResizeStop), 9 (BothStop)
                        if matches!(event.event_type, 5 | 7 | 9) {
                            cb(event.clone());
                        }
                    }
                }

                Self::handle_window_event(
                    &event,
                    windows,
                    virtual_grid,
                    monitors,
                    auto_display,
                    config,
                );

                // Add event to queue
                let _ = window_event_queue.push(event.clone());
            }

            // Process focus events (real-time, process all available)
            while let Some(focus_sample) = focus_subscriber.receive().unwrap_or(None) {
                let focus_event = *focus_sample;
                _focus_events_received += 1;
                had_activity = true;
                debug!(
                    "🎯 [FOCUS EVENT] HWND {} (PID: {}) at timestamp: {}",
                    focus_event.hwnd, focus_event.process_id, focus_event.timestamp
                );
                Self::handle_focus_event(&focus_event, focus_callback);

                // Add focus event to queue
                let _ = focus_event_queue.push(focus_event.clone());
            }

            // Process window details updates (real-time, process all available)
            while let Some(details_sample) = window_details_subscriber.receive().unwrap_or(None) {
                let details = *details_sample;
                _details_received += 1;
                had_activity = true;

                Self::handle_window_details(
                    &details,
                    windows,
                    virtual_grid,
                    monitors,
                    auto_display,
                    config,
                );

                // Add details to queue
                let _ = window_details_queue.push(details.clone());
            } // Process heartbeat messages to keep connection alive
            while let Some(heartbeat_sample) = heartbeat_subscriber.receive().unwrap_or(None) {
                let heartbeat = *heartbeat_sample;
                had_activity = true; // Reset disconnect counter on heartbeat

                // Check for shutdown heartbeat (iteration = 0)
                if heartbeat.server_iteration == 0 {
                    info!("💓 Received shutdown heartbeat from server - server is gracefully shutting down");
                    return MonitoringResult::ServerDisconnected;
                }
                // No need to log every normal heartbeat, just reset the timer
            }

            // Process IpcResponse messages (monitor list, etc)
            while let Some(response_sample) = response_subscriber.receive().unwrap_or(None) {
                let response = (*response_sample).clone();
                had_activity = true;
                // Handle monitor_list as a fixed-size array, using monitor_count
                // if let monitor_list= &response.monitor_list {
                //     if let Ok(mut monitors_lock) = monitors.lock() {
                //         monitors_lock.clear();
                //         for i in 0..monitor_list.monitor_count as usize {
                //             let m = &monitor_list.monitors[i];
                //             monitors_lock.push(MonitorGridInfo {
                //                 monitor_id: m.id,
                //                 width: m.width,
                //                 height: m.height,
                //                 x: m.x,
                //                 y: m.y,
                //                 grid: m
                //                     .grid
                //                     .iter()
                //                     .map(|row| {
                //                         row.iter()
                //                             .map(|&cell| if cell == 0 { None } else { Some(cell) })
                //                             .collect::<Vec<Option<u64>>>()
                //                     })
                //                     .collect::<Vec<Vec<Option<u64>>>>(),
                //             });
                //         }
                //         debug!(
                //             "[MONITOR LIST] Updated client monitor list: {} monitors",
                //             monitors_lock.len()
                //         );
                //     }
                // }
            }

            // Process window list messages (real-time, process all available)
            while let Some(window_list_sample) = window_list_subscriber.receive().unwrap_or(None) {
                let window_list = (*window_list_sample).clone();
                had_activity = true;
                
                println!(
                    "[WINDOW LIST] 🔥 RECEIVED window list: {} windows",
                    window_list.window_count
                );
                
                // Clear current window state and monitor grids
                windows.clear();
                
                // Clear virtual grid
                for idx in 0..virtual_grid.len() {
                    virtual_grid[idx].store(GridCell {
                        state: ClientCellState::Empty,
                        monitor_ids: [0; 4],
                        monitor_count: 0,
                    });
                }
                
                // Clear all monitor grids
                for mut monitor in monitors.iter_mut() {
                    for row in 0..monitor.grid.len() {
                        for col in 0..monitor.grid[row].len() {
                            monitor.grid[row][col] = None;
                        }
                    }
                }
                
                // Re-populate from window list
                for i in 0..window_list.window_count as usize {
                    let w = &window_list.windows[i];
                    let info = ClientWindowInfo {
                        hwnd: w.hwnd,
                        x: w.x,
                        y: w.y,
                        width: w.width,
                        height: w.height,
                        virtual_row_start: w.virtual_row_start,
                        virtual_row_end: w.virtual_row_end,
                        virtual_col_start: w.virtual_col_start,
                        virtual_col_end: w.virtual_col_end,
                        monitor_id: w.monitor_id,
                        monitor_row_start: w.monitor_row_start,
                        monitor_col_start: w.monitor_col_start,
                        monitor_row_end: w.monitor_row_end,
                        monitor_col_end: w.monitor_col_end,
                    };
                    windows.insert(w.hwnd, info);
                    
                    // Update virtual grid
                    for row in w.virtual_row_start..=w.virtual_row_end {
                        for col in w.virtual_col_start..=w.virtual_col_end {
                            if row < config.rows as u32 && col < config.cols as u32 {
                                let idx = (row as usize) * config.cols + (col as usize);
                                let mut cell = virtual_grid[idx].load();
                                cell.state = ClientCellState::Occupied(w.hwnd);
                                virtual_grid[idx].store(cell);
                            }
                        }
                    }
                    
                    // Update monitor grid (populate from window list data)
                    // First, ensure we have this monitor
                    if !monitors.contains_key(&w.monitor_id) {
                        monitors.insert(
                            w.monitor_id,
                            MonitorGridInfo {
                                monitor_id: w.monitor_id,
                                width: 1920, // Default values - will be updated by monitor list
                                height: 1080,
                                x: 0,
                                y: 0,
                                rows: config.rows,
                                cols: config.cols,
                                grid: vec![vec![None; config.cols]; config.rows],
                            },
                        );
                    }
                    
                    // Update monitor grid with window
                    if let Some(mut monitor) = monitors.get_mut(&w.monitor_id) {
                        for row in w.monitor_row_start..=w.monitor_row_end {
                            for col in w.monitor_col_start..=w.monitor_col_end {
                                if row < config.rows as u32 && col < config.cols as u32 {
                                    let hex_display = (w.hwnd % 100) as u8;
                                    println!("[WINDOW LIST] Assigning HWND {} (hex: {:02X}) to monitor {} grid cell [{}, {}]", w.hwnd, hex_display, w.monitor_id, row, col);
                                    monitor.grid[row as usize][col as usize] = Some(w.hwnd);
                                }
                            }
                        }
                    } else {
                        println!("[WINDOW LIST] WARNING: Monitor {} not found for window HWND {}", w.monitor_id, w.hwnd);
                    }
                    
                    println!(
                        "[WINDOW LIST] ✅ Added window: HWND {} at grid ({}-{}, {}-{})",
                        w.hwnd, w.virtual_row_start, w.virtual_row_end, 
                        w.virtual_col_start, w.virtual_col_end
                    );
                }
                
                println!(
                    "[WINDOW LIST] 🎯 Grid update complete: {} windows processed",
                    window_list.window_count
                );
                
                // Mark that we now have valid grid data
                has_valid_grid_data.store(true, std::sync::atomic::Ordering::Relaxed);
                println!("[WINDOW LIST] ✅ Valid grid data flag set to true");
                
                // Always display the grids after processing window list data
                println!("[WINDOW LIST] 📺 Displaying updated grids...");
                Self::display_current_grids(windows, virtual_grid, monitors, config);
                
                // Trigger auto-display if enabled
                if auto_display.load(std::sync::atomic::Ordering::Relaxed) {
                    println!("[WINDOW LIST] 📺 Auto-display flag is enabled");
                }
            }

            // Connection health monitoring
            if had_activity {
                consecutive_empty_cycles = 0; // Reset counter on activity
            } else {
                consecutive_empty_cycles += 1;
                if consecutive_empty_cycles >= max_empty_cycles {
                    warn!(
                        "⚠️ No data received for {} cycles - server may have disconnected",
                        consecutive_empty_cycles
                    );
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

                if LAST_STATUS_TIME.elapsed().as_secs() > 30 && !had_activity {
                    // Only during idle periods
                    let window_count = windows.len();
                    debug!("\n🔥 ===== CLIENT STATUS (IDLE) =====");
                    debug!("🔍 Monitoring: {} windows", window_count);
                    debug!("🎯 Focus events: enabled");
                    debug!("📡 Server connection: healthy");
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
        info!("✅ IPC client services started");
        Ok(())
    }
    /// Assign window to virtual grid (alias for existing method)
    pub fn assign_window_to_virtual_grid(
        &mut self,
        hwnd: u64,
        row: u32,
        col: u32,
    ) -> GridClientResult<()> {
        self.assign_window_to_virtual_cell(hwnd, row, col)
    }

    /// Animate window with specified duration and easing
    pub fn animate_window(
        &mut self,
        hwnd: u64,
        duration_ms: u32,
        easing: crate::EasingType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let easing_type = match easing {
            crate::EasingType::Linear => 0,
            crate::EasingType::EaseIn => 1,
            crate::EasingType::EaseOut => 2,
            crate::EasingType::EaseInOut => 3,
            crate::EasingType::Bounce => 4,
            crate::EasingType::Elastic => 5,
            crate::EasingType::Back => 6,
        };
        let command = IpcCommand {
            command_type: IpcCommandType::AnimateWindow,
            hwnd: Some(hwnd),
            target_row: None,
            target_col: None,
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: Some(duration_ms),
            easing_type: Some(easing_type),
            protocol_version: 1,
        };
        self.send_command(command)?;
        debug!("🎬 Animation command sent for window {}", hwnd);
        Ok(())
    }
    /// Move window to a specific grid cell (actually moves the window)
    pub fn move_window_to_cell(
        &mut self,
        hwnd: u64,
        row: u32,
        col: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let command = IpcCommand {
            command_type: IpcCommandType::MoveWindow,
            hwnd: Some(hwnd),
            target_row: Some(row),
            target_col: Some(col),
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.send_command(command)?;
        Ok(())
    }
    /// Register a callback to be called when window focus events occur
    /// This enables e_midi integration by allowing it to listen for focus changes
    pub fn set_focus_callback<F>(&mut self, callback: F) -> GridClientResult<()>
    where
        F: Fn(WindowFocusEvent) + Send + Sync + 'static,
    {
        let mut focus_callback_lock =
            safe_arc_lock(&self.focus_callback, "focus callback registration")?;
        *focus_callback_lock = Some(Box::new(callback));
        info!("🎯 Focus callback registered for e_midi integration");
        Ok(())
    }

    /// Remove the focus callback
    pub fn clear_focus_callback(&mut self) -> GridClientResult<()> {
        let mut focus_callback_lock =
            safe_arc_lock(&self.focus_callback, "focus callback clearing")?;
        *focus_callback_lock = None;
        info!("🎯 Focus callback cleared");
        Ok(())
    }

    /// Check if a focus callback is currently registered
    pub fn has_focus_callback(&self) -> bool {
        if let Ok(focus_callback_lock) = self.focus_callback.lock() {
            focus_callback_lock.is_some()
        } else {
            false
        }
    }

    fn create_background_subscribers() -> Result<
        (
            Subscriber<Service, WindowEvent, ()>,
            Subscriber<Service, WindowDetails, ()>,
            Subscriber<Service, WindowFocusEvent, ()>,
            Subscriber<Service, HeartbeatMessage, ()>,
            Subscriber<Service, IpcResponse, ()>,
            Subscriber<Service, crate::ipc_protocol::WindowListMessage, ()>,
        ),
        Box<dyn std::error::Error>,
    > {
        let node = NodeBuilder::new().create::<Service>()?;

        // Create event subscriber
        let event_service = node
            .service_builder(&ServiceName::new(GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowEvent>()
            .open()?;
        let event_subscriber = event_service.subscriber_builder().create()?;

        // Create window details subscriber
        let window_details_service = node
            .service_builder(&ServiceName::new(GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<WindowDetails>()
            .open()?;
        let window_details_subscriber = window_details_service.subscriber_builder().create()?;

        // Create focus events subscriber for e_midi integration
        let focus_service = node
            .service_builder(&ServiceName::new(GRID_FOCUS_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowFocusEvent>()
            .open()?;
        let focus_subscriber = focus_service.subscriber_builder().create()?;

        // Create heartbeat subscriber for connection monitoring
        let heartbeat_service = node
            .service_builder(&ServiceName::new(GRID_HEARTBEAT_SERVICE)?)
            .publish_subscribe::<HeartbeatMessage>()
            .open()?;
        let heartbeat_subscriber = heartbeat_service.subscriber_builder().create()?;

        // Create response subscriber for IpcResponse (monitor list, etc)
        let response_service = node
            .service_builder(&ServiceName::new(GRID_RESPONSE_SERVICE)?)
            .publish_subscribe::<IpcResponse>()
            .open()?;
        let response_subscriber = response_service.subscriber_builder().create()?;

        // Create window list subscriber for WindowListMessage
        println!("[DEBUG] Creating window list subscriber for service: {}", crate::ipc_protocol::GRID_WINDOW_LIST_SERVICE);
        let window_list_service = node
            .service_builder(&ServiceName::new(crate::ipc_protocol::GRID_WINDOW_LIST_SERVICE)?)
            .publish_subscribe::<crate::ipc_protocol::WindowListMessage>()
            .open()?;
        let window_list_subscriber = window_list_service.subscriber_builder().create()?;
        println!("[DEBUG] Window list subscriber created successfully");

        Ok((
            event_subscriber,
            window_details_subscriber,
            focus_subscriber,
            heartbeat_subscriber,
            response_subscriber,
            window_list_subscriber,
        ))
    }

    fn handle_window_event(
        event: &WindowEvent,
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        auto_display: &Arc<AtomicBool>,
        config: &GridConfig,
    ) {
        let event_name = match event.event_type {
            0 => "CREATED",
            1 => "DESTROYED",
            2 => "MOVED",
            3 => "STATE_CHANGED",
            4 => "MOVE_START",
            5 => "MOVE_STOP",
            6 => "RESIZE_START",
            7 => "RESIZE_STOP",
            _ => "UNKNOWN",
        };
        debug!(
            "[REAL-TIME EVENT] {}: HWND {} at ({}, {})",
            event_name, event.hwnd, event.row, event.col
        );
        match event.event_type {
            0 => {
                // Window created - we'll get window details shortly
                debug!(
                    "   New window {} created, waiting for details...",
                    event.hwnd
                );
            }
            1 => {
                // Window destroyed
                Self::remove_window_from_client(
                    event.hwnd,
                    windows,
                    virtual_grid,
                    monitors,
                    config,
                );
                debug!("   Removed window {} from client state", event.hwnd);
            }
            2 => {
                // Window moved - we'll get updated window details shortly
                debug!(
                    "   Window {} moved, waiting for updated details...",
                    event.hwnd
                );
            }
            4 => {
                // Move start
                debug!(
                    "   Window {} move started at ({}, {})",
                    event.hwnd, event.row, event.col
                );
            }
            5 => {
                // Move stop
                debug!(
                    "   Window {} move stopped at ({}, {})",
                    event.hwnd, event.row, event.col
                );
            }
            6 => {
                // Resize start
                debug!(
                    "   Window {} resize started at ({}, {})",
                    event.hwnd, event.row, event.col
                );
            }
            7 => {
                // Resize stop
                debug!(
                    "   Window {} resize stopped at ({}, {})",
                    event.hwnd, event.row, event.col
                );
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
            if auto_display.load(std::sync::atomic::Ordering::Relaxed) && (event.event_type == 0 || event.event_type == 1) && // Only for create/destroy
               LAST_EVENT_DISPLAY.elapsed().as_millis() > 500
            {
                // Max twice per second
                debug!("   Displaying grid after {} event...", event_name);
                // Use a display function compatible with lock-free types
                debug!("   (display_virtual_grid not implemented for lock-free types)");
                LAST_EVENT_DISPLAY = std::time::Instant::now();
            }
        }
    }
    /// Remove a window from all client state (lock-free version)
    fn remove_window_from_client(
        hwnd: u64,
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        config: &GridConfig,
    ) {
        // Remove from window cache
        windows.remove(&hwnd);

        // Remove from virtual grid
        let grid_size = config.rows * config.cols;
        for idx in 0..grid_size {
            let mut cell = virtual_grid[idx].load();
            if let ClientCellState::Occupied(existing_hwnd) = cell.state {
                if existing_hwnd == hwnd {
                    cell.state = ClientCellState::Empty;
                    virtual_grid[idx].store(cell);
                }
            }
        }

        // Remove from monitor grids
        for mut monitor in monitors.iter_mut() {
            for row in 0..monitor.grid.len() {
                for col in 0..monitor.grid[row].len() {
                    if monitor.grid[row][col] == Some(hwnd) {
                        monitor.grid[row][col] = None;
                    }
                }
            }
        }
    }
    fn handle_window_details(
        details: &WindowDetails,
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        auto_display: &Arc<AtomicBool>,
        config: &GridConfig,
    ) {
        debug!(
            "📊 [WINDOW UPDATE] HWND {} at ({}, {}) size {}x{}",
            details.hwnd, details.x, details.y, details.width, details.height
        );
        debug!(
            "   📍 Virtual Grid: ({}, {}) to ({}, {})",
            details.virtual_row_start,
            details.virtual_col_start,
            details.virtual_row_end,
            details.virtual_col_end
        );
        debug!(
            "   🖥️  Monitor {}: ({}, {}) to ({}, {})",
            details.monitor_id,
            details.monitor_row_start,
            details.monitor_col_start,
            details.monitor_row_end,
            details.monitor_col_end
        );

        // Update local window cache
        windows.insert(details.hwnd, ClientWindowInfo::from(*details));

        // Update virtual grid
        let grid_size = config.rows * config.cols;
        for row in 0..config.rows {
            for col in 0..config.cols {
                let idx = row * config.cols + col;
                let mut cell = virtual_grid[idx].load();
                if let ClientCellState::Occupied(existing_hwnd) = cell.state {
                    if existing_hwnd == details.hwnd {
                        cell.state = ClientCellState::Empty;
                        virtual_grid[idx].store(cell);
                    }
                }
            }
        }
        for row in details.virtual_row_start..=details.virtual_row_end {
            for col in details.virtual_col_start..=details.virtual_col_end {
                if row < config.rows as u32 && col < config.cols as u32 {
                    let idx = (row as usize) * config.cols + (col as usize);
                    let mut cell = virtual_grid[idx].load();
                    if let ClientCellState::OffScreen = cell.state {
                        // Don't overwrite off-screen markers
                    } else {
                        cell.state = ClientCellState::Occupied(details.hwnd);
                        virtual_grid[idx].store(cell);
                    }
                }
            }
        }

        // Update monitor grids
        if let Some(mut monitor) = monitors.get_mut(&details.monitor_id) {
            for row in 0..monitor.grid.len() {
                for col in 0..monitor.grid[row].len() {
                    if monitor.grid[row][col] == Some(details.hwnd) {
                        monitor.grid[row][col] = None;
                    }
                }
            }
            for row in details.monitor_row_start..=details.monitor_row_end {
                for col in details.monitor_col_start..=details.monitor_col_end {
                    if row < config.rows as u32 && col < config.cols as u32 {
                        monitor.grid[row as usize][col as usize] = Some(details.hwnd);
                    }
                }
            }
        }

        // Auto-display grid if enabled (but not too frequently)
        if auto_display.load(std::sync::atomic::Ordering::Relaxed) {
            static mut LAST_AUTO_DISPLAY: std::time::Instant = unsafe { std::mem::zeroed() };
            static mut AUTO_DISPLAY_INITIALIZED: bool = false;

            unsafe {
                if !AUTO_DISPLAY_INITIALIZED {
                    LAST_AUTO_DISPLAY = std::time::Instant::now();
                    AUTO_DISPLAY_INITIALIZED = true;
                }
                if LAST_AUTO_DISPLAY.elapsed().as_millis() > 1000 {
                    debug!("   🔄 Auto-displaying updated grid...");
                    // You may want to implement a lock-free display_virtual_grid for the new types
                    // For now, just log a message
                    debug!("   (display_virtual_grid not implemented for lock-free types)");
                    debug!("   ─────────────────────────────────────");
                    LAST_AUTO_DISPLAY = std::time::Instant::now();
                } else {
                    debug!(
                        "   ⏳ Auto-display throttled (last update {} ms ago)",
                        LAST_AUTO_DISPLAY.elapsed().as_millis()
                    );
                }
            }
        }
    }
    /// Handle focus events for e_midi integration
    fn handle_focus_event(
        focus_event: &WindowFocusEvent,
        focus_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowFocusEvent) + Send + Sync>>>>,
    ) {
        // Debug: Always log that we're handling a focus event
        let event_type = if focus_event.event_type == 0 {
            "FOCUSED"
        } else {
            "DEFOCUSED"
        };
        debug!(
            "🔍 [DEBUG] handle_focus_event called: {} window {}",
            event_type, focus_event.hwnd
        );

        // Invoke the callback if one is registered
        match safe_arc_lock(focus_callback, "focus event callback") {
            Ok(callback_lock) => {
                if let Some(ref callback) = *callback_lock {
                    debug!("🔍 [DEBUG] Calling focus callback for {} event", event_type);
                    callback(*focus_event);
                    debug!(
                        "🔍 [DEBUG] Focus callback completed for {} event",
                        event_type
                    );
                } else {
                    // Only log when no callback is registered (debugging)
                    info!(
                        "🎯 [FOCUS EVENT] {} window {} (no callback)",
                        event_type, focus_event.hwnd
                    );
                }
            }
            Err(e) => {
                warn!("⚠️ Failed to acquire focus callback lock: {}", e);
            }
        }
    }
    fn update_virtual_grid(
        details: &WindowDetails,
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
        let window_count = match windows.try_lock() {
            Ok(windows_lock) => windows_lock.len(),
            Err(_) => 0,
        };

        // Convert client grid to server grid format for display
        if let Ok(grid) = virtual_grid.try_lock() {
            let server_grid: Vec<Vec<crate::CellState>> = grid
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            ClientCellState::Empty => crate::CellState::Empty,
                            ClientCellState::Occupied(hwnd) => {
                                crate::CellState::Occupied(*hwnd as u64)
                            }
                            ClientCellState::OffScreen => crate::CellState::OffScreen,
                        })
                        .collect()
                })
                .collect();
            debug!("\n🔥 REAL-TIME GRID UPDATE:");

            // Use the centralized display function (server will provide bounds info)
            crate::grid_display::display_grid(
                &server_grid,
                config,
                window_count,
                &crate::grid_display::GridDisplayConfig::default(),
                Some("Client Grid Viewer"),
                None,
                None, // No bounds - let display function use defaults
            );
        }
    }
    fn update_monitor_grids(
        details: &WindowDetails,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
        config: &GridConfig,
    ) {
        if let Ok(mut monitors_lock) = monitors.lock() {
            // Ensure we have enough monitor grid entries (should have been initialized already)
            if details.monitor_id as usize >= monitors_lock.len() {
                warn!(
                    "⚠️ Monitor ID {} not found in initialized monitors (have {})",
                    details.monitor_id,
                    monitors_lock.len()
                );
                return;
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

            // Set new positions in monitor grid using the coordinates from the server
            for row in details.monitor_row_start..=details.monitor_row_end {
                for col in details.monitor_col_start..=details.monitor_col_end {
                    if row < config.rows as u32 && col < config.cols as u32 {
                        monitor.grid[row as usize][col as usize] = Some(details.hwnd);
                    }
                }
            }

            debug!(
                "🔧 Updated monitor {} grid: window {} assigned to cells ({},{}) to ({},{})",
                details.monitor_id,
                details.hwnd,
                details.monitor_row_start,
                details.monitor_col_start,
                details.monitor_row_end,
                details.monitor_col_end
            );
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
                warn!("⚠️ Windows cache locked, skipping grid display");
                return;
            }
        };

        // Convert client grid to server grid format for display
        if let Ok(grid) = virtual_grid.try_lock() {
            let server_grid: Vec<Vec<crate::CellState>> = grid
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            ClientCellState::Empty => crate::CellState::Empty,
                            ClientCellState::Occupied(hwnd) => {
                                crate::CellState::Occupied(*hwnd as u64)
                            }
                            ClientCellState::OffScreen => crate::CellState::OffScreen,
                        })
                        .collect()
                })
                .collect();
            // Use the centralized display function for consistency with server
            crate::grid_display::display_grid(
                &server_grid,
                &self.config,
                window_count,
                &crate::grid_display::GridDisplayConfig::default(),
                None,
                None, // Let display function determine dimensions
                None, // No specific bounds - use defaults
            );
        }

        // Display monitor grids using the centralized function like the server
        match monitors.try_lock() {
            Ok(monitors_lock) => {
                if !monitors_lock.is_empty() {
                    debug!("\n🖥️ Monitor Grids:");

                    for monitor in monitors_lock.iter() {
                        debug!(
                            "  Monitor {}: {}x{}",
                            monitor.monitor_id, monitor.width, monitor.height
                        );

                        // Convert monitor grid to server format
                        let mut server_monitor_grid =
                            vec![vec![crate::CellState::Empty; self.config.cols]; self.config.rows];
                        for row in 0..self.config.rows {
                            for col in 0..self.config.cols {
                                if row < monitor.grid.len() && col < monitor.grid[row].len() {
                                    server_monitor_grid[row][col] = match monitor.grid[row][col] {
                                        Some(hwnd) => crate::CellState::Occupied(hwnd as u64),
                                        None => crate::CellState::Empty,
                                    };
                                }
                            }
                        }
                        // Use centralized display for monitor grids
                        let monitor_title = format!("Monitor {} Grid", monitor.monitor_id);
                        let monitor_bounds = (
                            (monitor.x, monitor.y),
                            (monitor.x + monitor.width, monitor.y + monitor.height),
                        );
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
                warn!("⚠️ Monitor grids locked, skipping monitor grid display");
            }
        }
    }

    /// Display current grids (used from monitoring loop)
    fn display_current_grids(
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        config: &GridConfig,
    ) {
        let window_count = windows.len();
        
        // Display virtual grid
        let mut grid = vec![vec![ClientCellState::Empty; config.cols]; config.rows];
        for row in 0..config.rows {
            for col in 0..config.cols {
                let idx = row * config.cols + col;
                grid[row][col] = virtual_grid[idx].load().state;
            }
        }
        let server_grid: Vec<Vec<crate::CellState>> = grid
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        ClientCellState::Empty => crate::CellState::Empty,
                        ClientCellState::Occupied(hwnd) => crate::CellState::Occupied(*hwnd as u64),
                        ClientCellState::OffScreen => crate::CellState::OffScreen,
                    })
                    .collect()
            })
            .collect();
        
        println!("\n🔥 CLIENT VIRTUAL GRID:");
        crate::grid_display::display_grid(
            &server_grid,
            config,
            window_count,
            &crate::grid_display::GridDisplayConfig::default(),
            Some("Client Virtual Grid"),
            None,
            None,
        );
        
        // Display physical monitor grids
        if !monitors.is_empty() {
            for monitor in monitors.iter() {
                let mut server_monitor_grid =
                    vec![vec![crate::CellState::Empty; config.cols]; config.rows];
                for row in 0..config.rows {
                    for col in 0..config.cols {
                        server_monitor_grid[row][col] = match monitor.grid[row][col] {
                            Some(hwnd) => crate::CellState::Occupied(hwnd),
                            None => crate::CellState::Empty,
                        };
                    }
                }
                let monitor_title = format!("Client Monitor {} Grid", monitor.monitor_id);
                crate::grid_display::display_grid(
                    &server_monitor_grid,
                    config,
                    0,
                    &crate::grid_display::GridDisplayConfig::default(),
                    Some(&monitor_title),
                    Some((monitor.width, monitor.height)),
                    None,
                );
            }
        } else {
            println!("(No monitor grids available in client)");
        }
    }

    /// Display the current window list (similar to server's window list display)
    pub fn display_window_list(&self) {
        println!("\n📋 CLIENT WINDOW LIST:");
        println!("=====================");
        
        if self.windows.is_empty() {
            println!("(No windows tracked by client)");
            return;
        }
        
        println!("Windows tracked: {}", self.windows.len());
        
        let mut count = 0;
        for entry in self.windows.iter() {
            let (hwnd, window_info) = entry.pair();
            count += 1;
            
            println!("{}. HWND: {} (Monitor: {})", count, hwnd, window_info.monitor_id);
            println!("   Position: ({}, {}) Size: {}x{}", 
                window_info.x, window_info.y, window_info.width, window_info.height);
            println!("   Virtual Grid: ({},{}) to ({},{})", 
                window_info.virtual_row_start, window_info.virtual_col_start,
                window_info.virtual_row_end, window_info.virtual_col_end);
            println!("   Monitor Grid: ({},{}) to ({},{})", 
                window_info.monitor_row_start, window_info.monitor_col_start,
                window_info.monitor_row_end, window_info.monitor_col_end);
            
            if count >= 10 {
                let remaining = self.windows.len() - count;
                if remaining > 0 {
                    println!("   ... and {} more windows", remaining);
                }
                break;
            }
        }
        println!();
    }

    pub fn send_command(&mut self, command: IpcCommand) -> GridClientResult<()> {
        self.command_publisher
            .send_copy(command)
            .map(|_| ()) // Ignore the returned size, just return ()
            .map_err(|e| GridClientError::IpcError(format!("Failed to send command: {:?}", e)))
    }

    pub fn request_monitor_list(&mut self) -> GridClientResult<()> {
        let command = IpcCommand {
            command_type: IpcCommandType::GetMonitorList,
            hwnd: None,
            target_row: None,
            target_col: None,
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.send_command(command)
    }


    pub fn request_window_list(&mut self) -> GridClientResult<()> {
        let command = IpcCommand {
            command_type: IpcCommandType::GetWindowList,
            hwnd: None,
            target_row: None,
            target_col: None,
            monitor_id: None,
            layout_id: None,
                       animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.send_command(command)
    }

    pub fn request_grid_state(&mut self) -> GridClientResult<()> {
        let command = IpcCommand {
            command_type: IpcCommandType::GetGridState,
            hwnd: None,
            target_row: None,
            target_col: None,
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.send_command(command)
    }

    pub fn assign_window_to_virtual_cell(
        &mut self,
        hwnd: u64,
        row: u32,
        col: u32,
    ) -> GridClientResult<()> {
        // Validate coordinates
        validate_grid_coordinates(row, col, self.config.rows as u32, self.config.cols as u32)?;
        let command = IpcCommand {
            command_type: IpcCommandType::AssignToVirtualCell,
            hwnd: Some(hwnd),
            target_row: Some(row),
            target_col: Some(col),
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.send_command(command).map_err(|e| {
            GridClientError::IpcError(format!("Failed to assign window to virtual cell: {}", e))
        })
    }

    pub fn assign_window_to_monitor_cell(
        &mut self,
        hwnd: u64,
        row: u32,
        col: u32,
        monitor_id: u32,
    ) -> GridClientResult<()> {
        // Validate coordinates
        validate_grid_coordinates(row, col, self.config.rows as u32, self.config.cols as u32)?;
        let command = IpcCommand {
            command_type: IpcCommandType::AssignToMonitorCell,
            hwnd: Some(hwnd),
            target_row: Some(row),
            target_col: Some(col),
            monitor_id: Some(monitor_id),
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        self.send_command(command).map_err(|e| {
            GridClientError::IpcError(format!("Failed to assign window to monitor cell: {}", e))
        })
    }

    /// Display current grid state (alias for print_all_grids)
    pub fn display_current_grid(&self) {
        self.print_all_grids();
    }

    pub fn print_all_grids(&self) {
        // Print virtual grid
        println!();
        println!("=== VIRTUAL GRID (All Monitors Combined) ===");
        self.print_virtual_grid();
        
        // Print individual monitor grids
        for monitor in self.monitors.iter() {
            println!();
            println!("=== MONITOR {} GRID ===", monitor.monitor_id);
            println!(
                "Monitor bounds: ({}, {}) to ({}, {})",
                monitor.x, monitor.y,
                monitor.x + monitor.width as i32, monitor.y + monitor.height as i32
            );
            
            // Count windows on this monitor
            let mut windows_on_monitor = 0;
            let mut printed_windows = 0;
            for window in self.windows.iter() {
                let (hwnd, window_info) = window.pair();
                
                // Check if window is on this monitor
                if window_info.monitor_id == monitor.monitor_id {
                    windows_on_monitor += 1;
                    if printed_windows < 100 {
                        println!(
                            "Window HWND: 0x{:X} | Rect: ({}, {}) to ({}, {}) | Size: {}x{}",
                            hwnd,
                            window_info.x, window_info.y,
                            window_info.x + window_info.width as i32, window_info.y + window_info.height as i32,
                            window_info.width, window_info.height
                        );
                        println!(
                            "  └─ Monitor bounds: ({}, {}) to ({}, {}) | Size: {}x{}",
                            monitor.x, monitor.y,
                            monitor.x + monitor.width as i32, monitor.y + monitor.height as i32,
                            monitor.width, monitor.height
                        );
                        println!(
                            "  └─ Virtual grid cells: ({}, {}) to ({}, {})",
                            window_info.virtual_row_start, window_info.virtual_col_start,
                            window_info.virtual_row_end, window_info.virtual_col_end
                        );
                        println!(
                            "  └─ Monitor grid cells: ({}, {}) to ({}, {})",
                            window_info.monitor_row_start, window_info.monitor_col_start,
                            window_info.monitor_row_end, window_info.monitor_col_end
                        );
                        printed_windows += 1;
                    }
                }
            }
            if windows_on_monitor > 100 {
                println!("    ... and {} more windows", windows_on_monitor - 100);
            }
            println!("Windows on this monitor: {}", windows_on_monitor);
            println!(
                "Grid size: {} rows x {} cols ({} cells)",
                self.config.rows,
                self.config.cols,
                self.config.rows * self.config.cols
            );
            println!(
                "Monitor resolution: {}x{} px",
                monitor.width, monitor.height
            );
            
            // Print column headers
            print!("   ");
            for col in 0..self.config.cols {
                print!("{:2} ", col);
            }
            println!();
            
            // Print grid rows
            for row in 0..self.config.rows.min(32) {
                print!("{:2} ", row);
                for col in 0..self.config.cols.min(32) {
                    // Check what window (if any) occupies this cell
                    if let Some(hwnd) = monitor.grid[row][col] {
                        if hwnd == 0 {
                            print!(".. ");
                        } else {
                            print!("{:02X} ", hwnd & 0xFF);
                        }
                    } else {
                        print!(".. ");
                    }
                }
                println!();
            }
        }
        println!();
    }

    pub fn set_auto_display(&self, enabled: bool) {
        self.auto_display
            .store(enabled, std::sync::atomic::Ordering::Relaxed);
        info!(
            "🔄 Auto-display {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    pub fn is_auto_display_enabled(&self) -> bool {
        self.auto_display.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the current monitor data for real-time display (for TUI applications)
    pub fn get_monitor_data(&self) -> Vec<MonitorGridInfo> {
        self.monitors.iter().map(|kv| kv.value().clone()).collect()
    }

    /// Get the current window data for real-time display (for TUI applications)
    pub fn get_window_data(&self) -> HashMap<u64, ClientWindowInfo> {
        self.windows
            .iter()
            .map(|kv| (*kv.key(), kv.value().clone()))
            .collect()
    }

    /// Get the current virtual grid state for real-time display (for TUI applications)
    pub fn get_virtual_grid_state(&self) -> Vec<Vec<ClientCellState>> {
        {
            let grid_size = self.config.rows * self.config.cols;
            let mut grid = vec![vec![ClientCellState::Empty; self.config.cols]; self.config.rows];
            for row in 0..self.config.rows {
                for col in 0..self.config.cols {
                    let idx = row * self.config.cols + col;
                    grid[row][col] = self.virtual_grid[idx].load().state;
                }
            }
            grid
        }
    }

    /// Check if the client is connected and has recent data
    pub fn has_recent_data(&self) -> bool {
        let has_windows = !self.get_window_data().is_empty();
        let has_monitors = !self.get_monitor_data().is_empty();
        has_windows || has_monitors
    }

    pub fn stop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        info!("🛑 Stopping grid client...");
        // Send a Stop command to the server when stopping the client
        let command = IpcCommand {
            command_type: IpcCommandType::Stop,
            hwnd: None,
            target_row: None,
            target_col: None,
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: None,
            easing_type: None,
            protocol_version: 1,
        };
        let _ = self.send_command(command);
    }
        
}

impl Drop for GridClient {
    fn drop(&mut self) {
        self.stop();
    }
}
