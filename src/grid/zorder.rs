// Z-Order Grid - Multi-layer grid that tracks window stacking and visibility
// This grid type shows overlapping windows and which portions are visible

use crate::config::GridConfig;
use crate::display::format_hwnd_display;
use crate::grid::traits::{
    CellDisplay, GridError, GridResult, GridTrait, ZOrderGrid as ZOrderGridTrait,
};
use crate::window::info::RectWrapper;
use crate::window::WindowInfo;
use crate::window_tracker::WindowTracker;
use std::collections::{BTreeMap, HashMap};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{GetWindow, GetWindowRect, GW_HWNDPREV};

#[derive(Debug, Clone)]
pub enum ZOrderCellState {
    Empty,
    Occupied {
        windows: Vec<ZOrderWindow>, // Ordered from back to front (z-order)
    },
    OffScreen,
}

#[derive(Debug, Clone)]
pub struct ZOrderWindow {
    pub hwnd: u64,
    pub z_index: usize,             // 0 = back, higher = front
    pub coverage: f32,              // How much of the cell this window covers (0.0-1.0)
    pub is_visible: bool,           // Whether this window portion is visible (not obscured)
    pub visibility_percentage: f32, // Percentage of window that's visible in this cell
}

pub struct ZOrderGrid {
    config: GridConfig,
    grid: Vec<Vec<ZOrderCellState>>,
    windows: HashMap<u64, WindowInfo>,
    z_order_map: BTreeMap<usize, u64>, // z_index -> u64
    next_z_index: usize,
    window_tracker: Option<WindowTracker>, // Optional reference to main tracker
}

impl ZOrderGrid {
    pub fn new(config: GridConfig) -> Self {
        let grid = vec![vec![ZOrderCellState::Empty; config.cols]; config.rows];

        Self {
            config,
            grid,
            windows: HashMap::new(),
            z_order_map: BTreeMap::new(),
            next_z_index: 0,
            window_tracker: None,
        }
    }

    pub fn with_tracker(config: GridConfig, tracker: WindowTracker) -> Self {
        let mut grid = Self::new(config);
        grid.window_tracker = Some(tracker);
        grid
    }

