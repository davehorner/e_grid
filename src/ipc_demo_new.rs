use e_grid::{ipc, window_events, WindowTracker};
use iceoryx2::prelude::*;
use std::env;
use std::io::{self, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};

fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    // Generate a simple client ID
    let client_id = std::process::id() % 1000; // Use process ID mod 1000 for short ID

    println!("🔌 E-Grid IPC Client Starting (ID: {})", client_id);
    println!("======================================");

    // Add a delay to ensure server is ready
    std::thread::sleep(std::time::Duration::from_secs(2));
    // Create iceoryx2 node for client
    let node = match NodeBuilder::new().create::<iceoryx2::service::ipc::Service>() {
        Ok(node) => {
            println!("✅ [CLIENT {}] Node created successfully", client_id);
            node
        }
        Err(e) => {
            println!("❌ [CLIENT {}] Failed to create node: {}", client_id, e);
            return Err(e.into());
        }
    };

    // Subscribe to events
    let event_service = match node
        .service_builder(&ServiceName::new(ipc::GRID_EVENTS_SERVICE)?)
        .publish_subscribe::<ipc::WindowEvent>()
        .open()
    {
        Ok(service) => {
            println!("✅ [CLIENT {}] Connected to event service", client_id);
            service
        }
        Err(e) => {
            println!(
                "❌ [CLIENT {}] Failed to connect to event service: {}",
                client_id, e
            );
            return Err(e.into());
        }
    };

    let mut event_subscriber = event_service.subscriber_builder().create()?;

    // Subscribe to responses
    let response_service = node
        .service_builder(&ServiceName::new(ipc::GRID_RESPONSE_SERVICE)?)
        .publish_subscribe::<ipc::WindowResponse>()
        .open()?;

    let mut response_subscriber = response_service.subscriber_builder().create()?;
    // Create command publisher (optional for multiple clients)
    let command_service = node
        .service_builder(&ServiceName::new(ipc::GRID_COMMANDS_SERVICE)?)
        .publish_subscribe::<ipc::WindowCommand>()
        .open()?;
    let command_publisher = match command_service.publisher_builder().create() {
        Ok(publisher) => {
            println!(
                "✅ [CLIENT {}] Connected to command service as publisher",
                client_id
            );
            Some(publisher)
        }
        Err(e) => {
            println!(
                "⚠️ [CLIENT {}] Could not create command publisher (limit reached): {}",
                client_id, e
            );
            println!("   This client will be read-only (events/responses only)");
            None
        }
    };
    println!("✅ [CLIENT {}] Connected to all IPC services", client_id);
    println!(
        "📡 [CLIENT {}] Listening for events and responses...",
        client_id
    );
    if command_publisher.is_some() {
        println!(
            "💬 [CLIENT {}] Commands: 'g' = get grid, 'w' = get windows, 'q' = quit",
            client_id
        );
    } else {
        println!(
            "📖 [CLIENT {}] Read-only mode - no commands available",
            client_id
        );
    }

    // Send initial test commands (if we have a publisher)
    std::thread::sleep(std::time::Duration::from_secs(1));

    if let Some(ref publisher) = command_publisher {
        let test_command = ipc::WindowCommand {
            command_type: 1, // GetGridState
            hwnd: 0,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
        };
        publisher.send_copy(test_command)?;
        println!(
            "📤 [CLIENT {}] Sent initial GetGridState command",
            client_id
        );
    } // Main client loop
    let mut command_mode = false;
    let mut iterations = 0;

    // Print initial prompt if we can send commands
    if command_publisher.is_some() {
        println!("\n💬 [CLIENT {}] Type 'g' for grid, 'w' for windows, 'a' for assign, 'h' for help, 'q' to quit", client_id);
        print!("[CLIENT-{}]> ", client_id);
        io::stdout().flush()?;
        command_mode = true;
    }

    loop {
        // Check for events
        while let Some(event_sample) = event_subscriber.receive()? {
            let event = *event_sample;
            println!("📡 [CLIENT {}] Received Event: {:?}", client_id, event);

            // Convert to human-readable format
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

        // Check for responses
        while let Some(response_sample) = response_subscriber.receive()? {
            let response = *response_sample;
            println!(
                "📤 [CLIENT {}] Received Response: {:?}",
                client_id, response
            );

            // Convert to human-readable format
            match response.response_type {
                0 => println!("   ✅ Success"),
                1 => println!("   ❌ Error (code: {})", response.error_code),
                2 => {
                    println!("   📋 Window List: {} windows", response.window_count);
                    println!(
                        "   🔍 Grid display would show {} windows in grid layout",
                        response.window_count
                    );
                }
                3 => {
                    // Grid State response
                    println!("   📊 GRID STATE:");
                    println!("   📈 Total Windows: {}", response.window_count);
                    println!("   🟩 Occupied Cells: {}", response.data[0]);
                    println!("   ┌─────────────────────────────────────┐");
                    println!("   │ 📊 CURRENT GRID LAYOUT             │");
                    println!("   ├─────────────────────────────────────┤");
                    println!(
                        "   │ Windows: {} | Cells: {}             │",
                        response.window_count, response.data[0]
                    );
                    println!(
                        "   │ Utilization: {:.1}%                │",
                        if response.data[0] > 0 {
                            (response.data[0] as f32 / (12 * 8) as f32) * 100.0
                        } else {
                            0.0
                        }
                    );
                    println!("   └─────────────────────────────────────┘");
                }
                _ => println!("   ❓ Unknown response type: {}", response.response_type),
            }
        }

        // Process interactive commands periodically if we can send commands
        if command_mode && command_publisher.is_some() && iterations % 50 == 0 {
            println!("\n💬 [CLIENT {}] Enter command: 'g'=grid, 'w'=windows, 'a'=assign, 'h'=help, 'q'=quit", client_id);
            print!("[CLIENT-{}]> ", client_id);
            io::stdout().flush()?;

            let mut input = String::new();
            if std::io::stdin().read_line(&mut input).is_ok() {
                let input = input.trim().to_lowercase();
                if !input.is_empty() {
                    match input.as_str() {
                        "g" | "grid" => {
                            if let Some(ref publisher) = command_publisher {
                                let command = ipc::WindowCommand {
                                    command_type: 1, // GetGridState
                                    hwnd: 0,
                                    target_row: 0,
                                    target_col: 0,
                                    monitor_id: 0,
                                };
                                if let Err(e) = publisher.send_copy(command) {
                                    println!(
                                        "❌ [CLIENT-{}] Failed to send grid command: {}",
                                        client_id, e
                                    );
                                } else {
                                    println!("📤 [CLIENT-{}] Requested grid state", client_id);
                                }
                            }
                        }
                        "w" | "windows" => {
                            if let Some(ref publisher) = command_publisher {
                                let command = ipc::WindowCommand {
                                    command_type: 2, // GetWindowList
                                    hwnd: 0,
                                    target_row: 0,
                                    target_col: 0,
                                    monitor_id: 0,
                                };
                                if let Err(e) = publisher.send_copy(command) {
                                    println!(
                                        "❌ [CLIENT-{}] Failed to send windows command: {}",
                                        client_id, e
                                    );
                                } else {
                                    println!("📤 [CLIENT-{}] Requested window list", client_id);
                                }
                            }
                        }
                        "a" | "assign" => {
                            if let Some(ref publisher) = command_publisher {
                                println!("📍 [CLIENT-{}] Assign window to grid cell", client_id);
                                println!("Assignment mode:");
                                println!("  1. Virtual grid (spans all monitors)");
                                println!("  2. Specific monitor grid");
                                print!("Mode (1-2)> ");
                                io::stdout().flush().unwrap();

                                let mut mode_input = String::new();
                                if io::stdin().read_line(&mut mode_input).is_ok() {
                                    let mode = mode_input.trim();

                                    println!("Enter HWND (window handle):");
                                    print!("HWND> ");
                                    io::stdout().flush().unwrap();

                                    let mut hwnd_input = String::new();
                                    if io::stdin().read_line(&mut hwnd_input).is_ok() {
                                        if let Ok(hwnd) = hwnd_input.trim().parse::<u64>() {
                                            println!("Enter target row (0-7):");
                                            print!("Row> ");
                                            io::stdout().flush().unwrap();

                                            let mut row_input = String::new();
                                            if io::stdin().read_line(&mut row_input).is_ok() {
                                                if let Ok(row) = row_input.trim().parse::<u32>() {
                                                    println!("Enter target column (0-11):");
                                                    print!("Col> ");
                                                    io::stdout().flush().unwrap();

                                                    let mut col_input = String::new();
                                                    if io::stdin().read_line(&mut col_input).is_ok()
                                                    {
                                                        if let Ok(col) =
                                                            col_input.trim().parse::<u32>()
                                                        {
                                                            match mode {
                                                                "1" => {
                                                                    // Virtual grid assignment
                                                                    let command =
                                                                        ipc::WindowCommand {
                                                                            command_type: 3, // AssignWindowToVirtualCell
                                                                            hwnd,
                                                                            target_row: row,
                                                                            target_col: col,
                                                                            monitor_id: 0, // Ignored for virtual
                                                                        };
                                                                    if let Err(e) =
                                                                        publisher.send_copy(command)
                                                                    {
                                                                        println!("❌ [CLIENT-{}] Failed to send assign command: {}", client_id, e);
                                                                    } else {
                                                                        println!("📤 [CLIENT-{}] Requested VIRTUAL grid assignment of HWND {} to cell ({}, {})", client_id, hwnd, row, col);
                                                                    }
                                                                }
                                                                "2" => {
                                                                    // Monitor-specific assignment
                                                                    println!("Enter monitor ID (usually 0 for primary):");
                                                                    print!("Monitor> ");
                                                                    io::stdout().flush().unwrap();

                                                                    let mut monitor_input =
                                                                        String::new();
                                                                    if io::stdin()
                                                                        .read_line(
                                                                            &mut monitor_input,
                                                                        )
                                                                        .is_ok()
                                                                    {
                                                                        if let Ok(monitor_id) =
                                                                            monitor_input
                                                                                .trim()
                                                                                .parse::<u32>()
                                                                        {
                                                                            let command = ipc::WindowCommand {
                                                                                command_type: 4, // AssignWindowToMonitorCell
                                                                                hwnd,
                                                                                target_row: row,
                                                                                target_col: col,
                                                                                monitor_id,
                                                                            };
                                                                            if let Err(e) =
                                                                                publisher.send_copy(
                                                                                    command,
                                                                                )
                                                                            {
                                                                                println!("❌ [CLIENT-{}] Failed to send assign command: {}", client_id, e);
                                                                            } else {
                                                                                println!("📤 [CLIENT-{}] Requested MONITOR {} assignment of HWND {} to cell ({}, {})", client_id, monitor_id, hwnd, row, col);
                                                                            }
                                                                        } else {
                                                                            println!("❌ [CLIENT-{}] Invalid monitor ID", client_id);
                                                                        }
                                                                    } else {
                                                                        println!("❌ [CLIENT-{}] Failed to read monitor input", client_id);
                                                                    }
                                                                }
                                                                _ => {
                                                                    println!("❌ [CLIENT-{}] Invalid mode. Choose 1 or 2.", client_id);
                                                                }
                                                            }
                                                        } else {
                                                            println!("❌ [CLIENT-{}] Invalid column number", client_id);
                                                        }
                                                    } else {
                                                        println!("❌ [CLIENT-{}] Failed to read column input", client_id);
                                                    }
                                                } else {
                                                    println!(
                                                        "❌ [CLIENT-{}] Invalid row number",
                                                        client_id
                                                    );
                                                }
                                            } else {
                                                println!(
                                                    "❌ [CLIENT-{}] Failed to read row input",
                                                    client_id
                                                );
                                            }
                                        } else {
                                            println!("❌ [CLIENT-{}] Invalid HWND", client_id);
                                        }
                                    } else {
                                        println!(
                                            "❌ [CLIENT-{}] Failed to read HWND input",
                                            client_id
                                        );
                                    }
                                } else {
                                    println!("❌ [CLIENT-{}] Failed to read mode input", client_id);
                                }
                            }
                        }
                        "h" | "help" => {
                            println!("📋 [CLIENT-{}] Available commands:", client_id);
                            println!("   g/grid    - Request current grid state");
                            println!("   w/windows - Request window list");
                            println!("   a/assign  - Assign window to grid cell (virtual or monitor-specific)");
                            println!("   h/help    - Show this help");
                            println!("   q/quit    - Exit client");
                            println!("");
                            println!("📍 Assignment modes:");
                            println!(
                                "   1. Virtual grid: coordinates span all monitors (0,0) to (7,11)"
                            );
                            println!("   2. Monitor grid: coordinates on specific monitor (0,0) to (7,11)");
                        }
                        "q" | "quit" | "exit" => {
                            println!("👋 [CLIENT-{}] User requested exit", client_id);
                            break;
                        }
                        _ => {
                            println!(
                                "❓ [CLIENT-{}] Unknown command '{}'. Type 'h' for help.",
                                client_id, input
                            );
                        }
                    }
                }
            }
        }

        // Increment iterations counter
        iterations += 1;

        // Reset counter to send periodic commands
        if iterations > 150 {
            iterations = 0;
            println!("� [CLIENT] Continuing to listen for events...");
        }

        // Small delay to prevent busy waiting
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    // Check if running as client
    if args.len() > 1 && args[1] == "--client" {
        return run_client();
    }

    // If no arguments, try to detect if server is already running
    // We'll try to connect as a client first, and if that fails, start as server
    println!("🔍 Checking if server is already running...");

    // Try to create a client connection to see if server exists
    if let Ok(node) = NodeBuilder::new().create::<iceoryx2::service::ipc::Service>() {
        if let Ok(_) = node
            .service_builder(&ServiceName::new(ipc::GRID_EVENTS_SERVICE)?)
            .publish_subscribe::<ipc::WindowEvent>()
            .open()
        {
            println!("✅ Server detected! Running as client...");
            return run_client();
        }
    }

    println!("🚀 E-Grid with IPC Integration Demo (Server)");
    println!("============================================="); // Spawn client process
    println!("🔄 Spawning IPC client process in new terminal...");
    let current_exe = env::current_exe()?;
    let current_exe_str = current_exe.to_string_lossy();

    // Use a different approach - create a batch command that starts the client
    let mut client_process = Command::new("cmd")
        .args(&["/c", "start", "cmd", "/k", &current_exe_str, "--client"])
        .spawn()?;

    println!(
        "✅ Client terminal spawned with PID: {}",
        client_process.id()
    );

    // Give client time to start
    std::thread::sleep(std::time::Duration::from_millis(1000));

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

    // Set up window event hooks
    match window_events::setup_window_events(tracker_arc.clone()) {
        Ok(_) => println!("✅ Window event hooks set up successfully!"),
        Err(e) => {
            eprintln!("❌ Failed to set up window event hooks: {}", e);
            return Err(e.into());
        }
    }
    println!("\n🔄 Starting integrated event monitoring with IPC...");
    println!("📢 SERVER FEATURES:");
    println!("   • Real-time window event tracking");
    println!("   • IPC event publishing");
    println!("   • Command processing");
    println!("   • Multi-monitor grid support");
    println!(
        "   • Client process spawned with PID: {}",
        client_process.id()
    );
    println!("   • Type 'g' to show grid, 'e' to send event, 'r' to rescan, 'q' to quit");

    // Check if client is still running
    match client_process.try_wait() {
        Ok(Some(status)) => println!("⚠️ Client process already exited with status: {}", status),
        Ok(None) => println!("✅ Client process is running"),
        Err(e) => println!("❌ Error checking client process status: {}", e),
    }

    // Main event loop
    loop {
        // Periodically check if client is still running
        match client_process.try_wait() {
            Ok(Some(status)) => {
                println!("⚠️ Client process exited with status: {}", status);
                println!(
                    "   Try running the client manually: cargo run --bin ipc_demo_new -- --client"
                );
            }
            Ok(None) => {} // Still running
            Err(e) => println!("❌ Error checking client process: {}", e),
        }

        // Process any pending commands
        ipc_manager.process_commands()?;
        print!("\n[SERVER]> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

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
            }
            "c" | "commands" => {
                // Demo command processing
                println!("📨 Processing demo commands...");
                ipc_manager.process_commands()?;
            }
            "h" | "help" => {
                println!("📋 Available commands:");
                println!("   g/grid    - Show current grid state");
                println!("   r/rescan  - Rescan all windows");
                println!("   e/event   - Publish demo IPC event");
                println!("   c/commands- Process demo IPC commands");
                println!("   h/help    - Show this help");
                println!("   q/quit    - Exit program");
            }
            "q" | "quit" | "exit" => {
                println!("🧹 Cleaning up...");
                window_events::cleanup_hooks();
                println!("👋 Server shutting down!");
                break;
            }
            _ => {
                println!("❓ Unknown command '{}'. Type 'h' for help.", input);
            }
        }
    }

    Ok(())
}
