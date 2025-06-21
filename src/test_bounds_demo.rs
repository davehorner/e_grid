use e_grid::{GridConfig, CellState, grid_display};
use winapi::shared::windef::HWND;

fn main() {
    println!("=== TESTING GRID BOUNDS DISPLAY ===");
    
    // Create a simple test grid
    let config = GridConfig::new(4, 6); // Smaller grid for cleaner output
    let mut grid = vec![vec![CellState::Empty; config.cols]; config.rows];
    
    // Add a test window
    grid[1][2] = CellState::Occupied(0x12345678 as HWND);
    
    println!("\n1. Grid with bounds (single monitor):");
    grid_display::display_grid(
        &grid,
        &config,
        1, // 1 window
        &grid_display::GridDisplayConfig::default(),
        Some("Test Grid - With Bounds"),
        Some((1920, 1080)), // Monitor size
        Some(((0, 0), (1920, 1080))), // Bounds: top-left (0,0) to bottom-right (1920,1080)
    );
    
    println!("\n2. Grid with bounds (multi-monitor):");
    grid_display::display_grid(
        &grid,
        &config,
        1, // 1 window
        &grid_display::GridDisplayConfig::default(),
        Some("Test Grid - Multi-Monitor"),
        Some((3840, 1080)), // Dual monitor width
        Some(((-1920, 0), (1920, 1080))), // Bounds: left monitor at -1920, spans to right monitor
    );
    
    println!("\nBounds are displayed in the header as: Grid bounds: (left, top) to (right, bottom)");
}
