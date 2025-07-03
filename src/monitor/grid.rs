use crate::config::GridConfig;
use crate::grid::traits::{GridResult, GridTrait};
use crate::ipc_client::GridCell;
use crate::window::WindowInfo;
use std::collections::HashMap;
use std::sync::Arc;
use crossbeam_utils::atomic::AtomicCell;
use winapi::shared::windef::{HWND, RECT};

/// Monitor-specific grid that tracks windows per monitor
// #[derive(Debug, Clone)]
// pub struct MonitorGrid {
//     pub monitor_id: usize,
//     pub config: GridConfig,
//     pub windows: HashMap<u64, WindowInfo>,
//     pub cells: Vec<Vec<Option<crate::CellState>>>,
//     pub monitor_bounds: crate::window::info::RectWrapper,
// }
#[derive(Clone, Debug)]
pub struct MonitorGridInfo {
    pub monitor_id: u32,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub rows: usize,
    pub cols: usize,
    pub grid: Arc<Vec<AtomicCell<GridCell>>>,
}
// impl MonitorGrid {
//     pub fn new(monitor_id: usize, config: GridConfig) -> Self {
//         let cells = vec![vec![None; config.cols]; config.rows];
//         Self {
//             monitor_id,
//             config,
//             windows: HashMap::new(),
//             cells,
//             monitor_bounds: crate::window::info::RectWrapper::default(), // Default rect, should be updated with real values
//         }
//     }

//     pub fn update_config(&mut self, new_config: GridConfig) {
//         self.config = new_config;
//         self.cells = vec![vec![None; self.config.cols]; self.config.rows];
//         // Re-place all windows based on new grid configuration
//         self.rebuild_grid();
//     }

//     fn rebuild_grid(&mut self) {
//         // Clear current grid
//         for row in &mut self.cells {
//             for cell in row {
//                 *cell = None;
//             }
//         }

//         // Re-place all windows
//         let windows: Vec<_> = self.windows.keys().copied().collect();
//         for hwnd in windows {
//             if let Some(window_info) = self.windows.get(&hwnd) {
//                 if let Some((row, col)) = self.calculate_grid_position(&window_info.rect) {
//                     if row < self.config.rows && col < self.config.cols {
//                         self.cells[row][col] = Some(crate::CellState::Occupied(hwnd));
//                     }
//                 }
//             }
//         }
//     }

//     fn calculate_grid_position(&self, rect: &RECT) -> Option<(usize, usize)> {
//         // This is a simplified calculation - in a real implementation,
//         // you'd need monitor bounds and proper coordinate mapping
//         let grid_width = 1920; // Placeholder - should be monitor width
//         let grid_height = 1080; // Placeholder - should be monitor height

//         let cell_width = grid_width / self.config.cols as i32;
//         let cell_height = grid_height / self.config.rows as i32;

//         let col = ((rect.left + (rect.right - rect.left) / 2) / cell_width) as usize;
//         let row = ((rect.top + (rect.bottom - rect.top) / 2) / cell_height) as usize;

//         if row < self.config.rows && col < self.config.cols {
//             Some((row, col))
//         } else {
//             None
//         }
//     }

//     /// Update monitor grid from window information
//     pub fn update_from_windows(&mut self, windows: &dashmap::DashMap<u64, WindowInfo>) {
//         // Clear current grid state
//         self.clear();

//         // Re-add all windows that belong to this monitor
//         for entry in windows.iter() {
//             let (hwnd, window_info) = (entry.key(), entry.value());
//             // Check if monitor_id is present in monitor_cells (assuming it's a 2D array)
//             // You may need to adjust this logic based on the actual structure and meaning of monitor_cells.
//             if (self.monitor_id as usize) < window_info.monitor_cells.len() {
//                 self.windows.insert(*hwnd, window_info.clone());

//                 // Position window in grid based on its rectangle
//                 if let Some((row, col)) = self.calculate_grid_position(&window_info.rect) {
//                     let _ = self.assign_window(*hwnd, row, col);
//                 }
//             }
//         }
//     }

