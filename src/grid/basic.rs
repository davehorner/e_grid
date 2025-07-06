// Basic Grid - Standard single-layer grid implementation
// This maintains backwards compatibility with the existing grid system

use crate::config::GridConfig;
use crate::grid::traits::{CellDisplay, GridResult, GridTrait};
use crate::window::WindowInfo;
use std::collections::HashMap;
use winapi::shared::windef::RECT;

#[derive(Debug, Clone, PartialEq)]
pub enum BasicCellState {
    Empty,         // No window (on-screen area)
    Occupied(u64), // Window present
    OffScreen,     // Off-screen area (outside actual monitor bounds)
}

pub struct BasicGrid {
    config: GridConfig,
    grid: Vec<Vec<BasicCellState>>,
    windows: HashMap<u64, WindowInfo>,
    monitor_bounds: (i32, i32, i32, i32), // (left, top, right, bottom)
}

impl BasicGrid {
    pub fn new(config: GridConfig) -> Self {
        let grid = vec![vec![BasicCellState::Empty; config.cols]; config.rows];

        Self {
            config,
            grid,
            windows: HashMap::new(),
            monitor_bounds: (0, 0, 1920, 1080), // Default bounds
        }
    }

    pub fn with_monitor_bounds(config: GridConfig, bounds: (i32, i32, i32, i32)) -> Self {
        let mut grid = Self::new(config);
        grid.monitor_bounds = bounds;
        grid
    }

    pub fn set_monitor_bounds(&mut self, bounds: (i32, i32, i32, i32)) {
        self.monitor_bounds = bounds;
    }

    /// Calculate which cells a window occupies based on its rectangle
    fn window_to_cells(&self, rect: &RECT) -> Vec<(usize, usize)> {
        let (monitor_left, monitor_top, monitor_right, monitor_bottom) = self.monitor_bounds;
        let monitor_width = monitor_right - monitor_left;
        let monitor_height = monitor_bottom - monitor_top;

        let cell_width = monitor_width as f32 / self.config.cols as f32;
        let cell_height = monitor_height as f32 / self.config.rows as f32;

        let mut cells = Vec::new();

        // Calculate coverage threshold (30% of cell must be covered)
        let coverage_threshold = 0.3f32;

        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let cell_left = monitor_left + (col as f32 * cell_width) as i32;
                let cell_top = monitor_top + (row as f32 * cell_height) as i32;
                let cell_right = cell_left + cell_width as i32;
                let cell_bottom = cell_top + cell_height as i32;

                // Calculate intersection
                let intersect_left = rect.left.max(cell_left);
                let intersect_top = rect.top.max(cell_top);
                let intersect_right = rect.right.min(cell_right);
                let intersect_bottom = rect.bottom.min(cell_bottom);

                if intersect_left < intersect_right && intersect_top < intersect_bottom {
                    let intersect_area =
                        (intersect_right - intersect_left) * (intersect_bottom - intersect_top);
                    let cell_area = (cell_width * cell_height) as i32;
                    let coverage = intersect_area as f32 / cell_area as f32;

                    if coverage >= coverage_threshold {
                        cells.push((row, col));
                    }
                }
            }
        }

        cells
    }

    /// Add a window to the grid
    pub fn add_window(&mut self, hwnd: u64, window_info: WindowInfo) -> GridResult<()> {
        self.windows.insert(hwnd, window_info.clone());

        // Calculate which cells this window occupies
        let cells = self.window_to_cells(&window_info.window_rect);

        for (row, col) in cells {
            self.grid[row][col] = BasicCellState::Occupied(hwnd);
        }

        Ok(())
    }

    /// Print the basic grid
    pub fn print_grid(&self) {
        println!(
            "=== BASIC GRID ({} x {}) ===",
            self.config.rows, self.config.cols
        );

        // Print column headers
        print!("    ");
        for col in 0..self.config.cols {
            print!(" {:2}", col);
        }
        println!();

        // Print grid rows
        for (row, grid_row) in self.grid.iter().enumerate() {
            print!("{:2}: ", row);

            for cell in grid_row {
                match cell {
                    BasicCellState::Empty => print!(" . "),
                    BasicCellState::OffScreen => print!(" - "),
                    BasicCellState::Occupied(hwnd) => {
                        print!("{:>3}", hwnd);
                    }
                }
            }
            println!();
        }
        println!();
    }

    // Public getter methods for accessing private fields
    pub fn windows(&self) -> &HashMap<u64, WindowInfo> {
        &self.windows
    }

    pub fn grid(&self) -> &Vec<Vec<BasicCellState>> {
        &self.grid
    }
}

impl GridTrait for BasicGrid {
    fn config(&self) -> &GridConfig {
        &self.config
    }

    fn update(&mut self) -> GridResult<()> {
        // Clear the grid
        self.clear();

        // Re-add all windows with their current positions
        let windows_to_update: Vec<(u64, WindowInfo)> = self
            .windows
            .iter()
            .map(|(&hwnd, info)| (hwnd, info.clone()))
            .collect();

        for (hwnd, window_info) in windows_to_update {
            self.add_window(hwnd, window_info)?;
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.grid = vec![vec![BasicCellState::Empty; self.config.cols]; self.config.rows];
    }

    fn occupied_cells(&self) -> usize {
        self.grid
            .iter()
            .flat_map(|row| row.iter())
            .filter(|cell| matches!(cell, BasicCellState::Occupied(_)))
            .count()
    }

    fn is_cell_occupied(&self, row: usize, col: usize) -> GridResult<bool> {
        self.validate_coordinates(row, col)?;
        Ok(matches!(self.grid[row][col], BasicCellState::Occupied(_)))
    }

    fn get_cell_windows(&self, row: usize, col: usize) -> GridResult<Vec<u64>> {
        self.validate_coordinates(row, col)?;

        match self.grid[row][col] {
            BasicCellState::Occupied(hwnd) => Ok(vec![hwnd]),
            _ => Ok(Vec::new()),
        }
    }

    fn assign_window(&mut self, hwnd: u64, row: usize, col: usize) -> GridResult<()> {
        self.validate_coordinates(row, col)?;

        // Clear the window from its current position
        self.remove_window(hwnd)?;

        // Assign to new position
        self.grid[row][col] = BasicCellState::Occupied(hwnd);

        Ok(())
    }

    fn remove_window(&mut self, hwnd: u64) -> GridResult<()> {
        self.windows.remove(&hwnd);

        // Remove from grid
        for row in &mut self.grid {
            for cell in row {
                if let BasicCellState::Occupied(cell_hwnd) = cell {
                    if *cell_hwnd == hwnd {
                        *cell = BasicCellState::Empty;
                    }
                }
            }
        }
        Ok(())
    }

    fn get_all_windows(&self) -> Vec<u64> {
        self.windows.keys().cloned().collect()
    }
}

impl CellDisplay for BasicCellState {
    fn display_cell(&self) -> &str {
        match self {
            BasicCellState::Empty => " .",
            BasicCellState::Occupied(_) => "", // Will use get_hwnd() for display
            BasicCellState::OffScreen => " -",
        }
    }

    fn get_hwnd(&self) -> Option<u64> {
        match self {
            BasicCellState::Occupied(hwnd) => Some(*hwnd),
            _ => None,
        }
    }
}
