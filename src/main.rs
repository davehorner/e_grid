use crossterm::{
    cursor, event, execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
};
use e_grid::window_tracker::WindowTracker;
use e_grid::{ipc_server, window_events, GridClient};
use iceoryx2::prelude::*;
use iceoryx2::service::ipc::Service;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::{
    io::{self, Write},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
use winapi::shared::minwindef::{BOOL, DWORD, FALSE, TRUE};
use winapi::um::consoleapi::SetConsoleCtrlHandler;
use winapi::um::wincon::{
    CTRL_BREAK_EVENT, CTRL_CLOSE_EVENT, CTRL_C_EVENT, CTRL_LOGOFF_EVENT, CTRL_SHUTDOWN_EVENT,
};
use winapi::um::winuser::GetClassNameW;

// Global variables for graceful shutdown
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);
static mut GLOBAL_IPC_SERVER: Option<*mut ipc_server::GridIpcServer> = None;

const BANNER: &str = r#"
  ███████╗          ██████╗  ██████╗  ██╗ ██████╗ 
  ██╔════╝         ██╔════╝  ██╔══██╗ ██║ ██╔══██╗
  █████╗           ██║  ███╗ ██████╔╝ ██║ ██║  ██║
  ██╔══╝           ██║   ██║ ██╔══██╗ ██║ ██║  ██║
  ███████╗ ██████╗ ╚██████╔╝ ██║  ██║ ██║ ██████╔╝
  ╚══════╝ ╚═════╝  ╚═════╝  ╚═╝  ╚═╝ ╚═╝ ╚═════╝ 
"#;

/// Console control handler for graceful shutdown
unsafe extern "system" fn console_ctrl_handler(ctrl_type: DWORD) -> BOOL {
    match ctrl_type {
        CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT | CTRL_LOGOFF_EVENT
        | CTRL_SHUTDOWN_EVENT => {
            println!("\n🛑 Shutdown signal received - initiating graceful shutdown...");
            SHUTDOWN_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);

            // Send shutdown heartbeat if server is available
            if let Some(server_ptr) = GLOBAL_IPC_SERVER {
                if let Some(server) = server_ptr.as_mut() {
                    println!("💓 Sending shutdown heartbeat to connected clients...");
                    if let Err(e) = server.send_heartbeat(0, 0) {
                        // iteration=0 signals shutdown
                        println!("⚠️ Failed to send shutdown heartbeat: {}", e);
                    }
                }
            }

            // Give time for shutdown heartbeat to be sent
            std::thread::sleep(std::time::Duration::from_millis(100));
            TRUE
        }
        _ => FALSE,
    }
}

/// Check if the e_grid server is already running by trying to connect to an IPC service
fn is_server_running() -> bool {
    // Try to create a test subscriber to see if the service exists
    match NodeBuilder::new().create::<Service>() {
        Ok(node) => {
            // Try multiple services to ensure server is really running
            let services_to_check = [
                e_grid::ipc::GRID_EVENTS_SERVICE,
                e_grid::ipc::GRID_FOCUS_EVENTS_SERVICE,
                e_grid::ipc::GRID_COMMANDS_SERVICE,
            ];

            let mut services_available = 0;
            for service_name in &services_to_check {
                match node
                    .service_builder(&ServiceName::new(service_name).unwrap())
                    .publish_subscribe::<e_grid::ipc::WindowEvent>()
                    .open()
                {
                    Ok(_) => {
                        services_available += 1;
                    }
                    Err(_) => {
                        // Service not available
                    }
                }
            }

            if services_available >= 2 {
                println!(
                    "🔍 Detected existing e_grid server ({}/{} services available)",
                    services_available,
                    services_to_check.len()
                );
                true
            } else {
                println!(
                    "🔍 No existing e_grid server detected ({}/{} services available)",
                    services_available,
                    services_to_check.len()
                );
                false
            }
        }
        Err(_) => {
            println!("🔍 Unable to check for existing server (IPC system unavailable)");
            false
        }
    }
}

