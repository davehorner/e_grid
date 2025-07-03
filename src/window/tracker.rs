// // Window tracker - moved from lib.rs
// // This maintains the core window tracking functionality

// use dashmap::DashMap;
// use std::ptr;
// use std::sync::atomic::AtomicUsize;
// use std::sync::Arc;
// use winapi::shared::minwindef::LPARAM;
// use winapi::shared::windef::{HWND, RECT};
// use winapi::um::winuser::*;

// use crate::config::GridConfig;
// use crate::grid::animation::AnimationGrid;
// use crate::window::{WindowAnimation, WindowInfo};
// use crate::CellState;

// // Coverage threshold: percentage of cell area that must be covered by window
// const COVERAGE_THRESHOLD: f32 = 0.03; // 30% coverage required

// // Virtual monitor ID - always outside the range of physical monitors
// const VIRTUAL_MONITOR_ID: usize = 99;

// static WINDOW_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

// pub struct WindowTracker {
//     pub config: GridConfig, // Dynamic grid configuration
//     pub grid: Vec<Vec<CellState>>,
//     pub windows: DashMap<u64, WindowInfo>,
//     pub monitors: Arc<DashMap<u32, crate::monitor::grid::MonitorGridInfo>>,
//     pub monitor_rect: RECT, // Combined bounds of all monitors
//     pub active_animations: DashMap<HWND, WindowAnimation>,
//     pub next_window_id: AtomicUsize,
//     pub animation_grid: Option<AnimationGrid>, // Animation system integration
// }

// // #[derive(Debug, Clone, PartialEq)]
// // pub enum CellState {
// //     Empty,          // No window (on-screen area)
// //     Occupied(HWND), // Window present
// //     OffScreen,      // Off-screen area (outside actual monitor bounds)
// // }

// impl WindowTracker {
//     pub fn new() -> Self {
//         Self::new_with_config(GridConfig::default())
//     }

//     pub fn new_with_config(config: GridConfig) -> Self {
//         let grid = vec![vec![CellState::Empty; config.cols]; config.rows];

//         let mut tracker = Self {
//             config,
//             grid,
//             windows: DashMap::new(),
//             monitors: Arc::new(DashMap::new()),
//             monitor_rect: RECT {
//                 left: 0,
//                 top: 0,
//                 right: 1920,
//                 bottom: 1080,
//             },
//             active_animations: DashMap::new(),
//             next_window_id: AtomicUsize::new(1),
//             animation_grid: None,
//         };

//         tracker.initialize_monitors();
//         tracker.initialize_animation_grid();
//         tracker
//     }
//     pub fn get_window_title(hwnd: u64) -> String {
//         unsafe {
//             let mut buffer = [0u16; 256];
//             let len = GetWindowTextW(hwnd as HWND, buffer.as_mut_ptr(), buffer.len() as i32);
//             if len > 0 {
//                 String::from_utf16_lossy(&buffer[..len as usize])
//                     .chars()
//                     .take(50)
//                     .collect()
//             } else {
//                 String::new()
//             }
//         }
//     }
//     /// Initialize monitor detection and setup monitor grids
//     fn initialize_monitors(&mut self) {
//         self.monitors.clear();

//         // Get monitor information
//         let monitors = Self::get_monitors();
//         if monitors.is_empty() {
//             // Fallback to default monitor
//             let default_monitor_info = crate::monitor::grid::MonitorGridInfo {
//                 monitor_id: 0,
//                 width: 1920,
//                 height: 1080,
//                 x: 0,
//                 y: 0,
//                 rows: self.config.rows,
//                 cols: self.config.cols,
//                 grid: Arc::new((0..self.config.rows * self.config.cols).map(|_| {
//                     crossbeam_utils::atomic::AtomicCell::new(crate::ipc_client::GridCell {
//                         state: crate::ipc_client::ClientCellState::Empty,
//                         monitor_ids: [0; 4],
//                         monitor_count: 0,
//                     })
//                 }).collect()),
//             };
//             self.monitors.insert(0, default_monitor_info);
//             self.monitor_rect = RECT {
//                 left: 0,
//                 top: 0,
//                 right: 1920,
//                 bottom: 1080,
//             };
//         } else {
//             // Calculate combined monitor bounds
//             let mut combined_left = i32::MAX;
//             let mut combined_top = i32::MAX;
//             let mut combined_right = i32::MIN;
//             let mut combined_bottom = i32::MIN;

