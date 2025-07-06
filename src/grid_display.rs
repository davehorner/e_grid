// Grid Display Module - Centralized display formatting for consistent visualization
// This ensures both server and client show grids in exactly the same way
use winapi::um::winuser::GetForegroundWindow;
use winapi::shared::windef::HWND;

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
    grid: &Vec<Vec<CellState>>,
    config: &GridConfig,
    window_count: usize,
    display_config: &GridDisplayConfig,
    title: Option<&str>,
    monitor_bounds: Option<(i32, i32, i32, i32)>,
    monitor_id: Option<u32>,
    topmost_hwnd: Option<u64>,
) {
    // Get the currently focused window
    let focused_hwnd = unsafe {
        let fg_hwnd = GetForegroundWindow();
        if fg_hwnd.is_null() {
            None
        } else {
            Some(fg_hwnd as u64)
        }
    };

    // Debug: Print focused window info and check if it's in the grid
    if let Some(focused) = focused_hwnd {
        println!("DEBUG: Focused window HWND: 0x{:X} (last 2 digits: {:02X})", focused, focused & 0xFF);
        
        // Check if this focused window appears anywhere in the grid
        let mut found_in_grid = false;
        for row in 0..config.rows {
            for col in 0..config.cols {
                if let CellState::Occupied(hwnd) = grid[row][col] {
                    if hwnd == focused {
                        found_in_grid = true;
                        println!("DEBUG: Focused window FOUND in grid at row {} col {}", row, col);
                        break;
                    }
                }
            }
            if found_in_grid { break; }
        }
        if !found_in_grid {
            println!("DEBUG: Focused window NOT found in current grid");
        }
    } else {
        println!("DEBUG: No focused window detected");
    }

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

        if let Some((left, top, right, bottom)) = monitor_bounds {
            println!("Monitor: {}x{} px", right - left, bottom - top);
        }

        if let Some(id) = monitor_id {
            println!("Monitor ID: {}", id);
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
    for row in 0..config.rows.min(32) {
        print!("{:2} ", row);
        for col in 0..config.cols.min(32) {
            match grid[row][col] {
                CellState::Empty => print!(".. "),
                CellState::OffScreen => print!("XX "),
                CellState::Occupied(hwnd) => {
                    if hwnd == 0 || hwnd == u64::MAX {
                        print!("XX ");
                    } else {
                        // Debug: Check each window in the grid
                        let is_focused = Some(hwnd) == focused_hwnd;
                        let is_topmost = Some(hwnd) == topmost_hwnd;
                        
                        // Check if this is the focused window (blue) or topmost window (red)
                        if is_focused {
                            print!("\x1b[34m{:02X}\x1b[0m ", hwnd & 0xFF); // Blue for focused
                        } else if is_topmost {
                            print!("\x1b[31m{:02X}\x1b[0m ", hwnd & 0xFF); // Red for topmost
                        } else {
                            print!("{:02X} ", hwnd & 0xFF);
                        }
                    }
                }
            }
        }
        println!();
    }

    // Print legend if highlighting is enabled
    if topmost_hwnd.is_some() || focused_hwnd.is_some() {
        println!();
        println!("Legend:");
        if let Some(topmost) = topmost_hwnd {
            println!("  \x1b[31mRed\x1b[0m = Topmost window (HWND: 0x{:X})", topmost);
        }
        if let Some(focused) = focused_hwnd {
            println!("  \x1b[34mBlue\x1b[0m = Input focus window (HWND: 0x{:X})", focused);
        }
    }

    if display_config.show_headers && !display_config.compact_format {
        println!();
    }
}

/// Print the content of a single cell consistently
fn print_cell_content(cell: &CellState, display_config: &GridDisplayConfig,topmost_hwnd: Option<u64>) {
    match cell {
        CellState::Empty => print!(" . "),
        CellState::Occupied(hwnd) => {
            if display_config.hex_format {
                // Show last 2 digits of HWND in hex format (server style)
                let hwnd_u64 = *hwnd as u64;
                let display_val = (hwnd_u64 & 0xFF) as u8;
                let symbol = format!("{:02X}", display_val);
                if Some(hwnd_u64) == topmost_hwnd {
                    // Print in red (ANSI escape code)
                    print!("\x1b[31m{} \x1b[0m", symbol);
                } else {
                    print!("{} ", symbol);
                }
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
            Some((monitor.x, monitor.y, monitor.x + monitor.width, monitor.y + monitor.height)),
            Some(monitor.id),
            None, // No topmost_hwnd provided for monitor grids
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
        display_grid(&grid, &config, 0, &display_config, None, None, None, None);
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
            Some((0, 0, 1920, 1080)),
            None, // bounds_info
            None, // topmost_hwnd
        );
    }
}
