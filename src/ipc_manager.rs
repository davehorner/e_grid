// IPC manager logic for e_grid
// Contains only the GridIpcManager struct and its implementation.

use crate::ipc_protocol::*;
use crate::{GridConfig, WindowTracker};
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use log::{debug, info, warn};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use winapi::shared::windef::HWND;

pub struct GridIpcManager {
    tracker: Arc<Mutex<WindowTracker>>,
    event_listeners: Vec<Box<dyn Fn(&GridEvent) + Send + Sync>>,
    node: Option<Node<ipc::Service>>,
    event_publisher: Option<Publisher<ipc::Service, WindowEvent, ()>>,
    command_subscriber: Option<Subscriber<ipc::Service, IpcCommand, ()>>,
    command_publisher: Option<Publisher<ipc::Service, IpcCommand, ()>>,
    response_publisher: Option<Publisher<ipc::Service, IpcResponse, ()>>,
    response_subscriber: Option<Subscriber<ipc::Service, IpcResponse, ()>>,
    window_details_publisher: Option<Publisher<ipc::Service, WindowDetails, ()>>,
    layout_publisher: Option<Publisher<ipc::Service, GridLayoutMessage, ()>>,
    layout_subscriber: Option<Subscriber<ipc::Service, GridLayoutMessage, ()>>,
    cell_assignment_publisher: Option<Publisher<ipc::Service, GridCellAssignment, ()>>,
    cell_assignment_subscriber: Option<Subscriber<ipc::Service, GridCellAssignment, ()>>,
    animation_publisher: Option<Publisher<ipc::Service, AnimationCommand, ()>>,
    animation_subscriber: Option<Subscriber<ipc::Service, AnimationCommand, ()>>,
    animation_status_publisher: Option<Publisher<ipc::Service, AnimationStatus, ()>>,
    heartbeat_publisher: Option<Publisher<ipc::Service, HeartbeatMessage, ()>>,
    window_list_subscriber:
        Option<Subscriber<ipc::Service, crate::ipc_protocol::WindowListMessage, ()>>,

    is_running: bool,
}

// IPC Manager with full iceoryx2 integration
// pub struct GridIpcManager {
//     tracker: Arc<Mutex<WindowTracker>>,
//     event_listeners: Vec<Box<dyn Fn(&GridEvent) + Send + Sync>>,

//     // iceoryx2 node
//     node: Option<Node<ipc::Service>>,
//     // iceoryx2 services
//     event_publisher: Option<Publisher<ipc::Service, WindowEvent, ()>>,
//     command_subscriber: Option<Subscriber<ipc::Service, WindowCommand, ()>>,
//     response_publisher: Option<Publisher<ipc::Service, WindowResponse, ()>>,
//     window_details_publisher: Option<Publisher<ipc::Service, WindowDetails, ()>>, // Individual window details

//     // New services for grid layouts and animations
//     layout_publisher: Option<Publisher<ipc::Service, GridLayoutMessage, ()>>,
//     layout_subscriber: Option<Subscriber<ipc::Service, GridLayoutMessage, ()>>,
//     cell_assignment_publisher: Option<Publisher<ipc::Service, GridCellAssignment, ()>>,
//     cell_assignment_subscriber: Option<Subscriber<ipc::Service, GridCellAssignment, ()>>,
//     animation_publisher: Option<Publisher<ipc::Service, AnimationCommand, ()>>,
//     animation_subscriber: Option<Subscriber<ipc::Service, AnimationCommand, ()>>,
//     animation_status_publisher: Option<Publisher<ipc::Service, AnimationStatus, ()>>,
//     heartbeat_publisher: Option<Publisher<ipc::Service, HeartbeatMessage, ()>>,

//     is_running: bool,
// }

