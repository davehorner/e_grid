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
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use iceoryx2::service::ipc::Service;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
    pub title_len: u32,
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
            title_len: details.title_len,
        }
    }
}

pub struct GridClient {
    // Configuration
    config: GridConfig,

    // IPC components - only keep what we need for sending commands
    command_publisher: Publisher<Service, IpcCommand, ()>,
    window_list_subscriber: Option<Subscriber<Service, crate::ipc_protocol::WindowListMessage, ()>>,
    // Local grid state
    windows: Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
    virtual_grid: Arc<Mutex<Vec<Vec<ClientCellState>>>>,

    // Monitor information - store complete monitor grids
    monitors: Arc<Mutex<Vec<MonitorGridInfo>>>,

    // Control flags
    auto_display: Arc<Mutex<bool>>,
    running: Arc<Mutex<bool>>,

    // NEW: Focus event handling for e_midi integration
    focus_callback: Arc<Mutex<Option<Box<dyn Fn(WindowFocusEvent) + Send + Sync>>>>,
    // NEW: Window event callback for demo logging
    window_event_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    move_resize_start_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
    move_resize_stop_callback: Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
}

#[derive(Clone, Debug)]
pub struct MonitorGridInfo {
    pub monitor_id: u32,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
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
        match self.virtual_grid.try_lock() {
            Ok(grid) => Ok(grid.clone()),
            Err(_) => Err("Failed to acquire lock on virtual_grid".to_string()),
        }
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
            window_event_callback: Arc::new(Mutex::new(None)),
            move_resize_start_callback: Arc::new(Mutex::new(None)),
            move_resize_stop_callback: Arc::new(Mutex::new(None)),
            window_list_subscriber,
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