// Simple tracking using a static array of atomic values (for up to 1000 windows)
static MANAGEABLE_WINDOW_SLOTS: [AtomicU64; 1000] = {
    const INIT: AtomicU64 = AtomicU64::new(0);
    [INIT; 1000]
};

/// Add a window to the manageable windows tracking
fn add_manageable_window(hwnd: u64) {
    // Find an empty slot and store the HWND
    for slot in &MANAGEABLE_WINDOW_SLOTS {
        if slot
            .compare_exchange(0, hwnd, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return;
        }
    }
    // If we get here, all slots are full - could expand array or use different approach
    println!(
        "⚠️ Warning: Manageable window tracking slots full for HWND 0x{:X}",
        hwnd
    );
}

/// Remove a window from tracking and return true if it was found (was manageable)
fn remove_manageable_window(hwnd: u64) -> bool {
    for slot in &MANAGEABLE_WINDOW_SLOTS {
        if slot
            .compare_exchange(hwnd, 0, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return true; // Found and removed
        }
    }
    false // Not found
}

/// Check if a window is tracked as manageable
fn is_tracked_manageable_window(hwnd: u64) -> bool {
    for slot in &MANAGEABLE_WINDOW_SLOTS {
        if slot.load(Ordering::Acquire) == hwnd {
            return true;
        }
    }
    false
}

