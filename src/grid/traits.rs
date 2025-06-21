// Common traits and types for all grid implementations

use crate::config::GridConfig;
use crate::window::WindowInfo;
use std::collections::HashMap;
use winapi::shared::windef::{HWND, RECT};

/// Result type for grid operations
pub type GridResult<T> = Result<T, GridError>;

/// Errors that can occur during grid operations
#[derive(Debug, Clone)]
pub enum GridError {
    InvalidCoordinates {
        row: usize,
        col: usize,
        max_row: usize,
        max_col: usize,
    },
    WindowNotFound(HWND),
    ConfigurationError(String),
    DisplayError(String),
    AnimationError(String),
    ZOrderError(String),
}

impl std::fmt::Display for GridError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GridError::InvalidCoordinates {
                row,
                col,
                max_row,
                max_col,
            } => {
                write!(
                    f,
                    "Invalid coordinates ({}, {}), max is ({}, {})",
                    row, col, max_row, max_col
                )
            }
            GridError::WindowNotFound(hwnd) => write!(f, "Window {:?} not found", hwnd),
            GridError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
            GridError::DisplayError(msg) => write!(f, "Display error: {}", msg),
            GridError::AnimationError(msg) => write!(f, "Animation error: {}", msg),
            GridError::ZOrderError(msg) => write!(f, "Z-order error: {}", msg),
        }
    }
}

impl std::error::Error for GridError {}

/// Common trait for all grid implementations
pub trait GridTrait {
    /// Get the grid configuration
    fn config(&self) -> &GridConfig;

    /// Update the grid based on current window positions
    fn update(&mut self) -> GridResult<()>;

    /// Clear all window assignments
    fn clear(&mut self);

    /// Get the number of occupied cells
    fn occupied_cells(&self) -> usize;

    /// Check if a cell is occupied
    fn is_cell_occupied(&self, row: usize, col: usize) -> GridResult<bool>;

    /// Get windows in a specific cell (may be multiple for z-order grids)
    fn get_cell_windows(&self, row: usize, col: usize) -> GridResult<Vec<HWND>>;

    /// Assign a window to a specific cell
    fn assign_window(&mut self, hwnd: HWND, row: usize, col: usize) -> GridResult<()>;

    /// Remove a window from the grid
    fn remove_window(&mut self, hwnd: HWND) -> GridResult<()>;

    /// Get all windows currently tracked by this grid
    fn get_all_windows(&self) -> Vec<HWND>;

    /// Validate coordinates against grid bounds
    fn validate_coordinates(&self, row: usize, col: usize) -> GridResult<()> {
        let config = self.config();
        if row >= config.rows || col >= config.cols {
            return Err(GridError::InvalidCoordinates {
                row,
                col,
                max_row: config.rows - 1,
                max_col: config.cols - 1,
            });
        }
        Ok(())
    }
}

/// Trait for displaying grid cells
pub trait CellDisplay {
    /// Get the display string for this cell
    fn display_cell(&self) -> &str;

    /// Get the HWND if this cell contains a window
    fn get_hwnd(&self) -> Option<u64>;

    /// Get the z-order index if this is a layered cell
    fn get_z_order(&self) -> Option<usize> {
        None
    }

    /// Check if this cell is visible (not obscured by other windows)
    fn is_visible(&self) -> bool {
        true
    }
}

/// Trait for grids that support animation
pub trait AnimatableGrid: GridTrait {
    /// Start animating windows to new positions
    fn animate_to_layout(
        &mut self,
        target_layout: &HashMap<HWND, (usize, usize)>,
        duration_ms: u64,
    ) -> GridResult<()>;

    /// Update animation frame
    fn update_animations(&mut self) -> GridResult<Vec<HWND>>; // Returns completed animations

    /// Check if any animations are active
    fn has_active_animations(&self) -> bool;

    /// Stop all animations
    fn stop_all_animations(&mut self);
}

/// Trait for grids that support z-order visualization
pub trait ZOrderGrid: GridTrait {
    /// Get the number of layers in the z-order
    fn layer_count(&self) -> usize;

    /// Get windows at a specific layer
    fn get_layer_windows(&self, layer: usize) -> GridResult<Vec<HWND>>;

    /// Get the z-order of a window
    fn get_window_z_order(&self, hwnd: HWND) -> GridResult<usize>;

    /// Get the visibility map for all cells (which parts of windows are visible)
    fn get_visibility_map(&self) -> HashMap<(usize, usize), Vec<(HWND, bool)>>; // (row, col) -> [(hwnd, is_visible)]

    /// Bring window to front
    fn bring_to_front(&mut self, hwnd: HWND) -> GridResult<()>;

    /// Send window to back
    fn send_to_back(&mut self, hwnd: HWND) -> GridResult<()>;
}

/// Trait for grids that support layout saving/loading
pub trait LayoutGrid: GridTrait {
    /// Save the current layout with a name
    fn save_layout(&mut self, name: String) -> GridResult<()>;

    /// Load a saved layout
    fn load_layout(&mut self, name: &str) -> GridResult<()>;

    /// List all saved layouts
    fn list_layouts(&self) -> Vec<String>;

    /// Delete a saved layout
    fn delete_layout(&mut self, name: &str) -> GridResult<()>;
}
