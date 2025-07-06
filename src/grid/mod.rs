// Grid module - contains different types of grid implementations
// Each grid type has specialized functionality and visualization

pub mod animation;
pub mod basic;
pub mod layout;
pub mod monitor_grid;
pub mod traits;
pub mod zorder;

// Re-export the main grid types for easy access
pub use animation::AnimationGrid;
pub use basic::BasicGrid;
pub use layout::LayoutGrid;
pub use traits::{CellDisplay, GridError, GridResult, GridTrait};
pub use zorder::ZOrderGrid;

// Re-export common types used by all grids
pub use crate::config::GridConfig;
pub use crate::window::WindowInfo;
