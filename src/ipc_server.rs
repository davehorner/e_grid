use crate::ipc_protocol::{
    AnimationCommand, AnimationStatus, GridCellAssignment, GridEvent, GridLayoutMessage, GridState, HeartbeatMessage, IpcCommand, IpcCommandType, IpcResponse, IpcResponseType, MonitorList, WindowDetails, WindowEvent, WindowFocusEvent, WindowListMessage, ANIMATION_COMMANDS_SERVICE, ANIMATION_STATUS_SERVICE, GRID_CELL_ASSIGNMENTS_SERVICE, GRID_COMMANDS_SERVICE, GRID_EVENTS_SERVICE, GRID_FOCUS_EVENTS_SERVICE, GRID_HEARTBEAT_SERVICE, GRID_LAYOUT_SERVICE, GRID_RESPONSE_SERVICE, GRID_WINDOW_DETAILS_SERVICE, GRID_WINDOW_LIST_SERVICE, MAX_WINDOWS
};
use crate::WindowInfo;
// use crate::GridConfig;
use crate::config::grid_config::GridConfig;
use crate::ipc_protocol::GridCommand;
use crate::{
    heartbeat::HeartbeatService,
    window_events::{self, WindowEventConfig},
    WindowTracker,
};
use dashmap::DashMap;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use iceoryx2::service::ipc::Service;
use log::{debug, error, info, trace, warn};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use winapi::shared::windef::HWND;
use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

/// Dedicated IPC Server for E-Grid system
/// Manages all server-side IPC communications including:
/// - Publishing window events (create, move, destroy)
/// - Publishing individual window details for real-time updates
/// - Processing client commands (window assignment, grid requests)
/// - Publishing responses to client requests
pub struct GridIpcServer {
    // Core window tracker (still keep for other logic)
    tracker: Arc<Mutex<WindowTracker>>,
    // Lock-free window state for event system
    windows: Arc<DashMap<u64, crate::WindowInfo>>,
    config: GridConfig,

    // IPC Publishers
    event_publisher: Option<Publisher<Service, WindowEvent, ()>>,
    response_publisher: Option<Publisher<Service, IpcResponse, ()>>,
    window_details_publisher: Option<Publisher<Service, WindowDetails, ()>>,
    focus_publisher: Option<Publisher<Service, WindowFocusEvent, ()>>,
    layout_publisher: Option<Publisher<Service, GridLayoutMessage, ()>>,
    cell_assignment_publisher: Option<Publisher<Service, GridCellAssignment, ()>>,
    animation_status_publisher: Option<Publisher<Service, AnimationStatus, ()>>,
    heartbeat_publisher: Option<Publisher<Service, HeartbeatMessage, ()>>,
    window_list_publisher: Option<Publisher<Service, WindowListMessage, ()>>,
    
    // IPC Subscribers
    command_subscriber: Option<Subscriber<Service, IpcCommand, ()>>,
    layout_subscriber: Option<Subscriber<Service, GridLayoutMessage, ()>>,
    cell_assignment_subscriber: Option<Subscriber<Service, GridCellAssignment, ()>>,
    animation_subscriber: Option<Subscriber<Service, AnimationCommand, ()>>,
    // Server state
    is_running: bool,
    event_listeners: Vec<Box<dyn Fn(&GridEvent) + Send + Sync>>,

    // New library-based event handling
    heartbeat_service: Option<HeartbeatService>,
    focus_event_receiver: Option<mpsc::Receiver<(u64, bool)>>,
    event_receiver: Option<mpsc::Receiver<crate::ipc_protocol::GridEvent>>, // NEW: for window events
    // Add WindowEventSystem for move/resize event polling
    window_event_system: Option<crate::WindowEventSystem>,
}