    /// Calculate which cells a window rect occupies
    fn window_to_cells(
        &self,
        rect: &RECT,
        monitor_bounds: (i32, i32, i32, i32),
    ) -> Vec<(usize, usize, f32)> {
        let (monitor_left, monitor_top, monitor_right, monitor_bottom) = monitor_bounds;
        let monitor_width = monitor_right - monitor_left;
        let monitor_height = monitor_bottom - monitor_top;

        let cell_width = monitor_width as f32 / self.config.cols as f32;
        let cell_height = monitor_height as f32 / self.config.rows as f32;

        let mut cells = Vec::new();

        // Calculate which cells this window intersects
        let window_left = rect.left - monitor_left;
        let window_top = rect.top - monitor_top;
        let window_right = rect.right - monitor_left;
        let window_bottom = rect.bottom - monitor_top;

        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                let cell_left = col as f32 * cell_width;
                let cell_top = row as f32 * cell_height;
                let cell_right = cell_left + cell_width;
                let cell_bottom = cell_top + cell_height;

                // Calculate intersection
                let intersect_left = window_left.max(cell_left as i32) as f32;
                let intersect_top = window_top.max(cell_top as i32) as f32;
                let intersect_right = window_right.min(cell_right as i32) as f32;
                let intersect_bottom = window_bottom.min(cell_bottom as i32) as f32;

                if intersect_left < intersect_right && intersect_top < intersect_bottom {
                    let intersect_area =
                        (intersect_right - intersect_left) * (intersect_bottom - intersect_top);
                    let cell_area = cell_width * cell_height;
                    let coverage = intersect_area / cell_area;

                    if coverage > 0.1 {
                        // Only include cells with significant coverage
                        cells.push((row, col, coverage));
                    }
                }
            }
        }

        cells
    }

    /// Get the current z-order from Windows
    fn get_windows_z_order(&self) -> Vec<u64> {
        let mut z_order = Vec::new();
        let mut current_hwnd = unsafe { GetWindow(std::ptr::null_mut(), GW_HWNDPREV) };

        // Walk through windows from top to bottom
        while !current_hwnd.is_null() {
            let current_u64 = current_hwnd as u64;
            if self.windows.contains_key(&current_u64) {
                z_order.push(current_u64);
            }
            current_hwnd = unsafe { GetWindow(current_hwnd, GW_HWNDPREV) };
        }

        z_order
    }

    /// Calculate visibility for each window in each cell
    fn calculate_visibility(&mut self) {
        // Reset all visibility
        for row in &mut self.grid {
            for cell in row {
                if let ZOrderCellState::Occupied { windows } = cell {
                    for window in windows {
                        window.is_visible = false;
                        window.visibility_percentage = 0.0;
                    }
                }
            }
        }

        // Get current z-order
        let z_order = self.get_windows_z_order();

        // For each cell, calculate visibility from front to back
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if let ZOrderCellState::Occupied { windows } = &mut self.grid[row][col] {
                    let mut remaining_area = 1.0f32; // Start with full cell area available

                    // Sort windows by z-order (front to back)
                    windows.sort_by_key(|w| {
                        z_order
                            .iter()
                            .position(|&hwnd| hwnd == w.hwnd)
                            .unwrap_or(usize::MAX)
                    });

                    // Calculate visibility for each window
                    for window in windows {
                        if remaining_area > 0.0 {
                            let visible_area = window.coverage.min(remaining_area);
                            window.visibility_percentage = visible_area / window.coverage;
                            window.is_visible = visible_area > 0.01; // Visible if more than 1%
                            remaining_area -= visible_area;
                        }
                    }
                }
            }
        }
    }

    /// Add a window to the grid at its current position
    pub fn add_window(
        &mut self,
        hwnd: u64,
        window_info: WindowInfo,
        monitor_bounds: (i32, i32, i32, i32),
    ) -> GridResult<()> {
        self.windows.insert(hwnd, window_info.clone());

        // Assign z-index
        let z_index = self.next_z_index;
        self.z_order_map.insert(z_index, hwnd);
        self.next_z_index += 1;

        // Calculate which cells this window occupies
        let cells = self.window_to_cells(&window_info.window_rect, monitor_bounds);

        for (row, col, coverage) in cells {
            self.validate_coordinates(row, col)?;

            let cell = &mut self.grid[row][col];
            match cell {
                ZOrderCellState::Empty => {
                    *cell = ZOrderCellState::Occupied {
                        windows: vec![ZOrderWindow {
                            hwnd,
                            z_index,
                            coverage,
                            is_visible: true, // Will be recalculated
                            visibility_percentage: 1.0,
                        }],
                    };
                }
                ZOrderCellState::Occupied { windows } => {
                    windows.push(ZOrderWindow {
                        hwnd,
                        z_index,
                        coverage,
                        is_visible: true, // Will be recalculated
                        visibility_percentage: 1.0,
                    });
                }
                ZOrderCellState::OffScreen => {
                    // Window in off-screen area
                }
            }
        }

        // Recalculate visibility for all cells
        self.calculate_visibility();

        Ok(())
    }

    /// Print the grid with z-order information
    pub fn print_zorder_grid(&self) {
        println!("=== Z-ORDER GRID ===");
        println!("Shows window stacking and visibility (front windows first)");

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
                    ZOrderCellState::Empty => print!(" . "),
                    ZOrderCellState::OffScreen => print!(" - "),
                    ZOrderCellState::Occupied { windows } => {
                        if windows.is_empty() {
                            print!(" . ");
                        } else {
                            // Show the topmost visible window
                            if let Some(top_window) = windows
                                .iter()
                                .filter(|w| w.is_visible)
                                .max_by_key(|w| w.z_index)
                            {
                                let display = format_hwnd_display(top_window.hwnd as u64);
                                print!("{:>3}", display);
                            } else {
                                // All windows are hidden
                                print!(" H ");
                            }
                        }
                    }
                }
            }
            println!();
        }

        println!();
        self.print_zorder_legend();
    }

    /// Print detailed z-order information for each cell
    pub fn print_detailed_zorder(&self) {
        println!("=== DETAILED Z-ORDER INFORMATION ===");

        for (row, grid_row) in self.grid.iter().enumerate() {
            for (col, cell) in grid_row.iter().enumerate() {
                if let ZOrderCellState::Occupied { windows } = cell {
                    if !windows.is_empty() {
                        println!("Cell ({}, {}):", row, col);

                        let mut sorted_windows = windows.clone();
                        sorted_windows.sort_by_key(|w| w.z_index);

                        for (i, window) in sorted_windows.iter().enumerate() {
                            let title = self
                                .windows
                                .get(&window.hwnd)
                                .map(|w| {
                                    let nul_pos = w
                                        .title
                                        .iter()
                                        .position(|&c| c == 0)
                                        .unwrap_or(w.title.len());
                                    String::from_utf16_lossy(&w.title[..nul_pos])
                                })
                                .unwrap_or_else(|| "Unknown".to_string());

                            println!(
                                "  Layer {}: HWND {:?} - {} (Coverage: {:.1}%, Visible: {:.1}%) {}",
                                i,
                                window.hwnd,
                                if title.len() > 20 {
                                    &title[..20]
                                } else {
                                    &title
                                },
                                window.coverage * 100.0,
                                window.visibility_percentage * 100.0,
                                if window.is_visible { "👁" } else { "🚫" }
                            );
                        }
                        println!();
                    }
                }
            }
        }
    }

    fn print_zorder_legend(&self) {
        println!("Legend:");
        println!("  XX = Window handle (last 2 hex digits)");
        println!("  .  = Empty cell");
        println!("  -  = Off-screen area");
        println!("  H  = Hidden (all windows obscured)");
        println!("  👁  = Visible window portion");
        println!("  🚫 = Hidden window portion");
    }
}

