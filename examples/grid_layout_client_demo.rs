use e_grid::ipc;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎬 E-Grid Layout & Animation Client Demo");
    println!("=====================================");
    println!("This client demonstrates:");
    println!("  📤 Sending grid layout commands to server");
    println!("  🎬 Controlling window animations");
    println!("  💾 Saving and loading grid layouts");
    println!();

    // Create IPC connection
    let node = NodeBuilder::new().create::<iceoryx2::service::ipc::Service>()?;

    // Setup command publisher
    let command_service = node
        .service_builder(&ServiceName::new(ipc::GRID_COMMANDS_SERVICE)?)
        .publish_subscribe::<e_grid::ipc_protocol::IpcCommand>()
        .open()?;
    let mut command_publisher = command_service.publisher_builder().create()?;

    // Setup layout publisher
    let layout_service = node
        .service_builder(&ServiceName::new(ipc::GRID_LAYOUT_SERVICE)?)
        .publish_subscribe::<ipc::GridLayoutMessage>()
        .open()?;
    let layout_publisher = layout_service.publisher_builder().create()?;

    // Setup cell assignment publisher
    let cell_service = node
        .service_builder(&ServiceName::new(ipc::GRID_CELL_ASSIGNMENTS_SERVICE)?)
        .publish_subscribe::<ipc::GridCellAssignment>()
        .open()?;
    let cell_publisher = cell_service.publisher_builder().create()?;

    // Setup animation publisher
    let animation_service = node
        .service_builder(&ServiceName::new(ipc::ANIMATION_COMMANDS_SERVICE)?)
        .publish_subscribe::<ipc::AnimationCommand>()
        .open()?;
    let mut animation_publisher = animation_service.publisher_builder().create()?;

    // Setup response subscriber
    let response_service = node
        .service_builder(&ServiceName::new(ipc::GRID_RESPONSE_SERVICE)?)
        .publish_subscribe::<ipc::WindowResponse>()
        .open()?;
    let mut response_subscriber = response_service.subscriber_builder().create()?;

    println!("✅ Connected to E-Grid server services");
    println!();

    // Interactive menu loop
    loop {
        println!("🎮 E-Grid Layout & Animation Demo - Choose an action:");
        println!("   1. 📋 Get window list");
        println!("   2. 💾 Save current layout");
        println!("   3. 🗂️  Apply saved layout (animated)");
        println!("   4. 🎬 Animate specific window");
        println!("   5. 📊 Get animation status");
        println!("   6. 🔄 Demo automatic layout transitions");
        println!("   7. 🛑 Stop all animations");
        println!("   q. Quit");
        println!();

        print!("Enter your choice: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice = input.trim();

        match choice {
            "1" => get_window_list(&mut command_publisher)?,
            "2" => save_current_layout(&mut command_publisher)?,
            "3" => apply_layout_animated(&mut command_publisher)?,
            "4" => animate_specific_window(&mut animation_publisher)?,
            "5" => get_animation_status(&mut animation_publisher)?,
            "6" => demo_layout_transitions(&mut command_publisher, &mut animation_publisher)?,
            "7" => stop_all_animations(&mut animation_publisher)?,
            "q" | "Q" => break,
            _ => println!("❌ Invalid choice. Please try again."),
        }

        // Check for any responses
        check_responses(&mut response_subscriber)?;

        println!();
    }

    println!("👋 E-Grid Layout & Animation Demo finished");
    Ok(())
}

fn get_window_list(
    publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::WindowCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("📋 Requesting window list from server...");

    let command = ipc::WindowCommand {
        command_type: 2, // get_windows
        ..Default::default()
    };

    publisher.send_copy(command)?;
    println!("✅ Sent GetWindowList command");

    // Give server time to process
    thread::sleep(Duration::from_millis(100));
    Ok(())
}

