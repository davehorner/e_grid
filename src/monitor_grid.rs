use dashmap::DashMap;
use winapi::shared::windef::RECT;

use crate::{
    grid::{GridConfig, WindowInfo},
    util::meets_coverage_threshold,
    CellState,
};

#[derive(Clone, Debug, Copy)]
pub struct MonitorRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl MonitorRect {
    pub fn from_rect(rect: RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }

    pub fn to_rect(&self) -> RECT {
        RECT {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MonitorGrid {
    pub monitor_id: usize,
    pub monitor_rect: MonitorRect,
    pub config: GridConfig,
    pub grid: Vec<Vec<CellState>>,
}

impl MonitorGrid {
    pub fn new(monitor_id: usize, monitor_rect: RECT) -> Self {
        Self::new_with_config(monitor_id, monitor_rect, GridConfig::default())
    }
    /// Updates the grid for this monitor based on the provided windows.
    pub fn update_grid_for_monitor(
        &mut self,
        windows: &dashmap::DashMap<u64, crate::window::info::WindowInfo>,
    ) {
        // Clear the grid
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                self.grid[row][col] = crate::CellState::Empty;
            }
        }
        // Place windows on the grid
        for entry in windows.iter() {
            let window_info = entry.value();
            let rect = window_info.window_rect.0;
            let cells = self.window_to_grid_cells(&rect);
            for (row, col) in cells {
                if row < self.config.rows && col < self.config.cols {
                    self.grid[row][col] = crate::CellState::Occupied(window_info.hwnd);
                }
            }
        }
    }
    pub fn new_with_config(monitor_id: usize, monitor_rect: RECT, config: GridConfig) -> Self {
        let grid = vec![vec![CellState::Empty; config.cols]; config.rows];
        Self {
            monitor_id,
            monitor_rect: MonitorRect::from_rect(monitor_rect),
            config,
            grid,
        }
    }

    pub fn window_to_grid_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();

        // Skip windows with invalid coordinates
        if rect.right < rect.left || rect.bottom < rect.top {
            return cells;
        }

        let left = self.monitor_rect.left;
        let top = self.monitor_rect.top;
        let right = self.monitor_rect.right;
        let bottom = self.monitor_rect.bottom;

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

    pub fn update_grid(&mut self, windows: &DashMap<u64, WindowInfo>) {
        // Reset grid to empty
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                self.grid[row][col] = CellState::Empty;
            }
        }

        // Place windows on the grid
        for entry in windows {
            let (hwnd, window_info) = entry.pair();
            let grid_cells = self.window_to_grid_cells(&window_info.window_rect);
            for (row, col) in grid_cells {
                if row < self.config.rows && col < self.config.cols {
                    self.grid[row][col] = CellState::Occupied(*hwnd);
                }
            }
        }
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
                        // Show last 2 digits of handle for compactness
                        let display_val = (hwnd % 100) as u8;
                        print!("{:2X} ", display_val);
                    }
                    CellState::OffScreen => print!(" - "),
                }
            }
            println!();
        }
    }
}