impl GridTrait for ZOrderGrid {
    fn config(&self) -> &GridConfig {
        &self.config
    }

    fn update(&mut self) -> GridResult<()> {
        // Clear current grid
        self.clear();

        // Re-add all windows (this will recalculate positions and z-order)
        let windows_to_update: Vec<(u64, WindowInfo)> = self
            .windows
            .iter()
            .map(|(&hwnd, info)| (hwnd, info.clone()))
            .collect();

        for (hwnd, window_info) in windows_to_update {
            // Get current window rectangle
            let mut current_rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if unsafe { GetWindowRect(hwnd as HWND, &mut current_rect) } != 0 {
                let updated_info = WindowInfo {
                    window_rect: RectWrapper(current_rect),
                    ..window_info
                };

                // Use a default monitor bounds - this should be provided by the caller
                let monitor_bounds = (0, 0, 1920, 1080); // TODO: Get actual monitor bounds
                self.add_window(hwnd, updated_info, monitor_bounds)?;
            }
        }

        Ok(())
    }

    fn clear(&mut self) {
        self.grid = vec![vec![ZOrderCellState::Empty; self.config.cols]; self.config.rows];
        self.z_order_map.clear();
        self.next_z_index = 0;
    }

    fn occupied_cells(&self) -> usize {
        self.grid
            .iter()
            .flat_map(|row| row.iter())
            .filter(
                |cell| matches!(cell, ZOrderCellState::Occupied { windows } if !windows.is_empty()),
            )
            .count()
    }

    fn is_cell_occupied(&self, row: usize, col: usize) -> GridResult<bool> {
        self.validate_coordinates(row, col)?;
        Ok(matches!(
            self.grid[row][col],
            ZOrderCellState::Occupied { ref windows } if !windows.is_empty()
        ))
    }

    fn get_cell_windows(&self, row: usize, col: usize) -> GridResult<Vec<u64>> {
        self.validate_coordinates(row, col)?;

        match &self.grid[row][col] {
            ZOrderCellState::Occupied { windows } => Ok(windows.iter().map(|w| w.hwnd).collect()),
            _ => Ok(Vec::new()),
        }
    }

    fn assign_window(&mut self, _hwnd: u64, row: usize, col: usize) -> GridResult<()> {
        self.validate_coordinates(row, col)?;

        // This is a simplified implementation - in practice, you'd want to
        // calculate the proper window rectangle for the target cell
        Err(GridError::ConfigurationError(
            "Direct assignment not supported for ZOrderGrid - use add_window instead".to_string(),
        ))
    }

    fn remove_window(&mut self, hwnd: u64) -> GridResult<()> {
        self.windows.remove(&hwnd);

        // Remove from z-order map
        self.z_order_map.retain(|_, &mut v| v != hwnd);

        // Remove from grid cells
        for row in &mut self.grid {
            for cell in row {
                if let ZOrderCellState::Occupied { windows } = cell {
                    windows.retain(|w| w.hwnd != hwnd);
                    if windows.is_empty() {
                        *cell = ZOrderCellState::Empty;
                    }
                }
            }
        }

        // Recalculate visibility
        self.calculate_visibility();

        Ok(())
    }