impl GridIpcServer {
    /// Create a new IPC server instance
    pub fn new(
        tracker: Arc<Mutex<WindowTracker>>,
        windows: Arc<DashMap<u64, crate::WindowInfo>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Get the config from the tracker once during initialization
        let config = {
            let tracker_guard = tracker.lock().unwrap();
            tracker_guard.config.clone()
        };

        Ok(Self {
            tracker,
            windows,
            config,
            event_publisher: None,
            response_publisher: None,
            window_details_publisher: None,
            focus_publisher: None,
            layout_publisher: None,
            cell_assignment_publisher: None,
            animation_status_publisher: None,
            heartbeat_publisher: None,
            window_list_publisher: None,
            command_subscriber: None,
            layout_subscriber: None,
            cell_assignment_subscriber: None,
            animation_subscriber: None,
            is_running: false,
            event_listeners: Vec::new(),
            heartbeat_service: None,
            focus_event_receiver: None,
            event_receiver: None,
            window_event_system: None,
        })
    }
pub fn publish_window_list_message(&mut self) -> Result<(), Box<dyn std::error::Error>> {
    use crate::ipc_protocol::{WindowListMessage, WindowDetails, MAX_WINDOWS};
    let windows_snapshot = if let Ok(tracker) = self.tracker.lock() {
        tracker.windows.clone()
    } else {
        return Err("Failed to lock window tracker".into());
    };

    let mut msg = WindowListMessage {
        window_count: 0,
        windows: [WindowDetails::default(); MAX_WINDOWS],
    };
    for (i, entry) in windows_snapshot.iter().enumerate().take(MAX_WINDOWS) {
        let (hwnd, window_info) = entry.pair();
        msg.windows[i] = self.create_window_details_safe(*hwnd, &*window_info);
        msg.window_count += 1;
    }
    if let Some(ref mut publisher) = self.window_list_publisher {
        publisher.send_copy(msg)?;
    } else {
        return Err("Window list publisher not available".into());
    }
    Ok(())
}
    /// Initialize all IPC services
    pub fn setup_services(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 Setting up E-Grid IPC server services...");

        let node = NodeBuilder::new().create::<Service>()?;

        // Setup event publishing service
        println!(
            "[IPC] Creating service: {} (type: WindowEvent)",
            GRID_EVENTS_SERVICE
        );
        let event_service = node
            .service_builder(&ServiceName::new(GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowEvent>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.event_publisher = Some(event_service.publisher_builder().create()?);

        // Setup response publishing service
        println!(
            "[IPC] Creating service: {} (type: IpcResponse)",
            GRID_RESPONSE_SERVICE
        );
        let response_service = node
            .service_builder(&ServiceName::new(GRID_RESPONSE_SERVICE)?)
            .publish_subscribe::<IpcResponse>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.response_publisher = Some(response_service.publisher_builder().create()?);

        // Setup window details publishing service
        println!(
            "[IPC] Creating service: {} (type: WindowDetails)",
            GRID_WINDOW_DETAILS_SERVICE
        );
        let window_details_service = node
            .service_builder(&ServiceName::new(GRID_WINDOW_DETAILS_SERVICE)?)
            .publish_subscribe::<WindowDetails>()
            .max_publishers(8)
            .max_subscribers(8)
            .history_size(32)
            .subscriber_max_buffer_size(64)
            .open_or_create()?;
        self.window_details_publisher = Some(window_details_service.publisher_builder().create()?);

        // Setup focus events publishing service
        println!(
            "[IPC] Creating service: {} (type: WindowFocusEvent)",
            GRID_FOCUS_EVENTS_SERVICE
        );
        let focus_service = node
            .service_builder(&ServiceName::new(GRID_FOCUS_EVENTS_SERVICE)?)
            .publish_subscribe::<WindowFocusEvent>()
            .max_publishers(8)
            .max_subscribers(8)
            .history_size(16)
            .subscriber_max_buffer_size(32)
            .open_or_create()?;
        self.focus_publisher = Some(focus_service.publisher_builder().create()?);

        // Setup command subscription service
        println!(
            "[IPC DEBUG] Opening command service: {} (GridCommand)",
            GRID_COMMANDS_SERVICE
        );
        let command_service = node
            .service_builder(&ServiceName::new(GRID_COMMANDS_SERVICE)?)
            .publish_subscribe::<IpcCommand>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.command_subscriber = Some(command_service.subscriber_builder().create()?);

        // Setup grid layout services
        println!(
            "[IPC] Creating service: {} (type: GridLayoutMessage)",
            GRID_LAYOUT_SERVICE
        );
        let layout_service = node
            .service_builder(&ServiceName::new(GRID_LAYOUT_SERVICE)?)
            .publish_subscribe::<GridLayoutMessage>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.layout_publisher = Some(layout_service.publisher_builder().create()?);
        self.layout_subscriber = Some(layout_service.subscriber_builder().create()?);

        // Setup cell assignment services
        println!(
            "[IPC] Creating service: {} (type: GridCellAssignment)",
            GRID_CELL_ASSIGNMENTS_SERVICE
        );
        let cell_assignment_service = node
            .service_builder(&ServiceName::new(GRID_CELL_ASSIGNMENTS_SERVICE)?)
            .publish_subscribe::<GridCellAssignment>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.cell_assignment_publisher =
            Some(cell_assignment_service.publisher_builder().create()?);
        self.cell_assignment_subscriber =
            Some(cell_assignment_service.subscriber_builder().create()?);

        // Setup animation services
        println!(
            "[IPC] Creating service: {} (type: AnimationCommand)",
            ANIMATION_COMMANDS_SERVICE
        );
        let animation_service = node
            .service_builder(&ServiceName::new(ANIMATION_COMMANDS_SERVICE)?)
            .publish_subscribe::<AnimationCommand>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.animation_subscriber = Some(animation_service.subscriber_builder().create()?);

        // Setup animation status service
        println!(
            "[IPC] Creating service: {} (type: AnimationStatus)",
            ANIMATION_STATUS_SERVICE
        );
        let animation_status_service = node
            .service_builder(&ServiceName::new(ANIMATION_STATUS_SERVICE)?)
            .publish_subscribe::<AnimationStatus>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.animation_status_publisher =
            Some(animation_status_service.publisher_builder().create()?);

        // Setup heartbeat service
        println!(
            "[IPC] Creating service: {} (type: HeartbeatMessage)",
            GRID_HEARTBEAT_SERVICE
        );
        let heartbeat_service = node
            .service_builder(&ServiceName::new(GRID_HEARTBEAT_SERVICE)?)
            .publish_subscribe::<HeartbeatMessage>()
            .max_publishers(8)
            .max_subscribers(8)
            .open_or_create()?;
        self.heartbeat_publisher = Some(heartbeat_service.publisher_builder().create()?);

        let window_list_service = node
        .service_builder(&ServiceName::new(GRID_WINDOW_LIST_SERVICE)?)
        .publish_subscribe::<WindowListMessage>()
        .max_publishers(8)
        .max_subscribers(8)
        .open_or_create()?;
        self.window_list_publisher = Some(window_list_service.publisher_builder().create()?);

        self.is_running = true;
        Ok(())
    }

    /// Start the server event loop in the current thread
    pub fn run_event_loop(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        while self.is_running {
            // --- NEW: poll move/resize events ---
            if let Some(wes) = self.window_event_system.as_mut() {
                wes.poll_move_resize_events();
            }
            // --- END NEW ---
            // Process incoming commands from clients
            self.process_commands()?;

            // Process incoming focus events from the channel and publish them via IPC
            self.process_focus_events()?;

            // Process window events from the channel and publish them via IPC
            self.process_window_events()?;

            // Small delay to prevent busy waiting
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }
    /// Start the server event loop in a background thread
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
        let mut commands_to_process: Vec<IpcCommand> = Vec::new();
        // Collect all incoming commands
        if let Some(ref mut subscriber) = self.command_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let command = sample.clone(); // FIX: clone instead of move
                commands_to_process.push(command);
            }
        }
        // Process each command
        for command in commands_to_process {
            trace!("📨 Received command: {:?}", command);
            let response = self.handle_ipc_command(command)?;
            self.send_ipc_response(response)?;
        }
        Ok(())
    }

