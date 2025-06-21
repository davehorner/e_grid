// Display module for grid formatting and visualization
pub mod formatters;

// Re-export main functions
pub use formatters::{
    format_hwnd_display, print_column_headers, print_empty_cell, print_monitor_header,
    print_offscreen_cell, print_row_prefix, print_virtual_monitor_header,
};

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