    /// Rebuilds the virtual and physical (monitor) grids from a WindowListMessage
    pub fn rebuild_grids_from_window_list(&mut self, window_list: &crate::ipc_protocol::WindowListMessage) {
        // Clear current state
        if let Ok(mut grid) = self.virtual_grid.lock() {
            for row in 0..self.config.rows {
                for col in 0..self.config.cols {
                    grid[row][col] = ClientCellState::Empty;
                }
            }
        }
        if let Ok(mut windows) = self.windows.lock() {
            windows.clear();
        }
        if let Ok(mut monitors) = self.monitors.lock() {
            monitors.clear();
                // Rebuild monitor list from WindowListMessage if present
    for i in 0..window_list.monitor_count as usize {
        let m = &window_list.monitors[i];
        monitors.push(MonitorGridInfo { 
            monitor_id: m.id,
            width: m.width,
            height: m.height,
            x: m.x,
            y: m.y,
            grid: vec![vec![None; self.config.cols]; self.config.rows],
        });
    }
        }
        // Re-populate from window list
        for i in 0..window_list.window_count as usize {
            let w = &window_list.windows[i];
            let info = ClientWindowInfo::from(*w);
            if let Ok(mut windows) = self.windows.lock() {
                windows.insert(w.hwnd, info);
            }
            // Update virtual grid
            if let Ok(mut grid) = self.virtual_grid.lock() {
                for row in w.virtual_row_start..=w.virtual_row_end {
                    for col in w.virtual_col_start..=w.virtual_col_end {
                        if row < self.config.rows as u32 && col < self.config.cols as u32 {
                            grid[row as usize][col as usize] = ClientCellState::Occupied(w.hwnd);
                        }
                    }
                }
            }
            // Update monitor grid
            if let Ok(mut monitors) = self.monitors.lock() {
                if (w.monitor_id as usize) < monitors.len() {
                    let monitor = &mut monitors[w.monitor_id as usize];
                    for row in w.monitor_row_start..=w.monitor_row_end {
                        for col in w.monitor_col_start..=w.monitor_col_end {
                            if row < self.config.rows as u32 && col < self.config.cols as u32 {
                                monitor.grid[row as usize][col as usize] = Some(w.hwnd);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Print the current virtual grid (all windows, all monitors combined)
    pub fn print_virtual_grid(&self) {
        let window_count = match self.windows.try_lock() {
            Ok(windows_lock) => windows_lock.len(),
            Err(_) => 0,
        };
        if let Ok(grid) = self.virtual_grid.try_lock() {
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
    }

    /// Print all physical (per-monitor) grids
    pub fn print_physical_grids(&self) {
        if let Ok(monitors_lock) = self.monitors.try_lock() {
            if monitors_lock.is_empty() {
                println!("(No monitor grids available)");
                return;
            }
            for monitor in monitors_lock.iter() {
                let mut server_monitor_grid = vec![vec![crate::CellState::Empty; self.config.cols]; self.config.rows];
                for row in 0..self.config.rows {
                    for col in 0..self.config.cols {
                        server_monitor_grid[row][col] = match monitor.grid[row][col] {
                            Some(hwnd) => crate::CellState::Occupied(hwnd),
                            None => crate::CellState::Empty,
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
        } else {
            println!("(Monitor grids locked, cannot display)");
        }
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
        // Initialize basic client grid structure - monitor details will come from server
        debug!(
            "🔧 Initializing client grid structure {}x{}",
            self.config.rows, self.config.cols
        );

        {
            let mut grid = safe_arc_lock(&self.virtual_grid, "virtual grid initialization")?;
            // Initialize all cells as empty - server will provide actual on-screen/off-screen status
            for row in 0..self.config.rows {
                for col in 0..self.config.cols {
                    grid[row][col] = ClientCellState::Empty;
                }
            }
        }

        // Initialize empty monitor list - will be populated from server data
        {
            let mut monitors_lock = safe_arc_lock(&self.monitors, "monitors initialization")?;
            monitors_lock.clear();
            debug!("🔧 Client grid initialized - awaiting server data for monitor layouts");
        }

        Ok(())
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
        let config = self.config.clone(); // Clone the config for the background thread

        thread::spawn(move || {
            let mut connection_retry_count = 0;
            let max_retries = 5; // Reduced from 10
            let retry_delay = Duration::from_secs(3); // Increased from 2 seconds

            while *running.lock().unwrap() {
                // Try to create/recreate connection to server
                match Self::create_background_subscribers() {
                    Ok((
                        event_subscriber,
                        window_details_subscriber,
                        focus_subscriber,
                        heartbeat_subscriber,
                        response_subscriber,
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
                            &windows,
                            &virtual_grid,
                            &monitors,
                            &auto_display,
                            &running,
                            &focus_callback,
                            &window_event_callback,
                            &move_resize_start_callback,
                            &move_resize_stop_callback,
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

    // If you want to avoid Arc<Mutex<...>> locking for sharing data between threads,
    // you can use lock-free ring buffers (e.g., crossbeam::ArrayQueue, heapless::spsc::Queue, or similar).
    // This requires changing your data structures to use these queues for communication.
    // Example: Replace Arc<Mutex<HashMap<...>>> with a lock-free queue for events.

    // For illustration, here's how you might change the function signature to use ring buffers:
    fn run_monitoring_loop_with_queues(
        event_subscriber: &Subscriber<Service, WindowEvent, ()>,
        window_details_subscriber: &Subscriber<Service, WindowDetails, ()>,
        focus_subscriber: &Subscriber<Service, WindowFocusEvent, ()>,
        heartbeat_subscriber: &Subscriber<Service, HeartbeatMessage, ()>,
        response_subscriber: &Subscriber<Service, IpcResponse, ()>,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
        auto_display: &Arc<Mutex<bool>>,
        running: &Arc<Mutex<bool>>,
        focus_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowFocusEvent) + Send + Sync>>>>,
        window_event_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_resize_start_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        move_resize_stop_callback: &Arc<Mutex<Option<Box<dyn Fn(WindowEvent) + Send + Sync>>>>,
        config: &GridConfig,
        window_event_queue: &Arc<ArrayQueue<WindowEvent>>,
        window_details_queue: &Arc<ArrayQueue<WindowDetails>>,
        focus_event_queue: &Arc<ArrayQueue<WindowFocusEvent>>,
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
                if let monitor_list= &response.monitor_list {
                    if let Ok(mut monitors_lock) = monitors.lock() {
                        monitors_lock.clear();
                        for i in 0..monitor_list.monitor_count as usize {
                            let m = &monitor_list.monitors[i];
                            monitors_lock.push(MonitorGridInfo {
                                monitor_id: m.id,
                                width: m.width,
                                height: m.height,
                                x: m.x,
                                y: m.y,
                                grid: m
                                    .grid
                                    .iter()
                                    .map(|row| {
                                        row.iter()
                                            .map(|&cell| if cell == 0 { None } else { Some(cell) })
                                            .collect::<Vec<Option<u64>>>()
                                    })
                                    .collect::<Vec<Vec<Option<u64>>>>(),
                            });
                        }
                        debug!(
                            "[MONITOR LIST] Updated client monitor list: {} monitors",
                            monitors_lock.len()
                        );
                    }
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
                    let window_count = {
                        let windows_lock = windows.lock().unwrap();
                        windows_lock.len()
                    }; // Release lock immediately
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

        Ok((
            event_subscriber,
            window_details_subscriber,
            focus_subscriber,
            heartbeat_subscriber,
            response_subscriber,
        ))
    }

    fn handle_window_event(
        event: &WindowEvent,
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
                Self::remove_window_from_client(event.hwnd, windows, virtual_grid, monitors);
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
            if *auto_display.lock().unwrap() &&               (event.event_type == 0 || event.event_type == 1) && // Only for create/destroy
               LAST_EVENT_DISPLAY.elapsed().as_millis() > 500
            {
                // Max twice per second
                debug!("   Displaying grid after {} event...", event_name);
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
        details: &WindowDetails,
        windows: &Arc<Mutex<HashMap<u64, ClientWindowInfo>>>,
        virtual_grid: &Arc<Mutex<Vec<Vec<ClientCellState>>>>,
        monitors: &Arc<Mutex<Vec<MonitorGridInfo>>>,
        auto_display: &Arc<Mutex<bool>>,
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
        if let Ok(mut windows_lock) = windows.lock() {
            let window_info = ClientWindowInfo::from(*details);
            windows_lock.insert(details.hwnd, window_info);
        }

        // Update virtual grid
        Self::update_virtual_grid(&details, &virtual_grid);
        // Update monitor grids
        Self::update_monitor_grids(&details, &monitors, config); // Auto-display grid if enabled (but not too frequently)
        if *auto_display.lock().unwrap() {
            static mut LAST_AUTO_DISPLAY: std::time::Instant = unsafe { std::mem::zeroed() };
            static mut AUTO_DISPLAY_INITIALIZED: bool = false;

            unsafe {
                if !AUTO_DISPLAY_INITIALIZED {
                    LAST_AUTO_DISPLAY = std::time::Instant::now();
                    AUTO_DISPLAY_INITIALIZED = true;
                } // Only auto-display if it's been at least 1 second since last display
                if LAST_AUTO_DISPLAY.elapsed().as_millis() > 1000 {
                    debug!("   🔄 Auto-displaying updated grid...");
                    Self::display_virtual_grid(&virtual_grid, &windows, config);
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

    pub fn send_command(&mut self, command: IpcCommand) -> GridClientResult<()> {
        self.command_publisher
            .send_copy(command)
            .map(|_| ()) // Ignore the returned size, just return ()
            .map_err(|e| GridClientError::IpcError(format!("Failed to send command: {:?}", e)))
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

    pub fn display_current_grid(&self) {
        self.display_complete_grid(&self.virtual_grid, &self.windows, &self.monitors);
    }

    pub fn display_window_list(&self) {
        debug!("\n📋 Current Windows:");
        debug!("{}", "-".repeat(80));

        if let Ok(windows) = self.windows.lock() {
            if windows.is_empty() {
                debug!("   (No windows currently tracked)");
            } else {
                for (i, (hwnd, info)) in windows.iter().enumerate() {
                    debug!(
                        "   [{}] HWND: {} | Position: ({}, {}) | Size: {}x{}",
                        i + 1,
                        hwnd,
                        info.x,
                        info.y,
                        info.width,
                        info.height
                    );
                    debug!(
                        "       Virtual: ({},{}) to ({},{}) | Monitor {}: ({},{}) to ({},{})",
                        info.virtual_row_start,
                        info.virtual_col_start,
                        info.virtual_row_end,
                        info.virtual_col_end,
                        info.monitor_id,
                        info.monitor_row_start,
                        info.monitor_col_start,
                        info.monitor_row_end,
                        info.monitor_col_end
                    );
                }
            }
        }
        debug!("{}", "-".repeat(80));
    }

    pub fn set_auto_display(&self, enabled: bool) {
        *self.auto_display.lock().unwrap() = enabled;
        info!(
            "🔄 Auto-display {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    pub fn is_auto_display_enabled(&self) -> bool {
        self.auto_display.lock().map(|auto| *auto).unwrap_or(false)
    }

    /// Get the current monitor data for real-time display (for TUI applications)
    pub fn get_monitor_data(&self) -> Vec<MonitorGridInfo> {
        match self.monitors.try_lock() {
            Ok(monitors) => monitors.clone(),
            Err(_) => Vec::new(),
        }
    }

    /// Get the current window data for real-time display (for TUI applications)
    pub fn get_window_data(&self) -> HashMap<u64, ClientWindowInfo> {
        match self.windows.try_lock() {
            Ok(windows) => windows.clone(),
            Err(_) => HashMap::new(),
        }
    }

    /// Get the current virtual grid state for real-time display (for TUI applications)
    pub fn get_virtual_grid_state(&self) -> Vec<Vec<ClientCellState>> {
        match self.virtual_grid.try_lock() {
            Ok(grid) => grid.clone(),
            Err(_) => Vec::new(),
        }
    }

    /// Check if the client is connected and has recent data
    pub fn has_recent_data(&self) -> bool {
        let has_windows = !self.get_window_data().is_empty();
        let has_monitors = !self.get_monitor_data().is_empty();
        has_windows || has_monitors
    }

    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
        info!("🛑 Stopping grid client...");
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
}

impl Drop for GridClient {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