//             for (i, rect) in monitors.iter().enumerate() {
//                 let monitor_info = crate::monitor::grid::MonitorGridInfo {
//                     monitor_id: i as u32,
//                     width: rect.right - rect.left,
//                     height: rect.bottom - rect.top,
//                     x: rect.left,
//                     y: rect.top,
//                     rows: self.config.rows,
//                     cols: self.config.cols,
//                     grid: Arc::new((0..self.config.rows * self.config.cols).map(|_| {
//                         crossbeam_utils::atomic::AtomicCell::new(crate::ipc_client::GridCell {
//                             state: crate::ipc_client::ClientCellState::Empty,
//                             monitor_ids: [i as u32; 4],
//                             monitor_count: 1,
//                         })
//                     }).collect()),
//                 };
//                 self.monitors.insert(i as u32, monitor_info);

//                 combined_left = combined_left.min(rect.left);
//                 combined_top = combined_top.min(rect.top);
//                 combined_right = combined_right.max(rect.right);
//                 combined_bottom = combined_bottom.max(rect.bottom);
//             }

//             self.monitor_rect = RECT {
//                 left: combined_left,
//                 top: combined_top,
//                 right: combined_right,
//                 bottom: combined_bottom,
//             };
//         }

//         println!("📺 Initialized {} monitors", self.monitors.len());
//         println!(
//             "   Virtual screen: ({}, {}) to ({}, {})",
//             self.monitor_rect.left,
//             self.monitor_rect.top,
//             self.monitor_rect.right,
//             self.monitor_rect.bottom
//         );
//     }
//     pub fn is_manageable_window(hwnd: u64) -> bool {
//         unsafe {
//             let title = Self::get_window_title(hwnd);
//             if IsWindow(hwnd as HWND) == 0 {
//                 // println!("[DEBUG] Skipping hwnd=0x{:X}: not a valid window", hwnd);
//                 return false;
//             }
//             if IsWindowVisible(hwnd as HWND) == 0 {
//                 // println!("[DEBUG] Skipping hwnd=0x{:X}: not visible", hwnd);
//                 return false;
//             }
//             // Skip minimized windows
//             if IsIconic(hwnd as HWND) != 0 {
//                 // println!("[DEBUG] Skipping hwnd=0x{:X}: minimized", hwnd);
//                 return false;
//             }
//             let ex_style = GetWindowLongW(hwnd as HWND, GWL_EXSTYLE) as u32;
//             if (ex_style & WS_EX_TOOLWINDOW) != 0 {
//                 // println!("[DEBUG] Skipping hwnd=0x{:X}: toolwindow", hwnd);
//                 return false;
//             }
//             // println!("[DEBUG] Accepting hwnd=0x{:X}: '{}'", hwnd, title);
//             true
//         }
//     }
//     /// Initialize the animation grid system
//     fn initialize_animation_grid(&mut self) {
//         let monitor_bounds = (
//             self.monitor_rect.left,
//             self.monitor_rect.top,
//             self.monitor_rect.right,
//             self.monitor_rect.bottom,
//         );

//         self.animation_grid = Some(AnimationGrid::new(self.config.clone(), monitor_bounds));
//         println!("🎬 Animation grid initialized");
//     }

//     /// Get all monitor rectangles
//     fn get_monitors() -> Vec<RECT> {
//         let mut monitors = Vec::new();

//         unsafe {
//             EnumDisplayMonitors(
//                 ptr::null_mut(),
//                 ptr::null_mut(),
//                 Some(Self::monitor_enum_proc),
//                 &mut monitors as *mut Vec<RECT> as LPARAM,
//             );
//         }

//         monitors
//     }

//     /// Callback for monitor enumeration
//     unsafe extern "system" fn monitor_enum_proc(
//         _hmonitor: winapi::shared::windef::HMONITOR,
//         _hdc: winapi::shared::windef::HDC,
//         rect: *mut RECT,
//         lparam: LPARAM,
//     ) -> i32 {
//         let monitors = &mut *(lparam as *mut Vec<RECT>);
//         monitors.push(*rect);
//         1 // Continue enumeration
//     }

//     /// Get window rectangle safely
//     pub fn get_window_rect(hwnd: HWND) -> Option<RECT> {
//         let mut rect = RECT {
//             left: 0,
//             top: 0,
//             right: 0,
//             bottom: 0,
//         };
//         unsafe {
//             if GetWindowRect(hwnd, &mut rect) != 0 {
//                 Some(rect)
//             } else {
//                 None
//             }
//         }
//     }