    fn get_all_windows(&self) -> Vec<u64> {
        self.windows.keys().copied().collect()
    }
}

impl ZOrderGridTrait for ZOrderGrid {
    fn layer_count(&self) -> usize {
        self.z_order_map.len()
    }

    fn get_layer_windows(&self, layer: usize) -> GridResult<Vec<u64>> {
        if layer >= self.layer_count() {
            return Err(GridError::ZOrderError(format!(
                "Layer {} does not exist, max layer is {}",
                layer,
                self.layer_count() - 1
            )));
        }

        // Get windows at the specified z-order layer
        let windows: Vec<u64> = self
            .z_order_map
            .iter()
            .filter(|(&z_index, _)| z_index == layer)
            .map(|(_, &hwnd)| hwnd)
            .collect();

        Ok(windows)
    }

    fn get_window_z_order(&self, hwnd: u64) -> GridResult<usize> {
        for (&z_index, &window_hwnd) in &self.z_order_map {
            if window_hwnd == hwnd {
                return Ok(z_index);
            }
        }
        Err(GridError::WindowNotFound(hwnd))
    }

    fn get_visibility_map(&self) -> HashMap<(usize, usize), Vec<(u64, bool)>> {
        let mut visibility_map = HashMap::new();

        for (row, grid_row) in self.grid.iter().enumerate() {
            for (col, cell) in grid_row.iter().enumerate() {
                if let ZOrderCellState::Occupied { windows } = cell {
                    let window_visibility: Vec<(u64, bool)> =
                        windows.iter().map(|w| (w.hwnd, w.is_visible)).collect();

                    if !window_visibility.is_empty() {
                        visibility_map.insert((row, col), window_visibility);
                    }
                }
            }
        }

        visibility_map
    }

    fn bring_to_front(&mut self, hwnd: u64) -> GridResult<()> {
        // Find the current z-index
        let current_z = self.get_window_z_order(hwnd)?;

        // Remove from current position
        self.z_order_map.remove(&current_z);

        // Add to front (highest z-index)
        let max_z = self.z_order_map.keys().max().copied().unwrap_or(0);
        self.z_order_map.insert(max_z + 1, hwnd);

        // Recalculate visibility
        self.calculate_visibility();

        Ok(())
    }

    fn send_to_back(&mut self, hwnd: u64) -> GridResult<()> {
        // Find the current z-index
        let current_z = self.get_window_z_order(hwnd)?;

        // Remove from current position
        self.z_order_map.remove(&current_z);

        // Shift all other windows up
        let mut new_map = BTreeMap::new();
        for (&z_index, &window_hwnd) in &self.z_order_map {
            new_map.insert(z_index + 1, window_hwnd);
        }

        // Add this window at index 0 (back)
        new_map.insert(0, hwnd);
        self.z_order_map = new_map;

        // Recalculate visibility
        self.calculate_visibility();

        Ok(())
    }
}

impl CellDisplay for ZOrderCellState {
    fn display_cell(&self) -> &str {
        match self {
            ZOrderCellState::Empty => " .",
            ZOrderCellState::Occupied { .. } => "", // Use get_hwnd for display
            ZOrderCellState::OffScreen => " -",
        }
    }

    fn get_hwnd(&self) -> Option<u64> {
        match self {
            ZOrderCellState::Occupied { windows } => {
                // Return the topmost visible window
                windows
                    .iter()
                    .filter(|w| w.is_visible)
                    .max_by_key(|w| w.z_index)
                    .map(|w| w.hwnd)
            }
            _ => None,
        }
    }

    fn get_z_order(&self) -> Option<usize> {
        match self {
            ZOrderCellState::Occupied { windows } => windows
                .iter()
                .filter(|w| w.is_visible)
                .max_by_key(|w| w.z_index)
                .map(|w| w.z_index),
            _ => None,
        }
    }

    fn is_visible(&self) -> bool {
        match self {
            ZOrderCellState::Occupied { windows } => windows.iter().any(|w| w.is_visible),
            _ => false,
        }
    }
}