//     /// Print grid visualization (debugging)
//     pub fn print_grid(&self) {
//         println!(
//             "Monitor {} Grid ({}x{}):",
//             self.monitor_id, self.config.rows, self.config.cols
//         );
//         for row in &self.cells {
//             let mut line = String::new();
//             for cell in row {
//                 match cell {
//                     Some(crate::CellState::Occupied(hwnd)) => line.push_str(&format!("{:04X} ", hwnd & 0xFFFF)),
//                     Some(crate::CellState::Empty) => line.push_str(" .. "),
//                     Some(crate::CellState::OffScreen) => line.push_str(" XX "),
//                     None => line.push_str(" -- "),
//                 }
//             }
//             println!("{}", line);
//         }
//     }

//     /// Get monitor bounds
//     pub fn monitor_bounds(&self) -> (i32, i32, i32, i32) {
//         (self.monitor_bounds.left,
//         self.monitor_bounds.top,
//         self.monitor_bounds.right,
//         self.monitor_bounds.bottom)
//     }

//     /// Set monitor bounds
//     pub fn set_monitor_bounds(&mut self, bounds: (i32, i32, i32, i32)) {
//         self.monitor_bounds = crate::window::info::RectWrapper::from_bounds(
//             bounds.0,
//             bounds.1,
//             bounds.2,
//             bounds.3,
//         );
//     }

//     /// Calculate grid cells for a window rectangle (compatibility method)
//     pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
//         let left = self.monitor_bounds.left;
//         let top = self.monitor_bounds.top;
//         let right = self.monitor_bounds.right;
//         let bottom = self.monitor_bounds.bottom;
//         let monitor_width = right - left;
//         let monitor_height = bottom - top;

//         if monitor_width <= 0 || monitor_height <= 0 {
//             return Vec::new();
//         }

//         let cell_width = monitor_width / self.config.cols as i32;
//         let cell_height = monitor_height / self.config.rows as i32;

//         let mut cells = Vec::new();

//         // Calculate which cells the window overlaps
//         let start_col = ((rect.left - left) / cell_width).max(0) as usize;
//         let end_col =
//             (((rect.right - left) / cell_width) + 1).min(self.config.cols as i32) as usize;
//         let start_row = ((rect.top - top) / cell_height).max(0) as usize;
//         let end_row =
//             (((rect.bottom - top) / cell_height) + 1).min(self.config.rows as i32) as usize;

//         for row in start_row..end_row {
//             for col in start_col..end_col {
//                 if row < self.config.rows && col < self.config.cols {
//                     cells.push((row, col));
//                 }
//             }
//         }

//         cells
//     }
// }

// impl GridTrait for MonitorGrid {
//     fn config(&self) -> &GridConfig {
//         &self.config
//     }

//     fn update(&mut self) -> GridResult<()> {
//         self.rebuild_grid();
//         Ok(())
//     }

//     fn clear(&mut self) {
//         for row in &mut self.cells {
//             for cell in row {
//                 *cell = None;
//             }
//         }
//         self.windows.clear();
//     }

//     fn occupied_cells(&self) -> usize {
//         self.cells
//             .iter()
//             .flat_map(|row| row.iter())
//             .filter(|cell| cell.is_some())
//             .count()
//     }

//     fn is_cell_occupied(&self, row: usize, col: usize) -> GridResult<bool> {
//         self.validate_coordinates(row, col)?;
//         Ok(self.cells[row][col].is_some())
//     }

//     fn get_cell_windows(&self, row: usize, col: usize) -> GridResult<Vec<u64>> {
//         self.validate_coordinates(row, col)?;
//         let mut result = Vec::new();
//         if let Some(cell_state) = &self.cells[row][col] {
//             if let crate::CellState::Occupied(hwnd) = cell_state {
//                 result.push(*hwnd);
//             }
//         }
//         Ok(result)
//     }

//     fn assign_window(&mut self, hwnd: u64, row: usize, col: usize) -> GridResult<()> {
//         self.validate_coordinates(row, col)?;
//         if self.windows.contains_key(&hwnd) {
//             self.remove_window(hwnd)?;
//         }
//         self.cells[row][col] = Some(crate::CellState::Occupied(hwnd));
//         Ok(())
//     }

//     fn remove_window(&mut self, hwnd: u64) -> GridResult<()> {
//         self.windows.remove(&hwnd);
//         for row in &mut self.cells {
//             for cell in row {
//                 if *cell == Some(crate::CellState::Occupied(hwnd)) {
//                     *cell = None;
//                 }
//             }
//         }
//         Ok(())
//     }

//     fn get_all_windows(&self) -> Vec<u64> {
//         self.windows.keys().copied().collect()
//     }
// }
