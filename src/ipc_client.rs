use crate::grid_client_errors::{
    retry_with_backoff, safe_arc_lock, validate_grid_coordinates, GridClientError,
    GridClientResult, RetryConfig,
};
pub use crate::ipc_protocol::{
    HeartbeatMessage, IpcCommand, IpcCommandType, IpcResponse, WindowDetails, WindowEvent,
    WindowFocusEvent, GRID_COMMANDS_SERVICE, GRID_EVENTS_SERVICE, GRID_FOCUS_EVENTS_SERVICE,
    GRID_HEARTBEAT_SERVICE, GRID_RESPONSE_SERVICE, GRID_WINDOW_DETAILS_SERVICE,
};
use crate::{EasingType, GridConfig};
use crossbeam_utils::atomic::AtomicCell;
use dashmap::DashMap;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use iceoryx2::service::ipc::Service;
use log::{debug, error, info, warn};
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
    pub config: GridConfig,
    pub has_valid_grid_data: Arc<AtomicBool>,
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
    move_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    resize_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    pub virtual_grid: Arc<Vec<AtomicCell<GridCell>>>,
    pub physical_grids: Arc<Vec<AtomicCell<GridCell>>>,
    highlight_topmost: Arc<AtomicBool>,
}

#[derive(Clone, Debug)]
pub struct MonitorGridInfo {
    pub grid_type: crate::ipc_protocol::GridType,
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
    /// Register a callback to be called when window move events occur
    pub fn set_move_callback<F>(&mut self, callback: F) -> GridClientResult<()> 
    where
        F: Fn(WindowEvent) + Send + Sync + 'static,
    {
        let mut cb_lock = safe_arc_lock(&self.move_callback, "move callback registration")?;
        *cb_lock = Some(Box::new(callback));
        Ok(())
    }

    /// Register a callback to be called when window resize events occur
    pub fn set_resize_callback<F>(&mut self, callback: F) -> GridClientResult<()> 
    where
        F: Fn(WindowEvent) + Send + Sync + 'static,
    {
        let mut cb_lock = safe_arc_lock(&self.resize_callback, "resize callback registration")?;
        *cb_lock = Some(Box::new(callback));
        Ok(())
    }
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

