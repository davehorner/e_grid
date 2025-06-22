// E-Grid: Advanced Window Grid Management System
// This library provides multiple types of grids for different use cases

// Modular structure
pub mod config;
pub mod grid;
pub mod window;
pub mod display;
pub mod monitor;

// Import our error handling module
pub mod grid_client_errors;
pub use grid_client_errors::{GridClientError, GridClientResult, RetryConfig, 
                             retry_with_backoff, validate_grid_coordinates, 
                             safe_lock, safe_arc_lock};

// Re-export main types for convenience
pub use config::GridConfig;
pub use grid::{
    GridTrait, CellDisplay, GridError, GridResult,
    BasicGrid, ZOrderGrid, AnimationGrid, LayoutGrid
};
pub use window::{WindowInfo, WindowTracker, WindowAnimation};
pub use display::{format_hwnd_display, CellDisplay as DisplayCellTrait};

// Coverage threshold: percentage of cell area that must be covered by window
// to consider the window as occupying that cell (0.0 to 1.0)
const COVERAGE_THRESHOLD: f32 = 0.3; // 30% coverage required

// Virtual monitor ID - always outside the range of physical monitors
const VIRTUAL_MONITOR_ID: usize = 99;

// Legacy compatibility types - these will be deprecated
pub use grid::BasicGrid as LegacyWindowTracker;

// Re-export common easing types for animations
pub use grid::animation::EasingType;

// IPC module remains unchanged
pub mod ipc;
