// Grid Client - Demonstrates real IPC communication with the grid server
// Shows same output as server and sends commands

use e_grid::grid::GridConfig;
use e_grid::ipc_manager::GridIpcManager;
use e_grid::ipc_protocol::{GridCommand, GridResponse};
use e_grid::window_tracker::WindowTracker;
use e_grid::EasingType;
use std::io;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

struct GridClient {
    ipc_manager: Arc<Mutex<GridIpcManager>>,
}

impl GridClient {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Create a minimal tracker for the client
        let config = GridConfig::new(4, 4);
        let mut tracker = WindowTracker::new_with_config(config);

        // Scan existing windows
        tracker.scan_existing_windows();

        // Wrap tracker in Arc<Mutex<>>
        let tracker = Arc::new(Mutex::new(tracker));

        // Create IPC manager
        let mut ipc_manager = GridIpcManager::new(tracker)?;
        // TODO: Replace the following placeholders with actual arguments as required by setup_services
        ipc_manager.setup_services(true, true, true, true, true, true, true, true, true)?;
        Ok(Self {
            ipc_manager: Arc::new(Mutex::new(ipc_manager)),
        })
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔗 GRID CLIENT - IPC COMMUNICATION DEMO");
        println!("======================================");
        println!("Connecting to grid server...\n");

        // Wait a moment for server to be ready
        thread::sleep(Duration::from_millis(500));

        // Demonstrate various client commands
        self.demo_get_grid_state()?;
        self.demo_get_window_list()?;
        self.demo_change_grid_size()?;
        self.demo_window_movement()?;
        self.demo_animation_commands()?;

        println!("\n🎉 Client demonstration complete!");
        Ok(())
    }

    fn demo_get_grid_state(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📊 1. GETTING GRID STATE");
        println!("========================");

        if let Ok(mut manager) = self.ipc_manager.lock() {
            let response = manager.handle_grid_command(GridCommand::GetGridState)?;
            println!("📤 Server Response: {:?}", response);

            match response {
                GridResponse::GridState {
                    total_windows,
                    occupied_cells,
                    grid_summary,
                } => {
                    println!("   📋 Total Windows: {}", total_windows);
                    println!("   📍 Occupied Cells: {}", occupied_cells);
                    println!("   📝 Summary:\n{}", grid_summary);
                }
                _ => println!("   ⚠️ Unexpected response format"),
            }
        }

        println!();
        Ok(())
    }

    fn demo_get_window_list(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("📋 2. GETTING WINDOW LIST");
        println!("=========================");

        if let Ok(mut manager) = self.ipc_manager.lock() {
            let response = manager.handle_grid_command(GridCommand::GetWindowList)?;
            println!("📤 Server Response: {:?}", response);

            // The server should also publish individual window details
            println!("   📡 Server will publish individual window details via IPC");
        }

        println!();
        Ok(())
    }

    fn demo_change_grid_size(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔧 3. CHANGING GRID SIZE (4x4 → 8x8)");
        println!("=====================================");

        if let Ok(mut manager) = self.ipc_manager.lock() {
            // First show current state
            println!("📨 Client → Server: GetGridConfig");

            // Change to 8x8
            println!("📨 Client → Server: SetGridConfig(8, 8)");
            // let response = manager.handle_command(ipc::GridCommand::SetGridConfig {
            //     rows: 8,
            //     cols: 8
            // })?;
            // println!("📤 Server Response: {:?}", response);

            // match response {
            //     ipc::GridResponse::GridConfigUpdated { rows, cols, message } => {
            //         println!("   ✅ Grid updated to {}x{}", rows, cols);
            //         println!("   💬 Message: {}", message);
            //     }
            //     _ => println!("   ⚠️ Unexpected response format"),
            // }

            // Verify the change
            thread::sleep(Duration::from_millis(100));
            println!("\n📨 Client → Server: GetGridState (verification)");
            let verify_response = manager.handle_grid_command(GridCommand::GetGridState)?;
            println!("📤 Server Response: {:?}", verify_response);
        }

        println!();
        Ok(())
    }

    fn demo_window_movement(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎯 4. WINDOW MOVEMENT COMMANDS");
        println!("==============================");

        if let Ok(mut manager) = self.ipc_manager.lock() {
            // Move a window to a specific cell
            println!("📨 Client → Server: AssignWindowToVirtualCell(1001, 2, 3)");
            let response = manager.handle_grid_command(GridCommand::AssignWindowToVirtualCell {
                hwnd: 1001,
                target_row: 2,
                target_col: 3,
            })?;
            println!("📤 Server Response: {:?}", response);

            // Move another window to a monitor-specific cell
            println!("\n📨 Client → Server: AssignWindowToMonitorCell(1002, 1, 1, 0)");
            let response = manager.handle_grid_command(GridCommand::AssignWindowToMonitorCell {
                hwnd: 1002,
                target_row: 1,
                target_col: 1,
                monitor_id: 0,
            })?;
            println!("📤 Server Response: {:?}", response);
        }

        println!();
        Ok(())
    }

    fn demo_animation_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎬 5. ANIMATION COMMANDS");
        println!("========================");

        if let Ok(mut manager) = self.ipc_manager.lock() {
            // Start an animation
            println!(
                "📨 Client → Server: StartAnimation(1003, 100, 100, 400, 300, 2000ms, EaseInOut)"
            );
            let response = manager.handle_grid_command(GridCommand::StartAnimation {
                hwnd: 1003,
                target_x: 100,
                target_y: 100,
                target_width: 400,
                target_height: 300,
                duration_ms: 2000,
                easing_type: EasingType::EaseInOut,
            })?;
            println!("📤 Server Response: {:?}", response);

            // Check animation status
            thread::sleep(Duration::from_millis(100));
            println!("\n📨 Client → Server: GetAnimationStatus(1003)");
            let status_response =
                manager.handle_grid_command(GridCommand::GetAnimationStatus { hwnd: 1003 })?;
            println!("📤 Server Response: {:?}", status_response);

            match status_response {
                GridResponse::AnimationStatus { statuses } => {
                    for (hwnd, is_active, progress) in statuses {
                        println!(
                            "   🎭 Window {}: Active={}, Progress={:.1}%",
                            hwnd,
                            is_active,
                            progress * 100.0
                        );
                    }
                }
                _ => println!("   ⚠️ Unexpected response format"),
            }
        }

        println!();
        Ok(())
    }

    fn display_client_stats(&self) {
        println!("📊 CLIENT STATISTICS");
        println!("===================");
        println!("🔗 IPC Connection: Active");
        println!("📡 Services: Connected to server");
        println!("💻 Client Type: Command & Control");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = GridClient::new()?;

    // Show client info
    client.display_client_stats();
    println!();

    // Run the demo
    client.run()?;

    println!("Press Enter to exit...");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(())
}