/// Start the IPC server with integrated window tracking and focus events
fn start_server() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting E-Grid Server");
    println!("=========================");

    // No initialization needed for atomic array

    // Setup console control handler for graceful shutdown
    unsafe {
        if SetConsoleCtrlHandler(Some(console_ctrl_handler), TRUE) == 0 {
            println!("⚠️ Failed to set console control handler - graceful shutdown may not work");
        } else {
            println!("✅ Console control handler registered - supports graceful shutdown");
        }
    }

    // Create the window tracker
    let mut tracker = WindowTracker::new();
    println!("📊 Initializing window tracking...");
    tracker.scan_existing_windows();
    tracker.update_grid();
    tracker.print_virtual_grid();

    let tracker = Arc::new(Mutex::new(tracker));

    // Create and setup the IPC server
    let mut ipc_server = ipc_server::GridIpcServer::new(tracker.clone())?;

    // Set global server pointer for graceful shutdown
    unsafe {
        GLOBAL_IPC_SERVER = Some(&mut ipc_server as *mut _);
    }

    println!("\n🔧 Setting up IPC services...");
    ipc_server.setup_services()?;

    // Start IPC server monitoring
    println!("\n🔄 Starting IPC server monitoring...");
    ipc_server.start_background_event_loop()?;

    // Setup integrated WinEvent hooks for real-time monitoring including focus events
    println!("\n🔗 Setting up integrated WinEvent monitoring (includes focus tracking)...");
    if let Err(e) = ipc_server.setup_window_events() {
        println!("⚠️ Failed to setup WinEvents: {}", e);
        println!("   Continuing without real-time event monitoring...");
    }

    // Give the server a moment to be ready
    thread::sleep(Duration::from_millis(500));

    println!("\n✅ E-Grid Server fully operational!");
    println!("📡 Available IPC Services:");
    println!(
        "   • {} - Window lifecycle events",
        e_grid::ipc::GRID_EVENTS_SERVICE
    );
    println!(
        "   • {} - Window details/positions",
        e_grid::ipc::GRID_WINDOW_DETAILS_SERVICE
    );
    println!(
        "   • {} - Focus tracking events",
        e_grid::ipc::GRID_FOCUS_EVENTS_SERVICE
    );
    println!(
        "   • {} - Client commands",
        e_grid::ipc::GRID_COMMANDS_SERVICE
    );
    println!(
        "   • {} - Server responses",
        e_grid::ipc::GRID_RESPONSE_SERVICE
    );
    println!(
        "   • {} - Layout management",
        e_grid::ipc::GRID_LAYOUT_SERVICE
    );
    println!(
        "   • {} - Animation commands",
        e_grid::ipc::ANIMATION_COMMANDS_SERVICE
    );
    println!();

    // Print initial summary
    if let Ok(tracker) = tracker.lock() {
        println!(
            "📊 Server tracking {} windows across {} monitors",
            tracker.windows.len(),
            tracker.monitor_grids.len()
        );
    }
    println!("💡 Tip: Run 'cargo run --example simple_focus_demo' in another terminal to see focus events!");
    println!("🔄 Server running... Press Ctrl+C to stop");
    println!();

    // Main server event loop using the library's reusable message loop
    let mut _loop_count = 0u32;
    let mut last_update = std::time::Instant::now();
    let print_grid = Arc::new(AtomicBool::new(false));
    let print_grid_for_thread = Arc::clone(&print_grid);
    let rec = ipc_server.event_receiver.take();

    // If there's a receiver, print something on event
    if let Some(receiver) = rec {
        thread::spawn(move || {
            println!("🎧 Event listener thread started - waiting for window events...");
            while let Ok(event) = receiver.recv() {
                // Check for shutdown in the event thread too
                if SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }

                match &event {
                    e_grid::ipc_protocol::GridEvent::WindowCreated { hwnd, title, .. } => {
                        // Get window class for created windows
                        let class_name = get_window_class(*hwnd);
                        println!(
                            "🆕 CREATED: Window 0x{:X} [{}] '{}'",
                            hwnd, class_name, title
                        );

                        // Track this as a manageable window since it passed the filters
                        add_manageable_window(*hwnd);

                        print_grid_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    e_grid::ipc_protocol::GridEvent::WindowDestroyed { hwnd, title } => {
                        // Check if this was a tracked manageable window
                        if remove_manageable_window(*hwnd) {
                            // This was a manageable window we were tracking
                            let class_name = get_window_class(*hwnd); // May return "(unknown)" for destroyed windows
                            println!(
                                "💀 DESTROYED (Manageable): Window 0x{:X} [{}] '{}'",
                                hwnd, class_name, title
                            );
                            print_grid_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                        } else {
                            // This was not a manageable window we were tracking
                            println!(
                                "💀 DESTROYED (Non-manageable): Window 0x{:X} '{}'",
                                hwnd, title
                            );
                        }
                    }
                    e_grid::ipc_protocol::GridEvent::WindowFocused {
                        hwnd,
                        title,
                        process_id,
                    } => {
                        println!(
                            "🎯 FOCUS: Window 0x{:X} '{}' (PID: {}) gained focus",
                            hwnd, title, process_id
                        );
                        print_grid_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    e_grid::ipc_protocol::GridEvent::WindowDefocused {
                        hwnd,
                        title,
                        process_id,
                    } => {
                        println!(
                            "🎯 DEFOCUS: Window 0x{:X} '{}' (PID: {}) lost focus",
                            hwnd, title, process_id
                        );
                    }
                    e_grid::ipc_protocol::GridEvent::WindowMoveStart { hwnd, title, .. } => {
                        println!("🚀 MOVE START: Window 0x{:X} '{}'", hwnd, title);
                    }
                    e_grid::ipc_protocol::GridEvent::WindowMoveStop { hwnd, title, .. } => {
                        println!("🏁 MOVE STOP: Window 0x{:X} '{}'", hwnd, title);
                    }
                    e_grid::ipc_protocol::GridEvent::WindowResizeStart { hwnd, title, .. } => {
                        println!("📏 RESIZE START: Window 0x{:X} '{}'", hwnd, title);
                    }
                    e_grid::ipc_protocol::GridEvent::WindowResizeStop { hwnd, title, .. } => {
                        println!("📐 RESIZE STOP: Window 0x{:X} '{}'", hwnd, title);
                        // Print all grids when resize stops
                        print_grid_for_thread.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    _ => {
                        println!("📥 OTHER EVENT: {:?}", event);
                    }
                }
            }
            println!("🎧 Event listener thread ended");
        });
    } else {
        println!("❌ No event receiver available - window events will not be displayed");
    }

    window_events::run_message_loop(|| {
        // Check for shutdown request from console control handler
        if SHUTDOWN_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
            println!("🛑 Shutdown requested - exiting gracefully...");

            // Cleanup window events to stop the move/resize tracker
            window_events::cleanup_hooks();

            // Clear global server pointer
            unsafe {
                GLOBAL_IPC_SERVER = None;
            }

            return false; // Exit the loop
        }

        if print_grid.load(std::sync::atomic::Ordering::SeqCst) {
            if let Ok(tracker) = tracker.lock() {
                tracker.print_all_grids();
            }
            print_grid.store(false, std::sync::atomic::Ordering::SeqCst);
        }

        // Poll move/resize events (required for move/resize start/stop detection)
        ipc_server.poll_move_resize_events();

        // Process IPC commands from clients
        if let Err(e) = ipc_server.process_commands() {
            println!("⚠️ Error processing IPC commands: {}", e);
        }

        // Process focus events from the channel and publish them via IPC
        if let Err(e) = ipc_server.process_focus_events() {
            println!("⚠️ Error processing focus events: {}", e);
        }

        // Process window events from the channel and publish them via IPC
        if let Err(e) = ipc_server.process_window_events() {
            println!("⚠️ Error processing window events: {}", e);
        }

        // Process layout commands
        if let Err(e) = ipc_server.process_layout_commands() {
            println!("⚠️ Error processing layout commands: {}", e);
        }

        // Process animation commands
        if let Err(e) = ipc_server.process_animation_commands() {
            println!("⚠️ Error processing animation commands: {}", e);
        }

        // Update animations
        if let Ok((completed, failed)) = ipc_server.update_animations() {
            if !completed.is_empty() {
                println!("🎬 Completed animations for {} windows", completed.len());
            }
            if !failed.is_empty() {
                println!("⚠️ Failed animations for {} windows", failed.len());
            }
        }

        // Send heartbeat every 5 seconds to keep clients connected
        if last_update.elapsed().as_secs() > 5 {
            // Send heartbeat to keep clients connected
            let uptime_ms = std::time::Instant::now()
                .duration_since(last_update)
                .as_millis() as u64;
            if let Err(e) = ipc_server.send_heartbeat(_loop_count as u64, uptime_ms) {
                println!("⚠️ Failed to send heartbeat: {}", e);
            }

            last_update = std::time::Instant::now();
        }

        // Periodic status updates every 30 seconds
        static mut LAST_STATUS_DISPLAY: std::time::Instant = unsafe { std::mem::zeroed() };
        static mut STATUS_DISPLAY_INITIALIZED: bool = false;

        unsafe {
            if !STATUS_DISPLAY_INITIALIZED {
                LAST_STATUS_DISPLAY = std::time::Instant::now();
                STATUS_DISPLAY_INITIALIZED = true;
            }

            if LAST_STATUS_DISPLAY.elapsed().as_secs() > 30 {
                if let Ok(tracker) = tracker.lock() {
                    println!(
                        "📊 Status: {} windows, {} monitors, {} active animations",
                        tracker.windows.len(),
                        tracker.monitor_grids.len(),
                        tracker.active_animations.len()
                    );
                }
                LAST_STATUS_DISPLAY = std::time::Instant::now();
            }
        }

        _loop_count += 1;

        // Return true to continue the loop, false to exit
        true
    })?;

    println!("🛑 Server event loop ended, shutting down server...");
    println!("🧹 Cleaning up resources...");

    // Final cleanup
    window_events::cleanup_hooks();

    Ok(())
}