impl GridIpcManager {
    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            tracker,
            event_listeners: Vec::new(),
            node: None,
            event_publisher: None,
            command_subscriber: None,
            command_publisher: None,
            response_publisher: None,
            response_subscriber: None,
            window_details_publisher: None, // Initialize window details publisher
            layout_publisher: None,
            layout_subscriber: None,
            cell_assignment_publisher: None,
            cell_assignment_subscriber: None,
            animation_publisher: None,
            animation_subscriber: None,
            animation_status_publisher: None,
            heartbeat_publisher: None,
            window_list_subscriber: None,
            is_running: false,
        })
    }

    pub fn send_get_window_list_command(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use crate::ipc_protocol::{IpcCommand, IpcCommandType};
        let command = IpcCommand {
            command_type: IpcCommandType::GetWindowList,
            // ...fill other fields as needed, e.g., protocol_version...
            ..Default::default()
        };
        // There is no command_publisher, but you have response_publisher and event_publisher.
        // If you want to send a command, you need a command_publisher.
        // Let's add a command_publisher field to GridIpcManager and initialize it in setup_services.

        if let Some(ref mut publisher) = self.command_publisher {
            publisher.send_copy(command)?;
        }
        Ok(())
    }

    /// Setup only the requested IPC services. All booleans default to true for backward compatibility.
    pub fn setup_services(
        &mut self,
        events: bool,
        commands: bool,
        responses: bool,
        window_details: bool,
        layout: bool,
        cell_assignments: bool,
        animation: bool,
        animation_status: bool,
        heartbeat: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Setting up iceoryx2 IPC services...");

        // Create iceoryx2 node
        let node = NodeBuilder::new().create::<ipc::Service>()?;

        // Setup event publishing service
        if events {
            println!(
                "[IPC DEBUG] Opening event service: {} (WindowEvent)",
                GRID_EVENTS_SERVICE
            );
            let event_service = node
                .service_builder(&ServiceName::new(GRID_EVENTS_SERVICE)?)
                .publish_subscribe::<WindowEvent>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.event_publisher = Some(event_service.publisher_builder().create()?);
        }

        // Setup command subscription service
        if commands {
            println!(
                "[IPC DEBUG] Opening command service: {} (WindowCommand)",
                GRID_COMMANDS_SERVICE
            );
            let command_service = node
                .service_builder(&ServiceName::new(GRID_COMMANDS_SERVICE)?)
                .publish_subscribe::<IpcCommand>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.command_subscriber = Some(command_service.subscriber_builder().create()?);
        }

        // Setup response publishing service
        if responses {
            println!(
                "[IPC DEBUG] Opening response service: {} (IpcResponse)",
                GRID_RESPONSE_SERVICE
            );
            let response_service = node
                .service_builder(&ServiceName::new(GRID_RESPONSE_SERVICE)?)
                .publish_subscribe::<IpcResponse>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.response_publisher = Some(response_service.publisher_builder().create()?);
            self.response_subscriber = Some(response_service.subscriber_builder().create()?);
        }

        // Setup window details publishing service
        if window_details {
            println!(
                "[IPC DEBUG] Opening window details service: {} (WindowDetails)",
                GRID_WINDOW_DETAILS_SERVICE
            );
            let window_details_service = node
                .service_builder(&ServiceName::new(GRID_WINDOW_DETAILS_SERVICE)?)
                .publish_subscribe::<WindowDetails>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.window_details_publisher =
                Some(window_details_service.publisher_builder().create()?);
        }

        // Setup grid layout services
        if layout {
            println!(
                "[IPC DEBUG] Opening layout service: {} (GridLayoutMessage)",
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
        }

        // Setup cell assignment services
        if cell_assignments {
            println!(
                "[IPC DEBUG] Opening cell assignment service: {} (GridCellAssignment)",
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
        }

        // Setup animation services
        if animation {
            println!(
                "[IPC DEBUG] Opening animation service: {} (AnimationCommand)",
                ANIMATION_COMMANDS_SERVICE
            );
            let animation_service = node
                .service_builder(&ServiceName::new(ANIMATION_COMMANDS_SERVICE)?)
                .publish_subscribe::<AnimationCommand>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.animation_publisher = Some(animation_service.publisher_builder().create()?);
            self.animation_subscriber = Some(animation_service.subscriber_builder().create()?);
        }

        // Setup animation status service
        if animation_status {
            println!(
                "[IPC DEBUG] Opening animation status service: {} (AnimationStatus)",
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
        }

        // Setup heartbeat service
        if heartbeat {
            println!(
                "[IPC DEBUG] Opening heartbeat service: {} (HeartbeatMessage)",
                GRID_HEARTBEAT_SERVICE
            );
            let heartbeat_service = node
                .service_builder(&ServiceName::new(GRID_HEARTBEAT_SERVICE)?)
                .publish_subscribe::<HeartbeatMessage>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.heartbeat_publisher = Some(heartbeat_service.publisher_builder().create()?);
        }
        if commands {
            println!(
                "[IPC DEBUG] Opening command service: {} (IpcCommand)",
                GRID_COMMANDS_SERVICE
            );
            let command_service = node
                .service_builder(&ServiceName::new(GRID_COMMANDS_SERVICE)?)
                .publish_subscribe::<IpcCommand>()
                .max_publishers(8)
                .max_subscribers(8)
                .open_or_create()?;
            self.command_subscriber = Some(command_service.subscriber_builder().create()?);
            self.command_publisher = Some(command_service.publisher_builder().create()?);
        }
        // Store the node
        self.node = Some(node);
        info!("✅ iceoryx2 IPC services initialized successfully");
        if events {
            debug!("   📡 Event service: {}", GRID_EVENTS_SERVICE);
        }
        if commands {
            debug!("   📨 Command service: {}", GRID_COMMANDS_SERVICE);
        }
        if responses {
            debug!("   📤 Response service: {}", GRID_RESPONSE_SERVICE);
        }
        if window_details {
            debug!(
                "   📋 Window details service: {}",
                GRID_WINDOW_DETAILS_SERVICE
            );
        }
        if layout {
            debug!("   🗂️  Grid layout service: {}", GRID_LAYOUT_SERVICE);
        }
        if cell_assignments {
            debug!(
                "   📍 Cell assignment service: {}",
                GRID_CELL_ASSIGNMENTS_SERVICE
            );
        }
        if animation {
            debug!("   🎬 Animation service: {}", ANIMATION_COMMANDS_SERVICE);
        }
        if animation_status {
            debug!(
                "   📊 Animation status service: {}",
                ANIMATION_STATUS_SERVICE
            );
        }
        if heartbeat {
            debug!("   💓 Heartbeat service: {}", GRID_HEARTBEAT_SERVICE);
        }

        self.is_running = true;
        Ok(())
    }

    pub fn get_latest_window_list(&mut self) -> Option<WindowListMessage> {
        if let Some(ref mut subscriber) = self.window_list_subscriber {
            // Drain all available messages, return the last one (most recent)
            let mut latest = None;
            while let Some(sample) = subscriber.receive().ok().flatten() {
                latest = Some(*sample);
            }
            latest
        } else {
            None
        }
    }

    pub fn publish_event(&mut self, event: GridEvent) -> Result<(), Box<dyn std::error::Error>> {
        // Convert high-level event to zero-copy format
        let window_event = self.grid_event_to_window_event(&event);

        // Publish via iceoryx2
        if let Some(ref mut publisher) = self.event_publisher {
            publisher.send_copy(window_event)?;
            debug!("📡 Published event via iceoryx2: {:?}", event);
        }

        // Notify local listeners
        for listener in &self.event_listeners {
            listener(&event);
        }

        Ok(())
    }
    pub fn process_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut commands_to_process: Vec<IpcCommand> = Vec::new();

        // First, collect all incoming commands
        if let Some(ref mut subscriber) = self.command_subscriber {
            while let Some(sample) = subscriber.receive()? {
                let command = sample.clone();
                commands_to_process.push(command);
            }
        }

        // Then process each command
        for command in commands_to_process {
            debug!("📨 Received command: {:?}", command);

            // Convert to high-level command and process
            // let grid_command = Self::window_command_to_grid_command(&command);
            let response = self.handle_command(command)?;

            // Send response via iceoryx2
            self.send_response(response)?;
        }

        Ok(())
    }
    pub fn send_response(
        &mut self,
        response: IpcResponse,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref mut publisher) = self.response_publisher {
            debug!("📤 Sent response via iceoryx2: {:?}", &response);
            publisher.send_copy(response)?;
        }
        Ok(())
    }

    pub fn handle_command(
        &mut self,
        command: IpcCommand,
    ) -> Result<IpcResponse, Box<dyn std::error::Error>> {
        // Just print out the command for debugging
        println!("Received IpcCommand: {:?}", command);
        // Optionally, return a default response
        Ok(IpcResponse::default())
    }

    pub fn handle_grid_command(
        &mut self,
        command: GridCommand,
    ) -> Result<GridResponse, Box<dyn std::error::Error>> {
        match command {
            GridCommand::GetGridState => {
                if let Ok(tracker) = self.tracker.lock() {
                    let total_windows = tracker.windows.len();
                    let occupied_cells = self.count_occupied_cells(&tracker);

                    // Create a simple grid summary
                    let mut grid_summary = format!(
                        "Grid: {} windows, {} occupied cells\n",
                        total_windows, occupied_cells
                    );
                    grid_summary.push_str("Windows:\n");
                    for entry in tracker.windows.iter().take(10) {
                        // Limit to first 10 for brevity
                        let (hwnd, window) = entry.pair();
                        let title = if window.title.len() > 30 {
                            format!("{}...", String::from_utf16_lossy(&window.title[..30]))
                        } else {
                            String::from_utf16_lossy(&window.title)
                        };
                        grid_summary.push_str(&format!("  HWND {:?}: {}\n", hwnd, title));
                    }

                    if tracker.windows.len() > 10 {
                        grid_summary.push_str(&format!(
                            "  ... and {} more windows\n",
                            tracker.windows.len() - 10
                        ));
                    }

                    debug!(
                        "📊 Grid state: {} windows, {} occupied cells",
                        total_windows, occupied_cells
                    );
                    Ok(GridResponse::GridState {
                        total_windows,
                        occupied_cells,
                        grid_summary,
                    })
                } else {
                    Ok(GridResponse::Error(
                        "Failed to access window tracker".to_string(),
                    ))
                }
            }
            GridCommand::GetWindowList => {
                let hwnd_list = if let Ok(tracker) = self.tracker.lock() {
                    // Collect HWNDs for the window list response
                    tracker
                        .windows
                        .iter()
                        .map(|entry| *entry.key() as u64)
                        .collect::<Vec<u64>>()
                } else {
                    return Ok(GridResponse::Error(
                        "Failed to access window tracker".to_string(),
                    ));
                };

                debug!(
                    "📋 GetWindowList request - publishing details for {} windows",
                    hwnd_list.len()
                );

                // Publish individual window details for all windows
                if let Err(e) = self.publish_all_window_details() {
                    warn!("⚠️ Failed to publish window details: {}", e);
                }

                // Still return a simple response via the regular channel
                Ok(GridResponse::Success)
            }
            GridCommand::MoveWindowToCell {
                hwnd,
                target_row,
                target_col,
            } => {
                debug!(
                    "🎯 Request to move window {} to cell ({}, {})",
                    hwnd, target_row, target_col
                );

                // TODO: Implement actual window movement using Windows API
                match self.move_window_to_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!("Failed to move window: {}", e))),
                }
            }
            GridCommand::AssignWindowToVirtualCell {
                hwnd,
                target_row,
                target_col,
            } => {
                debug!(
                    "📍 Request to assign window {} to virtual grid cell ({}, {})",
                    hwnd, target_row, target_col
                );

                match self.assign_window_to_virtual_cell(hwnd, target_row, target_col) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!(
                        "Failed to assign window to virtual cell: {}",
                        e
                    ))),
                }
            }
            GridCommand::AssignWindowToMonitorCell {
                hwnd,
                target_row,
                target_col,
                monitor_id,
            } => {
                debug!(
                    "📍 Request to assign window {} to monitor {} cell ({}, {})",
                    hwnd, monitor_id, target_row, target_col
                );

                match self.assign_window_to_monitor_cell(hwnd, target_row, target_col, monitor_id) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!(
                        "Failed to assign window to monitor cell: {}",
                        e
                    ))),
                }
            }
            GridCommand::ApplyGridLayout {
                layout_name,
                duration_ms,
                easing_type,
            } => {
                info!(
                    "🗂️ Request to apply grid layout '{:?}' with {}ms duration",
                    layout_name, duration_ms
                );

                match std::str::from_utf8(&layout_name)
                    .map(|s| self.apply_saved_layout(s, duration_ms, easing_type))
                    .unwrap_or_else(|e| Err(format!("Invalid UTF-8 in layout_name: {}", e).into()))
                {
                    Ok(count) => {
                        info!(
                            "✅ Started {} animations for layout '{:?}'",
                            count, layout_name
                        );
                        Ok(GridResponse::Success)
                    }
                    Err(e) => Ok(GridResponse::Error(format!(
                        "Failed to apply layout: {}",
                        e
                    ))),
                }
            }
            GridCommand::SaveCurrentLayout { layout_name } => {
                info!("💾 Request to save current layout as '{:?}'", layout_name);

                match self.save_current_layout(String::from_utf8_lossy(&layout_name).to_string()) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!("Failed to save layout: {}", e))),
                }
            }
            GridCommand::GetSavedLayouts => {
                info!("📋 Request for saved layouts list");
                if let Ok(tracker) = self.tracker.lock() {
                    let layout_names: Vec<String> = tracker.list_saved_layouts();
                    Ok(GridResponse::SavedLayouts { layout_names })
                } else {
                    Ok(GridResponse::Error(
                        "Failed to access window tracker".to_string(),
                    ))
                }
            }
            GridCommand::StartAnimation {
                hwnd,
                target_x,
                target_y,
                target_width,
                target_height,
                duration_ms,
                easing_type,
            } => {
                info!(
                    "🎬 Request to animate window {} to ({}, {}) size {}x{}",
                    hwnd, target_x, target_y, target_width, target_height
                );

                let target_rect = winapi::shared::windef::RECT {
                    left: target_x,
                    top: target_y,
                    right: target_x + target_width as i32,
                    bottom: target_y + target_height as i32,
                };

                match self.start_window_animation(hwnd, target_rect, duration_ms, easing_type) {
                    Ok(_) => Ok(GridResponse::Success),
                    Err(e) => Ok(GridResponse::Error(format!(
                        "Failed to start animation: {}",
                        e
                    ))),
                }
            }
            GridCommand::GetAnimationStatus { hwnd } => {
                info!("📊 Request for animation status of window {}", hwnd);

                if let Ok(tracker) = self.tracker.lock() {
                    let statuses = if hwnd == 0 {
                        // Get status for all active animations
                        tracker
                            .active_animations
                            .iter()
                            .map(|entry| {
                                let (h, anim) = entry.pair();
                                let progress = if anim.is_completed() {
                                    1.0
                                } else {
                                    anim.start_time.elapsed().as_secs_f32()
                                        / anim.duration.as_secs_f32()
                                };
                                (*h as u64, !anim.is_completed(), progress)
                            })
                            .collect()
                    } else {
                        // Get status for specific window
                        if let Some(anim) = tracker.active_animations.get(&hwnd) {
                            let progress = if anim.is_completed() {
                                1.0
                            } else {
                                anim.start_time.elapsed().as_secs_f32()
                                    / anim.duration.as_secs_f32()
                            };
                            vec![(hwnd, !anim.is_completed(), progress)]
                        } else {
                            vec![(hwnd, false, 0.0)]
                        }
                    };

                    Ok(GridResponse::AnimationStatus { statuses })
                } else {
                    Ok(GridResponse::Error(
                        "Failed to access window tracker".to_string(),
                    ))
                }
            }
            GridCommand::GetGridConfig => {
                // This command should be handled by the server, not here
                // Return an error indicating this command is not supported in this context
                Ok(GridResponse::Error(
                    "GetGridConfig command not supported in this handler".to_string(),
                ))
            }
        }
    }
    fn move_window_to_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // TODO: Implement actual window movement logic
        // This would involve:
        // 1. Calculate target position based on grid dimensions
        // 2. Use Windows API to move the window
        // 3. Update the internal tracking
        debug!(
            "🔧 TODO: Implement window movement for HWND {} to ({}, {})",
            hwnd, target_row, target_col
        );
        Ok(())
    }

    // Assignment to virtual grid (coordinates span all monitors)
    fn assign_window_to_virtual_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            // Find the window in our tracking and save the title first
            let window_title = if let Some(window) = tracker.windows.get(&hwnd) {
                window.title.clone()
            } else {
                return Err(format!("Window with HWND {} not found in tracker", hwnd).into());
            };
            // Now we can safely modify the window
            if let Some(_window) = tracker.windows.get_mut(&hwnd) {
                // // Clear existing grid assignments for this window
                // window.grid_cells = [(0, 0); MAX_WINDOW_GRID_CELLS];

                // // Assign to the new cell (first slot)
                // window.grid_cells[0] = (target_row, target_col);
                info!(
                    "✅ Assigned window {} '{}' to virtual grid cell ({}, {})",
                    hwnd,
                    if window_title.len() > 30 {
                        format!("{}...", String::from_utf16_lossy(&window_title[..30]))
                    } else {
                        String::from_utf16_lossy(&window_title).to_string()
                    },
                    target_row,
                    target_col
                );
            }

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
                    let window_width = cell_width.max(100); // Minimum width of 100
                    let window_height = cell_height.max(50); // Minimum height of 50
                    let window_left = cell_left;
                    let window_top = cell_top;

                    debug!(
                        "🔧 Cell bounds: ({}, {}) to ({}, {}) [{}x{}]",
                        cell_left, cell_top, cell_right, cell_bottom, cell_width, cell_height
                    );
                    debug!(
                        "🎯 Window bounds: ({}, {}) [{}x{}]",
                        window_left, window_top, window_width, window_height
                    );

                    // Move the window to the calculated position
                    if let Err(e) = self.move_window_to_position(
                        hwnd,
                        window_left,
                        window_top,
                        window_width,
                        window_height,
                    ) {
                        warn!("⚠️ Failed to physically move window {}: {}", hwnd, e);
                    } else {
                        info!(
                            "🎯 Successfully moved window {} to virtual grid cell ({}, {})",
                            hwnd, target_row, target_col
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "⚠️ Failed to calculate position for virtual grid cell ({}, {}): {}",
                        target_row, target_col, e
                    );
                }
            }

            // Publish an event about the assignment
            let event = GridEvent::WindowMoved {
                hwnd,
                title: String::from_utf16_lossy(
                    &window_title[..window_title
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(window_title.len())],
                ),
                old_row: 0, // We don't track previous assignment currently
                old_col: 0,
                new_row: target_row,
                new_col: target_col,
                // TODO: Get actual position data from window tracker
                grid_top_left_row: target_row,
                grid_top_left_col: target_col,
                grid_bottom_right_row: target_row,
                grid_bottom_right_col: target_col,
                real_x: 0,
                real_y: 0,
                real_width: 0,
                real_height: 0,
                monitor_id: 0,
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
    fn assign_window_to_monitor_cell(
        &mut self,
        hwnd: u64,
        target_row: usize,
        target_col: usize,
        monitor_id: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            // Find the window in our tracking and save the title first
            let window_title = if let Some(window) = tracker.windows.get(&hwnd) {
                window.title.clone()
            } else {
                return Err(format!("Window with HWND {} not found in tracker", hwnd).into());
            };

            // Check if the monitor exists
            if monitor_id >= tracker.monitor_grids.len() {
                return Err(format!(
                    "Monitor {} does not exist. Available monitors: 0-{}",
                    monitor_id,
                    tracker.monitor_grids.len() - 1
                )
                .into());
            }
            // Now we can safely modify the window
            if let Some(_window) = tracker.windows.get_mut(&hwnd) {
                // // Clear existing monitor assignments for this window
                // for row in window.monitor_cells.iter_mut() {
                //     for cell in row.iter_mut() {
                //         *cell = (0, 0);
                //     }
                // }

                // // Assign to the new monitor cell
                // if monitor_id < window.monitor_cells.len() {
                //     window.monitor_cells[monitor_id][0] = (target_row, target_col);
                // } else {
                //     warn!(
                //         "⚠️ Monitor ID {} out of bounds for monitor_cells array",
                //         monitor_id
                //     );
                // }
                debug!(
                    "✅ Assigned window {} '{}' to monitor {} grid cell ({}, {})",
                    hwnd,
                    if window_title.len() > 30 {
                        format!("{}...", String::from_utf16_lossy(&window_title[..30]))
                    } else {
                        String::from_utf16_lossy(&window_title)
                    },
                    monitor_id,
                    target_row,
                    target_col
                );
            }

            // Update both virtual and monitor grids
            tracker.update_grid();
            tracker.update_monitor_grids();

            // Release the tracker lock before moving the window
            drop(tracker);
            // Calculate the target position for the monitor grid cell
            match self.calculate_monitor_cell_position(target_row, target_col, monitor_id) {
                Ok((cell_left, cell_top, cell_right, cell_bottom)) => {
                    // Calculate cell dimensions
                    let cell_width = cell_right - cell_left;
                    let cell_height = cell_bottom - cell_top;

                    // Calculate window position and size to fill the cell
                    let window_width = cell_width.max(100); // Minimum width of 100
                    let window_height = cell_height.max(50); // Minimum height of 50
                    let window_left = cell_left;
                    let window_top = cell_top;
                    debug!(
                        "🔧 Monitor {} cell bounds: ({}, {}) to ({}, {}) [{}x{}]",
                        monitor_id,
                        cell_left,
                        cell_top,
                        cell_right,
                        cell_bottom,
                        cell_width,
                        cell_height
                    );
                    debug!(
                        "🎯 Window bounds: ({}, {}) [{}x{}]",
                        window_left, window_top, window_width, window_height
                    );

                    // Move the window to the calculated position
                    if let Err(e) = self.move_window_to_position(
                        hwnd,
                        window_left,
                        window_top,
                        window_width,
                        window_height,
                    ) {
                        warn!("⚠️ Failed to physically move window {}: {}", hwnd, e);
                    } else {
                        info!(
                            "🎯 Successfully moved window {} to monitor {} grid cell ({}, {})",
                            hwnd, monitor_id, target_row, target_col
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "⚠️ Failed to calculate position for monitor {} grid cell ({}, {}): {}",
                        monitor_id, target_row, target_col, e
                    );
                }
            }
            // Publish an event about the assignment
            let event = GridEvent::WindowMoved {
                hwnd,
                title: String::from_utf16_lossy(&window_title),
                old_row: 0, // We don't track previous assignment currently
                old_col: 0,
                new_row: target_row,
                new_col: target_col,
                // TODO: Get actual position data from window tracker
                grid_top_left_row: target_row,
                grid_top_left_col: target_col,
                grid_bottom_right_row: target_row,
                grid_bottom_right_col: target_col,
                real_x: 0,
                real_y: 0,
                real_width: 0,
                real_height: 0,
                monitor_id: monitor_id as u32,
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
        // let mut occupied = std::collections::HashSet::new();
        // for entry in &tracker.windows {
        //     let (_, window) = entry.pair();
        //     for &(row, col) in &window.grid_cells {
        //         occupied.insert((row, col));
        //     }
        // }
        // occupied.len()
        0
    } // Conversion functions between high-level and zero-copy types
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
                real_width: real_width2,
                real_height: real_height2,
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
                real_width: *real_width2,
                real_height: *real_height2,
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
                real_width: real_width2,
                real_height: real_height2,
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
                real_width: *real_width2,
                real_height: *real_height2,
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
            GridEvent::WindowFocused { hwnd, .. } => WindowEvent {
                event_type: 4, // Focus event type
                hwnd: *hwnd,
                timestamp,
                ..Default::default()
            },
            GridEvent::WindowDefocused { hwnd, .. } => WindowEvent {
                event_type: 5, // Defocus event type
                hwnd: *hwnd,
                timestamp,
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
            },
            5 => GridCommand::ApplyGridLayout {
                layout_name: {
                    let mut s: heapless::String<64> = heapless::String::new();
                    use core::fmt::Write;
                    let _ = write!(s, "layout_{}", command.layout_id);
                    let mut arr = [0u8; 64];
                    let bytes = s.as_bytes();
                    let len = bytes.len().min(64);
                    arr[..len].copy_from_slice(&bytes[..len]);
                    arr
                },
                duration_ms: command.animation_duration_ms,
                easing_type: match command.easing_type {
                    0 => crate::EasingType::Linear,
                    1 => crate::EasingType::EaseIn,
                    2 => crate::EasingType::EaseOut,
                    3 => crate::EasingType::EaseInOut,
                    4 => crate::EasingType::Bounce,
                    5 => crate::EasingType::Elastic,
                    6 => crate::EasingType::Back,
                    _ => crate::EasingType::Linear,
                },
            },
            6 => GridCommand::SaveCurrentLayout {
                layout_name: {
                    let mut s: heapless::String<64> = heapless::String::new();
                    use core::fmt::Write;
                    let _ = write!(s, "layout_{}", command.layout_id);
                    let mut arr = [0u8; 64];
                    let bytes = s.as_bytes();
                    let len = bytes.len().min(64);
                    arr[..len].copy_from_slice(&bytes[..len]);
                    arr
                },
            },
            7 => GridCommand::GetSavedLayouts,
            _ => GridCommand::GetGridState, // Default fallback
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
            GridResponse::GridState {
                total_windows,
                occupied_cells,
                ..
            } => WindowResponse {
                response_type: 3, // New response type for grid state
                error_code: 0,
                window_count: *total_windows as u32,
                data: [*occupied_cells as u64, 0, 0, 0], // Pack occupied_cells into first data slot
            },
            GridResponse::SavedLayouts { layout_names } => WindowResponse {
                response_type: 4, // Saved layouts response
                error_code: 0,
                window_count: layout_names.len() as u32,
                ..Default::default()
            },
            GridResponse::AnimationStatus { statuses } => WindowResponse {
                response_type: 5, // Animation status response
                error_code: 0,
                window_count: statuses.len() as u32,
                data: [
                    statuses.len() as u64,
                    statuses.iter().filter(|(_, active, _)| *active).count() as u64,
                    0,
                    0,
                ],
            },
            GridResponse::GridConfig(_) => WindowResponse {
                response_type: 6, // Grid config response
                error_code: 0,
                ..Default::default()
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
        info!("🔄 Starting iceoryx2 IPC event loop...");

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
        info!("🛑 iceoryx2 IPC event loop stopped");
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
    }

    // Publish individual window details for real-time client updates
    pub fn publish_window_details(&mut self, hwnd: HWND) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            if let Some(window_info) = tracker.windows.get(&(hwnd as u64)) {
                let details = self.window_info_to_details(hwnd, &*window_info);
                if let Some(ref mut publisher) = self.window_details_publisher {
                    publisher.send_copy(details)?;
                    debug!("📤 Published window details for HWND {:?}", hwnd);
                } else {
                    warn!("⚠️ Window details publisher not available");
                }
            }
        }
        Ok(())
    }

    // Publish details for all current windows (useful for client initialization)
    pub fn publish_all_window_details(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            let window_count = tracker.windows.len();
            debug!("📤 Publishing details for {} windows...", window_count);
            for entry in &tracker.windows {
                let (hwnd, window_info) = entry.pair();
                let details = self.window_info_to_details(*hwnd as HWND, &*window_info);

                if let Some(ref mut publisher) = self.window_details_publisher {
                    publisher.send_copy(details)?;
                } else {
                    warn!("⚠️ Window details publisher not available");
                    break;
                }
            }

            info!("✅ Published details for {} windows", window_count);
        }
        Ok(())
    }

    // Calculate position for virtual grid (coordinates span all monitors)
    fn calculate_virtual_cell_position(
        &self,
        target_row: usize,
        target_col: usize,
    ) -> Result<(i32, i32, i32, i32), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            // Use the virtual grid (spanning all monitors)
            let (left, top, right, bottom) = (
                tracker.monitor_rect.left,
                tracker.monitor_rect.top,
                tracker.monitor_rect.right,
                tracker.monitor_rect.bottom,
            );

            let cell_width = (right - left) / tracker.config.cols as i32;
            let cell_height = (bottom - top) / tracker.config.rows as i32;

            if target_row < tracker.config.rows && target_col < tracker.config.cols {
                let cell_left = left + (target_col as i32 * cell_width);
                let cell_top = top + (target_row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;
                debug!("🔧 Calculated VIRTUAL GRID cell position for ({}, {}): screen coords ({}, {}) to ({}, {})", 
                    target_row, target_col, cell_left, cell_top, cell_right, cell_bottom);
                debug!(
                    "   Virtual screen bounds: ({}, {}) to ({}, {})",
                    left, top, right, bottom
                );

                Ok((cell_left, cell_top, cell_right, cell_bottom))
            } else {
                Err(format!(
                    "Invalid virtual grid coordinates: ({}, {}). Max is ({}, {})",
                    target_row,
                    target_col,
                    tracker.config.rows - 1,
                    tracker.config.cols - 1
                )
                .into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    // Calculate position for specific monitor grid
    fn calculate_monitor_cell_position(
        &self,
        target_row: usize,
        target_col: usize,
        monitor_id: usize,
    ) -> Result<(i32, i32, i32, i32), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            // Get the specific monitor
            if monitor_id >= tracker.monitor_grids.len() {
                return Err(format!(
                    "Monitor {} does not exist. Available monitors: 0-{}",
                    monitor_id,
                    tracker.monitor_grids.len() - 1
                )
                .into());
            }

            let monitor = &tracker.monitor_grids[monitor_id];
            let left = monitor.monitor_rect.left;
            let top = monitor.monitor_rect.top;
            let right = monitor.monitor_rect.right;
            let bottom = monitor.monitor_rect.bottom;
            let cell_width = (right - left) / monitor.config.cols as i32;
            let cell_height = (bottom - top) / monitor.config.rows as i32;

            if target_row < monitor.config.rows && target_col < monitor.config.cols {
                let cell_left = left + (target_col as i32 * cell_width);
                let cell_top = top + (target_row as i32 * cell_height);
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;
                debug!("🔧 Calculated MONITOR {} GRID cell position for ({}, {}): screen coords ({}, {}) to ({}, {})", 
                    monitor_id, target_row, target_col, cell_left, cell_top, cell_right, cell_bottom);
                debug!(
                    "   Monitor {} bounds: ({}, {}) to ({}, {})",
                    monitor_id, left, top, right, bottom
                );

                Ok((cell_left, cell_top, cell_right, cell_bottom))
            } else {
                Err(format!(
                    "Invalid monitor grid coordinates: ({}, {}). Max is ({}, {})",
                    target_row,
                    target_col,
                    monitor.config.rows - 1,
                    monitor.config.cols - 1
                )
                .into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }
    fn move_window_to_position(
        &self,
        hwnd: u64,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use winapi::um::winuser::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

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
                info!(
                    "🔧 Moved window {} to position ({}, {}) with size {}x{}",
                    hwnd, left, top, width, height
                );
                // Rescan the grid to update internal tracking after the window move
                if let Ok(mut tracker) = self.tracker.lock() {
                    debug!("🔄 Rescanning grid after window movement...");
                    // Update the window's rectangle in our tracking
                    if let Some(new_rect) = crate::WindowTracker::get_window_rect(hwnd) {
                        if let Some(mut window) = tracker.windows.get_mut(&hwnd) {
                            window.window_rect =
                                crate::window::info::RectWrapper::from_rect(new_rect);
                            debug!(
                                "   📍 Updated window {} rect to ({}, {}) - ({}, {})",
                                hwnd, new_rect.left, new_rect.top, new_rect.right, new_rect.bottom
                            );
                        }

                        // Now recalculate grid cells after releasing the mutable reference
                        let grid_cells = tracker.window_to_grid_cells(&new_rect);
                        let monitor_cells_map = tracker.calculate_monitor_cells(&new_rect);
                        // Update the window with the new grid assignments
                        if let Some(window) = tracker.windows.get_mut(&(hwnd_handle as u64)) {
                            // Convert Vec<(usize, usize)> to [(usize, usize); 16]
                            // let mut arr = [(0usize, 0usize); crate::MAX_WINDOW_GRID_CELLS];
                            // for (i, val) in
                            //     grid_cells.iter().take(MAX_WINDOW_GRID_CELLS).enumerate()
                            // {
                            //     arr[i] = *val;
                            // }
                            // window.grid_cells = arr;

                            // Convert HashMap<usize, Vec<(usize, usize)>> to [[(usize, usize); 8]; 8]
                            let mut monitor_cells_arr = [[(0usize, 0usize); 8]; 8];
                            for (monitor_idx, cells_vec) in monitor_cells_map.iter() {
                                if *monitor_idx < 8 {
                                    for (cell_idx, cell) in cells_vec.iter().take(8).enumerate() {
                                        monitor_cells_arr[*monitor_idx][cell_idx] = *cell;
                                    }
                                }
                            }
                            // window.monitor_cells = monitor_cells_arr;

                            // debug!("   🔄 Recalculated grid assignments: {} virtual cells, {} monitor assignments",
                            //     window.grid_cells.len(), window.monitor_cells.len());
                        }
                    }
                    // Update both virtual and monitor grids
                    tracker.update_grid();
                    tracker.update_monitor_grids();

                    debug!("✅ Grid rescan complete after window movement");
                } else {
                    warn!("⚠️ Failed to acquire tracker lock for grid rescan");
                }

                Ok(())
            }
        }
    }

    // Helper function to convert WindowInfo to WindowDetails for IPC
    fn window_info_to_details(&self, hwnd: HWND, window_info: &crate::WindowInfo) -> WindowDetails {
        // Calculate grid positions for the window
        // Construct a RECT from the WindowInfo fields
        let rect = winapi::shared::windef::RECT {
            left: window_info.window_rect.left,
            top: window_info.window_rect.top,
            right: window_info.window_rect.right,
            bottom: window_info.window_rect.bottom,
        };

        // Get virtual grid positions
        let (virtual_start_row, virtual_start_col, virtual_end_row, virtual_end_col) =
            if let Ok(tracker) = self.tracker.lock() {
                let cells = tracker.window_to_grid_cells(&rect);
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
                    let left = monitor.monitor_rect.left;
                    let top = monitor.monitor_rect.top;
                    let right = monitor.monitor_rect.right;
                    let bottom = monitor.monitor_rect.bottom;
                    if center_x >= left && center_x < right && center_y >= top && center_y < bottom
                    {
                        // Get grid positions within this monitor
                        let cells = monitor.window_to_grid_cells(&rect);
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
            // title_len: window_info.title.len().min(255) as u32, // Cap at 255 chars
            // title: {
            //     let s = String::from_utf16_lossy(&window_info.title)
            //         .chars()
            //         .take(255)
            //         .collect::<String>();
            //     let mut arr = [0u8; 256];
            //     let bytes = s.as_bytes();
            //     let len = bytes.len().min(256);
            //     arr[..len].copy_from_slice(&bytes[..len]);
            //     arr
            // },
        }
    }

    // Grid Layout and Animation Methods
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

    pub fn update_animations(
        &mut self,
    ) -> Result<(Vec<u64>, Vec<u64>), Box<dyn std::error::Error>> {
        if let Ok(mut tracker) = self.tracker.lock() {
            let (completed, failed) = tracker.update_animations();
            Ok((
                completed.into_iter().map(|hwnd| hwnd as u64).collect(),
                failed.into_iter().map(|hwnd| hwnd as u64).collect(),
            ))
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }

    pub fn publish_grid_layout(
        &mut self,
        layout_name: &str,
        config: &GridConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Ok(tracker) = self.tracker.lock() {
            if let Some(layout) = tracker.get_saved_layout(layout_name) {
                // Create layout message
                let layout_message = GridLayoutMessage {
                    message_type: 0,                                        // apply_layout
                    layout_id: layout_name.chars().map(|c| c as u32).sum(), // Simple hash
                    animation_duration_ms: 1000,                            // Default duration
                    easing_type: 0,                                         // Linear
                    grid_rows: config.rows as u8,
                    grid_cols: config.cols as u8,
                    total_cells: layout
                        .virtual_grid
                        .iter()
                        .flatten()
                        .filter(|cell| cell.is_some())
                        .count() as u16,
                    layout_name_hash: layout_name.chars().map(|c| c as u64).sum(),
                };

                // Send layout header
                if let Some(ref mut publisher) = self.layout_publisher {
                    publisher.send_copy(layout_message)?;
                }

                // Send individual cell assignments
                if let Some(ref mut cell_publisher) = self.cell_assignment_publisher {
                    for row in 0..config.rows {
                        for col in 0..config.cols {
                            if let Some(hwnd) = layout.virtual_grid[row][col] {
                                let cell_assignment = GridCellAssignment {
                                    row: row as u8,
                                    col: col as u8,
                                    hwnd: hwnd as u64,
                                    monitor_id: 255, // Virtual grid
                                    reserved: [0; 5],
                                };
                                cell_publisher.send_copy(cell_assignment)?;
                            }
                        }
                    }
                }

                info!(
                    "📤 Published grid layout '{}' with {} occupied cells",
                    layout_name, layout_message.total_cells
                );
                Ok(())
            } else {
                Err(format!("Layout '{}' not found", layout_name).into())
            }
        } else {
            Err("Failed to acquire tracker lock".into())
        }
    }
    pub fn process_layout_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut commands_to_process = Vec::new();

        // First, collect all incoming commands
        if let Some(ref mut subscriber) = self.layout_subscriber {
            while let Some(sample) = subscriber.receive()? {
                commands_to_process.push(*sample);
            }
        }

        // Then process each command
        for layout_msg in commands_to_process {
            debug!("🗂️ Received layout command: {:?}", layout_msg);

            match layout_msg.message_type {
                0 => {
                    // apply_layout
                    // This would typically be handled by receiving cell assignments
                    info!("📥 Layout application request received");
                }
                1 => {
                    // save_current_layout
                    let layout_name = format!("layout_{}", layout_msg.layout_id);
                    if let Err(e) = self.save_current_layout(layout_name.clone()) {
                        warn!("⚠️ Failed to save layout {}: {}", layout_name, e);
                    }
                }
                2 => {
                    // get_saved_layouts
                    // Send response with saved layouts
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
        Ok(())
    }
    pub fn process_animation_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut commands_to_process = Vec::new();

        // First, collect all incoming commands
        if let Some(ref mut subscriber) = self.animation_subscriber {
            while let Some(sample) = subscriber.receive()? {
                commands_to_process.push(*sample);
            }
        }

        // Then process each command
        for anim_cmd in commands_to_process {
            debug!("🎬 Received animation command: {:?}", anim_cmd);

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

                    if let Err(e) = self.start_window_animation(
                        anim_cmd.hwnd,
                        target_rect,
                        anim_cmd.duration_ms,
                        easing_type,
                    ) {
                        warn!(
                            "⚠️ Failed to start animation for window {}: {}",
                            anim_cmd.hwnd, e
                        );
                    }
                }
                1 => {
                    // stop_animation
                    if let Ok(tracker) = self.tracker.lock() {
                        if anim_cmd.hwnd == 0 {
                            tracker.active_animations.clear();
                            info!("🛑 Stopped all animations");
                        } else {
                            tracker.active_animations.remove(&anim_cmd.hwnd);
                            info!("🛑 Stopped animation for window {}", anim_cmd.hwnd);
                        }
                    }
                }
                4 => {
                    // get_status
                    // Send animation status - this could be enhanced to send via status publisher
                    debug!("📊 Animation status request for window {}", anim_cmd.hwnd);
                }
                _ => {
                    warn!(
                        "⚠️ Unknown animation command type: {}",
                        anim_cmd.command_type
                    );
                }
            }
        }
        Ok(())
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
}
