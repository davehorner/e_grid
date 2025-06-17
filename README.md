# Window Grid Tracker

A real-time window tracking application that displays which grid cells are covered by windows on your screen.

## Features

- **Real-time tracking**: Uses Windows Shell Hooks to detect window creation, destruction, and activation
- **Visual grid display**: Shows an 8x12 grid representing your screen with colored blocks for windows
- **Multi-window support**: Each window gets a different color in the grid
- **Live updates**: Grid updates automatically when windows are moved, resized, or closed

## How it works

The application uses the same Windows Shell Hook mechanism as dwm-win32:

1. **RegisterShellHookWindow()** - Registers to receive shell hook messages
2. **HSHELL_WINDOWCREATED** - Detects new windows
3. **HSHELL_WINDOWDESTROYED** - Detects closed windows  
4. **HSHELL_WINDOWACTIVATED** - Detects window focus/activation events
5. **Periodic scanning** - Also rescans windows every 500ms to catch moves/resizes

## Grid Layout

- Screen is divided into an 8x12 grid (8 rows, 12 columns)
- Each cell represents a portion of your screen
- Windows are shown as colored blocks (██) in the cells they cover
- Empty cells are shown as dots (··)

## Usage

```bash
# Build the application
cargo build --release

# Run the grid tracker
cargo run --bin grid_tracker
```

## Controls

- The application runs in the terminal and updates automatically
- Press Ctrl+C to exit
- Grid updates happen in real-time as you open, close, move, or resize windows

## Example Output

```
Window Grid Tracker - 8x12 Grid
══════════════════════════════════════════════════════════
    0  1  2  3  4  5  6  7  8  9 10 11 
 0 ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· 
 1 ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· 
 2 ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· 
 3 ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· 
 4 ██ ██ ██ ·· ·· ·· ·· ██ ██ ██ ██ ██ 
 5 ██ ██ ██ ·· ·· ·· ·· ██ ██ ██ ██ ██ 
 6 ██ ██ ██ ·· ·· ·· ·· ██ ██ ██ ██ ██ 
 7 ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· 

Active Windows:
██ Notepad (cells: 12)
██ File Explorer (cells: 20)
```

## Technical Details

- Written in Rust using `winapi` for Windows APIs
- Uses `crossterm` for terminal UI with colors
- Tracks windows in a HashMap with their positions and grid cells
- Converts window rectangles to grid coordinates based on screen size
- Filters out system windows and tool windows to show only manageable windows

This gives you the foundation for implementing more advanced grid-based window management features.