    pub fn set_highlight_topmost(&mut self, highlight: bool) -> GridClientResult<()> {
        self.highlight_topmost
            .store(highlight, std::sync::atomic::Ordering::Relaxed);
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

        let monitor_list_service = node
            .service_builder(
                &ServiceName::new(crate::ipc_protocol::GRID_MONITOR_LIST_SERVICE).map_err(|e| {
                    GridClientError::IpcError(format!(
                        "Failed to create monitor list service name: {}",
                        e
                    ))
                })?,
            )
            .publish_subscribe::<crate::ipc_protocol::MonitorList>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()
            .map_err(|e| {
                GridClientError::IpcError(format!("Failed to create monitor list service: {}", e))
            })?;
        let monitor_list_subscriber = Some(monitor_list_service.subscriber_builder().create().map_err(|e| {
                    GridClientError::IpcError(format!(
                        "Failed to create monitor list subscriber: {:?} Check/delete: C:\\Temp\\iceoryx2",
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
            move_callback: Arc::new(Mutex::new(None)),
            resize_callback: Arc::new(Mutex::new(None)),
            physical_grids: Arc::new(physical_grids),
            window_list_subscriber,
            monitor_list_subscriber,
            highlight_topmost: Arc::new(AtomicBool::new(false)),
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
                right: winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_XVIRTUALSCREEN,
                ) + winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_CXVIRTUALSCREEN,
                ),
                bottom: winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_YVIRTUALSCREEN,
                ) + winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_CYVIRTUALSCREEN,
                ),
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
                    if cell_right > monitor_rect.left
                        && cell_left < monitor_rect.right
                        && cell_bottom > monitor_rect.top
                        && cell_top < monitor_rect.bottom
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
    /// Static version of initialize_offscreen_cells for use in monitoring loop
    fn initialize_offscreen_cells_static(
        monitor_list: &crate::ipc_protocol::MonitorList,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        config: &GridConfig,
    ) {
        // Get virtual screen bounds (like server does)
        let virtual_rect = unsafe {
            winapi::shared::windef::RECT {
                left: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_XVIRTUALSCREEN),
                top: winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_YVIRTUALSCREEN),
                right: winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_XVIRTUALSCREEN,
                ) + winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_CXVIRTUALSCREEN,
                ),
                bottom: winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_YVIRTUALSCREEN,
                ) + winapi::um::winuser::GetSystemMetrics(
                    winapi::um::winuser::SM_CYVIRTUALSCREEN,
                ),
            }
        };

        let cell_width = (virtual_rect.right - virtual_rect.left) / config.cols as i32;
        let cell_height = (virtual_rect.bottom - virtual_rect.top) / config.rows as i32;

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
            println!(
                "[OFFSCREEN] Monitor {}: ({}, {}) to ({}, {})",
                m.monitor_id,
                m.x,
                m.y,
                m.x + m.width,
                m.y + m.height
            );
        }

        println!(
            "[OFFSCREEN] Virtual screen: ({}, {}) to ({}, {})",
            virtual_rect.left, virtual_rect.top, virtual_rect.right, virtual_rect.bottom
        );
        println!("[OFFSCREEN] Cell size: {}x{}", cell_width, cell_height);

        // Initialize all cells based on whether they're on an actual monitor
        for row in 0..config.rows {
            for col in 0..config.cols {
                let idx = row * config.cols + col;
                let current_cell = virtual_grid[idx].load();

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
                    if cell_right > monitor_rect.left
                        && cell_left < monitor_rect.right
                        && cell_bottom > monitor_rect.top
                        && cell_top < monitor_rect.bottom
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

                if matches!(current_cell.state, ClientCellState::Empty)
                    || matches!(current_cell.state, ClientCellState::OffScreen)
                {
                    virtual_grid[idx].store(GridCell {
                        state: new_state,
                        monitor_ids: current_cell.monitor_ids,
                        monitor_count: current_cell.monitor_count,
                    });

                    if new_state == ClientCellState::OffScreen {
                        println!("[OFFSCREEN] Cell [{}, {}] marked as offscreen", row, col);
                    }
                }
            }
        }

        println!("[OFFSCREEN] Virtual grid offscreen initialization complete");
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
        let move_callback = self.move_callback.clone();
        let resize_callback = self.resize_callback.clone();
        let has_valid_grid_data = self.has_valid_grid_data.clone();
        let config = self.config.clone();

        thread::spawn(move || {
            let mut connection_retry_count = 0;
            let max_retries = 5;
            let retry_delay = Duration::from_secs(3);

            while running.load(std::sync::atomic::Ordering::Relaxed) {
                match Self::create_background_subscribers() {
                    Ok((
                        event_subscriber,
                        window_details_subscriber,
                        focus_subscriber,
                        heartbeat_subscriber,
                        response_subscriber,
                        window_list_subscriber,
                        monitor_list_subscriber,
                    )) => {
                        if connection_retry_count > 0 {
                            info!(
                                "✅ Successfully reconnected to e_grid server (attempt {})",
                                connection_retry_count + 1
                            );
                        } else {
                            info!("🔍 Background monitoring started - listening for real-time updates + focus events...");
                        }
                        connection_retry_count = 0;

                        let monitoring_result = Self::run_monitoring_loop(
                            &event_subscriber,
                            &window_details_subscriber,
                            &focus_subscriber,
                            &heartbeat_subscriber,
                            &response_subscriber,
                            &window_list_subscriber,
                            &monitor_list_subscriber,
                            &windows,
                            &virtual_grid,
                            &monitors,
                            &auto_display,
                            &running,
                            &focus_callback,
                            &window_event_callback,
                            &move_resize_start_callback,
                            &move_resize_stop_callback,
                            &move_callback,
                            &resize_callback,
                            &has_valid_grid_data,
                            &config,
                        );

                        match monitoring_result {
                            MonitoringResult::ServerDisconnected => {
                                warn!("⚠️ Lost connection to e_grid server - attempting to reconnect...");
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
                            break;
                        }
                        thread::sleep(retry_delay);
                    }
                }
            }
            debug!("🛑 Background monitoring stopped");
        });

        // Wait for initial connection attempt
        thread::sleep(Duration::from_millis(500));

        // Request MONITOR DATA FIRST AND WAIT FOR IT
        println!("📡 Requesting MONITOR DATA FIRST (required before window data)...");

        for attempt in 1..=5 {
            match self.request_monitor_list() {
                Ok(_) => {
                    println!("✅ Monitor list request #{} sent", attempt);
                    break;
                }
                Err(e) => {
                    println!("❌ Failed to send monitor list request #{}: {}", attempt, e);
                    if attempt < 5 {
                        thread::sleep(Duration::from_millis(500));
                    }
                }
            }
        }

        // Give server time to process and respond
        thread::sleep(Duration::from_millis(1000));

        // Now request window data
        println!("📡 Now requesting window data...");
        match self.request_window_list() {
            Ok(_) => println!("✅ Window list request sent"),
            Err(e) => println!("❌ Failed to send window list request: {}", e),
        }

        match self.request_grid_state() {
            Ok(_) => println!("✅ Grid state request sent"),
            Err(e) => println!("❌ Failed to send grid state request: {}", e),
        }

        Ok(())
    }

    fn create_background_subscribers() -> Result<
        (
            Subscriber<Service, WindowEvent, ()>,
            Subscriber<Service, WindowDetails, ()>,
            Subscriber<Service, WindowFocusEvent, ()>,
            Subscriber<Service, HeartbeatMessage, ()>,
            Subscriber<Service, IpcResponse, ()>,
            Subscriber<Service, crate::ipc_protocol::WindowListMessage, ()>,
            Subscriber<Service, crate::ipc_protocol::MonitorList, ()>,
        ),
        Box<dyn std::error::Error>,
    > {
        let node = NodeBuilder::new().create::<Service>()?;

        let event_service = node
            .service_builder(&ServiceName::new(GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowEvent>()
            .open()?;
        let event_subscriber = event_service.subscriber_builder().create()?;

        let window_details_service = node
            .service_builder(&ServiceName::new(GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<WindowDetails>()
            .open()?;
        let window_details_subscriber = window_details_service.subscriber_builder().create()?;

        let focus_service = node
            .service_builder(&ServiceName::new(GRID_FOCUS_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowFocusEvent>()
            .open()?;
        let focus_subscriber = focus_service.subscriber_builder().create()?;

        let heartbeat_service = node
            .service_builder(&ServiceName::new(GRID_HEARTBEAT_SERVICE)?)
            .publish_subscribe::<HeartbeatMessage>()
            .open()?;
        let heartbeat_subscriber = heartbeat_service.subscriber_builder().create()?;

        let response_service = node
            .service_builder(&ServiceName::new(GRID_RESPONSE_SERVICE)?)
            .publish_subscribe::<IpcResponse>()
            .open()?;
        let response_subscriber = response_service.subscriber_builder().create()?;

        let window_list_service = node
            .service_builder(&ServiceName::new(
                crate::ipc_protocol::GRID_WINDOW_LIST_SERVICE,
            )?)
            .publish_subscribe::<crate::ipc_protocol::WindowListMessage>()
            .open()?;
        let window_list_subscriber = window_list_service.subscriber_builder().create()?;

        let monitor_list_service = node
            .service_builder(&ServiceName::new(
                crate::ipc_protocol::GRID_MONITOR_LIST_SERVICE,
            )?)
            .publish_subscribe::<crate::ipc_protocol::MonitorList>()
            .open()?;
        let monitor_list_subscriber = monitor_list_service.subscriber_builder().create()?;

        Ok((
            event_subscriber,
            window_details_subscriber,
            focus_subscriber,
            heartbeat_subscriber,
            response_subscriber,
            window_list_subscriber,
            monitor_list_subscriber,
        ))
    }

    fn run_monitoring_loop(
        event_subscriber: &Subscriber<Service, WindowEvent, ()>,
        window_details_subscriber: &Subscriber<Service, WindowDetails, ()>,
        focus_subscriber: &Subscriber<Service, WindowFocusEvent, ()>,
        heartbeat_subscriber: &Subscriber<Service, HeartbeatMessage, ()>,
        response_subscriber: &Subscriber<Service, IpcResponse, ()>,
        window_list_subscriber: &Subscriber<Service, crate::ipc_protocol::WindowListMessage, ()>,
        monitor_list_subscriber: &Subscriber<Service, crate::ipc_protocol::MonitorList, ()>,
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        auto_display: &Arc<AtomicBool>,
        running: &Arc<AtomicBool>,
        focus_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowFocusEvent) + Send + Sync>>>>,
        window_event_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_resize_start_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_resize_stop_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        resize_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        has_valid_grid_data: &Arc<AtomicBool>,
        config: &GridConfig,
    ) -> MonitoringResult {
        let mut consecutive_empty_cycles = 0;
        let max_empty_cycles = 200;
        let mut pending_window_list: Option<crate::ipc_protocol::WindowListMessage> = None;
        let mut monitor_request_timer = std::time::Instant::now();
        let mut last_debug_time = std::time::Instant::now();

        // Add event filtering to reduce stray events
        let mut last_move_resize_event: Option<(u64, u8, std::time::Instant)> = None; // (hwnd, event_type, timestamp)
        let event_debounce_ms = 100; // Ignore duplicate events within 100ms

        loop {
            if !running.load(std::sync::atomic::Ordering::Relaxed) {
                return MonitoringResult::Shutdown;
            }

            let mut had_activity = false;

            // Debug status every 5 seconds
            if last_debug_time.elapsed().as_secs() >= 5 {
                // println!(
                //     "[MONITORING] 📊 Status: monitors={}, windows={}, pending_windows={}",
                //     monitors.len(),
                //     windows.len(),
                //     pending_window_list.as_ref().map_or(0, |w| w.window_count)
                // );
                last_debug_time = std::time::Instant::now();
            }

            // CRITICAL: Process window events FIRST (this was missing!)
            while let Some(event_sample) = event_subscriber.receive().unwrap_or(None) {
                let window_event = *event_sample;
                had_activity = true;

                let should_process = match window_event.event_type.clone() {
                    crate::EVENT_TYPE_WINDOW_MOVE_START
                    | crate::EVENT_TYPE_WINDOW_MOVE_STOP
                    | crate::EVENT_TYPE_WINDOW_MOVE
                    | crate::EVENT_TYPE_WINDOW_RESIZE
                    | crate::EVENT_TYPE_WINDOW_RESIZE_START
                    | crate::EVENT_TYPE_WINDOW_RESIZE_STOP => {
                        // Move/resize events
                        let now = std::time::Instant::now();
                        let should_skip = if let Some((last_hwnd, last_type, last_time)) =
                            last_move_resize_event
                        {
                            last_hwnd == window_event.hwnd
                                && last_type == window_event.event_type
                                && now.duration_since(last_time).as_millis() < event_debounce_ms
                        } else {
                            false
                        };

                        if should_skip {
                            println!(
                                "🚫 [CLIENT] Skipping duplicate event: type={} hwnd={} (within {}ms)",
                                window_event.event_type, window_event.hwnd, event_debounce_ms
                            );
                            false
                        } else {
                            last_move_resize_event =
                                Some((window_event.hwnd, window_event.event_type, now));
                            true
                        }
                    }
                    _ => true, // Process all other events normally
                };

                if !should_process {
                    continue;
                }

                println!(
                    "🔥 [CLIENT] Received window event: type={} hwnd={}",
                    window_event.event_type, window_event.hwnd
                );

                // Call window event callback
                if let Ok(cb_lock) = window_event_callback.lock() {
                    if let Some(ref cb) = *cb_lock {
                        cb(window_event);
                    }
                }

                // Call specific move/resize callbacks based on event type
                match window_event.event_type {
                    crate::EVENT_TYPE_WINDOW_CREATED => {
                        println!("🆕 [CLIENT] Window created event");
                    }
                    crate::EVENT_TYPE_WINDOW_MOVE => {
                        println!("🚚 [CLIENT] Window moved event");
                        if let Ok(cb_lock) = move_callback.lock() {
                            if let Some(ref cb) = *cb_lock {
                                cb(window_event);
                            } else {
                                println!("⚠️ [CLIENT] Move callback is None!");
                            }
                        }
                    }
                    crate::EVENT_TYPE_WINDOW_MOVE_START | crate::EVENT_TYPE_WINDOW_RESIZE_START => {
                        println!(
                            "🚀 [CLIENT] Triggering move/resize START callback for event type {}",
                            window_event.event_type
                        );
                        if let Ok(cb_lock) = move_resize_start_callback.lock() {
                            if let Some(ref cb) = *cb_lock {
                                cb(window_event);
                            } else {
                                println!("⚠️ [CLIENT] Move/resize start callback is None!");
                            }
                        }
                    }
                    crate::EVENT_TYPE_WINDOW_MOVE_STOP | crate::EVENT_TYPE_WINDOW_RESIZE_STOP => {
                        println!(
                            "🏁 [CLIENT] Triggering move/resize STOP callback for event type {}",
                            window_event.event_type
                        );
                        if let Ok(cb_lock) = move_resize_stop_callback.lock() {
                            if let Some(ref cb) = *cb_lock {
                                cb(window_event);
                            } else {
                                println!("⚠️ [CLIENT] Move/resize stop callback is None!");
                            }
                        }
                    }
                    crate::EVENT_TYPE_WINDOW_RESIZE => {
                        println!("📐 [CLIENT] Triggering RESIZE callback for event type {}", window_event.event_type);
                        if let Ok(cb_lock) = resize_callback.lock() {
                            if let Some(ref cb) = *cb_lock {
                                cb(window_event);
                            } else {
                                println!("⚠️ [CLIENT] Resize callback is None!");
                            }
                        }
                    }
                    _ => {
                        println!(
                            "ℹ️ [CLIENT] Other window event type: {}",
                            window_event.event_type
                        );
                    }
                }
            }

            // If no monitors received yet, keep requesting them
            if monitors.is_empty() && monitor_request_timer.elapsed().as_secs() >= 2 {
                println!("[MONITORING] ⚠️ Still no monitors received - requesting again...");
                // Note: Can't call self.request_monitor_list() from here since we don't have self
                // The background thread needs to handle this differently
                monitor_request_timer = std::time::Instant::now();
            }

            // Process monitor list messages FIRST - this is CRITICAL
            while let Some(monitor_list_sample) = monitor_list_subscriber.receive().unwrap_or(None)
            {
                let monitor_list = (*monitor_list_sample).clone();
                had_activity = true;

                println!(
                    "[MONITOR LIST] 🔥 FINALLY RECEIVED monitor list: {} monitors",
                    monitor_list.monitor_count
                );

                // Debug the actual monitor data we received
                for i in 0..monitor_list.monitor_count as usize {
                    let m = &monitor_list.monitors[i];
                    println!(
                        "[MONITOR LIST] 🖥️ Monitor {}: ID={}, {}x{} at ({},{})",
                        i, m.monitor_id, m.width, m.height, m.x, m.y
                    );
                }

                monitors.clear();
                for i in 0..monitor_list.monitor_count as usize {
                    let m = &monitor_list.monitors[i];
                    let rows = config.rows;
                    let cols = config.cols;
                    let grid = vec![vec![None; cols]; rows];

                    monitors.insert(
                        m.monitor_id,
                        MonitorGridInfo {
                            grid_type: m.grid_type,
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
                    println!("[MONITOR LIST] ✅ Added monitor {} to client", m.monitor_id);
                }

                Self::initialize_offscreen_cells_static(&monitor_list, virtual_grid, config);

                // NOW process any pending window data since we have monitors
                if let Some(window_list) = pending_window_list.take() {
                    println!(
                        "[MONITOR LIST] 🔄 NOW processing pending window list with {} windows",
                        window_list.window_count
                    );
                    Self::process_window_list(
                        &window_list,
                        windows,
                        virtual_grid,
                        monitors,
                        config,
                        has_valid_grid_data,
                    );
                }

                println!(
                    "[MONITOR LIST] 🎉 Monitor processing complete - client now has {} monitors",
                    monitors.len()
                );
            }

            // Process window list messages - but ONLY if we have monitors
            while let Some(window_list_sample) = window_list_subscriber.receive().unwrap_or(None) {
                let window_list = (*window_list_sample).clone();
                had_activity = true;

                println!(
                    "[WINDOW LIST] 🔥 RECEIVED window list: {} windows",
                    window_list.window_count
                );

                if monitors.is_empty() {
                    println!("[WINDOW LIST] ❌ BLOCKING: No monitor data available yet - storing window list for later");
                    pending_window_list = Some(window_list);
                    continue;
                }

                println!("[WINDOW LIST] ✅ We have monitors - processing window list immediately");
                Self::process_window_list(
                    &window_list,
                    windows,
                    virtual_grid,
                    monitors,
                    config,
                    has_valid_grid_data,
                );
            }

            // Process other events (focus, heartbeat, etc.)
            while let Some(focus_sample) = focus_subscriber.receive().unwrap_or(None) {
                let focus_event = *focus_sample;
                had_activity = true;
                if let Ok(cb_lock) = focus_callback.lock() {
                    if let Some(ref cb) = *cb_lock {
                        cb(focus_event);
                    }
                }
            }

            while let Some(heartbeat_sample) = heartbeat_subscriber.receive().unwrap_or(None) {
                let heartbeat = *heartbeat_sample;
                had_activity = true;
                if heartbeat.server_iteration == 0 {
                    return MonitoringResult::ServerDisconnected;
                }
            }

            // Connection health monitoring
            if had_activity {
                consecutive_empty_cycles = 0;
            } else {
                consecutive_empty_cycles += 1;
                if consecutive_empty_cycles >= max_empty_cycles {
                    return MonitoringResult::ServerDisconnected;
                }
            }

            thread::sleep(Duration::from_millis(50));
        }
    }

    fn process_window_list(
        window_list: &crate::ipc_protocol::WindowListMessage,
        windows: &Arc<DashMap<u64, ClientWindowInfo>>,
        virtual_grid: &Arc<Vec<AtomicCell<GridCell>>>,
        monitors: &Arc<DashMap<u32, MonitorGridInfo>>,
        config: &GridConfig,
        has_valid_grid_data: &Arc<AtomicBool>,
    ) {
        println!(
            "[WINDOW LIST] 📋 Processing window list with {} windows",
            window_list.window_count
        );

        // Get desktop HWND for filtering (same as server)
        let desktop_hwnd = unsafe { winapi::um::winuser::GetDesktopWindow() } as u64;
        println!(
            "[WINDOW LIST] 🖥️ Desktop HWND: 0x{:X} (will be filtered out)",
            desktop_hwnd
        );

        // Clear current window data
        windows.clear();

        // Clear virtual grid but preserve OffScreen cells
        for idx in 0..virtual_grid.len() {
            let current_cell = virtual_grid[idx].load();
            let new_state = match current_cell.state {
                ClientCellState::OffScreen => ClientCellState::OffScreen,
                _ => ClientCellState::Empty,
            };
            virtual_grid[idx].store(GridCell {
                state: new_state,
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

        // Get z-order map to determine topmost windows like the server does
        let z_order_map = crate::util::get_hwnd_z_order_map();
        println!(
            "[WINDOW LIST] 🔍 Z-order map has {} entries for topmost calculation",
            z_order_map.len()
        );

        // First pass: Collect all valid windows (filtering desktop HWND like server)
        let mut valid_windows = Vec::new();
        let mut filtered_count = 0;

        for i in 0..window_list.window_count as usize {
            let w = &window_list.windows[i];

            // Filter out desktop HWND like the server does
            if w.hwnd == desktop_hwnd {
                filtered_count += 1;
                println!("[WINDOW LIST] 🚫 Filtered out desktop HWND 0x{:X}", w.hwnd);
                continue;
            }

            valid_windows.push(w);
            let info = ClientWindowInfo::from(*w);
            windows.insert(w.hwnd, info);

            println!("[WINDOW LIST] ✅ Valid window HWND 0x{:X} on monitor {} at virtual ({},{}) to ({},{}) | monitor ({},{}) to ({},{})", 
                     w.hwnd, w.monitor_id, w.virtual_row_start, w.virtual_col_start, w.virtual_row_end, w.virtual_col_end,
                     w.monitor_row_start, w.monitor_col_start, w.monitor_row_end, w.monitor_col_end);
        }

        println!(
            "[WINDOW LIST] 📊 After filtering: {} valid windows, {} filtered out",
            valid_windows.len(),
            filtered_count
        );

        // Second pass: Build grids using z-order priority (same algorithm as server)
        let mut virtual_grid_updates = 0;
        let mut monitor_grid_updates = 0;

        for w in &valid_windows {
            // Update virtual grid with z-order consideration
            let mut cells_updated_for_this_window = 0;
            for row in w.virtual_row_start..=w.virtual_row_end {
                for col in w.virtual_col_start..=w.virtual_col_end {
                    if row < config.rows as u32 && col < config.cols as u32 {
                        let idx = (row as usize) * config.cols + (col as usize);
                        let current_cell = virtual_grid[idx].load();

                        if !matches!(current_cell.state, ClientCellState::OffScreen) {
                            // Check if this window should replace the current one based on z-order
                            let should_update = match current_cell.state {
                                ClientCellState::Empty => true,
                                ClientCellState::Occupied(current_hwnd) => {
                                    // Use z-order to determine topmost (lower z-order = more topmost)
                                    let current_z = z_order_map
                                        .get(&current_hwnd)
                                        .copied()
                                        .unwrap_or(usize::MAX);
                                    let new_z =
                                        z_order_map.get(&w.hwnd).copied().unwrap_or(usize::MAX);
                                    let wins = new_z < current_z;
                                    // if wins {
                                    //     println!("[WINDOW LIST] 🔄 Cell [{},{}]: 0x{:X} (z={}) replaces 0x{:X} (z={})", 
                                    //             row, col, w.hwnd, new_z, current_hwnd, current_z);
                                    // }
                                    wins
                                }
                                ClientCellState::OffScreen => false,
                            };

                            if should_update {
                                virtual_grid[idx].store(GridCell {
                                    state: ClientCellState::Occupied(w.hwnd),
                                    monitor_ids: current_cell.monitor_ids,
                                    monitor_count: current_cell.monitor_count,
                                });
                                cells_updated_for_this_window += 1;
                                virtual_grid_updates += 1;
                            }
                        }
                    }
                }
            }

            // Update monitor grid with z-order consideration
            if let Some(mut monitor) = monitors.get_mut(&w.monitor_id) {
                let mut monitor_cells_updated = 0;
                for row in w.monitor_row_start..=w.monitor_row_end {
                    for col in w.monitor_col_start..=w.monitor_col_end {
                        if row < config.rows as u32 && col < config.cols as u32 {
                            let current_hwnd = monitor.grid[row as usize][col as usize];

                            // Check if this window should replace the current one based on z-order
                            let should_update = match current_hwnd {
                                None => true,
                                Some(current) => {
                                    // Use z-order to determine topmost (lower z-order = more topmost)
                                    let current_z =
                                        z_order_map.get(&current).copied().unwrap_or(usize::MAX);
                                    let new_z =
                                        z_order_map.get(&w.hwnd).copied().unwrap_or(usize::MAX);
                                    let wins = new_z < current_z;
                                    // if wins {
                                    //     println!("[WINDOW LIST] 🔄 Monitor {} Cell [{},{}]: 0x{:X} (z={}) replaces 0x{:X} (z={})", 
                                    //             w.monitor_id, row, col, w.hwnd, new_z, current, current_z);
                                    // }
                                    wins
                                }
                            };

                            if should_update {
                                monitor.grid[row as usize][col as usize] = Some(w.hwnd);
                                monitor_cells_updated += 1;
                                monitor_grid_updates += 1;
                            }
                        }
                    }
                }
                println!(
                    "[WINDOW LIST] 🖥️  Updated {} monitor {} grid cells for HWND 0x{:X}",
                    monitor_cells_updated, w.monitor_id, w.hwnd
                );
            } else {
                println!(
                    "[WINDOW LIST] ⚠️ Monitor {} not found for window HWND 0x{:X}",
                    w.monitor_id, w.hwnd
                );
            }
        }

        has_valid_grid_data.store(true, std::sync::atomic::Ordering::Relaxed);
        println!("[WINDOW LIST] ✅ Grid data marked as valid - {} valid windows processed (after filtering), {} virtual cells updated, {} monitor cells updated", 
                 valid_windows.len(), virtual_grid_updates, monitor_grid_updates);

        // Debug: Check virtual grid state after processing
        let mut final_occupied = 0;
        let mut final_empty = 0;
        let mut final_offscreen = 0;
        for idx in 0..virtual_grid.len() {
            match virtual_grid[idx].load().state {
                ClientCellState::Occupied(_) => final_occupied += 1,
                ClientCellState::Empty => final_empty += 1,
                ClientCellState::OffScreen => final_offscreen += 1,
            }
        }
        println!(
            "[WINDOW LIST] 🔍 Final virtual grid state: {} occupied, {} empty, {} offscreen",
            final_occupied, final_empty, final_offscreen
        );
    }

    /// Request grid state from server
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

    /// Register a callback to be called when window focus events occur
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

    /// Display the current window list
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

            println!(
                "{}. HWND: {} (Monitor: {})",
                count, hwnd, window_info.monitor_id
            );
            println!(
                "   Position: ({}, {}) Size: {}x{}",
                window_info.x, window_info.y, window_info.width, window_info.height
            );
            println!(
                "   Virtual Grid: ({},{}) to ({},{})",
                window_info.virtual_row_start,
                window_info.virtual_col_start,
                window_info.virtual_row_end,
                window_info.virtual_col_end
            );
            println!(
                "   Monitor Grid: ({},{}) to ({},{})",
                window_info.monitor_row_start,
                window_info.monitor_col_start,
                window_info.monitor_row_end,
                window_info.monitor_col_end
            );

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

    /// Display current grid state (alias for print_all_grids)
    pub fn display_current_grid(&self) {
        self.print_all_grids();
    }

    /// Get the current grid configuration
    pub fn get_config(&self) -> &GridConfig {
        &self.config
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

    /// Print the current virtual grid (all windows, all monitors combined)
    pub fn print_virtual_grid(&self) {
        let has_valid_data = self
            .has_valid_grid_data
            .load(std::sync::atomic::Ordering::Relaxed);
        let monitor_count = self.monitors.len();
        let window_count = self.windows.len();

        println!(
            "🔍 [DEBUG] Virtual grid status: valid_data={}, monitors={}, windows={}",
            has_valid_data, monitor_count, window_count
        );

        if !has_valid_data && monitor_count == 0 {
            println!("(Grid not available yet: waiting for data from server)");
            return;
        }

        // Find the topmost window using the same approach as the server
        let mut topmost_hwnd: Option<u64> = None;
        if self
            .highlight_topmost
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            let z_order_map = crate::util::get_hwnd_z_order_map();
            let mut topmost_z: Option<usize> = None;

            println!("🔍 [DEBUG] Z-order map has {} entries", z_order_map.len());

            // Get all windows from the client's window list (same as server receives)
            for window in self.windows.iter() {
                let (hwnd, _) = window.pair();
                if let Some(&z) = z_order_map.get(hwnd) {
                    println!("🔍 [DEBUG] Window 0x{:X} has z-order {}", hwnd, z);
                    if topmost_z.map_or(true, |tz| z < tz) {
                        topmost_hwnd = Some(*hwnd);
                        topmost_z = Some(z);
                    }
                } else {
                    println!("🔍 [DEBUG] Window 0x{:X} not found in z-order map", hwnd);
                }
            }

            if let Some(topmost) = topmost_hwnd {
                println!(
                    "🔍 [DEBUG] Client topmost window: 0x{:X} (z-order: {})",
                    topmost,
                    topmost_z.unwrap_or(0)
                );
            } else {
                println!("🔍 [DEBUG] No topmost window found by client");
            }
        }

        // Convert virtual grid to display format
        let mut grid = vec![vec![ClientCellState::Empty; self.config.cols]; self.config.rows];
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let idx = row * self.config.cols + col;
                grid[row][col] = self.virtual_grid[idx].load().state;
            }
        }

        // Debug what's actually in the virtual grid
        let mut occupied_cells = 0;
        let mut offscreen_cells = 0;
        let mut empty_cells = 0;
        let mut sample_hwnds = Vec::new();

        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                match grid[row][col] {
                    ClientCellState::Occupied(hwnd) => {
                        occupied_cells += 1;
                        if sample_hwnds.len() < 5 {
                            sample_hwnds.push(hwnd);
                        }
                    }
                    ClientCellState::OffScreen => offscreen_cells += 1,
                    ClientCellState::Empty => empty_cells += 1,
                }
            }
        }
        println!(
            "🔍 [DEBUG] Virtual grid contents: {} occupied, {} offscreen, {} empty",
            occupied_cells, offscreen_cells, empty_cells
        );

        if !sample_hwnds.is_empty() {
            print!("🔍 [DEBUG] Sample HWNDs in virtual grid: ");
            for hwnd in &sample_hwnds {
                print!("0x{:X} ", hwnd);
            }
            println!();
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
            topmost_hwnd,
        );
    }

    pub fn print_all_grids(&self) {
        let has_valid_data = self
            .has_valid_grid_data
            .load(std::sync::atomic::Ordering::Relaxed);
        let monitor_count = self.monitors.len();
        let window_count = self.windows.len();

        println!(
            "🔍 [DEBUG] All grids status: valid_data={}, monitors={}, windows={}",
            has_valid_data, monitor_count, window_count
        );

        // If we have monitor data, we can display grids even if valid_data flag isn't set
        if monitor_count == 0 {
            println!("(Grids not available yet: waiting for monitor data from server)");
            println!("💡 Make sure the e_grid server is running!");
            return;
        }

        // Find the topmost window using the SAME approach as the server
        let mut topmost_hwnd: Option<u64> = None;
        if self
            .highlight_topmost
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            let z_order_map = crate::util::get_hwnd_z_order_map();
            let mut topmost_z: Option<usize> = None;

            // Use the same logic as the server: check all windows in our window list
            for window in self.windows.iter() {
                let (hwnd, _) = window.pair();
                if let Some(&z) = z_order_map.get(hwnd) {
                    if topmost_z.map_or(true, |tz| z < tz) {
                        topmost_hwnd = Some(*hwnd);
                        topmost_z = Some(z);
                    }
                }
            }

            if let Some(topmost) = topmost_hwnd {
                println!(
                    "🔍 [DEBUG] Client determined topmost window: 0x{:X} (z-order: {})",
                    topmost,
                    topmost_z.unwrap_or(0)
                );
            }
        }

        // Print virtual grid - call the actual method
        println!();
        println!("=== VIRTUAL GRID (All Monitors Combined) ===");
        self.print_virtual_grid();

        // Print individual monitor grids with the SAME topmost highlighting
        for monitor in self.monitors.iter() {
            println!();
            println!("=== MONITOR {} GRID ===", monitor.monitor_id);
            println!(
                "Monitor bounds: ({}, {}) to ({}, {})",
                monitor.x,
                monitor.y,
                monitor.x + monitor.width as i32,
                monitor.y + monitor.height as i32
            );

            // Count windows on this monitor and debug their HWNDs
            let mut windows_on_monitor = 0;
            let mut monitor_hwnds = Vec::new();
            for window in self.windows.iter() {
                let (hwnd, window_info) = window.pair();
                if window_info.monitor_id == monitor.monitor_id {
                    windows_on_monitor += 1;
                    monitor_hwnds.push(*hwnd);
                }
            }
            println!("Windows on this monitor: {}", windows_on_monitor);
            if !monitor_hwnds.is_empty() {
                print!("HWNDs on this monitor: ");
                for hwnd in &monitor_hwnds {
                    print!("0x{:X} ", hwnd);
                }
                println!();
            }

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
                        if hwnd == 0 || hwnd == u64::MAX {
                            print!("XX ");
                        } else {
                            // Check if this is the topmost window and should be highlighted
                            if self
                                .highlight_topmost
                                .load(std::sync::atomic::Ordering::Relaxed)
                                && topmost_hwnd == Some(hwnd)
                            {
                                print!("\x1b[31m{:02X}\x1b[0m ", hwnd & 0xFF); // Red foreground
                            } else {
                                print!("{:02X} ", hwnd & 0xFF);
                            }
                        }
                    } else {
                        print!(".. ");
                    }
                }
                println!();
            }
        }

        // Print legend if highlighting is enabled and we found a topmost window
        if self
            .highlight_topmost
            .load(std::sync::atomic::Ordering::Relaxed)
            && topmost_hwnd.is_some()
        {
            println!();
            println!("Legend: \x1b[31mRed\x1b[0m = Topmost window (lowest Z-order)");
        }
        println!();
    }

    /// Shutdown the client and stop background monitoring
    pub fn shutdown(&mut self) {
        info!("🛑 GridClient shutdown requested");

        // Signal the background thread to stop
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // Clear all data
        self.windows.clear();
        self.monitors.clear();

        // Reset grid state
        let grid_size = self.config.rows * self.config.cols;
        for idx in 0..grid_size {
            self.virtual_grid[idx].store(GridCell {
                state: ClientCellState::Empty,
                monitor_ids: [0; 4],
                monitor_count: 0,
            });
        }

        // Clear callbacks
        if let Ok(mut cb) = self.focus_callback.lock() {
            *cb = None;
        }
        if let Ok(mut cb) = self.window_event_callback.lock() {
            *cb = None;
        }
        if let Ok(mut cb) = self.move_resize_start_callback.lock() {
            *cb = None;
        }
        if let Ok(mut cb) = self.move_resize_stop_callback.lock() {
            *cb = None;
        }

        // Reset flags
        self.has_valid_grid_data
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.auto_display
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.highlight_topmost
            .store(false, std::sync::atomic::Ordering::Relaxed);

        info!("✅ GridClient shutdown complete");
    }

    /// Check if the client is still running
    pub fn is_running(&self) -> bool {
        self.running.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Get a copy of all monitor data
    pub fn get_monitor_data(&self) -> Vec<MonitorGridInfo> {
        self.monitors
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a copy of all window data
    pub fn get_window_data(&self) -> Vec<ClientWindowInfo> {
        self.windows
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get monitor list (alias for get_monitor_data)
    pub fn get_monitor_list(&self) -> Vec<MonitorGridInfo> {
        self.get_monitor_data()
    }
    /// Get the current virtual grid state for inspection
    pub fn get_virtual_grid_state(&self) -> Vec<Vec<ClientCellState>> {
        let mut grid = vec![vec![ClientCellState::Empty; self.config.cols]; self.config.rows];
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let idx = row * self.config.cols + col;
                grid[row][col] = self.virtual_grid[idx].load().state;
            }
        }
        grid
    }
    /// Move window to grid cell with animation
    pub fn move_window_to_cell(
        &mut self,
        hwnd: u64,
        row: u32,
        col: u32,
        duration_ms: u32,
        easing: EasingType,
    ) -> GridClientResult<()> {
        // Validate coordinates
        validate_grid_coordinates(row, col, self.config.rows as u32, self.config.cols as u32)?;
        let command = IpcCommand {
            command_type: IpcCommandType::MoveWindowToCell,
            hwnd: Some(hwnd),
            target_row: Some(row),
            target_col: Some(col),
            monitor_id: None,
            layout_id: None,
            animation_duration_ms: Some(duration_ms),
            easing_type: Some(easing),
            protocol_version: 1,
        };
        self.send_command(command)
            .map_err(|e| GridClientError::IpcError(format!("Failed to move window to cell: {}", e)))
    }
}