//     /// Update the grid based on current window positions
//     pub fn update_grid(&mut self) {
//         // Clear the grid
//         for row in &mut self.grid {
//             for cell in row {
//                 *cell = CellState::Empty;
//             }
//         }

//         // Re-populate the grid with current window positions
//         for entry in self.windows.iter() {
//             let (hwnd, window_info) = entry.pair();
//             let cells = self.window_to_grid_cells(&window_info.window_rect);

//             for (row, col) in cells {
//                 if row < self.config.rows && col < self.config.cols {
//                     self.grid[row][col] = CellState::Occupied(*hwnd);
//                 }
//             }
//         }

//         // Mark off-screen areas
//         self.mark_offscreen_areas();
//     }

//     /// Update monitor grids
//     pub fn update_monitor_grids(&mut self) {
//         // Update monitor grids based on current windows
//         for monitor_entry in self.monitors.iter() {
//             let (monitor_id, monitor_info) = monitor_entry.pair();
//             // Clear the grid for this monitor
//             for i in 0..(monitor_info.rows * monitor_info.cols) {
//                 monitor_info.grid[i].store(crate::ipc_client::GridCell {
//                     state: crate::ipc_client::ClientCellState::Empty,
//                     monitor_ids: [*monitor_id; 4],
//                     monitor_count: 1,
//                 });
//             }
            
//             // Re-add windows that belong to this monitor
//             for window_entry in self.windows.iter() {
//                 let (hwnd, window_info) = window_entry.pair();
//                 // Simple monitor assignment based on window center point
//                 let window_center_x = (window_info.window_rect.left + window_info.window_rect.right) / 2;
//                 let window_center_y = (window_info.window_rect.top + window_info.window_rect.bottom) / 2;
                
//                 if window_center_x >= monitor_info.x && 
//                    window_center_x < monitor_info.x + monitor_info.width &&
//                    window_center_y >= monitor_info.y && 
//                    window_center_y < monitor_info.y + monitor_info.height {
                    
//                     // Calculate grid position within this monitor
//                     let relative_x = window_center_x - monitor_info.x;
//                     let relative_y = window_center_y - monitor_info.y;
                    
//                     let col = (relative_x * monitor_info.cols as i32 / monitor_info.width) as usize;
//                     let row = (relative_y * monitor_info.rows as i32 / monitor_info.height) as usize;
                    
//                     if row < monitor_info.rows && col < monitor_info.cols {
//                         let idx = row * monitor_info.cols + col;
//                         monitor_info.grid[idx].store(crate::ipc_client::GridCell {
//                             state: crate::ipc_client::ClientCellState::Occupied(*hwnd),
//                             monitor_ids: [*monitor_id; 4],
//                             monitor_count: 1,
//                         });
//                     }
//                 }
//             }
//         }
//     }

//     /// Calculate which grid cells a window occupies
//     pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
//         let monitor_width = self.monitor_rect.right - self.monitor_rect.left;
//         let monitor_height = self.monitor_rect.bottom - self.monitor_rect.top;

//         let cell_width = monitor_width as f32 / self.config.cols as f32;
//         let cell_height = monitor_height as f32 / self.config.rows as f32;

//         let mut cells = Vec::new();

//         for row in 0..self.config.rows {
//             for col in 0..self.config.cols {
//                 let cell_left = self.monitor_rect.left + (col as f32 * cell_width) as i32;
//                 let cell_top = self.monitor_rect.top + (row as f32 * cell_height) as i32;
//                 let cell_right = cell_left + cell_width as i32;
//                 let cell_bottom = cell_top + cell_height as i32;

//                 // Calculate intersection
//                 let intersect_left = rect.left.max(cell_left);
//                 let intersect_top = rect.top.max(cell_top);
//                 let intersect_right = rect.right.min(cell_right);
//                 let intersect_bottom = rect.bottom.min(cell_bottom);

//                 if intersect_left < intersect_right && intersect_top < intersect_bottom {
//                     let intersect_area =
//                         (intersect_right - intersect_left) * (intersect_bottom - intersect_top);
//                     let cell_area = (cell_width * cell_height) as i32;
//                     let coverage = intersect_area as f32 / cell_area as f32;

//                     if coverage >= COVERAGE_THRESHOLD {
//                         cells.push((row, col));
//                     }
//                 }
//             }
//         }

//         cells
//     }

