# E-Grid: Advanced Window Grid Management System

A comprehensive window management system that provides real-time window tracking, grid-based positioning, and IPC-based window assignment across multiple monitors.

## 🎯 Core Features

### Real-Time Window Tracking
- **Shell Hook Integration**: Uses Windows Shell Hooks to detect window creation, destruction, and activation
- **Multi-Monitor Support**: Tracks windows across all connected monitors with per-monitor grids
- **Visual Grid Display**: Shows both virtual (spanning all monitors) and per-monitor 8x12 grids
- **Live Updates**: Automatic grid updates when windows are moved, resized, or closed

### Advanced Window Assignment
- **Dual Assignment Modes**:
  - **Virtual Grid**: Assign windows using coordinates spanning all monitors
  - **Monitor-Specific**: Assign windows to specific cells on individual monitors
- **Interactive IPC Client**: Real-time window assignment through command-line interface
- **Grid State Synchronization**: Automatic rescanning and updating after window movements
- **Smart Cell Detection**: Coverage-based algorithm that only marks cells as occupied when windows cover ≥30% of the cell area

### IPC Communication System
- **iceoryx2 Integration**: High-performance inter-process communication
- **Client-Server Architecture**: Separate client for interactive control, server for window management
- **Real-Time Events**: Live broadcasting of window events and grid state changes
- **Command Processing**: Support for grid queries, window listing, and assignments

### Intelligent Grid Detection
- **Coverage Threshold**: Configurable percentage (default 30%) of cell area that must be covered to consider a cell occupied
- **Precise Assignment**: When assigning windows to single cells, only that cell shows as occupied
- **Flexible Detection**: Handles windows of any size with accurate grid representation
- **No False Positives**: Eliminates issues with windows appearing in multiple cells due to boundary overlaps

## 🎯 Smart Grid Detection System

### Coverage-Based Cell Assignment

E-Grid uses an intelligent coverage-based algorithm to determine which grid cells are occupied by windows. Instead of simple boundary checking, it calculates the actual intersection area between windows and grid cells.

### How It Works

1. **Coverage Calculation**: For each potential grid cell, the system calculates what percentage of the cell area is covered by the window
2. **Threshold Comparison**: Only cells with coverage ≥ 30% (configurable) are marked as occupied
3. **Precise Assignment**: When you assign a window to a single cell, it will only show up in that cell unless it significantly overlaps others

### Configuration

```rust
// In src/lib.rs - adjustable coverage threshold
const COVERAGE_THRESHOLD: f32 = 0.3; // 30% coverage required
```

**Threshold Options:**
- **`0.1`** (10%) - Very sensitive, small overlaps count as occupation
- **`0.3`** (30%) - **Default**, balanced approach for most use cases
- **`0.5`** (50%) - Window must cover majority of cell to count
- **`0.8`** (80%) - Very strict, window must nearly fill entire cell

### Benefits

- ✅ **Accurate Single-Cell Assignment**: Windows assigned to one cell show up in only that cell
- ✅ **No False Positives**: Eliminates boundary-overlap issues
- ✅ **Flexible Window Sizes**: Works with any window dimensions
- ✅ **Consistent Behavior**: Predictable grid representation regardless of window positioning

## 🏗️ Architecture

The system consists of several key components:

1. **WindowTracker Core** (`lib.rs`): Main window tracking and grid management
2. **Shell Hooks** (`window_events.rs`): Windows API integration for event detection
3. **IPC Layer** (`ipc.rs`): iceoryx2-based inter-process communication
4. **Interactive Client** (`ipc_demo.rs`): Command-line interface for window control

### Grid System
- **Virtual Grid**: Single unified grid spanning all monitors (coordinates are global)
- **Monitor Grids**: Individual 8x12 grids for each connected monitor
- **Automatic Scaling**: Grid coordinates calculated based on monitor resolution and layout
- **Coverage-Based Detection**: Uses intersection area calculations to determine cell occupancy
- **Configurable Threshold**: Adjustable coverage percentage (default 30%) for precise grid representation

## 🚀 Getting Started

### Prerequisites
- Windows 10/11
- Rust (latest stable)
- Git

### Installation
```bash
git clone <repository>
cd e_grid
cargo build --release
```

### Basic Usage

#### 1. Run the Interactive Demo (Recommended)
```bash
cargo run --bin ipc_demo
```
This starts both the server and an interactive client for testing window assignments.

#### 2. Manual Server/Client Setup
```bash
# Terminal 1 - Start the server
cargo run --bin ipc_demo

# Terminal 2 - Start interactive client
cargo run --bin ipc_demo -- --client
```

#### 3. Other Testing Tools
```bash
# Simple grid display (legacy)
cargo run --bin simple_grid

# Debug window positions
cargo run --bin debug_positions

# Basic grid tracking
cargo run --bin basic_grid
```

## 🎮 Interactive Client Commands

Once the client is running, you can use these commands:

### Window Assignment
- **`assign`** - Interactive window assignment
  - Choose `v` for **virtual grid** (coordinates span all monitors)
  - Choose `m` for **monitor-specific** (coordinates relative to individual monitor)
  - Enter window HWND (get from `list` command)
  - Enter target row and column coordinates

### Information Commands
- **`list`** - Display all tracked windows with their HWNDs
- **`grid`** - Show current grid state for all monitors
- **`quit`** - Exit the client

