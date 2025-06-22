// Visual Grid Demo - Shows actual working grid with server/client communication
// Demonstrates animated transition from 4x4 to 8x8 grid

use e_grid::*;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use winapi::shared::windef::{HWND, RECT};

const CLEAR_SCREEN: &str = "\x1B[2J\x1B[1;1H";
const GRID_4X4: (usize, usize) = (4, 4);
const GRID_8X8: (usize, usize) = (8, 8);

struct VisualGridDemo {
    tracker: Arc<Mutex<WindowTracker>>,
    ipc_manager: Option<Arc<Mutex<ipc::GridIpcManager>>>,
    current_config: GridConfig,
    animation_start: Option<Instant>,
    animation_duration: Duration,
    start_config: GridConfig,
    target_config: GridConfig,
}

impl VisualGridDemo {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = GridConfig::new(GRID_4X4.0, GRID_4X4.1);
        let tracker = Arc::new(Mutex::new(WindowTracker::new()));
          // Initialize with some example windows
        {
            let mut tracker_lock = tracker.lock().unwrap();
            tracker_lock.config = config.clone();
            
            // Add some simulated windows to make the demo interesting
            for i in 0..8 {
                let hwnd = (1000 + i) as HWND;
                let title = format!("Window {}", i + 1);
                let x = (i % 4) * 400 + 100;
                let y = (i / 4) * 300 + 100;
                let rect = RECT {
                    left: x as i32,
                    top: y as i32,
                    right: (x + 300) as i32,
                    bottom: (y + 200) as i32,
                };
                  let window_info = WindowInfo {
                    hwnd,
                    title,
                    class_name: "DemoWindow".to_string(),
                    rect,
                    is_minimized: false,
                    is_visible: true,
                    process_id: 1000 + i as u32,
                    grid_cells: vec![(i / 4, i % 4)],
                    monitor_cells: std::collections::HashMap::new(),
                };
                
                tracker_lock.windows.insert(hwnd, window_info);
            }
            
            tracker_lock.update_grid();
        }

