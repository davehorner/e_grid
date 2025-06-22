use e_grid::{ipc, window_events, WindowTracker};
use iceoryx2::prelude::*;
use std::env;
use std::io::{self, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};

fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔌 E-Grid IPC Client Starting");
    println!("=============================");

    // Create iceoryx2 node for client
    let node = NodeBuilder::new().create::<iceoryx2::service::ipc::Service>()?;

    // Subscribe to events
    let event_service = node
        .service_builder(&ServiceName::new(ipc::GRID_EVENTS_SERVICE)?)
        .publish_subscribe::<ipc::WindowEvent>()
        .open()?;

    let event_subscriber = event_service.subscriber_builder().create()?;

    // Subscribe to responses
    let response_service = node
        .service_builder(&ServiceName::new(ipc::GRID_RESPONSE_SERVICE)?)
        .publish_subscribe::<ipc::WindowResponse>()
        .open()?;
    let response_subscriber = response_service.subscriber_builder().create()?;
    // Subscribe to individual window details
    let window_details_service = node
        .service_builder(&ServiceName::new(ipc::GRID_WINDOW_DETAILS_SERVICE)?)
        .publish_subscribe::<ipc::WindowDetails>()
        .open()?;

    let window_details_subscriber = window_details_service.subscriber_builder().create()?;

    // Create command publisher
    let command_service = node
        .service_builder(&ServiceName::new(ipc::GRID_COMMANDS_SERVICE)?)
        .publish_subscribe::<ipc::WindowCommand>()
        .open()?;

    let command_publisher = command_service.publisher_builder().create()?;

    println!("✅ Connected to IPC services");
    println!("📡 Listening for events and responses...");
    println!("📋 Available commands:");
    println!("   • 'assign' - Assign a window to a grid cell");
    println!("   • 'list' - List all windows");
    println!("   • 'grid' - Show grid state");
    println!("   • 'quit' - Exit client");

    // Interactive client loop
    loop {
        print!("\n[CLIENT] > ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        match input.as_str() {
            "assign" => {
                // Get assignment mode
                print!("Choose assignment mode (v=virtual, m=monitor): ");
                io::stdout().flush()?;
                let mut mode_input = String::new();
                io::stdin().read_line(&mut mode_input)?;
                let mode = mode_input.trim().to_lowercase();
                if mode == "v" || mode == "virtual" {
                    // Virtual grid assignment
                    print!("Enter window HWND: ");
                    io::stdout().flush()?;
                    let mut hwnd_input = String::new();
                    io::stdin().read_line(&mut hwnd_input)?;
                    let hwnd: u64 = hwnd_input.trim().parse().unwrap_or(0);

                    print!("Enter target row (0-based): ");
                    io::stdout().flush()?;
                    let mut row_input = String::new();
                    io::stdin().read_line(&mut row_input)?;
                    let row: u32 = row_input.trim().parse().unwrap_or(0);

                    print!("Enter target column (0-based): ");
                    io::stdout().flush()?;
                    let mut col_input = String::new();
                    io::stdin().read_line(&mut col_input)?;
                    let col: u32 = col_input.trim().parse().unwrap_or(0);

                    let command = ipc::WindowCommand {
                        command_type: 5, // AssignToVirtualCell
                        hwnd,
                        target_row: row,
                        target_col: col,
                        monitor_id: 0, // Not used for virtual assignment
                        layout_id: 0,
                        animation_duration_ms: 0,
                        easing_type: 0,
                    };

                    command_publisher.send_copy(command)?;
                    println!(
                        "📤 Sent virtual assignment command: HWND {} to ({}, {})",
                        hwnd, row, col
                    );
                } else if mode == "m" || mode == "monitor" {
                    // Monitor grid assignment
                    print!("Enter window HWND: ");
                    io::stdout().flush()?;
                    let mut hwnd_input = String::new();
                    io::stdin().read_line(&mut hwnd_input)?;
                    let hwnd: u64 = hwnd_input.trim().parse().unwrap_or(0);

                    print!("Enter monitor ID: ");
                    io::stdout().flush()?;
                    let mut monitor_input = String::new();
                    io::stdin().read_line(&mut monitor_input)?;
                    let monitor_id: u32 = monitor_input.trim().parse().unwrap_or(0);
                    print!("Enter target row (0-based): ");
                    io::stdout().flush()?;
                    let mut row_input = String::new();
                    io::stdin().read_line(&mut row_input)?;
                    let row: u32 = row_input.trim().parse().unwrap_or(0);

                    print!("Enter target column (0-based): ");
                    io::stdout().flush()?;
                    let mut col_input = String::new();
                    io::stdin().read_line(&mut col_input)?;
                    let col: u32 = col_input.trim().parse().unwrap_or(0);

                    let command = ipc::WindowCommand {
                        command_type: 6, // AssignToMonitorCell
                        hwnd,
                        target_row: row,
                        target_col: col,
                        monitor_id,
                        layout_id: 0,
                        animation_duration_ms: 0,
                        easing_type: 0,
                    };

                    command_publisher.send_copy(command)?;
                    println!(
                        "📤 Sent monitor assignment command: HWND {} to Monitor {} ({}, {})",
                        hwnd, monitor_id, row, col
                    );
                } else {
                    println!("❌ Invalid assignment mode. Use 'v' for virtual or 'm' for monitor.");
                }
            }
            "list" | "w" => {
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
                command_publisher.send_copy(command)?;
                println!("Requested window list");
                // Wait and aggressively check for window details
                println!("Searching for window details...");
                for attempt in 1..=10 {
                    std::thread::sleep(std::time::Duration::from_millis(50));

                    // Process individual window details
                    while let Some(window_details_sample) = window_details_subscriber.receive()? {
                        let details = *window_details_sample;
                        println!(
                            "Window Details: HWND {} at ({}, {}) size {}x{}",
                            details.hwnd, details.x, details.y, details.width, details.height
                        );
                        println!(
                            "  Virtual Grid: ({}, {}) to ({}, {})",
                            details.virtual_row_start,
                            details.virtual_col_start,
                            details.virtual_row_end,
                            details.virtual_col_end
                        );
                        println!(
                            "  Monitor {}: ({}, {}) to ({}, {})",
                            details.monitor_id,
                            details.monitor_row_start,
                            details.monitor_col_start,
                            details.monitor_row_end,
                            details.monitor_col_end
                        );
                        println!("  Title length: {}", details.title_len);
                        println!();
                    }
                }
            }
            "grid" => {
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
                command_publisher.send_copy(command)?;
                println!("📤 Sent GetGridState command");
            }
            "quit" | "exit" | "q" => {
                println!("👋 Client shutting down...");
                break;
            }
            _ => {
                println!("❓ Unknown command. Available: assign, list, grid, quit");
            }
        }

        // Check for events and responses after each command
        while let Some(event_sample) = event_subscriber.receive()? {
            let event = *event_sample;
            println!("📡 [EVENT] {:?}", event);

            match event.event_type {
                0 => println!(
                    "   ✨ Window Created: HWND {} at ({}, {})",
                    event.hwnd, event.row, event.col
                ),
                1 => println!("   💥 Window Destroyed: HWND {}", event.hwnd),
                2 => println!(
                    "   🔄 Window Moved: HWND {} from ({}, {}) to ({}, {})",
                    event.hwnd, event.old_row, event.old_col, event.row, event.col
                ),
                3 => println!(
                    "   📊 Grid State: {} windows, {} occupied cells",
                    event.total_windows, event.occupied_cells
                ),
                _ => println!("   ❓ Unknown event type: {}", event.event_type),
            }
        }

        while let Some(response_sample) = response_subscriber.receive()? {
            let response = *response_sample;
            println!("� [RESPONSE] {:?}", response);

            match response.response_type {
                0 => println!("   ✅ Success"),
                1 => println!("   ❌ Error (code: {})", response.error_code),
                2 => println!("   📋 Window List: {} windows", response.window_count),
                _ => println!("   ❓ Unknown response type: {}", response.response_type),
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    // Check if running as client
    if args.len() > 1 && args[1] == "--client" {
        return run_client();
    }

    println!("🚀 E-Grid with IPC Integration Demo (Server)");
    println!("=============================================");

    // Spawn client process
    println!("🔄 Spawning IPC client process...");
    let current_exe = env::current_exe()?;
    let _client_process = Command::new(current_exe).arg("--client").spawn()?;
    // Give client time to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Create window tracker
    let mut tracker = WindowTracker::new();
    println!("📊 Initial scan for windows...");
    tracker.scan_existing_windows();

    // Initialize grid displays
    tracker.initialize_monitor_grids();
    tracker.update_monitor_grids();
    tracker.print_all_grids();

    let tracker_arc = Arc::new(Mutex::new(tracker));
    // Set up IPC manager
    let mut ipc_manager = ipc::GridIpcManager::new(tracker_arc.clone())?;
    ipc_manager.setup_services()?;
    // We can't move IPC manager to background thread due to iceoryx2 Send/Sync constraints
    // Instead, we'll process commands in the main loop with non-blocking input

    // Set up window event hooks
    let config = window_events::WindowEventConfig::new(tracker_arc.clone()).with_debug(true);
    match window_events::setup_window_events(config) {
        Ok(_) => println!("✅ Window event hooks set up successfully!"),
        Err(e) => {
            eprintln!("❌ Failed to set up window event hooks: {}", e);
            return Err(e.into());
        }
    }
    println!("\n🔄 Starting integrated event monitoring with IPC...");
    println!("📢 FEATURES AVAILABLE:");
    println!("   • Real-time window event tracking");
    println!("   • Continuous IPC command processing (no blocking)");
    println!("   • IPC event publishing");
    println!("   • Multi-monitor grid support");
    println!("   • Type 'g' to show grid, 'r' to rescan, 'q' to quit");

    // Use a channel to receive user input in a non-blocking way
    let (tx, rx) = std::sync::mpsc::channel();

    // Spawn a thread to handle user input
    std::thread::spawn(move || {
        loop {
            print!("\n> ");
            io::stdout().flush().unwrap();

            let mut input = String::new();
            if io::stdin().read_line(&mut input).is_ok() {
                if tx.send(input.trim().to_lowercase()).is_err() {
                    break; // Main thread has exited
                }
            }
        }
    });

    println!("🚀 Server ready! IPC commands will be processed continuously.");
    println!("   Commands are handled in real-time while you can still interact with the server.");

    // Main event loop with continuous IPC processing
    loop {
        // Process IPC commands continuously (non-blocking)
        if let Err(e) = ipc_manager.process_commands() {
            eprintln!("❌ IPC command processing error: {}", e);
        }
        // Check for user input (non-blocking)
        if let Ok(input) = rx.try_recv() {
            match input.as_str() {
                "g" | "grid" => {
                    if let Ok(tracker) = tracker_arc.lock() {
                        tracker.print_all_grids();
                    }
                }
                "r" | "rescan" => {
                    if let Ok(mut tracker) = tracker_arc.lock() {
                        println!("🔄 Rescanning windows...");
                        tracker.scan_existing_windows();
                        tracker.update_monitor_grids();
                        tracker.print_all_grids();
                    }
                }
                "e" | "event" => {
                    // Demo IPC event publishing
                    let event = ipc::GridEvent::GridStateChanged {
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        total_windows: {
                            if let Ok(tracker) = tracker_arc.lock() {
                                tracker.windows.len()
                            } else {
                                0
                            }
                        },
                        occupied_cells: 0, // Would calculate actual occupied cells
                    };
                    ipc_manager.publish_event(event)?;
                    println!("📤 Published demo event");
                }
                "c" | "commands" => {
                    // Show IPC processing status
                    println!("📨 IPC commands are being processed continuously");
                    println!("   Commands are handled automatically in real-time");
                }
                "h" | "help" => {
                    println!("📋 Available commands:");
                    println!("   g/grid    - Show current grid state");
                    println!("   r/rescan  - Rescan all windows");
                    println!("   e/event   - Publish demo IPC event");
                    println!("   c/commands- Show IPC processing status");
                    println!("   h/help    - Show this help");
                    println!("   q/quit    - Exit program");
                    println!("📡 Note: IPC commands are processed continuously in main loop");
                }
                "q" | "quit" | "exit" => {
                    println!("🧹 Cleaning up...");
                    window_events::cleanup_hooks();
                    println!("👋 Goodbye!");
                    break;
                }
                _ => {
                    println!("❓ Unknown command '{}'. Type 'h' for help.", input);
                }
            }
        }

        // Small delay to prevent busy waiting while still being responsive
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}
