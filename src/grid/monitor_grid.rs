use dashmap::DashMap;
use winapi::shared::windef::{HWND, RECT};

use crate::{
    grid::{GridConfig, WindowInfo},
    util::meets_coverage_threshold,
    CellState,
};

#[derive(Clone, Debug)]
pub struct MonitorGrid {
    pub monitor_id: usize,
    pub monitor_rect: (i32, i32, i32, i32), // (left, top, right, bottom)
    pub config: super::GridConfig,
    pub grid: Vec<Vec<CellState>>,
}

impl MonitorGrid {
    pub fn new(monitor_id: usize, monitor_rect: RECT) -> Self {
        Self::new_with_config(monitor_id, monitor_rect, GridConfig::default())
    }

    pub fn new_with_config(monitor_id: usize, monitor_rect: RECT, config: GridConfig) -> Self {
        let grid = vec![vec![CellState::Empty; config.cols]; config.rows];
        Self {
            monitor_id,
            monitor_rect: (
                monitor_rect.left,
                monitor_rect.top,
                monitor_rect.right,
                monitor_rect.bottom,
            ),
            config,
            grid,
        }
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates
        if rect.left < -30000
            || rect.top < -30000
            || rect.right < rect.left
            || rect.bottom < rect.top
        {
            return cells;
        }

        let (left, top, right, bottom) = self.monitor_rect;

        // Check if window intersects with this monitor
        if rect.right <= left || rect.left >= right || rect.bottom <= top || rect.top >= bottom {
            return cells; // Window is not on this monitor
        }

        let cell_width = (right - left) / self.config.cols as i32;
        let cell_height = (bottom - top) / self.config.rows as i32;

        if cell_width <= 0 || cell_height <= 0 {
            return cells;
        }

        // Calculate potential range of cells that might be affected
        let start_col = ((rect.left.max(left) - left) / cell_width).max(0) as usize;
        let end_col =
            ((rect.right.min(right) - left) / cell_width).min(self.config.cols as i32 - 1) as usize;
        let start_row = ((rect.top.max(top) - top) / cell_height).max(0) as usize;
        let end_row = ((rect.bottom.min(bottom) - top) / cell_height)
            .min(self.config.rows as i32 - 1) as usize;

        // Check coverage for each potentially affected cell
        for row in start_row..=end_row {
            for col in start_col..=end_col {
                if row < self.config.rows && col < self.config.cols {
                    // Calculate the exact bounds of this grid cell
                    let cell_rect = RECT {
                        left: left + (col as i32 * cell_width),
                        top: top + (row as i32 * cell_height),
                        right: left + ((col + 1) as i32 * cell_width),
                        bottom: top + ((row + 1) as i32 * cell_height),
                    };

                    // Only include cell if window meets coverage threshold
                    if meets_coverage_threshold(rect, &cell_rect) {
                        cells.push((row, col));
                    }
                }
            }
        }

        cells
    }

    pub fn update_grid(&mut self, windows: &DashMap<HWND, WindowInfo>) {
        // Reset grid to empty
        println!("[DEBUG] Resetting grid to empty for monitor_id {}", self.monitor_id);
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
            self.grid[row][col] = CellState::Empty;
            }
        }

        // Place windows on the grid
        println!("[DEBUG] Placing windows on the grid for monitor_id {}", self.monitor_id);
        for entry in windows {
            let (hwnd, window_info) = entry.pair();
            println!(
            "[DEBUG] Processing window HWND={:?}, rect=({},{},{},{})",
            hwnd,
            window_info.window_rect.left,
            window_info.window_rect.top,
            window_info.window_rect.right,
            window_info.window_rect.bottom
            );
            let grid_cells = self.window_to_grid_cells(&window_info.window_rect);
            println!(
            "[DEBUG] Window HWND={:?} covers grid cells: {:?}",
            hwnd, grid_cells
            );
            for (row, col) in grid_cells.iter().cloned() {
            if row < self.config.rows && col < self.config.cols {
                println!(
                "[DEBUG] Marking cell ({},{}) as Occupied by HWND={:?}",
                row, col, hwnd
                );
                self.grid[row][col] = CellState::Occupied(*hwnd as u64);
            } else {
                println!(
                "[DEBUG] Skipping out-of-bounds cell ({},{}) for HWND={:?}",
                row, col, hwnd
                );
            }
            }
        }
        println!("[DEBUG] Grid update complete for monitor_id {}", self.monitor_id);
    }

    pub fn print_grid(&self) {
        // Column headers
        print!("    ");
        for col in 0..self.config.cols {
            print!(" {:2}", col);
        }
        println!();

        for row in 0..self.config.rows {
            print!("{:2}: ", row);
            for col in 0..self.config.cols {
                match self.grid[row][col] {
                    CellState::Empty => print!(" . "),
                    CellState::Occupied(hwnd) => {
                        // Show last 2 digits of HWND for compactness
                        let hwnd_u64 = hwnd as u64;
                        let display_val = (hwnd_u64 % 100) as u8;
                        print!("{:2X} ", display_val);
                    }
                    CellState::OffScreen => print!(" - "),
                }
            }
            println!();
        }
    }
}
