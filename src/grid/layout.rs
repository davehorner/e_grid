// Layout Grid - Grid that supports saving and loading window arrangements

use crate::config::GridConfig;
use crate::grid::basic::BasicGrid;
use crate::grid::traits::{GridError, GridResult, GridTrait, LayoutGrid as LayoutGridTrait};
use crate::window_tracker::WindowTracker;
use crate::CellState;
use std::collections::HashMap;
use std::time::Instant;

pub struct LayoutGrid {
    basic_grid: BasicGrid,
    saved_layouts: HashMap<String, GridLayout>,
}
#[derive(Clone, Debug)]
pub struct MonitorGridLayout {
    pub monitor_id: usize,
    pub config: GridConfig,
    pub grid: Vec<Vec<Option<u64>>>,
}
#[derive(Debug, Clone)]
pub struct GridLayout {
    pub name: String,
    pub config: GridConfig,
    pub virtual_grid: Vec<Vec<Option<u64>>>,
    pub monitor_grids: Vec<MonitorGridLayout>,
    pub window_positions: HashMap<String, (usize, usize)>, // window_title -> (row, col)
    pub created_at: Instant,
}

impl LayoutGrid {
    pub fn new(config: GridConfig) -> Self {
        Self {
            basic_grid: BasicGrid::new(config),
            saved_layouts: HashMap::new(),
        }
    }

    pub fn with_monitor_bounds(config: GridConfig, bounds: (i32, i32, i32, i32)) -> Self {
        Self {
            basic_grid: BasicGrid::with_monitor_bounds(config, bounds),
            saved_layouts: HashMap::new(),
        }
    }

    /// Get the current layout as a snapshot
    pub fn capture_current_layout(&self, name: String) -> GridLayout {
        let mut window_positions = HashMap::new();
        // Capture current window positions
        for (hwnd, window_info) in self.basic_grid.windows() {
            // Convert window title from [u16; 256] to String
            let title_str = String::from_utf16_lossy(&window_info.title)
                .trim_end_matches('\0')
                .to_string();
            // Find where this window is positioned in the grid
            for row in 0..self.basic_grid.config().rows {
                for col in 0..self.basic_grid.config().cols {
                    if let Ok(cell_windows) = self.basic_grid.get_cell_windows(row, col) {
                        if cell_windows.contains(hwnd) {
                            window_positions.insert(title_str.clone(), (row, col));
                            break;
                        }
                    }
                }
            }
        }
        GridLayout {
            name,
            config: self.basic_grid.config().clone(),
            // Build the virtual_grid as a 2D Vec of Option<u64> based on the current grid state
            virtual_grid: (0..self.basic_grid.config().rows)
                .map(|row| {
                    (0..self.basic_grid.config().cols)
                        .map(|col| {
                            let cell_windows = self
                                .basic_grid
                                .get_cell_windows(row, col)
                                .unwrap_or_default();
                            cell_windows.get(0).copied()
                        })
                        .collect()
                })
                .collect(),
            monitor_grids: Vec::new(),
            window_positions,
            created_at: std::time::Instant::now(),
        }
    }

    /// Apply a saved layout by moving windows to their saved positions
    fn apply_layout(&mut self, layout: &GridLayout) -> GridResult<()> {
        // Find windows that match the saved layout by title
        let mut windows_to_move = HashMap::new();

        for (hwnd, window_info) in self.basic_grid.windows() {
            let title_str = String::from_utf16_lossy(&window_info.title)
                .trim_end_matches('\0')
                .to_string();
            if let Some(&(row, col)) = layout.window_positions.get(&title_str) {
                windows_to_move.insert(*hwnd, (row, col));
            }
        }

        // Move windows to their saved positions
        for (hwnd, (row, col)) in windows_to_move {
            self.basic_grid.assign_window(hwnd, row, col)?;
        }

        println!(
            "📋 Applied layout '{}' - moved {} windows",
            layout.name,
            layout.window_positions.len()
        );
        Ok(())
    }
    pub fn new_with_config(name: String, config: GridConfig) -> GridLayout {
        let virtual_grid = vec![vec![None; config.cols]; config.rows];
        GridLayout {
            name,
            config,
            virtual_grid,
            monitor_grids: Vec::new(),
            window_positions: HashMap::new(),
            created_at: Instant::now(),
        }
    }
    pub fn from_current_state(tracker: &WindowTracker, name: String) -> GridLayout {
        let mut layout = Self::new_with_config(name, tracker.config.clone());

        // Extract virtual grid layout
        for row in 0..tracker.config.rows {
            for col in 0..tracker.config.cols {
                if let CellState::Occupied(hwnd) = tracker.grid[row][col] {
                    layout.virtual_grid[row][col] = Some(hwnd);
                }
            }
        }

        // Extract monitor grid layouts
        for monitor_grid in &tracker.monitor_grids {
            let mut monitor_layout = MonitorGridLayout {
                monitor_id: monitor_grid.monitor_id,
                config: monitor_grid.config.clone(),
                grid: vec![vec![None; monitor_grid.config.cols]; monitor_grid.config.rows],
            };

            for row in 0..monitor_grid.config.rows {
                for col in 0..monitor_grid.config.cols {
                    if let CellState::Occupied(hwnd) = monitor_grid.grid[row][col] {
                        monitor_layout.grid[row][col] = Some(hwnd);
                    }
                }
            }

            layout.monitor_grids.push(monitor_layout);
        }

        layout
    }
}

impl GridTrait for LayoutGrid {
    fn config(&self) -> &GridConfig {
        self.basic_grid.config()
    }

    fn update(&mut self) -> GridResult<()> {
        self.basic_grid.update()
    }

    fn clear(&mut self) {
        self.basic_grid.clear()
    }

    fn occupied_cells(&self) -> usize {
        self.basic_grid.occupied_cells()
    }

    fn is_cell_occupied(&self, row: usize, col: usize) -> GridResult<bool> {
        self.basic_grid.is_cell_occupied(row, col)
    }

    fn get_cell_windows(&self, row: usize, col: usize) -> GridResult<Vec<u64>> {
        self.basic_grid.get_cell_windows(row, col)
    }

    fn assign_window(&mut self, hwnd: u64, row: usize, col: usize) -> GridResult<()> {
        self.basic_grid.assign_window(hwnd, row, col)
    }

    fn remove_window(&mut self, hwnd: u64) -> GridResult<()> {
        self.basic_grid.remove_window(hwnd)
    }

    fn get_all_windows(&self) -> Vec<u64> {
        self.basic_grid.get_all_windows()
    }
}

impl LayoutGridTrait for LayoutGrid {
    fn save_layout(&mut self, name: String) -> GridResult<()> {
        let layout = self.capture_current_layout(name.clone());
        self.saved_layouts.insert(name.clone(), layout);
        println!("💾 Saved layout '{}'", name);
        Ok(())
    }
    fn load_layout(&mut self, name: &str) -> GridResult<()> {
        if let Some(layout) = self.saved_layouts.get(name).cloned() {
            self.apply_layout(&layout)?;
            Ok(())
        } else {
            Err(GridError::ConfigurationError(format!(
                "Layout '{}' not found",
                name
            )))
        }
    }

    fn list_layouts(&self) -> Vec<String> {
        self.saved_layouts.keys().cloned().collect()
    }

    fn delete_layout(&mut self, name: &str) -> GridResult<()> {
        if self.saved_layouts.remove(name).is_some() {
            println!("🗑️  Deleted layout '{}'", name);
            Ok(())
        } else {
            Err(GridError::ConfigurationError(format!(
                "Layout '{}' not found",
                name
            )))
        }
    }
}