//     /// Mark off-screen areas in the grid
//     fn mark_offscreen_areas(&mut self) {
//         // This is a simplified implementation
//         // In a full implementation, you'd calculate which cells are outside monitor bounds
//     }

//     /// Add a window to tracking
//     pub fn add_window(&mut self, hwnd: HWND, title: String, rect: RECT) {
//         let window_info = WindowInfo::new(hwnd, title.as_str(), rect);
//         self.windows.insert(hwnd as u64, window_info);
//         self.update_grid();
//         self.update_monitor_grids();
//     }

//     /// Remove a window from tracking
//     pub fn remove_window(&mut self, hwnd: HWND) {
//         self.windows.remove(&(hwnd as u64));
//         self.active_animations.remove(&hwnd);
//         self.update_grid();
//         self.update_monitor_grids();
//     }
//     /// Update a window's position
//     pub fn update_window(&mut self, hwnd: HWND, new_rect: RECT) {
//         // Update window info in a separate scope to avoid borrowing conflicts
//         {
//             if let Some(mut window_info) = self.windows.get_mut(&(hwnd as u64)) {
//                 window_info.update_rect(new_rect);
//             }
//         }

//         // Now update grids without borrowing conflicts
//         self.update_grid();
//         self.update_monitor_grids();
//     }

//     /// Get the number of occupied cells
//     pub fn occupied_cells(&self) -> usize {
//         self.grid
//             .iter()
//             .flat_map(|row| row.iter())
//             .filter(|cell| matches!(cell, CellState::Occupied(_)))
//             .count()
//     }

//     /// Print the current grid state
//     pub fn print_grid(&self) {
//         println!(
//             "=== WINDOW TRACKER GRID ({} x {}) ===",
//             self.config.rows, self.config.cols
//         );

//         // Print column headers
//         print!("    ");
//         for col in 0..self.config.cols {
//             print!(" {:2}", col);
//         }
//         println!();

//         // Print grid rows
//         for (row, grid_row) in self.grid.iter().enumerate() {
//             print!("{:2}: ", row);

//             for cell in grid_row {
//                 match cell {
//                     CellState::Empty => print!(" . "),
//                     CellState::OffScreen => print!(" - "),
//                     CellState::Occupied(hwnd) => {
//                         let display = crate::display::format_hwnd_display(*hwnd as u64);
//                         print!("{:>3}", display);
//                     }
//                 }
//             }
//             println!();
//         }
//         println!();
//     }

//     /// Print all grids (virtual + monitors)
//     pub fn print_all_grids(&self) {
//         // Print virtual grid
//         crate::display::print_virtual_monitor_header(
//             VIRTUAL_MONITOR_ID,
//             (
//                 self.monitor_rect.left,
//                 self.monitor_rect.top,
//                 self.monitor_rect.right,
//                 self.monitor_rect.bottom,
//             ),
//             // Use occupied_cells() for the window count in the grid, not self.windows.len()
//             self.occupied_cells(),
//         );
//         self.print_grid();

//         // Print individual monitor grids
//         for monitor_entry in self.monitors.iter() {
//             let (monitor_id, monitor_info) = monitor_entry.pair();
            
//             // Count occupied cells for this monitor grid
//             let mut occupied_count = 0;
//             for i in 0..(monitor_info.rows * monitor_info.cols) {
//                 let cell = monitor_info.grid[i].load();
//                 if matches!(cell.state, crate::ipc_client::ClientCellState::Occupied(_)) {
//                     occupied_count += 1;
//                 }
//             }
            
//             self.print_monitor_grid_with_window_count(*monitor_id, monitor_info, occupied_count);
//         }
//     }

//     /// List saved layouts (placeholder)
//     pub fn list_saved_layouts(&self) -> Vec<String> {
//         // Placeholder for layout functionality
//         vec!["default".to_string(), "development".to_string()]
//     }

//     /// Calculate monitor cells for a window rectangle
//     pub fn calculate_monitor_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
//         // This is a simplified calculation based on the grid configuration
//         let monitor_width = 1920; // Should be dynamically determined
//         let monitor_height = 1080; // Should be dynamically determined

//         let cell_width = monitor_width / self.config.cols as i32;
//         let cell_height = monitor_height / self.config.rows as i32;

//         let mut cells = Vec::new();

