use e_grid::ipc_client::GridClient;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔥 E-Grid Non-Interactive Client Demo - Real-Time Grid Reconstruction");
    println!("=====================================================================");
    println!("This demo will automatically:");
    println!("  📡 Connect to the E-Grid server");
    println!("  📋 Request and display window lists");
    println!("  🔍 Monitor real-time window updates");
    println!("  🎨 Show grid state changes automatically with red highlighting for topmost window");
    println!("  🎯 Demonstrate grid assignments");
    println!();

    // Create the grid client
    let mut client = GridClient::new()?;

    // Register window event callback to log all event types
    client.set_window_event_callback(|event| {
        // Enhanced visually distinct log messages for move/resize START/STOP events
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
        match event.event_type {
            4 => {
                println!("\n\n🚦 MOVE START: {:?}", event);
            }
            5 => {
                println!("🚦 MOVE STOP: {:?}\n\n", event);
            }
            6 => {
                println!("\n\n📏 RESIZE START: {:?}", event);
            }
            7 => {
                println!("📏 RESIZE STOP: {:?}\n\n", event);
            }
            _ => {
                println!(
                    "[WINDOW EVENT] {}: HWND {} at ({}, {})",
                    event_name, event.hwnd, event.row, event.col
                );
            }
        }
    })?;
    let (focus_tx, focus_rx) = mpsc::channel::<e_grid::ipc_client::WindowFocusEvent>();
    let focus_tx_cb = focus_tx.clone();
    client.set_focus_callback(move |focus_event| {
        // Enhanced visually distinct log messages for focus events
        let event_name = if focus_event.event_type == 0 {
            "FOCUSED"
        } else {
            "DEFOCUSED"
        };
        println!(
            "[FOCUS EVENT] {}: HWND {} (PID: {}) at timestamp: {}",
            event_name, focus_event.hwnd, focus_event.process_id, focus_event.timestamp
        );
        focus_tx_cb
            .send(focus_event)
            .expect("Failed to send focus event");
    })?;
    println!("📋 Registering focus callback...");
    // If you want to process focus events in the main thread, use a second channel:
    // let (focus_tx, focus_rx) = mpsc::channel::<e_grid::ipc_client::WindowEvent>();
    // and send events from the spawned thread to the main thread via focus_tx.

    // Instead of spawning a thread, process focus events in the main loop below.
    // If you want to process focus events asynchronously, use a thread only for receiving and forwarding events, not for accessing GridClient.
    // Here, we will process focus events in the main loop.
    // Enable red highlighting for topmost window
    client.set_highlight_topmost(true)?;

    // Enable auto-display for real-time updates
    // client.set_auto_display(true);
    // Start background monitoring for real-time updates
    client.start_background_monitoring()?;

    println!("✅ Connected to E-Grid server");
    println!("🔍 Background monitoring started - real-time updates enabled!");
    println!("🎨 Red highlighting enabled for topmost window!");
    println!("📡 Initial window data requested automatically");
    println!();

    // Give some time for initial data to arrive and show status
    for i in 1..=8 {
        thread::sleep(Duration::from_millis(500));
        let monitor_count = client.monitors.len();
        let window_count = client.windows.len();
        let has_valid_grids = client
            .has_valid_grid_data
            .load(std::sync::atomic::Ordering::Relaxed);

        println!(
            "⏳ Waiting for data... ({}s elapsed, {} monitors, {} windows, grids valid: {})",
            (i as f32) * 0.5,
            monitor_count,
            window_count,
            has_valid_grids
        );

        if monitor_count > 0 && has_valid_grids {
            break;
        }
    }

    // println!("\n📋 Initial Window List:");
    // client.display_window_list();

    println!("\n📊 Initial Grid State:");
    client.print_all_grids();

    // Non-interactive demo loop with automatic actions
    let mut demo_cycle = 0;

    loop {
        demo_cycle += 1;
        //println!("🔄 Demo Cycle #{}", demo_cycle);

        // Process any focus events received from the channel
        while let Ok(focus_event) = focus_rx.try_recv() {
            println!(
                "🔍 [DEBUG] DEMO CALLBACK CALLED for event type: {}",
                focus_event.event_type
            );

            let event_name = if focus_event.event_type == 0 {
                "🟢 FOCUSED"
            } else {
                "🔴 DEFOCUSED"
            };

            println!(
                "{} - Window: {} (PID: {}) at timestamp: {}",
                event_name, focus_event.hwnd, focus_event.process_id, focus_event.timestamp
            );

            println!("   ─────────────────────────────");
            println!("   🗺️  Printing all grids for debug:");
            client.print_all_grids();
        }

        // Periodic actions to demonstrate functionality
        // println!("🔄 Demo Cycle #{}", demo_cycle);

        // Wait between demo actions - this gives time for real-time updates to show
        thread::sleep(Duration::from_millis(400));

        // Occasional longer pause to let user observe
        // if demo_cycle % 10 == 0 {
        //     println!("\n⏸️  Pausing to observe real-time updates...");
        //     println!("   (This is a good time to move windows around!)");
        //     println!("   🎨 Watch for the \x1b[31mred highlighting\x1b[0m on the topmost window!");
        //     thread::sleep(Duration::from_secs(5));
        // }
    }
}
