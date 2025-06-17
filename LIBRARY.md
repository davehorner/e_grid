# E-Grid Library

A Rust library for tracking windows and mapping them to a virtual grid across multiple monitors on Windows systems.

## Library Structure

### Core Components

#### `WindowTracker`
The main struct that manages window tracking and grid mapping.

**Key Features:**
- Multi-monitor support using virtual screen coordinates
- Real-time window enumeration and tracking
- Grid-based window positioning (8x12 default grid)
- Shell hook integration for live updates

**Public Methods:**
- `new()` - Create a new window tracker
- `scan_existing_windows()` - Enumerate all current windows
- `add_window(hwnd)` - Add a specific window to tracking
- `remove_window(hwnd)` - Remove a window from tracking
- `update_window(hwnd)` - Update window position/size
- `print_grid()` - Display the current grid state
- `get_monitor_info()` - Get virtual screen dimensions

#### `WindowInfo`
Represents information about a tracked window.

**Fields:**
- `hwnd` - Windows handle
- `title` - Window title (truncated to 50 chars)
- `rect` - Window rectangle coordinates
- `grid_cells` - List of grid cells the window occupies

#### `shell_hook` Module
Utilities for Windows Shell Hook integration to receive real-time window events.

**Functions:**
- `setup_shell_hook(tracker)` - Initialize shell hook for live updates
- `window_proc()` - Windows message handler

### Constants

- `GRID_ROWS: usize = 8` - Number of grid rows
- `GRID_COLS: usize = 12` - Number of grid columns

## Usage Examples

### Basic Usage

```rust
use e_grid::WindowTracker;

fn main() {
    let mut tracker = WindowTracker::new();
    
    // Get monitor info
    let (left, top, width, height) = tracker.get_monitor_info();
    println!("Virtual Screen: {}x{} px", width, height);
    
    // Scan for windows
    tracker.scan_existing_windows();
    
    // Display grid
    tracker.print_grid();
}
```

### Advanced Usage with Shell Hook

```rust
use e_grid::{WindowTracker, shell_hook};
use std::sync::{Arc, Mutex};

fn main() {
    let tracker = WindowTracker::new();
    let tracker_arc = Arc::new(Mutex::new(tracker));
    
    match shell_hook::setup_shell_hook(tracker_arc.clone()) {
        Ok(_hwnd) => {
            // Message loop for real-time updates
            // ... handle Windows messages
        }
        Err(e) => {
            println!("Shell hook failed: {}", e);
            // Fallback to periodic updates
        }
    }
}
```

## Binary Examples

The library comes with several binary examples:

### `basic_example`
Simple demonstration of core functionality.
```bash
cargo run --bin basic_example
```

### `simple_grid_new`
Full-featured window tracker with shell hook integration and periodic updates.
```bash
cargo run --bin simple_grid_new
```

### `monitor_test`
Utility to test monitor detection and display system information.
```bash
cargo run --bin monitor_test
```

## Features

### Multi-Monitor Support
- Automatically detects all monitors using `GetSystemMetrics`
- Uses virtual screen coordinates to span the entire desktop
- Grid cells map proportionally across all displays

### Window Filtering
- Filters out system windows, hidden windows, and tool windows
- Focuses on user-manageable application windows
- Handles minimized windows (at coordinates -32000,-32000)

### Grid Mapping
- 8x12 grid by default (96 total cells)
- Each window mapped to the grid cells it occupies
- Visual representation with `##` for occupied cells, `..` for empty

### Safety Features
- Enumeration counter prevents infinite loops
- Bounds checking for grid calculations
- Error handling for Windows API calls

## Dependencies

- `winapi` - Windows API bindings
- `crossterm` - Terminal utilities (for examples)

## Testing

Run the test suite:
```bash
cargo test --lib
```

## Architecture Notes

The library is designed with separation of concerns:
- Core window tracking logic in `lib.rs`
- Shell hook integration in `shell_hook` module
- Binary examples demonstrate different usage patterns
- Clean public API for integration into other projects

This structure allows the library to be used in different contexts:
- Simple one-time window scanning
- Real-time window monitoring
- Integration into larger window management systems