    /// Handle an IpcCommand and return an IpcResponse
    fn handle_ipc_command(
        &mut self,
        command: IpcCommand,
    ) -> Result<Box<IpcResponse>, Box<dyn std::error::Error>> {
          match command.command_type {
            IpcCommandType::GetWindowList => {
                self.publish_window_list_message()?;
                // Optionally, return an ACK or minimal response
                Ok(Box::new(IpcResponse {
                    response_type: IpcResponseType::Ack,
                    has_grid_state: 0,
                    grid_state: GridState::default(),
                    has_monitor_list: 0,
                    monitor_list: MonitorList::default(),
                    window_count: 0,
                    window_list: Box::new(core::array::from_fn(|_| WindowInfo::default())),
                    has_error_message: 0,
                    error_message_len: 0,
                    error_message: [0; 256],
                    protocol_version: command.protocol_version,
                }))
            }
            IpcCommandType::GetGridState => {
                // TODO: Fill with actual grid state if available
                let has_grid_state = 1;
                let grid_state = GridState::default();
                Ok(Box::new(IpcResponse {
                    response_type: IpcResponseType::GridState,
                    has_grid_state,
                    grid_state,
                    has_monitor_list: 0,
                    monitor_list: MonitorList::default(),
                    window_count: 0,
                    window_list: Box::new(core::array::from_fn(|_| WindowInfo::default())),
                    has_error_message: 0,
                    error_message_len: 0,
                    error_message: [0; 256],
                    protocol_version: command.protocol_version,
                }))
            }
            IpcCommandType::GetWindowList => {
                // TODO: Fill window_list and window_count with real data
                let mut window_list: Box<[WindowInfo; MAX_WINDOWS]> = Box::new(core::array::from_fn(|_| WindowInfo::default()));
                let window_count = 0; // Set to actual count when implemented
                Ok(Box::new(IpcResponse {
                    response_type: IpcResponseType::WindowList,
                    has_grid_state: 0,
                    grid_state: GridState::default(),
                    has_monitor_list: 0,
                    monitor_list: MonitorList::default(),
                    window_count,
                    window_list,
                    has_error_message: 0,
                    error_message_len: 0,
                    error_message: [0; 256],
                    protocol_version: command.protocol_version,
                }))
            }
            IpcCommandType::MoveWindow
            | IpcCommandType::AnimateWindow
            | IpcCommandType::AssignToVirtualCell
            | IpcCommandType::AssignToMonitorCell => {
                Ok(Box::new(IpcResponse {
                    response_type: IpcResponseType::Ack,
                    has_grid_state: 0,
                    grid_state: GridState::default(),
                    has_monitor_list: 0,
                    monitor_list: MonitorList::default(),
                    window_count: 0,
                    window_list: Box::new(core::array::from_fn(|_| WindowInfo::default())),
                    has_error_message: 0,
                    error_message_len: 0,
                    error_message: [0; 256],
                    protocol_version: command.protocol_version,
                }))
            }
            IpcCommandType::FocusWindow | IpcCommandType::GetMonitorList => {
                // TODO: Fill monitor_list with real data if available
                let has_monitor_list = 1;
                let monitor_list = MonitorList::default();
                Ok(Box::new(IpcResponse {
                    response_type: IpcResponseType::MonitorList,
                    has_grid_state: 0,
                    grid_state: GridState::default(),
                    has_monitor_list,
                    monitor_list,
                    window_count: 0,
                    window_list: Box::new(core::array::from_fn(|_| WindowInfo::default())),
                    has_error_message: 0,
                    error_message_len: 0,
                    error_message: [0; 256],
                    protocol_version: command.protocol_version,
                }))
            }
        }
    }

    /// Send an IpcResponse to clients
fn send_ipc_response(
    &mut self,
    response: Box<IpcResponse>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(ref mut publisher) = self.response_publisher {
        publisher.send_copy(*response)?; // move out of the box
        trace!("📤 Sent IpcResponse");
    }
    Ok(())
}

    /// Process incoming focus events from the channel and publish them via IPC
    pub fn process_focus_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Collect events first to avoid borrowing conflicts
        let mut events = Vec::new();
        if let Some(ref receiver) = self.focus_event_receiver {
            while let Ok(event) = receiver.try_recv() {
                events.push(event);
            }
        } else {
            error!("❌ [DEBUG] focus_event_receiver is None!");
        }

