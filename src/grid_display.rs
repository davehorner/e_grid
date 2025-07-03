// Grid Display Module - Centralized display formatting for consistent visualization
// This ensures both server and client show grids in exactly the same way

use crate::{CellState, GridConfig};

/// Standard grid display configuration
pub struct GridDisplayConfig {
    pub show_headers: bool,
    pub show_window_details: bool,
    pub compact_format: bool,
    pub hex_format: bool, // true = hex display, false = symbolic display
}

impl Default for GridDisplayConfig {
    fn default() -> Self {
        Self {
            show_headers: true,
            show_window_details: false,
            compact_format: false,
            hex_format: true, // Use hex format by default for server consistency
        }
    }
}

/// Unified grid display function that both server and client can use
pub fn display_grid(
    grid: &[Vec<CellState>],
    config: &GridConfig,
    window_count: usize,
    display_config: &GridDisplayConfig,
    title: Option<&str>,
    monitor_info: Option<(i32, i32)>,              // (width, height)
    bounds_info: Option<((i32, i32), (i32, i32))>, // ((left, top), (right, bottom))
) {
    if display_config.show_headers {
        if !display_config.compact_format {
            println!();
            println!("{}", "=".repeat(60));
        }

        if let Some(title) = title {
            println!("{}", title);
        } else {
            println!(
                "Window Grid Tracker - {}x{} Grid ({} windows)",
                config.rows, config.cols, window_count
            );
        }

        if let Some((width, height)) = monitor_info {
            println!("Monitor: {}x{} px", width, height);
        }

        if let Some(((left, top), (right, bottom))) = bounds_info {
            println!(
                "Grid bounds: ({}, {}) to ({}, {})",
                left, top, right, bottom
            );
        }

        if !display_config.compact_format {
            println!("{}", "=".repeat(60));
        }
    }

    // Print column headers
    print!("    ");
    for col in 0..config.cols {
        print!(" {:2}", col);
    }
    println!();

    // Print grid rows
    for row in 0..config.rows {
        print!("{:2}: ", row);
        for col in 0..config.cols {
            if row < grid.len() && col < grid[row].len() {
                print_cell_content(&grid[row][col], display_config);
            } else {
                print!(" ? "); // Fallback for invalid indices
            }
        }
        println!();
    }

    if display_config.show_headers && !display_config.compact_format {
        println!();
    }
}

/// Print the content of a single cell consistently
fn print_cell_content(cell: &CellState, display_config: &GridDisplayConfig) {
    match cell {
        CellState::Empty => print!(" . "),
        CellState::Occupied(hwnd) => {
            if display_config.hex_format {
                // Show last 2 digits of HWND in hex format (server style)
                let hwnd_u64 = *hwnd as u64;
                let display_val = (hwnd_u64 % 100) as u8;
                print!("{:2X} ", display_val);
            } else {
                // Show symbolic representation
                print!("## ");
            }
        }
        CellState::OffScreen => print!("XX "), // Changed from " - " to "XX "
    }
}

/// Display multiple monitor grids in a consistent format
pub fn display_monitor_grids(
    monitors: &[MonitorGridInfo],
    config: &GridConfig,
    display_config: &GridDisplayConfig,
) {
    if monitors.is_empty() {
        println!("No monitor grids available");
        return;
    }

    println!("\n🖥️ Monitor Grids:");
    for (i, monitor) in monitors.iter().enumerate() {
        println!(
            "  Monitor {} (ID: {}): {}x{} at ({}, {})",
            i, monitor.id, monitor.width, monitor.height, monitor.x, monitor.y
        );
        let monitor_title = format!("Monitor {} Grid", monitor.id);
        let monitor_bounds = (
            (monitor.x, monitor.y),
            (monitor.x + monitor.width, monitor.y + monitor.height),
        );

        display_grid(
            &monitor
                .grid
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|&cell| match cell {
                            Some(hwnd) => CellState::Occupied(hwnd),
                            None => CellState::Empty,
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            config,
            0,
            display_config,
            Some(&monitor_title),
            Some((monitor.width, monitor.height)),
            Some(monitor_bounds),
        );
        println!();
    }
}

/// Information about a monitor grid for display purposes
pub struct MonitorGridInfo {
    pub id: u32,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub grid: Vec<Vec<Option<u64>>>, // Grid of window handles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellState;

    #[test]
    fn test_display_grid_basic() {
        let config = GridConfig::new(3, 4);
        let grid = vec![vec![CellState::Empty; 4]; 3];
        let display_config = GridDisplayConfig::default();

        // Should not panic
        display_grid(&grid, &config, 0, &display_config, None, None, None);
    }
    #[test]
    fn test_display_grid_with_windows() {
        let config = GridConfig::new(2, 3);
        let mut grid = vec![vec![CellState::Empty; 3]; 2];
        grid[0][1] = CellState::Occupied(123 as winapi::shared::windef::HWND as u64);
        grid[1][2] = CellState::OffScreen;

        let display_config = GridDisplayConfig::default();

        // Should not panic and display correctly
        display_grid(
            &grid,
            &config,
            2,
            &display_config,
            Some("Test Grid"),
            Some((1920, 1080)),
            None,
        );
    }
}
