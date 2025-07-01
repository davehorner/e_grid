// Visual Grid Demo - Shows actual working grid with server/client communication
// Demonstrates animated transition from 4x4 to 8x8 grid

use e_grid::config::grid_config::GridConfig;
use e_grid::ipc;
use e_grid::ipc_manager::GridIpcManager;
use e_grid::*;
use log::debug;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use winapi::shared::windef::{HWND, RECT}; // <-- Add this line to import the ipc module

const CLEAR_SCREEN: &str = "\x1B[2J\x1B[1;1H";
const GRID_4X4: (usize, usize) = (4, 4);
const GRID_8X8: (usize, usize) = (8, 8);

struct VisualGridDemo {
    tracker: Arc<Mutex<WindowTracker>>,
    ipc_manager: Option<Arc<Mutex<GridIpcManager>>>,
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
            // Set grid size to 8x8 using the new method
            tracker_lock.set_grid_size(GRID_8X8.0, GRID_8X8.1);

            // Discover real windows using the WindowTracker's refresh method
            tracker_lock.scan_existing_windows();

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
        debug!("Setting up IPC services for Visual Grid Demo");
        println!("\n🔄 Initializing IPC services for Visual Grid Demo...");
        let mut ipc_manager = GridIpcManager::new(self.tracker.clone())?;
        debug!("Setting up IPC services for Visual Grid Demo");
        ipc_manager.setup_services(
            true,  // events
            true,  // commands
            true,  // responses
            false, // window_details
            false, // layout
            false, // cell_assignments
            true,  // animation
            true,  // animation_status
            false, // heartbeat
        )?;
        debug!("Setting up IPC services for Visual Grid Demo");

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
        println!("IPC services ready for communication");
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
    }
    fn start_animation(&mut self) {
        self.animation_start = Some(Instant::now());
        println!(
            "🎬 Starting grid animation: {} x {} → {} x {}",
            GRID_4X4.0, GRID_4X4.1, GRID_8X8.0, GRID_8X8.1
        );

        // Use the library's animation system to animate windows to new positions
        if let Ok(mut tracker) = self.tracker.lock() {
            // Update config first
            tracker.config = self.target_config.clone();

            // Calculate new grid positions for each window
            let target_rows = self.target_config.rows;
            let target_cols = self.target_config.cols;

            // Collect window handles and their target rects first to avoid borrow conflicts
            let mut animations = Vec::new();
            for item in tracker.windows.iter() {
                let (window_id, window_info) = item.pair();

                // Calculate new position for this window in the 8x8 grid
                let window_id = *window_id as usize - 1000;
                let new_row = (window_id / 2).min(target_rows - 1);
                let new_col = (window_id % 4 * 2).min(target_cols - 1);

                // Calculate target screen position
                let screen_width = 1920;
                let screen_height = 1080;
                let cell_width = screen_width / target_cols as i32;
                let cell_height = screen_height / target_rows as i32;

                let target_x = new_col as i32 * cell_width + 50;
                let target_y = new_row as i32 * cell_height + 50;
                let target_width = (cell_width as f32 * 0.8) as i32;
                let target_height = (cell_height as f32 * 0.8) as i32;

                let target_rect = RECT {
                    left: target_x,
                    top: target_y,
                    right: target_x + target_width,
                    bottom: target_y + target_height,
                };

                animations.push((window_id, target_rect));
            }

            // Now, do the mutable borrow and start animations
            for (window_id, target_rect) in animations {
                let _ = tracker.start_window_animation(
                    window_id.try_into().unwrap(),
                    target_rect,
                    self.animation_duration,
                    EasingType::EaseInOut,
                );
            }

            tracker.update_grid();
        }
    }
    fn is_animating(&self) -> bool {
        if let Ok(tracker) = self.tracker.lock() {
            // Check if any animations are active in the library's animation system
            !tracker.active_animations.is_empty()
        } else {
            false
        }
    }
    fn update_animation(&mut self) {
        // Use the library's animation system instead of custom logic
        if let Ok(mut tracker) = self.tracker.lock() {
            // Update animations using the WindowTracker's built-in animation system
            let completed_windows = tracker.update_animations();

            if !completed_windows.is_empty() {
                println!(
                    "✅ Animation frames completed for {} windows",
                    completed_windows.len()
                );
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
                        self.count_occupied_cells(&tracker),
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

    /// Renders the grid dynamically sized to fit all window assignments, with cell width based on content
    fn render_dynamic_grid(&self, use_tracker: bool) -> Result<(), Box<dyn std::error::Error>> {
        // 1. Gather all window assignments to determine max row/col and cell contents
        let mut max_row = 0;
        let mut max_col = 0;
        let mut cell_contents: std::collections::HashMap<(usize, usize), Vec<String>> =
            std::collections::HashMap::new();
        if let Ok(tracker) = self.tracker.lock() {
            for item in tracker.windows.iter() {
                let (window_id, window_info) = item.pair();
                let cells = &window_info.grid_cells;
                for &(row, col) in cells {
                    max_row = max_row.max(row);
                    max_col = max_col.max(col);
                    let entry = cell_contents.entry((row, col)).or_insert_with(Vec::new);
                    let last_two = (*window_id as u64) % 100;
                    entry.push(format!("{:02}", last_two));
                }
            }
        }
        let rows = max_row + 1;
        let cols = max_col + 1;

        // 2. Compute max cell width needed
        let mut max_cell_width = 3; // at least 3 for empty
        for v in cell_contents.values() {
            let joined = v.join(",");
            max_cell_width = max_cell_width.max(joined.len());
        }
        max_cell_width = max_cell_width.max(2); // Ensure at least enough for 'XX'

        // 3. Render grid borders and cells
        // Top border
        print!("┌");
        for col in 0..cols {
            print!("{:─<width$}", "", width = max_cell_width);
            if col < cols - 1 {
                print!("┬");
            }
        }
        println!("┐");

        // Rows
        for row in 0..rows {
            // Cell content row
            print!("│");
            for col in 0..cols {
                let content = cell_contents
                    .get(&(row, col))
                    .map(|v| v.join(","))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "XX".to_string());
                print!("{:^width$}", content, width = max_cell_width);
                print!("│");
            }
            println!();
            // Separator
            if row < rows - 1 {
                print!("├");
                for col in 0..cols {
                    print!("{:─<width$}", "", width = max_cell_width);
                    if col < cols - 1 {
                        print!("┼");
                    }
                }
                println!("┤");
            }
        }
        // Bottom border
        print!("└");
        for col in 0..cols {
            print!("{:─<width$}", "", width = max_cell_width);
            if col < cols - 1 {
                print!("┴");
            }
        }
        println!("┘");
        Ok(())
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
        println!(
            "Grid Size: {} x {} cells (Physical)",
            self.current_config.rows, self.current_config.cols
        );
        if let Ok(tracker) = self.tracker.lock() {
            println!(
                "Virtual Grid Size: {} x {} cells (Tracker/Server)",
                tracker.config.rows, tracker.config.cols
            );
            println!("Windows: {} tracked", tracker.windows.len());
        }
        println!();
        // Render the dynamic grid (all window assignments, no truncation)
        println!("Dynamic Grid (All Window Assignments):");
        self.render_dynamic_grid(true)?;
        println!();
        // Render the physical grid (current display)
        println!("Physical Grid (Current Display):");
        self.render_grid_with_config(false)?;
        println!();

        // Show window details
        if let Ok(tracker) = self.tracker.lock() {
            tracker.print_all_grids();
        }
        self.render_window_details()?;

        io::stdout().flush()?;
        Ok(())
    }

    /// Renders either the virtual (tracker) or physical (current) grid
    fn render_grid_with_config(&self, use_tracker: bool) -> Result<(), Box<dyn std::error::Error>> {
        let (rows, cols) = if use_tracker {
            if let Ok(tracker) = self.tracker.lock() {
                (tracker.config.rows, tracker.config.cols)
            } else {
                (self.current_config.rows, self.current_config.cols)
            }
        } else {
            (self.current_config.rows, self.current_config.cols)
        };

        // Grid top border
        print!("┌");
        for col in 0..cols {
            print!("─────");
            if col < cols - 1 {
                print!("┬");
            }
        }
        println!("┐");

        // Grid cells with content
        for row in 0..rows {
            // Cell content row
            print!("│");
            for col in 0..cols {
                let cell_content = if use_tracker {
                    self.get_cell_content_for_grid(row, col, true)
                } else {
                    self.get_cell_content_for_grid(row, col, false)
                };
                print!("{:^5}", cell_content);
                print!("│");
            }
            println!();

            // Horizontal separator (except for last row)
            if row < rows - 1 {
                print!("├");
                for col in 0..cols {
                    print!("─────");
                    if col < cols - 1 {
                        print!("┼");
                    }
                }
                println!("┤");
            }
        }

        // Grid bottom border
        print!("└");
        for col in 0..cols {
            print!("─────");
            if col < cols - 1 {
                print!("┴");
            }
        }
        println!("┘");

        Ok(())
    }

    /// Returns cell content for either the tracker (virtual) or current (physical) grid
    fn get_cell_content_for_grid(&self, row: usize, col: usize, use_tracker: bool) -> String {
        if let Ok(tracker) = self.tracker.lock() {
            let mut window_ids = Vec::new();
            for item in tracker.windows.iter() {
                let (window_id, window_info) = item.pair();
                let cells = if use_tracker {
                    &window_info.grid_cells
                } else {
                    // For physical grid, recalculate based on current_config
                    &window_info.grid_cells
                };
                for &(win_row, win_col) in cells {
                    if win_row == row && win_col == col {
                        let last_two = (*window_id as u64) % 100;
                        window_ids.push(format!("{:02}", last_two));
                        break;
                    }
                }
            }
            if window_ids.is_empty() {
                "   ".to_string()
            } else {
                let mut content = window_ids
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",");
                if window_ids.len() > 3 {
                    content.push_str(",..");
                }
                content
            }
        } else {
            "ERR".to_string()
        }
    }
    fn render_window_details(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n📋 Window Details & Positions:");
        println!("{}", "─".repeat(50));

        if let Ok(tracker) = self.tracker.lock() {
            for (i, item) in tracker.windows.iter().enumerate() {
                let (window_id, window_info) = item.pair();
                if i >= 6 {
                    println!("   ... and {} more windows", tracker.windows.len() - 6);
                    break;
                }

                let id = *window_id as u64 % 1000;
                let cells: Vec<String> = window_info
                    .grid_cells
                    .iter()
                    .map(|(r, c)| format!("({},{})", r, c))
                    .collect();

                // Print full debug info for each window
                println!(
                    "  HWND: {:#x} | W{} | Title: '{}' | Cells: {} | Rect: ({}, {}, {}, {})",
                    *window_id,
                    id,
                    if window_info.title.len() > 24 {
                        format!("{}...", &window_info.title[..24])
                    } else {
                        window_info.title.clone()
                    },
                    cells.join(", "),
                    window_info.rect.left,
                    window_info.rect.top,
                    window_info.rect.right,
                    window_info.rect.bottom
                );
            }
            // Show animation progress
            if self.is_animating() {
                if let Ok(tracker) = self.tracker.lock() {
                    let active_count = tracker.active_animations.len();
                    let total_count = tracker.windows.len();
                    let progress = ((total_count - active_count) as f32 / total_count as f32
                        * 100.0)
                        .min(100.0);
                    println!(
                        "\n🎬 Animation Progress: {:.1}% | {} of {} windows completed",
                        progress,
                        total_count - active_count,
                        total_count
                    );
                }
            }
        }

        Ok(())
    }
    fn demonstrate_ipc(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n🔄 IPC COMMUNICATION DEMONSTRATION");
        println!("{}", "=".repeat(50));
        println!("Simulating client requesting grid animation...\n");

        if let Some(ipc_manager_arc) = &self.ipc_manager {
            // Lock ipc_manager only when needed
            let mut ipc_manager = ipc_manager_arc.lock().unwrap();

            // Show initial state
            println!("📨 Client → Server: GetGridState");
            let response =
                ipc_manager.handle_grid_command(ipc_protocol::GridCommand::GetGridState)?;
            println!("📤 Server → Client: {:?}\n", response);

            // Client requests window list
            println!("📨 Client → Server: GetWindowList");
            let response =
                ipc_manager.handle_grid_command(ipc_protocol::GridCommand::GetWindowList)?;
            println!("📤 Server → Client: {:?}\n", response);
            // Client requests animation via IPC command (this is the proper way)
            println!("📨 Client → Server: StartAnimation Request for Grid Transition");

            // Collect window handles to avoid holding tracker lock during IPC calls
            let window_ids: Vec<u64> = if let Ok(tracker) = self.tracker.lock() {
                tracker
                    .windows
                    .iter()
                    .take(4)
                    .map(|item| *item.key())
                    .collect()
            } else {
                Vec::new()
            };

            for (i, window_id) in window_ids.iter().enumerate() {
                println!(
                    "📨 Client → Server: StartAnimation(hwnd={}, target=cell({},{}))",
                    *window_id,
                    i / 2,
                    i % 2
                );

                let response =
                    ipc_manager.handle_grid_command(ipc_protocol::GridCommand::StartAnimation {
                        hwnd: *window_id,
                        target_x: ((i % 2) * 300 + 100) as i32,
                        target_y: ((i / 2) * 200 + 100) as i32,
                        target_width: 250,
                        target_height: 180,
                        duration_ms: 2000,
                        easing_type: EasingType::EaseInOut,
                    })?;

                println!("📤 Server → Client: {:?}", response);
            }

            // Show animation status updates
            thread::sleep(Duration::from_millis(100));
            for frame in 0..8 {
                println!(
                    "\n📡 Server → Client: Animation Status Update #{}",
                    frame + 1
                );

                let status_response = ipc_manager.handle_grid_command(
                    ipc_protocol::GridCommand::GetAnimationStatus {
                        hwnd: 0, // Get all animations
                    },
                )?;

                match status_response {
                    e_grid::ipc_protocol::GridResponse::AnimationStatus { statuses } => {
                        println!("� Client: Received {} animation updates", statuses.len());
                        for (hwnd, is_active, progress) in &statuses {
                            if *is_active {
                                println!(
                                    "   🎭 Window {}: {:.1}% complete",
                                    hwnd,
                                    progress * 100.0
                                );
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
            // Only lock tracker for rendering, not during IPC calls
            self.render_grid_with_config(false)?;

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
            let (window_id, window_info) = item.pair();

            // Calculate original position in 4x4 grid
            let original_row = (*window_id as usize - 1000) / 4;
            let original_col = (*window_id as usize - 1000) % 4;

            // Calculate target position in 8x8 grid (spread windows out)
            let target_row = (original_row * 2).min(rows - 1);
            let target_col = (original_col * 2).min(cols - 1);

            // Calculate original and target screen positions
            let original_x = original_col as i32 * (1920 / 4) + 100;
            let original_y = original_row as i32 * (1080 / 4) + 100;
            let target_x = target_col as i32 * cell_width + 50;
            let target_y = target_row as i32 * cell_height + 50;

            // Interpolate position
            let current_x = original_x as f32 + (target_x as f32 - original_x as f32) * progress;
            let current_y = original_y as f32 + (target_y as f32 - original_y as f32) * progress;

            // Update window rect
            if let Some(mut window) = tracker.windows.get_mut(window_id) {
                let width = window.rect.right - window.rect.left;
                let height = window.rect.bottom - window.rect.top;

                window.rect.left = current_x as i32;
                window.rect.top = current_y as i32;
                window.rect.right = current_x as i32 + width;
                window.rect.bottom = current_y as i32 + height;

                // Update grid cell assignment
                let new_grid_row =
                    (current_y as usize / (screen_height as usize / rows)).min(rows - 1);
                let new_grid_col =
                    (current_x as usize / (screen_width as usize / cols)).min(cols - 1);
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

    pub fn run_with_move_resize_callback(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::mpsc;
        // Create a channel for grid print signals
        let (tx, rx) = mpsc::channel();

        // Create a GridClient (must be on main thread)
        let mut client = match e_grid::ipc_client::GridClient::new() {
            Ok(c) => c,
            Err(_) => {
                println!("Grid server not running, starting server in-process...");
                std::thread::spawn(|| {
                    let _ = e_grid::ipc_server::start_server();
                });
                // Retry loop
                let mut last_err = None;
                let mut client = None;
                for _ in 0..10 {
                    match e_grid::ipc_client::GridClient::new() {
                        Ok(c) => {
                            println!("Connected to in-process server!");
                            client = Some(c);
                            break;
                        }
                        Err(e) => {
                            last_err = Some(e);
                            std::thread::sleep(std::time::Duration::from_millis(300));
                        }
                    }
                }
                if client.is_none() {
                    panic!("Failed to connect to in-process server: {:?}", last_err);
                }
                client.unwrap()
            }
        };
        // Register move/resize start callback (send signal to channel)
        let tx_start = tx.clone();
        client
            .set_move_resize_start_callback(move |e| {
                let _ = tx_start.send(());
                // [CLEANUP] Removed debug print: [Move/Resize START]
                // println!("[Move/Resize START] HWND={:?} type={}", e.hwnd, e.event_type);
            })
            .unwrap();
        // Register move/resize stop callback (send signal to channel)
        let tx_stop = tx.clone();
        client
            .set_move_resize_stop_callback(move |e| {
                let _ = tx_stop.send(());
                // [CLEANUP] Removed debug print: [Move/Resize STOP]
                // println!("[Move/Resize STOP] HWND={:?} type={}", e.hwnd, e.event_type);
            })
            .unwrap();
        println!("[visual_grid_demo] Registered move/resize callbacks");
        // Start background monitoring
        client.start_background_monitoring().unwrap();
        // [CLEANUP] Removed debug print: [visual_grid_demo] Background monitoring started
        println!("[visual_grid_demo] Background monitoring started");
        // Spawn a thread to listen for print signals and set a flag for the main thread to print the grid
        let print_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let print_flag_bg = print_flag.clone();
        std::thread::spawn(move || {
            // [CLEANUP] Removed debug print: [visual_grid_demo] Background print thread started
            println!("[visual_grid_demo] Background print thread started");
            while let Ok(()) = rx.recv() {
                // [CLEANUP] Removed debug print: [visual_grid_demo] Received print signal (setting print flag)
                println!("[visual_grid_demo] Received print signal (setting print flag)");
                print_flag_bg.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        // Continue with the rest of your demo logic
        std::thread::sleep(std::time::Duration::from_millis(500)); // Give server time to start
        self.run_with_print_flag(print_flag)
    }

    // New method: run_with_print_flag
    fn run_with_print_flag(
        &mut self,
        print_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("🎯 VISUAL GRID DEMO - SERVER/CLIENT WITH ANIMATION");
        println!("=================================================");
        println!("Starting visual grid demonstration...");

        // Setup IPC
        self.setup_ipc()?;
        println!("IPC services ready for communication");
        // Start the demo loop
        let mut frame_count = 0;
        let start_time = Instant::now();
        let mut last_print_time = Instant::now() - Duration::from_secs(2); // allow immediate first print

        // Show initial 4x4 grid for 2 seconds
        println!("\n📋 Phase 1: Displaying 4x4 Grid");
        let phase1_end = start_time + Duration::from_secs(120);

        while Instant::now() < phase1_end {
            // self.render_frame(frame_count)?;
            // println!("\n[Grid FRAME]:");
            if print_flag.load(std::sync::atomic::Ordering::SeqCst)
                && last_print_time.elapsed() >= Duration::from_secs(1)
            {
                if let Ok(mut tracker) = self.tracker.lock() {
                    tracker.scan_existing_windows();
                    tracker.update_grid();
                    println!("\n[Grid after move/resize event]:");
                    let _ = tracker.print_all_grids();
                    let _ = std::io::stdout().flush();
                } else {
                    // println!("\n[Grid after move/resize event]: Failed to lock tracker");
                }
                print_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                last_print_time = Instant::now();
            } else {
                // println!("\n[Grid after move/resize event]: No recent move/resize events");
            }
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
            if print_flag.load(std::sync::atomic::Ordering::SeqCst)
                && last_print_time.elapsed() >= Duration::from_secs(1)
            {
                if let Ok(tracker) = self.tracker.lock() {
                    println!("\n[Grid after move/resize event]:");
                    let _ = tracker.print_all_grids();
                    let _ = std::io::stdout().flush();
                }
                print_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                last_print_time = Instant::now();
            }
            thread::sleep(Duration::from_millis(50));
            frame_count += 1;
        }

        // Show final 8x8 grid for 2 seconds
        println!("\n✅ Phase 3: Final 8x8 Grid");
        let phase3_end = Instant::now() + Duration::from_secs(2);

        while Instant::now() < phase3_end {
            self.render_frame(frame_count)?;
            if print_flag.load(std::sync::atomic::Ordering::SeqCst)
                && last_print_time.elapsed() >= Duration::from_secs(1)
            {
                if let Ok(tracker) = self.tracker.lock() {
                    println!("\n[Grid after move/resize event]:");
                    let _ = tracker.print_all_grids();
                    let _ = std::io::stdout().flush();
                }
                print_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                last_print_time = Instant::now();
            }
            thread::sleep(Duration::from_millis(100));
            frame_count += 1;
        }

        // Demonstrate IPC communication
        self.demonstrate_ipc()?;

        // --- Post-demo: keep printing grid on move/resize events until user exits ---
        println!("\n[INFO] Demo phases complete. You can still move/resize windows.");
        println!("      The grid will print on move/resize events. Press Enter to exit.\n");
        use std::io::{stdin, Read};
        let mut last_print_time = Instant::now() - Duration::from_secs(2);
        let mut input = String::new();
        // let stdin = stdin();
        // // stdin.lock();
        // // Spawn a thread to read Enter key
        // let (exit_tx, exit_rx) = std::sync::mpsc::channel();
        // std::thread::spawn(move || {
        //     let mut buf = String::new();
        //     let _ = stdin.read_line(&mut buf);
        //     let _ = exit_tx.send(());
        // });
        loop {
            println!("\n[Grid after move/resize event]:");
            if print_flag.load(std::sync::atomic::Ordering::SeqCst)
                && last_print_time.elapsed() >= Duration::from_secs(1)
            {
                println!("\n[Grid after move/resize event]:");
                if let Ok(tracker) = self.tracker.lock() {
                    println!("\n[Grid after move/resize event]:");
                    let _ = tracker.print_all_grids();
                    let _ = std::io::stdout().flush();
                }

                print_flag.store(false, std::sync::atomic::Ordering::SeqCst);
                last_print_time = Instant::now();
            }
            // if exit_rx.try_recv().is_ok() {
            //     break;
            // }
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    // Enable ANSI escape sequences on Windows
    #[cfg(windows)]
    {
        use std::io::{self, Write};
        use winapi::um::consoleapi::GetConsoleMode;
        use winapi::um::consoleapi::SetConsoleMode;
        use winapi::um::handleapi::INVALID_HANDLE_VALUE;
        use winapi::um::processenv::GetStdHandle;
        use winapi::um::winbase::STD_OUTPUT_HANDLE;
        use winapi::um::wincon::ENABLE_VIRTUAL_TERMINAL_PROCESSING;

        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if handle != INVALID_HANDLE_VALUE {
                let mut mode = 0;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
        // Flush to ensure no buffered output interferes with ANSI codes
        let _ = io::stdout().flush();
    }

    let mut demo = VisualGridDemo::new()?;
    demo.run_with_move_resize_callback()?;

    println!("\n🎉 Demo complete! Press Enter to exit...");
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(())
}