### Example Session
```
[CLIENT] > list
📤 Sent GetWindowList command
📤 [RESPONSE] Window List: 12 windows

[CLIENT] > assign
Choose assignment mode (v=virtual, m=monitor): v
Enter window HWND: 12345678
Enter target row (0-based): 2
Enter target column (0-based): 5
📤 Sent virtual assignment command: HWND 12345678 to (2, 5)

[CLIENT] > grid
📤 Sent GetGridState command
📊 Grid State: 12 windows, 8 occupied cells
```

## 🖥️ Server Commands

The server provides these interactive commands:

- **`g`** / **`grid`** - Display all monitor grids
- **`r`** / **`rescan`** - Force rescan of all windows
- **`e`** / **`event`** - Publish demo IPC event
- **`c`** / **`commands`** - Process demo IPC commands
- **`h`** / **`help`** - Show available commands
- **`q`** / **`quit`** - Exit the server

## 📊 Grid Display Example

```
Virtual Grid (All Monitors):
════════════════════════════════════════════════════════════════
    0  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 
 0 ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· 
 1 ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· 
 2 ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· ·· ·· ·· ██ ██ ██ ██ ·· ·· ·· ·· ·· 
 3 ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· 
 4 ██ ██ ██ ·· ·· ·· ·· ██ ██ ██ ██ ██ ·· ·· ·· ·· ·· ·· ·· ██ ██ ██ ██ ██ 
 5 ██ ██ ██ ·· ·· ·· ·· ██ ██ ██ ██ ██ ·· ·· ·· ·· ·· ·· ·· ██ ██ ██ ██ ██ 
 6 ██ ██ ██ ·· ·· ·· ·· ██ ██ ██ ██ ██ ·· ·· ·· ·· ·· ·· ·· ██ ██ ██ ██ ██ 
 7 ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· ·· 

Monitor 0 Grid (1920x1080):
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
██ Notepad [HWND: 12345678] (cells: 12)
██ File Explorer [HWND: 87654321] (cells: 20)
██ VS Code [HWND: 11223344] (cells: 16)
```

## 🔧 Technical Implementation

### Core Technologies
- **Rust** with `winapi` for Windows API integration
- **iceoryx2** for high-performance IPC communication
- **crossterm** for colored terminal output
- **Windows Shell Hooks** for real-time event detection

### Key Features
- **Thread-Safe Design**: Uses `Arc<Mutex<>>` for safe concurrent access
- **Multi-Monitor Aware**: Automatic detection and handling of monitor configurations
- **Memory Safe**: Leverages Rust's ownership system and explicit safety documentation
- **Modular Architecture**: Clean separation of concerns between tracking, IPC, and UI
- **Smart Grid Detection**: Coverage-based algorithm with configurable thresholds for precise cell assignment

### Window Management Process
1. **Shell Hook Registration**: Registers for `HSHELL_WINDOWCREATED`, `HSHELL_WINDOWDESTROYED`, and `HSHELL_WINDOWACTIVATED` events
2. **Multi-Monitor Detection**: Enumerates all connected monitors and their configurations
3. **Coverage-Based Grid Calculation**: Calculates intersection areas between windows and grid cells
4. **Threshold-Based Assignment**: Only assigns cells where window coverage exceeds configurable threshold
5. **Real-Time Updates**: Automatic rescanning after window movements and assignments
5. **IPC Communication**: Publishes events and processes commands through iceoryx2

### Safety and Reliability
- **Static Safety**: Explicit use of raw pointers with comprehensive safety documentation
- **Error Handling**: Comprehensive error handling throughout the IPC and windowing layers
- **Resource Cleanup**: Proper cleanup of hooks, handles, and IPC resources
- **Thread Safety**: All shared state protected by mutexes

## 🚧 Future Enhancements

- **Window Snapping**: Automatic window positioning based on grid assignments
- **Configuration System**: User-customizable grid sizes and monitor layouts
- **Hotkey Integration**: Global hotkeys for quick window assignments
- **Session Management**: Save and restore window layouts
- **Tiling Policies**: Advanced tiling algorithms and window arrangement patterns

## 🤝 Contributing

This project demonstrates advanced Rust patterns for:
- Windows API integration
- Inter-process communication
- Multi-monitor handling
- Real-time system integration
- Safe concurrent programming

Perfect foundation for building sophisticated window management tools and desktop environments.

## 🔍 Troubleshooting

### Single-Cell Assignment Issues

**Problem**: "When I assign a window to one cell, it shows up in multiple cells"

**Solution**: This has been resolved with the coverage-based detection system. The new algorithm:
- Calculates exact intersection areas between windows and cells
- Only marks cells as occupied when coverage exceeds 30% threshold
- Ensures single-cell assignments appear in only the target cell

**Adjusting Sensitivity**: If you need different behavior, modify the coverage threshold:
```rust
// In src/lib.rs - make more or less sensitive
const COVERAGE_THRESHOLD: f32 = 0.5; // Increase for stricter detection
const COVERAGE_THRESHOLD: f32 = 0.1; // Decrease for more sensitive detection
```

### Grid Display Accuracy

The grid now accurately reflects window positions:
- ✅ **Single-cell assignments** show up in exactly one cell
- ✅ **Large windows** may span multiple cells based on actual coverage
- ✅ **Small windows** only occupy cells they significantly overlap
- ✅ **Boundary cases** are handled intelligently with area calculations