        Ok(Self {
            tracker,
            ipc_manager: None,
            current_config: config.clone(),
            animation_start: None,
            animation_duration: Duration::from_secs(3),
            start_config: config.clone(),
            target_config: GridConfig::new(GRID_8X8.0, GRID_8X8.1),
        })
    }

    fn setup_ipc(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut ipc_manager = ipc::GridIpcManager::new(self.tracker.clone())?;
        ipc_manager.setup_services()?;
        
        self.ipc_manager = Some(Arc::new(Mutex::new(ipc_manager)));
        println!("✅ IPC services initialized");
        Ok(())
    }

    fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎯 VISUAL GRID DEMO - SERVER/CLIENT WITH ANIMATION");
        println!("=================================================");
        println!("Starting visual grid demonstration...");
        
        // Setup IPC
        self.setup_ipc()?;
        
        // Start the demo loop
        let mut frame_count = 0;
        let start_time = Instant::now();
        
        // Show initial 4x4 grid for 2 seconds
        println!("\n📋 Phase 1: Displaying 4x4 Grid");
        let phase1_end = start_time + Duration::from_secs(2);
        
        while Instant::now() < phase1_end {
            self.render_frame(frame_count)?;
            thread::sleep(Duration::from_millis(100));
            frame_count += 1;
        }
        
        // Start animation to 8x8
        println!("\n🎬 Phase 2: Animating 4x4 → 8x8 Grid");
        self.start_animation();
        
        // Animation phase
        while self.is_animating() {
            self.update_animation();
            self.render_frame(frame_count)?;
            thread::sleep(Duration::from_millis(50));
            frame_count += 1;
        }
        
        // Show final 8x8 grid for 2 seconds
        println!("\n✅ Phase 3: Final 8x8 Grid");
        let phase3_end = Instant::now() + Duration::from_secs(2);
        
        while Instant::now() < phase3_end {
            self.render_frame(frame_count)?;
            thread::sleep(Duration::from_millis(100));
            frame_count += 1;
        }
        
        // Demonstrate IPC communication
        self.demonstrate_ipc()?;
        
        Ok(())
    }    fn start_animation(&mut self) {
        self.animation_start = Some(Instant::now());
        println!("🎬 Starting grid animation: {} x {} → {} x {}", 
                 GRID_4X4.0, GRID_4X4.1, GRID_8X8.0, GRID_8X8.1);
        
        // Use the library's animation system to animate windows to new positions
        if let Ok(mut tracker) = self.tracker.lock() {
            // Update config first
            tracker.config = self.target_config.clone();
            
            // Calculate new grid positions for each window
            let target_rows = self.target_config.rows;
            let target_cols = self.target_config.cols;
            
            for item in tracker.windows.iter() {
                let (hwnd, window_info) = item.pair();
                
                // Calculate new position for this window in the 8x8 grid
                let window_id = (*hwnd as usize - 1000);
                let new_row = (window_id / 2).min(target_rows - 1);
                let new_col = (window_id % 4 * 2).min(target_cols - 1);
                
                // Calculate target screen position
                let screen_width = 1920;
                let screen_height = 1080;
                let cell_width = screen_width / target_cols as i32;
                let cell_height = screen_height / target_rows as i32;
                
                let target_x = new_col * cell_width + 50;
                let target_y = new_row * cell_height + 50;
                let target_width = (cell_width * 0.8) as i32;
                let target_height = (cell_height * 0.8) as i32;
                
                let target_rect = RECT {
                    left: target_x,
                    top: target_y,
                    right: target_x + target_width,
                    bottom: target_y + target_height,
                };
                
                // Start animation using the library's animation system
                let _ = tracker.start_window_animation(
                    *hwnd,
                    target_rect,
                    self.animation_duration,
                    EasingType::EaseInOut
                );
            }
            
            tracker.update_grid();
        }
    }    fn is_animating(&self) -> bool {
        if let Ok(tracker) = self.tracker.lock() {
            // Check if any animations are active in the library's animation system
            !tracker.active_animations.is_empty()
        } else {
            false
        }
    }fn update_animation(&mut self) {
        // Use the library's animation system instead of custom logic
        if let Ok(mut tracker) = self.tracker.lock() {
            // Update animations using the WindowTracker's built-in animation system
            let completed_windows = tracker.update_animations();
            
            if !completed_windows.is_empty() {
                println!("✅ Animation frames completed for {} windows", completed_windows.len());
            }
            
            // Update current grid config based on window positions
            let mut total_cells = 0;
            let mut max_row = 0;
            let mut max_col = 0;
            
            for item in tracker.windows.iter() {
                let (_, window_info) = item.pair();
                for &(row, col) in &window_info.grid_cells {
                    max_row = max_row.max(row);
                    max_col = max_col.max(col);
                    total_cells += 1;
                }
            }
            
            // Update our display config to match the actual grid state
            if max_row > 0 || max_col > 0 {
                self.current_config = GridConfig::new(max_row + 1, max_col + 1);
            }
            
            // Publish animation updates via IPC
            if let Some(ipc_manager_arc) = &self.ipc_manager {
                if let Ok(mut ipc_manager) = ipc_manager_arc.lock() {
                    let _ = ipc_manager.publish_grid_state_changed(
                        tracker.windows.len(),
                        self.count_occupied_cells(&tracker)
                    );
                }
            }
            
            // Check if we still have active animations
            if tracker.active_animations.is_empty() && self.animation_start.is_some() {
                self.animation_start = None;
                println!("✅ All animations complete! Windows repositioned to new grid.");
            }
        }
    }

    fn ease_in_out_cubic(&self, t: f32) -> f32 {
        if t < 0.5 {
            4.0 * t * t * t
        } else {
            1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
        }
    }

    fn interpolate_size(&self, start: usize, end: usize, progress: f32) -> usize {
        let start_f = start as f32;
        let end_f = end as f32;
        (start_f + (end_f - start_f) * progress).round() as usize
    }

    fn render_frame(&self, frame: u32) -> Result<(), Box<dyn std::error::Error>> {
        print!("{}", CLEAR_SCREEN);
        
        // Header
        println!("🎯 VISUAL GRID DEMO - Frame #{}", frame);
        println!("{}", "=".repeat(60));
          let status = if self.is_animating() {
            if let Ok(tracker) = self.tracker.lock() {
                let active_count = tracker.active_animations.len();
                format!("🎬 ANIMATING: {} windows in motion", active_count)
            } else {
                "🎬 ANIMATING".to_string()
            }
        } else {
            "📋 STATIC DISPLAY".to_string()
        };
        
        println!("Status: {}", status);
        println!("Grid Size: {} x {} cells", self.current_config.rows, self.current_config.cols);
        
        if let Ok(tracker) = self.tracker.lock() {
            println!("Windows: {} tracked", tracker.windows.len());
        }
        
        println!();
        
        // Render the actual grid
        self.render_grid()?;
        
        // Show window details
        self.render_window_details()?;
        
        io::stdout().flush()?;
        Ok(())
    }

    fn render_grid(&self) -> Result<(), Box<dyn std::error::Error>> {
        let rows = self.current_config.rows;
        let cols = self.current_config.cols;
        
        // Grid top border
        print!("┌");
        for col in 0..cols {
            print!("─────");
            if col < cols - 1 { print!("┬"); }
        }
        println!("┐");
        
        // Grid cells with content
        for row in 0..rows {
            // Cell content row
            print!("│");
            for col in 0..cols {
                let cell_content = self.get_cell_content(row, col);
                print!("{:^5}", cell_content);
                print!("│");
            }
            println!();
            
            // Horizontal separator (except for last row)
            if row < rows - 1 {
                print!("├");
                for col in 0..cols {
                    print!("─────");
                    if col < cols - 1 { print!("┼"); }
                }
                println!("┤");
            }
        }
        
        // Grid bottom border
        print!("└");
        for col in 0..cols {
            print!("─────");
            if col < cols - 1 { print!("┴"); }
        }
        println!("┘");
        
        Ok(())
    }

    fn get_cell_content(&self, row: usize, col: usize) -> String {
        if let Ok(tracker) = self.tracker.lock() {
            // Count windows in this cell
            let mut window_count = 0;
            let mut window_ids = Vec::new();
            
            for item in tracker.windows.iter() {
                let (hwnd, window_info) = item.pair();
                for &(win_row, win_col) in &window_info.grid_cells {
                    if win_row == row && win_col == col {
                        window_count += 1;
                        window_ids.push(*hwnd as u64 % 1000); // Show last 3 digits
                        break;
                    }
                }
            }
            
            match window_count {
                0 => "   ".to_string(),
                1 => format!("W{}", window_ids[0]),
                _ => format!("{}W", window_count),
            }
        } else {
            "ERR".to_string()
        }
    }    fn render_window_details(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📋 Window Details & Positions:");
        println!("{}", "─".repeat(50));
        
        if let Ok(tracker) = self.tracker.lock() {
            for (i, item) in tracker.windows.iter().enumerate() {
                let (hwnd, window_info) = item.pair();
                if i >= 6 { // Show more windows during animation
                    println!("   ... and {} more windows", tracker.windows.len() - 6);
                    break;
                }
                
                let id = *hwnd as u64 % 1000;
                let cells: Vec<String> = window_info.grid_cells.iter()
                    .map(|(r, c)| format!("({},{})", r, c))
                    .collect();
                
                // Show both grid position and screen coordinates during animation
                if self.is_animating() {
                    println!("  W{}: {} -> Cell: {} | Pos: ({}, {})", 
                        id, 
                        if window_info.title.len() > 10 {
                            format!("{}...", &window_info.title[..10])
                        } else {
                            window_info.title.clone()
                        },
                        cells.join(", "),
                        window_info.rect.left,
                        window_info.rect.top
                    );
                } else {
                    println!("  W{}: {} -> Cells: {}", 
                        id, 
                        if window_info.title.len() > 12 {
                            format!("{}...", &window_info.title[..12])
                        } else {
                            window_info.title.clone()
                        },
                        cells.join(", ")
                    );
                }
            }
              // Show animation progress
            if self.is_animating() {
                if let Ok(tracker) = self.tracker.lock() {
                    let active_count = tracker.active_animations.len();
                    let total_count = tracker.windows.len();
                    let progress = ((total_count - active_count) as f32 / total_count as f32 * 100.0).min(100.0);
                    println!("\n🎬 Animation Progress: {:.1}% | {} of {} windows completed", 
                             progress, total_count - active_count, total_count);
                }
            }
        }
        
        Ok(())
    }    fn demonstrate_ipc(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🔄 IPC COMMUNICATION DEMONSTRATION");
        println!("{}", "=".repeat(50));
        println!("Simulating client requesting grid animation...\n");
        
        if let Some(ipc_manager_arc) = &self.ipc_manager {
            let mut ipc_manager = ipc_manager_arc.lock().unwrap();
            
            // Show initial state
            println!("📨 Client → Server: GetGridState");
            let response = ipc_manager.handle_command(ipc::GridCommand::GetGridState)?;
            println!("📤 Server → Client: {:?}\n", response);
            
            // Client requests window list  
            println!("📨 Client → Server: GetWindowList");
            let response = ipc_manager.handle_command(ipc::GridCommand::GetWindowList)?;
            println!("📤 Server → Client: {:?}\n", response);
              // Client requests animation via IPC command (this is the proper way)
            println!("📨 Client → Server: StartAnimation Request for Grid Transition");
            
            // Simulate multiple window animations requested by client
            if let Ok(tracker) = self.tracker.lock() {
                for (i, item) in tracker.windows.iter().enumerate().take(4) {
                    let (hwnd, _) = item.pair();
                    
                    println!("📨 Client → Server: StartAnimation(hwnd={}, target=cell({},{}))", 
                             *hwnd as u64, i / 2, i % 2);
                    
                    let response = ipc_manager.handle_command(ipc::GridCommand::StartAnimation {
                        hwnd: *hwnd as u64,
                        target_x: (i % 2) * 300 + 100,
                        target_y: (i / 2) * 200 + 100,
                        target_width: 250,
                        target_height: 180,
                        duration_ms: 2000,
                        easing_type: EasingType::EaseInOut,
                    })?;
                    
                    println!("📤 Server → Client: {:?}", response);
                }
            }
            
            // Show animation status updates
            thread::sleep(Duration::from_millis(100));
            for frame in 0..8 {
                println!("\n📡 Server → Client: Animation Status Update #{}", frame + 1);
                
                let status_response = ipc_manager.handle_command(ipc::GridCommand::GetAnimationStatus {
                    hwnd: 0, // Get all animations
                })?;
                
                match status_response {
                    ipc::GridResponse::AnimationStatus { statuses } => {
                        println!("� Client: Received {} animation updates", statuses.len());
                        for (hwnd, is_active, progress) in &statuses {
                            if *is_active {
                                println!("   🎭 Window {}: {:.1}% complete", hwnd, progress * 100.0);
                            }
                        }
                        
                        // Break if all animations are complete
                        if !statuses.iter().any(|(_, active, _)| *active) {
                            println!("✅ All animations completed!");
                            break;
                        }
                    }
                    _ => println!("📥 Client: Unexpected response format"),
                }
                
                thread::sleep(Duration::from_millis(300));
            }
            
            // Final state
            println!("\n📋 Final grid state after client request:");
            self.render_grid()?;
            
            println!("\n✅ IPC Demo Complete: Client successfully requested grid animation");
            println!("   🔄 Server processed request and animated windows");
            println!("   📡 Client received real-time animation updates");
            println!("   🎯 Both client and server show synchronized grid state");
        }
        
        Ok(())
    }

    fn animate_windows_to_grid(&self, tracker: &mut WindowTracker, progress: f32) {
        let rows = self.current_config.rows;
        let cols = self.current_config.cols;
        
        // Calculate virtual screen bounds (simulated)
        let screen_width = 1920;
        let screen_height = 1080;
        let cell_width = screen_width / cols as i32;
        let cell_height = screen_height / rows as i32;
        
        for item in tracker.windows.iter() {
            let (hwnd, window_info) = item.pair();
            
            // Calculate original position in 4x4 grid
            let original_row = (*hwnd as usize - 1000) / 4;
            let original_col = (*hwnd as usize - 1000) % 4;
            
            // Calculate target position in 8x8 grid (spread windows out)
            let target_row = (original_row * 2).min(rows - 1);
            let target_col = (original_col * 2).min(cols - 1);
            
            // Calculate original and target screen positions
            let original_x = original_col * (1920 / 4) + 100;
            let original_y = original_row * (1080 / 4) + 100;
            let target_x = target_col * cell_width + 50;
            let target_y = target_row * cell_height + 50;
            
            // Interpolate position
            let current_x = original_x as f32 + (target_x as f32 - original_x as f32) * progress;
            let current_y = original_y as f32 + (target_y as f32 - original_y as f32) * progress;
            
            // Update window rect
            if let Some(mut window) = tracker.windows.get_mut(hwnd) {
                let width = window.rect.right - window.rect.left;
                let height = window.rect.bottom - window.rect.top;
                
                window.rect.left = current_x as i32;
                window.rect.top = current_y as i32;
                window.rect.right = current_x as i32 + width;
                window.rect.bottom = current_y as i32 + height;
                
                // Update grid cell assignment
                let new_grid_row = (current_y as usize / (screen_height as usize / rows)).min(rows - 1);
                let new_grid_col = (current_x as usize / (screen_width as usize / cols)).min(cols - 1);
                window.grid_cells = vec![(new_grid_row, new_grid_col)];
            }
        }
    }

    fn count_occupied_cells(&self, tracker: &WindowTracker) -> usize {
        let mut occupied = std::collections::HashSet::new();
        for item in tracker.windows.iter() {
            let (_, window_info) = item.pair();
            for &(row, col) in &window_info.grid_cells {
                occupied.insert((row, col));
            }
        }
        occupied.len()
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable ANSI escape sequences on Windows
    #[cfg(windows)]
    {
        use winapi::um::consoleapi::SetConsoleMode;
        use winapi::um::handleapi::INVALID_HANDLE_VALUE;
        use winapi::um::processenv::GetStdHandle;
        use winapi::um::winbase::STD_OUTPUT_HANDLE;
        use winapi::um::wincon::ENABLE_VIRTUAL_TERMINAL_PROCESSING;

        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if handle != INVALID_HANDLE_VALUE {
                SetConsoleMode(handle, ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }

    let mut demo = VisualGridDemo::new()?;
    demo.run()?;
    
    println!("\n🎉 Demo complete! Press Enter to exit...");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    Ok(())
}