fn save_current_layout(
    publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::WindowCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    print!("💾 Enter layout name to save: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let layout_name = input.trim();

    if layout_name.is_empty() {
        println!("❌ Layout name cannot be empty");
        return Ok(());
    }

    // Simple hash of layout name for ID
    let layout_id: u32 = layout_name.chars().map(|c| c as u32).sum();

    let command = ipc::WindowCommand {
        command_type: 6, // save_layout
        layout_id,
        ..Default::default()
    };

    publisher.send_copy(command)?;
    println!("✅ Sent SaveCurrentLayout command for '{}'", layout_name);
    Ok(())
}

fn apply_layout_animated(
    publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::WindowCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    print!("🗂️ Enter layout name to apply: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let layout_name = input.trim();

    if layout_name.is_empty() {
        println!("❌ Layout name cannot be empty");
        return Ok(());
    }

    print!("🎬 Enter animation duration (ms, default 1000): ");
    io::stdout().flush()?;

    let mut duration_input = String::new();
    io::stdin().read_line(&mut duration_input)?;
    let duration_ms = if duration_input.trim().is_empty() {
        1000
    } else {
        duration_input.trim().parse().unwrap_or(1000)
    };

    print!("📈 Choose easing (0=Linear, 1=EaseIn, 2=EaseOut, 3=EaseInOut, 4=Bounce, 5=Elastic, 6=Back, default 0): ");
    io::stdout().flush()?;

    let mut easing_input = String::new();
    io::stdin().read_line(&mut easing_input)?;
    let easing_type = if easing_input.trim().is_empty() {
        0
    } else {
        easing_input.trim().parse().unwrap_or(0).clamp(0, 6)
    };

    let layout_id: u32 = layout_name.chars().map(|c| c as u32).sum();

    let command = ipc::WindowCommand {
        command_type: 5, // apply_grid_layout
        layout_id,
        animation_duration_ms: duration_ms,
        easing_type,
        ..Default::default()
    };

    publisher.send_copy(command)?;
    println!(
        "✅ Sent ApplyGridLayout command for '{}' with {}ms duration and easing {}",
        layout_name, duration_ms, easing_type
    );
    Ok(())
}

fn animate_specific_window(
    publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::AnimationCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    print!("🎬 Enter window HWND (decimal): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let hwnd: u64 = input.trim().parse().unwrap_or(0);

    if hwnd == 0 {
        println!("❌ Invalid HWND");
        return Ok(());
    }

    print!("📍 Enter target X position: ");
    io::stdout().flush()?;
    let mut x_input = String::new();
    io::stdin().read_line(&mut x_input)?;
    let target_x: i32 = x_input.trim().parse().unwrap_or(100);

    print!("📍 Enter target Y position: ");
    io::stdout().flush()?;
    let mut y_input = String::new();
    io::stdin().read_line(&mut y_input)?;
    let target_y: i32 = y_input.trim().parse().unwrap_or(100);

    print!("📏 Enter target width (default 400): ");
    io::stdout().flush()?;
    let mut w_input = String::new();
    io::stdin().read_line(&mut w_input)?;
    let target_width: u32 = w_input.trim().parse().unwrap_or(400);

    print!("📏 Enter target height (default 300): ");
    io::stdout().flush()?;
    let mut h_input = String::new();
    io::stdin().read_line(&mut h_input)?;
    let target_height: u32 = h_input.trim().parse().unwrap_or(300);

    print!("⏱️ Enter duration (ms, default 2000): ");
    io::stdout().flush()?;
    let mut dur_input = String::new();
    io::stdin().read_line(&mut dur_input)?;
    let duration_ms: u32 = dur_input.trim().parse().unwrap_or(2000);

    print!("📈 Choose easing (0=Linear, 1=EaseIn, 2=EaseOut, 3=EaseInOut, 4=Bounce, 5=Elastic, 6=Back, default 4): ");
    io::stdout().flush()?;
    let mut easing_input = String::new();
    io::stdin().read_line(&mut easing_input)?;
    let easing_type: u8 = easing_input.trim().parse().unwrap_or(4).clamp(0, 6);

    let command = ipc::AnimationCommand {
        command_type: 0, // start_animation
        hwnd,
        duration_ms,
        easing_type,
        target_x,
        target_y,
        target_width,
        target_height,
    };

    publisher.send_copy(command)?;
    println!(
        "✅ Sent animation command for window {} to ({}, {}) size {}x{} over {}ms",
        hwnd, target_x, target_y, target_width, target_height, duration_ms
    );
    Ok(())
}

fn get_animation_status(
    publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::AnimationCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("📊 Requesting animation status for all windows...");

    let command = ipc::AnimationCommand {
        command_type: 4, // get_status
        hwnd: 0,         // All windows
        ..Default::default()
    };

    publisher.send_copy(command)?;
    println!("✅ Sent GetAnimationStatus command");
    Ok(())
}

fn demo_layout_transitions(
    command_publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::WindowCommand, ()>,
    animation_publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::AnimationCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔄 Starting automatic layout transition demo...");
    println!("This will:");
    println!("  1. Save the current layout as 'demo_start'");
    println!("  2. Create some test animations");
    println!("  3. Save that as 'demo_end'");
    println!("  4. Transition between layouts with different easing functions");

    // Step 1: Save current layout
    let save_command = ipc::WindowCommand {
        command_type: 6, // save_layout
        layout_id: "demo_start".chars().map(|c| c as u32).sum(),
        ..Default::default()
    };
    command_publisher.send_copy(save_command)?;
    println!("💾 Saved current layout as 'demo_start'");

    thread::sleep(Duration::from_millis(500));

    // Step 2: Apply demo layout with bounce animation
    println!("🎬 Applying demo layout with Bounce easing...");
    let apply_command = ipc::WindowCommand {
        command_type: 5, // apply_grid_layout
        layout_id: "demo_start".chars().map(|c| c as u32).sum(),
        animation_duration_ms: 3000,
        easing_type: 4, // Bounce
        ..Default::default()
    };
    command_publisher.send_copy(apply_command)?;

    thread::sleep(Duration::from_secs(4));

    // Step 3: Apply with elastic easing
    println!("🎬 Applying demo layout with Elastic easing...");
    let elastic_command = ipc::WindowCommand {
        command_type: 5, // apply_grid_layout
        layout_id: "demo_start".chars().map(|c| c as u32).sum(),
        animation_duration_ms: 2500,
        easing_type: 5, // Elastic
        ..Default::default()
    };
    command_publisher.send_copy(elastic_command)?;

    thread::sleep(Duration::from_secs(3));

    println!("✅ Layout transition demo completed!");
    Ok(())
}

fn stop_all_animations(
    publisher: &mut Publisher<iceoryx2::service::ipc::Service, ipc::AnimationCommand, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("🛑 Stopping all active animations...");

    let command = ipc::AnimationCommand {
        command_type: 1, // stop_animation
        hwnd: 0,         // All windows
        ..Default::default()
    };

    publisher.send_copy(command)?;
    println!("✅ Sent StopAllAnimations command");
    Ok(())
}

fn check_responses(
    subscriber: &mut Subscriber<iceoryx2::service::ipc::Service, ipc::WindowResponse, ()>,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Some(sample) = subscriber.receive()? {
        let response = *sample;
        match response.response_type {
            0 => println!("✅ Server: Success"),
            1 => println!("❌ Server: Error (code {})", response.error_code),
            2 => println!(
                "📋 Server: Window list contains {} windows",
                response.window_count
            ),
            3 => println!(
                "📊 Server: Grid state - {} windows, {} occupied cells",
                response.window_count, response.data[0]
            ),
            4 => println!(
                "🗂️ Server: {} saved layouts available",
                response.window_count
            ),
            5 => println!(
                "🎬 Server: Animation status - {} animations ({} active)",
                response.data[0], response.data[1]
            ),
            _ => println!(
                "❓ Server: Unknown response type {}",
                response.response_type
            ),
        }
    }
    Ok(())
}