        // Process all collected focus events
        for (hwnd, is_focused) in events {
            // Convert u64 back to HWND and publish via IPC
            let event_type = if is_focused { "FOCUSED" } else { "DEFOCUSED" };

            if let Err(e) = self.publish_focus_event_from_library(hwnd, is_focused) {
                error!("❌ Failed to publish focus event via IPC: {:?}", e);
            }

            // Reset heartbeat when focus events occur
            if let Some(ref mut heartbeat) = self.heartbeat_service {
                heartbeat.reset();
            }
        }
        Ok(())
    }

    /// Process window events from the channel and publish them via IPC
    pub fn process_window_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let events: Vec<_> = if let Some(ref event_receiver) = self.event_receiver {
            event_receiver.try_iter().collect()
        } else {
            Vec::new()
        };
        let event_count = events.len();
        if event_count > 0 {
            info!(
                "[process_window_events] Processing {} window events...",
                event_count
            );
        }
        for event in events {
            info!("[process_window_events] Publishing event: {:?}", event);
            if let Err(e) = self.publish_event(event) {
                error!(
                    "❌ [process_window_events] Failed to publish event: {:?}",
                    e
                );
            }
        }
        if event_count > 0 {
            info!(
                "[process_window_events] Finished processing {} window events.",
                event_count
            );
        }
        Ok(())
    }

    /// Publish a window event to all connected clients
    pub fn publish_event(&mut self, event: GridEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Convert high-level event to zero-copy format
        let window_event = self.grid_event_to_window_event(&event);
        // --- Enhanced visual logging for move/resize START/STOP events ---
        match &event {
            GridEvent::WindowMoveStart { .. } | GridEvent::WindowResizeStart { .. } => {
                info!("\n\n📡 Published event: {:?}", event);
            }
            GridEvent::WindowMoveStop { .. } | GridEvent::WindowResizeStop { .. } => {
                info!("📡 Published event: {:?}\n\n", event);
            }
            _ => {
                info!("📡 Published event: {:?}", event);
            }
        }
        // Publish via iceoryx2
        if let Some(ref mut publisher) = self.event_publisher {
            if let Err(e) = publisher.send_copy(window_event) {
                error!("❌ Failed to send event to IPC: {:?}", e);
            } else {
                debug!("[publish_event] Event sent to IPC: {:?}", event);
            }
        } else {
            error!("❌ Event publisher is None - not initialized!");
        }

        // Notify local listeners
        for listener in &self.event_listeners {
            listener(&event);
        }
        debug!("[publish_event] Event sent to local listeners: {:?}", event);

        Ok(())
    }

    /// Publish details for a specific window
    pub fn publish_window_details(&mut self, hwnd: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Use try_lock to avoid blocking if the tracker is locked elsewhere.
        if let Ok(tracker) = self.tracker.try_lock() {
            if let Some(window_info) = tracker.windows.get(&hwnd) {
                // Create the details first (immutable borrow)
                let details = self.create_window_details_safe(hwnd, &*window_info);

                // Then publish (mutable borrow)
                if let Some(ref mut publisher) = self.window_details_publisher {
                    publisher.send_copy(details)?;
                    debug!("Published window details for HWND {:?}", hwnd);
                }
            } else {
                warn!("No WindowInfo found for HWND {:?}", hwnd);
            }
        } else {
            warn!("Could not acquire tracker lock for HWND {:?}", hwnd);
        }
        Ok(())
    }

    /// Publish details for all current windows
    pub fn publish_all_window_details(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Get a snapshot of windows to avoid holding the lock during publishing
        let windows_snapshot = if let Ok(tracker) = self.tracker.lock() {
            info!("📤 Publishing details for {} windows (already filtered by is_manageable_window)...", tracker.windows.len());
            tracker.windows.clone()
        } else {
            return Err("Failed to lock window tracker".into());
        };

        let total_window_count = windows_snapshot.len();
        let mut published_count = 0;
        let mut failed_count = 0;
        for entry in &windows_snapshot {
            let (hwnd, window_info) = entry.pair();
            // No additional filtering - windows in tracker are already pre-filtered by is_manageable_window
            // This ensures client and server see the same set of windows

            // Create details without holding tracker lock to avoid deadlock
            let details = self.create_window_details_safe(*hwnd, &*window_info);

            // Publish the details
            if let Some(ref mut publisher) = self.window_details_publisher {
                match publisher.send_copy(details) {
                    Ok(_) => {
                        published_count += 1; // Print all published windows to verify they're all being sent
                        println!(
                            "   ✅ Published window {} (#{}/{}): '{}'",
                            *hwnd as u64,
                            published_count,
                            total_window_count,
                            String::from_utf16_lossy(&window_info.title)
                                .chars()
                                .take(40)
                                .collect::<String>()
                        );

                        // Small delay to prevent overwhelming the IPC system
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(e) => {
                        error!("   ❌ Failed to publish window {}: {}", *hwnd as u64, e);
                        failed_count += 1;
                        // Continue with other windows instead of failing completely
                    }
                }
            } else {
                error!("⚠️ Window details publisher not available");
                return Err("Window details publisher not available".into());
            }
        }
        info!(
            "✅ Successfully published details for {}/{} windows (failed: {})",
            published_count, total_window_count, failed_count
        );
        Ok(())
    }
    /// Publish focus event for window focus tracking (NEW: for e_midi integration)
    pub fn publish_focus_event(&mut self, hwnd: u64, event_type: u8) {
        // Get window information for the focus event
        let process_id = unsafe {
            let mut process_id: u32 = 0;
            winapi::um::winuser::GetWindowThreadProcessId(hwnd as HWND, &mut process_id);
            process_id
        };

        // Get window title for hashing
        let window_title = unsafe {
            let mut buffer: [u16; 256] = [0; 256];
            let len = winapi::um::winuser::GetWindowTextW(
                hwnd as HWND,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            );
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
            let focus_event = WindowFocusEvent {
                event_type,
                hwnd,
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
                error!("❌ Failed to publish focus event: {:?}", e);
            } else {
                let event_name = if event_type == 0 {
                    "FOCUSED"
                } else {
                    "DEFOCUSED"
                };
                info!(
                    "🎯 Published {} event: HWND {} (PID: {}) Title: '{}'",
                    event_name,
                    hwnd as u64,
                    process_id,
                    if window_title.len() > 30 {
                        &window_title[..30]
                    } else {
                        &window_title
                    }
                );
            }
        } else {
            warn!("⚠️ Focus publisher not available");
        }
    }
    /// Publish focus event for window focus tracking (compatible with library-based events)
    pub fn publish_focus_event_from_library(
        &mut self,
        hwnd: u64,
        is_focused: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let process_id = unsafe {
            let mut process_id: u32 = 0;
            winapi::um::winuser::GetWindowThreadProcessId(hwnd as HWND, &mut process_id);
            process_id
        };
        let window_title = unsafe {
            let mut buffer: [u16; 256] = [0; 256];
            let len = winapi::um::winuser::GetWindowTextW(
                hwnd as HWND,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
            );
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
            let focus_event = WindowFocusEvent {
                event_type: if is_focused { 0 } else { 1 }, // 0=FOCUSED, 1=DEFOCUSED
                hwnd,
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
            publisher.send_copy(focus_event)?;

            let event_name = if is_focused { "FOCUSED" } else { "DEFOCUSED" };
            info!(
                "🎯 Published {} event: HWND {} (PID: {}) Title: '{}'",
                event_name,
                hwnd as u64,
                process_id,
                if window_title.len() > 30 {
                    &window_title[..30]
                } else {
                    &window_title
                }
            );
        } else {
            error!("❌ [DEBUG] Focus publisher is None - not initialized!");
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Focus publisher not initialized",
            )));
        }
        Ok(())
    }

    /// Simple hash function for strings  
    fn hash_string(&self, s: &str) -> u64 {
        let mut hash = 0u64;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
        hash
    }
    /// Create window details without holding the tracker lock to avoid deadlocks
    fn create_window_details_safe(
        &self,
        hwnd: u64,
        window_info: &crate::WindowInfo,
    ) -> WindowDetails {
        // Use WindowInfo rect fields directly
        let left = window_info.rect.left;
        let top = window_info.rect.top;
        let right = window_info.rect.right;
        let bottom = window_info.rect.bottom;

        // Get screen dimensions for proper grid calculation
        let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        // Calculate proper virtual grid position based on actual screen dimensions
        let cell_width = screen_width / self.config.cols as i32;
        let cell_height = screen_height / self.config.rows as i32;

        let virtual_row = if cell_height > 0 && top >= 0 {
            ((top / cell_height).max(0).min(self.config.rows as i32 - 1)) as u32
        } else {
            0
        };

        let virtual_col = if cell_width > 0 && left >= 0 {
            ((left / cell_width).max(0).min(self.config.cols as i32 - 1)) as u32
        } else {
            0
        };

        // Calculate end positions based on window size
        let virtual_row_end = if cell_height > 0 && bottom > top {
            ((bottom / cell_height)
                .max(virtual_row as i32)
                .min(self.config.rows as i32)) as u32
        } else {
            virtual_row + 1
        };

        let virtual_col_end = if cell_width > 0 && right > left {
            ((right / cell_width)
                .max(virtual_col as i32)
                .min(self.config.cols as i32)) as u32
        } else {
            virtual_col + 1
        };

        WindowDetails {
            hwnd,
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,

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

            // Title field (convert UTF-16 to UTF-8 and fit into [u8; 256])
            title: {
                let utf8 = String::from_utf16_lossy(&window_info.title);
                let bytes = utf8.as_bytes();
                let mut arr = [0u8; 256];
                let len = bytes.len().min(256);
                arr[..len].copy_from_slice(&bytes[..len]);
                arr
            },
            // Title length for validation
            title_len: {
                let utf8 = String::from_utf16_lossy(&window_info.title);
                utf8.len().min(255) as u32
            },
        }
    }

    /// Add an event listener for local event handling
    pub fn add_event_listener<F>(&mut self, listener: F)
    where
        F: Fn(&GridEvent) + Send + Sync + 'static,
    {
        self.event_listeners.push(Box::new(listener));
    }

    /// Stop the server
    pub fn stop(&mut self) {
        self.is_running = false;
        error!("🛑 E-Grid IPC server stopped");
    }

    /// Get the current grid configuration
    pub fn get_config(&self) -> &GridConfig {
        &self.config
    } // Convenience methods for publishing specific events
    pub fn publish_window_created(
        &mut self,
        hwnd: u64,
        title: String,
        row: usize,
        col: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::WindowCreated {
            hwnd,
            title,
            row,
            col,
            // TODO: Get actual position data from window tracker
            grid_top_left_row: row,
            grid_top_left_col: col,
            grid_bottom_right_row: row,
            grid_bottom_right_col: col,
            real_x: 0,
            real_y: 0,
            real_width: 0,
            real_height: 0,
            monitor_id: 0,
        };
        self.publish_event(event)
    }

    pub fn publish_window_destroyed(
        &mut self,
        hwnd: u64,
        title: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::WindowDestroyed { hwnd, title };
        self.publish_event(event)
    }

    pub fn publish_window_moved(
        &mut self,
        hwnd: u64,
        title: String,
        old_row: usize,
        old_col: usize,
        new_row: usize,
        new_col: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::WindowMoved {
            hwnd,
            title,
            old_row,
            old_col,
            new_row,
            new_col,
            // TODO: Get actual position data from window tracker
            grid_top_left_row: new_row,
            grid_top_left_col: new_col,
            grid_bottom_right_row: new_row,
            grid_bottom_right_col: new_col,
            real_x: 0,
            real_y: 0,
            real_width: 0,
            real_height: 0,
            monitor_id: 0,
        };
        self.publish_event(event)
    }

    pub fn publish_grid_state_changed(
        &mut self,
        total_windows: usize,
        occupied_cells: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event = GridEvent::GridStateChanged {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            total_windows,
            occupied_cells,
        };
        self.publish_event(event)
    } // Private helper methods
    fn count_occupied_cells(&self, tracker: &WindowTracker) -> usize {
        let mut occupied = std::collections::HashSet::new();
        for entry in &tracker.windows {
            let (_, window) = entry.pair();
            for &(row, col) in &window.grid_cells {
                occupied.insert((row, col));
            }
        }
        occupied.len()
    }
    /// Move a window to a specific grid cell
    pub fn move_window_to_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let hwnd_ptr = hwnd as winapi::shared::windef::HWND;

        if let Ok(mut tracker) = self.tracker.lock() {
            tracker
                .move_window_to_cell(hwnd, target_row, target_col)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Assign a window to a virtual grid cell
    pub fn assign_window_to_virtual_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker
                .assign_window_to_virtual_cell(hwnd, target_row, target_col)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Assign a window to a monitor-specific grid cell
    pub fn assign_window_to_monitor_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
        monitor_id: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker
                .assign_window_to_monitor_cell(hwnd, target_row, target_col, monitor_id)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Apply a saved layout by name
    pub fn apply_saved_layout(
        &mut self,
        layout_name: &str,
        duration_ms: u32,
        easing_type: crate::EasingType,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            if let Some(layout) = tracker.get_saved_layout(layout_name) {
                let duration = std::time::Duration::from_millis(duration_ms as u64);
                tracker
                    .apply_grid_layout(&layout, duration, easing_type)
                    .map_err(|e| e.into())
            } else {
                Err(format!("Layout '{}' not found", layout_name).into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Save the current layout with a given name
    pub fn save_current_layout(
        &mut self,
        layout_name: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            tracker.save_current_layout(layout_name);
            Ok(())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Get all saved layout names
    pub fn get_saved_layouts(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            Ok(tracker.list_saved_layouts())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Start animation for a specific window
    pub fn start_window_animation(
        &mut self,
        hwnd: u64,
        target_rect: winapi::shared::windef::RECT,
        duration_ms: u32,
        easing_type: crate::EasingType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            let duration = std::time::Duration::from_millis(duration_ms as u64);
            tracker
                .start_window_animation(hwnd, target_rect, duration, easing_type)
                .map_err(|e| e.into())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Stop animation for a specific window
    pub fn stop_window_animation(&mut self, hwnd: u64) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            tracker.active_animations.remove(&hwnd);
            Ok(())
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Get animation status for a specific window
    pub fn get_animation_status(
        &self,
        hwnd: u64,
    ) -> Result<Option<crate::window::animation::WindowAnimation>, Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            Ok(tracker
                .active_animations
                .get(&hwnd)
                .map(|anim_ref| anim_ref.clone()))
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    /// Poll move/resize events from the window event system (if present)
    pub fn poll_move_resize_events(&mut self) {
        if let Some(wes) = self.window_event_system.as_mut() {
            wes.poll_move_resize_events();
        }
    }

    // Conversion helper methods
    fn grid_event_to_window_event(&self, event: &GridEvent) -> WindowEvent {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        match event {
            GridEvent::WindowCreated {
                hwnd,
                row,
                col,
                grid_top_left_row,
                grid_top_left_col,
                grid_bottom_right_row,
                grid_bottom_right_col,
                real_x,
                real_y,
                real_width,
                real_height,
                monitor_id,
                ..
            } => WindowEvent {
                event_type: 0,
                hwnd: *hwnd,
                row: *row as u32,
                col: *col as u32,
                grid_top_left_row: *grid_top_left_row as u32,
                grid_top_left_col: *grid_top_left_col as u32,
                grid_bottom_right_row: *grid_bottom_right_row as u32,
                grid_bottom_right_col: *grid_bottom_right_col as u32,
                real_x: *real_x,
                real_y: *real_y,
                real_width: *real_width,
                real_height: *real_height,
                monitor_id: *monitor_id,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowDestroyed { hwnd, .. } => WindowEvent {
                event_type: 1,
                hwnd: *hwnd,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowMoved {
                hwnd,
                old_row,
                old_col,
                new_row,
                new_col,
                grid_top_left_row,
                grid_top_left_col,
                grid_bottom_right_row,
                grid_bottom_right_col,
                real_x,
                real_y,
                real_width,
                real_height,
                monitor_id,
                ..
            } => WindowEvent {
                event_type: 2,
                hwnd: *hwnd,
                old_row: *old_row as u32,
                old_col: *old_col as u32,
                row: *new_row as u32,
                col: *new_col as u32,
                grid_top_left_row: *grid_top_left_row as u32,
                grid_top_left_col: *grid_top_left_col as u32,
                grid_bottom_right_row: *grid_bottom_right_row as u32,
                grid_bottom_right_col: *grid_bottom_right_col as u32,
                real_x: *real_x,
                real_y: *real_y,
                real_width: *real_width,
                real_height: *real_height,
                monitor_id: *monitor_id,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowMoveStart {
                hwnd,
                current_row,
                current_col,
                grid_top_left_row,
                grid_top_left_col,
                grid_bottom_right_row,
                grid_bottom_right_col,
                real_x,
                real_y,
                real_width,
                real_height,
                monitor_id,
                ..
            } => WindowEvent {
                event_type: 4, // move_start
                hwnd: *hwnd,
                row: *current_row as u32,
                col: *current_col as u32,
                grid_top_left_row: *grid_top_left_row as u32,
                grid_top_left_col: *grid_top_left_col as u32,
                grid_bottom_right_row: *grid_bottom_right_row as u32,
                grid_bottom_right_col: *grid_bottom_right_col as u32,
                real_x: *real_x,
                real_y: *real_y,
                real_width: *real_width,
                real_height: *real_height,
                monitor_id: *monitor_id,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowMoveStop {
                hwnd,
                final_row,
                final_col,
                grid_top_left_row,
                grid_top_left_col,
                grid_bottom_right_row,
                grid_bottom_right_col,
                real_x,
                real_y,
                real_width,
                real_height,
                monitor_id,
                ..
            } => WindowEvent {
                event_type: 5, // move_stop
                hwnd: *hwnd,
                row: *final_row as u32,
                col: *final_col as u32,
                grid_top_left_row: *grid_top_left_row as u32,
                grid_top_left_col: *grid_top_left_col as u32,
                grid_bottom_right_row: *grid_bottom_right_row as u32,
                grid_bottom_right_col: *grid_bottom_right_col as u32,
                real_x: *real_x,
                real_y: *real_y,
                real_width: *real_width,
                real_height: *real_height,
                monitor_id: *monitor_id,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowResizeStart {
                hwnd,
                current_row,
                current_col,
                grid_top_left_row,
                grid_top_left_col,
                grid_bottom_right_row,
                grid_bottom_right_col,
                real_x,
                real_y,
                real_width,
                real_height,
                monitor_id,
                ..
            } => WindowEvent {
                event_type: 6, // resize_start
                hwnd: *hwnd,
                row: *current_row as u32,
                col: *current_col as u32,
                grid_top_left_row: *grid_top_left_row as u32,
                grid_top_left_col: *grid_top_left_col as u32,
                grid_bottom_right_row: *grid_bottom_right_row as u32,
                grid_bottom_right_col: *grid_bottom_right_col as u32,
                real_x: *real_x,
                real_y: *real_y,
                real_width: *real_width,
                real_height: *real_height,
                monitor_id: *monitor_id,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowResizeStop {
                hwnd,
                final_row,
                final_col,
                grid_top_left_row,
                grid_top_left_col,
                grid_bottom_right_row,
                grid_bottom_right_col,
                real_x,
                real_y,
                real_width,
                real_height,
                monitor_id,
                ..
            } => WindowEvent {
                event_type: 7, // resize_stop
                hwnd: *hwnd,
                row: *final_row as u32,
                col: *final_col as u32,
                grid_top_left_row: *grid_top_left_row as u32,
                grid_top_left_col: *grid_top_left_col as u32,
                grid_bottom_right_row: *grid_bottom_right_row as u32,
                grid_bottom_right_col: *grid_bottom_right_col as u32,
                real_x: *real_x,
                real_y: *real_y,
                real_width: *real_width,
                real_height: *real_height,
                monitor_id: *monitor_id,
                timestamp,
                ..Default::default()
            },
            GridEvent::GridStateChanged {
                timestamp,
                total_windows,
                occupied_cells,
            } => WindowEvent {
                event_type: 3,
                timestamp: *timestamp,
                total_windows: *total_windows as u32,
                occupied_cells: *occupied_cells as u32,
                ..Default::default()
            },
        }
    }

    /// Setup window event monitoring using the new library-based system
    pub fn setup_window_events(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Create a channel for focus events
        let (focus_sender, focus_receiver) = mpsc::channel::<(u64, bool)>();
        self.focus_event_receiver = Some(focus_receiver);
        // Create a channel for window move/resize events
        let (event_sender, event_receiver) = mpsc::channel::<crate::ipc_protocol::GridEvent>();
        self.event_receiver = Some(event_receiver);
        // --- NEW: Setup WindowEventSystem for move/resize ---
        // Convert Arc<DashMap<u64, WindowInfo>> to Arc<DashMap<*mut winapi::shared::windef::HWND__, WindowInfo>>
        let hwnd_map: Arc<DashMap<*mut winapi::shared::windef::HWND__, crate::WindowInfo>> =
            Arc::new(DashMap::new());
        for entry in self.windows.iter() {
            let (hwnd_u64, win_info) = entry.pair();
            let hwnd_ptr = *hwnd_u64 as *mut winapi::shared::windef::HWND__;
            hwnd_map.insert(hwnd_ptr, win_info.clone());
        }
        let mut wes = crate::WindowEventSystem::new(hwnd_map.clone());
        let event_sender_for_wes = event_sender.clone();
        // Only send to the channel; do not attempt to clone or use the publisher here
        wes.set_event_callback(move |event: crate::ipc_protocol::GridEvent| {
            println!("[SERVER CALLBACK] Window event: {:?}", event);
            let _ = event_sender_for_wes.send(event.clone());
        });
        // Create window event configuration with focus and event publishing callbacks
        let event_sender_for_config = event_sender.clone();
        let config = WindowEventConfig {
            tracker: self.tracker.clone(),
            focus_callback: Some(Box::new(move |hwnd: u64, is_focused: bool| {
                info!(
                    "🎯 Focus event: HWND {} - {}",
                    hwnd,
                    if is_focused { "FOCUSED" } else { "DEFOCUSED" }
                );
                let _ = focus_sender.send((hwnd, is_focused));
            })),
            heartbeat_reset: Some(Box::new(|| {
                // This callback will be called when window events occur
                //println!("💓 Heartbeat reset triggered by window event");
            })),
            event_callback: Some(Box::new(move |event: crate::ipc_protocol::GridEvent| {
                // Debug: Log every event received by the callback
                debug!("[event_callback] Received event: {:?}", event);
                // Send event to the main event loop via channel
                if let Err(e) = event_sender_for_config.send(event.clone()) {
                    error!("❌ Failed to send event via channel: {:?}", e);
                }
            })),
            debug_mode: true,
            move_resize_event_queue: Some(wes.event_queue.clone()),
            move_resize_states: Some(wes.states.clone()),
        };
        // Setup window events using the new library system
        window_events::setup_window_events(config).map_err(|e| {
            Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                as Box<dyn std::error::Error>
        })?;
        // Initialize heartbeat service with 30-second timeout
        self.heartbeat_service = Some(HeartbeatService::new(Duration::from_secs(3)));
        self.window_event_system = Some(wes);
        Ok(())
    }

    /// Process layout commands from clients
    pub fn process_layout_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut subscriber) = self.layout_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let layout_msg = *sample;
                info!("🗂️ Received layout command: {:?}", layout_msg);

                match layout_msg.message_type {
                    0 => {
                        // apply_layout
                        info!("📥 Layout application request received");
                    }
                    1 => {
                        // save_current_layout
                        let layout_name = format!("layout_{}", layout_msg.layout_id);
                        if let Ok(mut tracker) = self.tracker.lock() {
                            tracker.save_current_layout(layout_name.clone());
                            info!("💾 Saved current layout as '{}'", layout_name);
                        }
                    }
                    2 => {
                        // get_saved_layouts
                        info!("📋 Saved layouts request received");
                    }
                    _ => {
                        warn!(
                            "⚠️ Unknown layout command type: {}",
                            layout_msg.message_type
                        );
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
                    0 => {
                        // start_animation
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
                            let duration =
                                std::time::Duration::from_millis(anim_cmd.duration_ms as u64);
                            if let Err(e) = tracker.start_window_animation(
                                anim_cmd.hwnd,
                                target_rect,
                                duration,
                                easing_type,
                            ) {
                                println!(
                                    "⚠️ Failed to start animation for window {}: {}",
                                    anim_cmd.hwnd, e
                                );
                            }
                        }
                    }
                    1 => {
                        // stop_animation
                        if let Ok(tracker) = self.tracker.lock() {
                            if anim_cmd.hwnd == 0 {
                                tracker.active_animations.clear();
                                println!("🛑 Stopped all animations");
                            } else {
                                tracker.active_animations.remove(&anim_cmd.hwnd);
                                println!("🛑 Stopped animation for window {}", anim_cmd.hwnd);
                            }
                        }
                    }
                    4 => {
                        // get_status
                        println!("📊 Animation status request for window {}", anim_cmd.hwnd);
                        // Could publish status here
                    }
                    _ => {
                        println!(
                            "⚠️ Unknown animation command type: {}",
                            anim_cmd.command_type
                        );
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

    /// Send heartbeat message to keep clients connected during idle periods
    pub fn send_heartbeat(
        &mut self,
        iteration: u64,
        uptime_ms: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref heartbeat_publisher) = self.heartbeat_publisher {
            let heartbeat = HeartbeatMessage {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
                server_iteration: iteration,
                uptime_ms,
            };
            heartbeat_publisher.send_copy(heartbeat)?;
        }
        Ok(())
    }

    /// Enumerate monitors and build a MonitorList (stub: single monitor for now)
    fn enumerate_monitors(&self) -> crate::ipc_protocol::MonitorList {
        use crate::ipc_protocol::{GridType, MonitorGridInfo, MonitorList};
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use std::ptr;
        use winapi::shared::windef::{HDC, LPRECT};
        use winapi::um::winuser::{EnumDisplayMonitors, GetMonitorInfoW, MONITORINFOEXW};

        struct MonitorEnumContext {
            monitors: Vec<MonitorGridInfo>,
            next_id: u32,
        }
        let mut context = MonitorEnumContext {
            monitors: Vec::new(),
            next_id: 0,
        };

        unsafe extern "system" fn monitor_enum_proc(
            hmonitor: winapi::shared::windef::HMONITOR,
            _hdc: HDC,
            lprc: LPRECT,
            lparam: isize,
        ) -> i32 {
            let context = &mut *(lparam as *mut MonitorEnumContext);
            let mut mi: MONITORINFOEXW = std::mem::zeroed();
            mi.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _) != 0 {
                let rect = mi.rcMonitor;
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;
                let name = OsString::from_wide(&mi.szDevice)
                    .to_string_lossy()
                    .trim_end_matches('\0')
                    .to_string();
                let name_len = name.len().min(255) as u32;
                context.monitors.push(MonitorGridInfo {
                    id: context.next_id,
                    grid_type: GridType::Physical,
                    width,
                    height,
                    x: rect.left,
                    y: rect.top,
                    rows: 1, // or your default
                    cols: 1, // or your default
                    name: {
                        let mut arr = [0u8; 64];
                        let bytes = name.as_bytes();
                        let len = bytes.len().min(64);
                        arr[..len].copy_from_slice(&bytes[..len]);
                        arr
                    },
                    name_len,
                    grid: [[0u64; 32]; 32], // No grid data
                });
                context.next_id += 1;
            }
            1 // continue enumeration
        }

        unsafe {
            EnumDisplayMonitors(
                ptr::null_mut(),
                ptr::null(),
                Some(monitor_enum_proc),
                &mut context as *mut _ as isize,
            );
        }

        let mut monitors_array: [MonitorGridInfo; 16] = [MonitorGridInfo::default(); 16];
        let count = context.monitors.len().min(16);
        for (i, monitor) in context.monitors.into_iter().take(16).enumerate() {
            monitors_array[i] = monitor;
        }
        MonitorList {
            monitors: monitors_array,
            monitor_count: count as u32,
        }
    }
}

impl Drop for GridIpcServer {
    fn drop(&mut self) {
        // Cleanup window events using the library system
        window_events::cleanup_hooks();
    }
}

/// Start the E-Grid server in-process, calling the provided callback on each event loop tick.
/// This function blocks until the server is stopped.
pub fn start_server_with_tick<F>(mut tick_callback: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut(),
{
    // Create the window tracker
    let tracker = Arc::new(Mutex::new(WindowTracker::new()));
    // Get the shared lock-free window state
    let windows = {
        let tracker_guard = tracker.lock().unwrap();
        tracker_guard.windows.clone()
    };
    // Create and setup the IPC server
    let mut ipc_server = crate::ipc_server::GridIpcServer::new(tracker.clone(), Arc::new(windows))?;
    ipc_server.setup_services()?;
    ipc_server.start_background_event_loop()?;
    // Setup WinEvent hooks for real-time monitoring (optional, can ignore errors)
    let _ = ipc_server.setup_window_events();
    // Give the server a moment to be ready
    thread::sleep(Duration::from_millis(500));
    // Main server event loop (blocks until shutdown)
    window_events::run_message_loop(|| {
        ipc_server.poll_move_resize_events();
        let _ = ipc_server.process_commands();
        let _ = ipc_server.process_focus_events();
        let _ = ipc_server.process_window_events();
        let _ = ipc_server.process_layout_commands();
        let _ = ipc_server.process_animation_commands();
        let _ = ipc_server.update_animations();
        tick_callback(); // Call the user-provided callback
        true
    })?;

    Ok(())
}

// The original function now just calls the new one with an empty closure
pub fn start_server() -> Result<(), Box<dyn std::error::Error>> {
    start_server_with_tick(|| {})
}