//         // Calculate which cells the window overlaps
//         let start_col = (rect.left / cell_width).max(0) as usize;
//         let end_col = ((rect.right / cell_width) + 1).min(self.config.cols as i32) as usize;
//         let start_row = (rect.top / cell_height).max(0) as usize;
//         let end_row = ((rect.bottom / cell_height) + 1).min(self.config.rows as i32) as usize;

//         for row in start_row..end_row {
//             for col in start_col..end_col {
//                 if row < self.config.rows && col < self.config.cols {
//                     cells.push((row, col));
//                 }
//             }
//         }

//         cells
//     }

//     /// Get a saved layout (placeholder - should integrate with LayoutGrid)
//     pub fn get_saved_layout(&self, _name: &str) -> Option<crate::grid::layout::GridLayout> {
//         // This should integrate with the LayoutGrid implementation
//         // For now, return None to indicate no layout found
//         None
//     }

//     /// Apply a grid layout (placeholder - should integrate with LayoutGrid)
//     pub fn apply_grid_layout(
//         &mut self,
//         _layout: &crate::grid::layout::GridLayout,
//         _duration: std::time::Duration,
//         _easing_type: crate::grid::animation::EasingType,
//     ) -> Result<(), Box<dyn std::error::Error>> {
//         // This should integrate with the LayoutGrid implementation
//         Err("Layout functionality not yet integrated".into())
//     }

//     /// Save current layout (placeholder - should integrate with LayoutGrid)
//     pub fn save_current_layout(&mut self, _name: String) {
//         // This should integrate with the LayoutGrid implementation
//         // For now, this is a no-op
//     }

//     /// Start window animation (placeholder - should integrate with AnimationGrid)
//     pub fn start_window_animation(
//         &mut self,
//         _hwnd: HWND,
//         _target_rect: RECT,
//         _duration: std::time::Duration,
//         _easing_type: crate::grid::animation::EasingType,
//     ) -> Result<(), Box<dyn std::error::Error>> {
//         // This should integrate with the AnimationGrid implementation
//         Err("Animation functionality not yet integrated".into())
//     }

//     /// Update animations (placeholder - should integrate with AnimationGrid)
//     pub fn update_animations(&mut self) -> Vec<u64> {
//         // This should integrate with the AnimationGrid implementation
//         // Return empty vector for now
//         Vec::new()
//     }

//     /// Print monitor grid with window count
//     fn print_monitor_grid_with_window_count(&self, monitor_id: u32, monitor_info: &crate::monitor::grid::MonitorGridInfo, window_count: usize) {
//         println!(
//             "=== MONITOR {} GRID ===",
//             monitor_id
//         );
//         println!(
//             "Monitor bounds: ({}, {}) to ({}, {})",
//             monitor_info.x, monitor_info.y, 
//             monitor_info.x + monitor_info.width, 
//             monitor_info.y + monitor_info.height
//         );
//         println!("Windows on this monitor: {}", window_count);
//         println!(
//             "Grid size: {} rows x {} cols ({} cells)",
//             monitor_info.rows,
//             monitor_info.cols,
//             monitor_info.rows * monitor_info.cols
//         );
//         println!(
//             "Monitor resolution: {}x{} px",
//             monitor_info.width,
//             monitor_info.height
//         );
        
//         // Print column headers
//         print!("    ");
//         for col in 0..monitor_info.cols {
//             print!(" {:2}", col);
//         }
//         println!();
        
//         // Print grid rows
//         for row in 0..monitor_info.rows {
//             print!("{:2}: ", row);
//             for col in 0..monitor_info.cols {
//                 let idx = row * monitor_info.cols + col;
//                 let cell = monitor_info.grid[idx].load();
//                 match cell.state {
//                     crate::ipc_client::ClientCellState::Empty => print!(" .."),
//                     crate::ipc_client::ClientCellState::OffScreen => print!(" --"),
//                     crate::ipc_client::ClientCellState::Occupied(hwnd) => {
//                         let display = crate::display::format_hwnd_display(hwnd);
//                         print!(" {:>2}", display);
//                     }
//                 }
//             }
//             println!();
//         }
//         println!();
//     }
// }

// impl crate::display::CellDisplay for CellState {
//     fn display_cell(&self) -> &str {
//         match self {
//             CellState::Empty => " .",
//             CellState::Occupied(_) => "", // Will use get_hwnd() for display
//             CellState::OffScreen => " -",
//         }
//     }

//     fn get_hwnd(&self) -> Option<u64> {
//         match self {
//             CellState::Occupied(hwnd) => Some(*hwnd as u64),
//             _ => None,
//         }
//     }
// }