/// Start a detached client (simple grid visualization)
fn start_detached_client() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎮 Starting detached grid visualization client...");
    // Use the existing grid_client_demo as the detached client
    let child = Command::new("cargo")
        .args(&["run", "--example", "grid_client_demo"])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    // Don't wait for the child - let it run detached
    println!("✅ Client started in detached mode (PID: {})", child.id());
    println!("   The client will display real-time grid updates");

    Ok(())
}

/// Interactive mode - show live grid with colors like the original main.rs
fn interactive_mode() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎮 Interactive Grid Visualization Mode");
    println!("=====================================");
    println!("Connecting to e_grid server...");

    let mut last_display = std::time::Instant::now();
    let mut last_connection_attempt = std::time::Instant::now();
    let mut client: Option<GridClient> = None;
    let mut connection_status = "Disconnected";

    // Enable terminal colors
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;

    println!("🔄 Starting interactive grid display...");
    println!("Press Ctrl+C to exit");

    loop {
        // Try to connect/reconnect if needed
        if client.is_none() && last_connection_attempt.elapsed().as_secs() >= 3 {
            match GridClient::new() {
                Ok(mut new_client) => {
                    println!("✅ Connected to e_grid server");
                    if let Err(e) = new_client.start_background_monitoring() {
                        println!("⚠️ Failed to start monitoring: {}", e);
                    }
                    client = Some(new_client);
                    connection_status = "Connected";
                }
                Err(e) => {
                    if last_connection_attempt.elapsed().as_secs() >= 10 {
                        println!("❌ Failed to connect to e_grid server: {}", e);
                        println!("🔄 Will retry connection in 3 seconds...");
                    }
                    connection_status = "Reconnecting...";
                }
            }
            last_connection_attempt = std::time::Instant::now();
        }

        // Update display every 1000ms for smooth updates
        if last_display.elapsed().as_millis() > 1000 {
            // Clear screen and show updated grid
            queue!(stdout, cursor::MoveTo(0, 0))?;
            queue!(stdout, Clear(ClearType::All))?;

            // Header
            queue!(stdout, SetForegroundColor(Color::Cyan))?;
            queue!(stdout, Print("E-Grid Interactive Visualization\n"))?;
            queue!(stdout, Print("═".repeat(50)))?;
            queue!(stdout, Print("\n"))?;
            queue!(stdout, ResetColor)?;

            // Connection status
            match connection_status {
                "Connected" => {
                    queue!(stdout, SetForegroundColor(Color::Green))?;
                    queue!(stdout, Print("📊 Connected to e_grid server\n"))?;
                }
                "Reconnecting..." => {
                    queue!(stdout, SetForegroundColor(Color::Yellow))?;
                    queue!(stdout, Print("🔄 Reconnecting to e_grid server...\n"))?;
                }
                _ => {
                    queue!(stdout, SetForegroundColor(Color::Red))?;
                    queue!(stdout, Print("❌ Disconnected from e_grid server\n"))?;
                }
            }
            queue!(stdout, ResetColor)?;

            // Request grid state from server if connected
            if let Some(ref mut client_ref) = client {
                match client_ref.request_grid_state() {
                    Ok(_) => {
                        // Grid state request successful
                    }
                    Err(e) => {
                        queue!(stdout, SetForegroundColor(Color::Red))?;
                        queue!(
                            stdout,
                            Print(format!("⚠️ Server communication error: {}\n", e))
                        )?;
                        queue!(stdout, ResetColor)?;
                        // Reset client to trigger reconnection
                        client = None;
                        connection_status = "Reconnecting..."; // This will trigger reconnection on next cycle
                        last_connection_attempt = std::time::Instant::now(); // Reset timer to allow immediate retry
                    }
                }
            }

            // Add instructions
            queue!(stdout, Print("\n💡 Commands available:\n"))?;
            queue!(
                stdout,
                Print("   - Run focus demos: cargo run --example simple_focus_demo\n")
            )?;
            queue!(
                stdout,
                Print("   - Full client demo: cargo run --bin grid_client_demo\n")
            )?;
            queue!(
                stdout,
                Print("   - Test focus events: test_focus_defocus.bat\n")
            )?;
            queue!(stdout, Print("\n🔄 Live grid updates every second...\n"))?;

            stdout.flush()?;
            last_display = std::time::Instant::now();
        } // Small delay
        thread::sleep(Duration::from_millis(100)); // Check for Ctrl+C using crossterm events
        if event::poll(Duration::from_millis(0))? {
            if let event::Event::Key(key_event) = event::read()? {
                if key_event.code == event::KeyCode::Char('c')
                    && key_event.modifiers.contains(event::KeyModifiers::CONTROL)
                {
                    println!("\n🛑 Ctrl+C pressed - exiting interactive mode...");
                    return Ok(());
                }
            }
        }
    }
}

fn show_help() {
    println!("{}", BANNER);
    println!("Usage: e_grid [command]");
    println!();
    println!("Commands:");
    println!("  (no args)     Auto-detect: start server if not running, or interactive client");
    println!("  server        Force start server mode");
    println!("  client        Force start interactive client mode");
    println!("  help          Show this help message");
    println!();
    println!("Auto-Detection Logic:");
    println!("  1. Check if e_grid server is already running");
    println!("  2. If running: start detached grid client + interactive mode");
    println!("  3. If not running: start server + detached client");
    println!();
    println!("Features:");
    println!("  🎯 Focus Event Tracking - Real-time window focus/defocus events");
    println!("  📊 Multi-Monitor Grids - Per-monitor and virtual grid tracking");
    println!("  🎬 Window Animations - Smooth window transitions");
    println!("  💾 Layout Management - Save and restore window arrangements");
    println!("  📡 Multi-Client IPC - Up to 8 clients per service");
    println!();
    println!("Examples:");
    println!("  e_grid                            # Auto-detect and start appropriate mode");
    println!("  e_grid server                     # Force server mode");
    println!("  e_grid client                     # Force client mode");
    println!("  cargo run --example simple_focus_demo  # Test focus events");
    println!();
}

/// Get the window class name for a given HWND
fn get_window_class(hwnd: u64) -> String {
    unsafe {
        let hwnd = hwnd as isize as winapi::shared::windef::HWND;
        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);

        if class_len > 0 {
            use std::ffi::OsString;
            use std::os::windows::ffi::OsStringExt;
            OsString::from_wide(&class_buf[..class_len as usize])
                .to_string_lossy()
                .into_owned()
        } else {
            "(unknown)".to_string()
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("help") | Some("-h") | Some("--help") => {
            show_help();
            return Ok(());
        }
        Some("server") => {
            // Force server mode
            return start_server();
        }
        Some("client") => {
            // Force client mode
            return interactive_mode();
        }
        Some(unknown) => {
            println!("❌ Unknown command: {}", unknown);
            println!("Run 'e_grid help' for usage information");
            return Ok(());
        }
        None => {
            // Auto-detect mode
        }
    }

    // Auto-detection logic
    println!("{}", BANNER);
    println!("🔍 Auto-detecting e_grid server status...");

    if is_server_running() {
        println!("✅ E-Grid server is already running!");
        println!("🎮 Starting client in interactive mode...");

        // Then start interactive mode
        interactive_mode()
    } else {
        println!("🚀 Starting e_grid server...");

        // Start server in background thread so we can also start a client
        let server_handle = thread::spawn(|| {
            if let Err(e) = start_server() {
                println!("❌ Server failed: {}", e);
            }
        });

        // Give server time to start
        println!("⏳ Waiting for server to initialize...");
        thread::sleep(Duration::from_millis(3000));

        // Wait for server
        server_handle.join().unwrap();
        Ok(())
    }
}
