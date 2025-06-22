# E-Grid: Advanced Window Grid Management System

A comprehensive, event-driven window management system that provides real-time window tracking, grid-based positioning, and efficient IPC-based communication across multiple monitors.

NOTICE: This repository is in an interesting state; the examples and functionality may or may not be implemented. Have a look at [e_midi](https://crates.io/crates/e_midi) which includes an example of playing a midi sound using the focus and defocused events. Aside from that; Feel free to take a look at the LLM cruft, it will give you an idea of some of the directions this project could go.

Dave Horner 6/25 MIT/Apache License

### Architecture Overview

```
┌─────────────────────┐    WinEvents     ┌──────────────────────┐
│   Windows System    │ ──────────────→  │   IPC Server Demo    │
│ - Window Creation   │                  │  - Minimal callbacks │
│ - Window Movement   │                  │  - Main loop logic   │
│ - Focus Changes     │                  │  - Window rescanning │
│ - Window Destroy    │                  │  - IPC publishing    │
└─────────────────────┘                  │  - Focus tracking    │
                                         └──────────┬───────────┘
                                                    │ iceoryx2 IPC
                                                    │ Multi-Service:
                                                    │ • Grid Events
                                                    │ • Window Details  
                                                    │ • Focus Events ⭐
                                                    │ • Commands
                                                    │ • Responses
                                                    ▼
                          ┌─────────────────────────────────────────┐
                          │              Client Applications         │
                          ├─────────────────────┬───────────────────┤
                          │  Grid Client Demo   │  Focus Demo Apps  │
                          │  - Real-time grids  │  - Focus tracking │
                          │  - Window details   │  - Event logging  │
                          │  - Throttled UI     │  - App filtering  │
                          │  - Non-blocking     │  - Multi-client   │
                          └─────────────────────┴───────────────────┘
```

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
- **iceoryx2** for high-performance IPC communication (64KB buffer sizes)
- **crossterm** for colored terminal output
- **Windows WinEvents** for real-time event detection with minimal callbacks

### Key Features
- **Thread-Safe Design**: Uses `Arc<Mutex<>>` for safe concurrent access
- **Deadlock Prevention**: Minimal WinEvent processing, heavy work in main loops
- **Multi-Monitor Aware**: Automatic detection and handling of monitor configurations
- **Memory Safe**: Leverages Rust's ownership system and explicit safety documentation
- **Modular Architecture**: Clean separation of concerns between tracking, IPC, and UI
- **Smart Grid Detection**: Coverage-based algorithm with configurable thresholds for precise cell assignment
- **Non-blocking UI**: Client uses try_lock and display throttling for responsive interface

### Window Management Process
1. **WinEvent Registration**: Registers for comprehensive window events (CREATE, MOVE, DESTROY, etc.)
2. **Minimal Callbacks**: WinEvent handlers only log events, preventing deadlocks
3. **Main Loop Processing**: Server periodically scans windows and updates grid (every 2 seconds)
4. **Multi-Monitor Detection**: Enumerates all connected monitors and their configurations
5. **Coverage-Based Grid Calculation**: Calculates intersection areas between windows and grid cells
6. **Threshold-Based Assignment**: Only assigns cells where window coverage exceeds configurable threshold
7. **Incremental Updates**: Server publishes only changed window details via IPC
8. **Real-Time Synchronization**: Client maintains matching grid state through event processing
9. **IPC Communication**: High-performance message passing through iceoryx2 with large buffers

### Safety and Reliability
- **Static Safety**: Explicit use of raw pointers with comprehensive safety documentation
- **Deadlock Prevention**: Minimal processing in system callbacks, heavy work in main loops
- **Error Handling**: Comprehensive error handling throughout the IPC and windowing layers
- **Resource Cleanup**: Proper cleanup of hooks, handles, and IPC resources
- **Thread Safety**: All shared state protected by mutexes with non-blocking try_lock patterns
- **IPC Reliability**: Large buffer sizes (64KB) and error recovery prevent message loss

## 🎯 **NEW: Unified E-Grid Binary**

**Major Update**: E-Grid now provides a single, intelligent `e_grid` binary that auto-detects your needs!

✨ **Smart Auto-Detection:**
- **No server running?** → Starts server + detached client automatically
- **Server already running?** → Connects as interactive client
- **Force specific mode** → Use `e_grid server` or `e_grid client`

```bash
# One command does it all - auto-detects and starts appropriate mode
cargo run --bin e_grid

# Or use the built binary directly
./target/debug/e_grid
```

**🎯 What you get:**
- **Full server** with focus tracking, multi-monitor grids, animations, layouts
- **Detached client** for real-time grid visualization  
- **Interactive mode** for live grid monitoring
- **Smart detection** - no manual server/client coordination needed

[📖 **Jump to Quick Start**](#-quick-start---unified-binary)

---

## 🎯 **NEW: Focus Event Tracking Integration**

**Major Update**: E-Grid now includes comprehensive window focus tracking directly integrated into the main server!

✨ **Key Highlights:**
- **Complete Focus Coverage**: Both FOCUSED and DEFOCUSED events in real-time
- **Zero Setup**: No separate focus server needed - it's built into the main e_grid server
- **Production Ready**: Full integration with existing grid management features
- **Multi-Client Support**: Up to 8 applications can subscribe to focus events simultaneously
- **Rich Event Data**: Process ID, window titles, hash-based identification, precise timestamps

```bash
# Quick Test - Focus Events with Main Server
cargo run --bin e_grid                   # Auto-starts server + client
cargo run --example simple_focus_demo    # Terminal 2: Focus tracking client
# Now click between windows to see real-time FOCUSED/DEFOCUSED events!
```

[📖 **Jump to Focus Event Documentation**](#-focus-event-tracking)

---

## 🎯 Core Features

### Event-Driven Real-Time Window Tracking
- **WinEvent Integration**: Uses Windows WinEvent hooks for true real-time window detection
- **Non-blocking Architecture**: Minimal WinEvent callbacks prevent system deadlocks
- **Efficient IPC**: iceoryx2-based high-performance inter-process communication
- **Multi-Monitor Support**: Tracks windows across all connected monitors with per-monitor grids
- **Visual Grid Display**: Shows both virtual (spanning all monitors) and per-monitor 8x12 grids

### Advanced Client-Server Architecture
- **Dedicated Server**: `ipc_server_demo` - Handles window tracking and event publishing
- **Intelligent Client**: `grid_client_demo` - Real-time grid reconstruction and display
- **Live Synchronization**: Server publishes individual window details for efficient updates
- **Command Processing**: GetWindowList, GetGridState, window assignment commands
- **Background Monitoring**: Client receives real-time updates and maintains matching grid state

### 🎯 **NEW: Focus Event Tracking System**
- **Real-Time Focus Detection**: Track window focus/defocus events as they happen
- **Complete Event Coverage**: Both FOCUSED (gained focus) and DEFOCUSED (lost focus) events
- **Rich Event Data**: Process ID, window title, app hash, and precise timestamps
- **Multi-Client Support**: Up to 8 simultaneous focus event subscribers
- **Hash-Based Identification**: Efficient app and window identification for filtering
- **Production Ready**: Integrated into main server, no separate focus server needed

**Focus Event Types:**
- **FOCUSED (0)**: When a window gains focus (user clicks on it)
- **DEFOCUSED (1)**: When a window loses focus (user clicks elsewhere)
- **Window Details**: Process ID, window title, calculated hashes for identification
- **Timing**: Microsecond-precision timestamps for event ordering

### Smart Window Detection & Assignment
- **Coverage-Based Algorithm**: Only marks cells as occupied when windows cover ≥30% of cell area
- **Dual Assignment Modes**:
  - **Virtual Grid**: Assign windows using coordinates spanning all monitors
  - **Monitor-Specific**: Assign windows to specific cells on individual monitors
- **Precise Grid Representation**: Eliminates false positives from boundary overlaps
- **Configurable Thresholds**: Adjustable coverage percentage for different use cases

### Efficient Communication System
- **Event Publishing**: Server publishes CREATE/MOVE/DESTROY events for individual windows
- **On-Demand Data**: Client requests full window list only when needed
- **Incremental Updates**: Server sends only changed window details, not full state
- **Deadlock Prevention**: Minimal processing in system callbacks
- **High Performance**: Zero-copy data sharing via iceoryx2

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

### Quick Start - Event-Driven System

#### 1. Start the Server (Real-time Window Tracking)
```bash
cargo run --bin ipc_server_demo
```
This starts the main server that:
- Tracks all windows in real-time using WinEvents
- Publishes window details and grid state via IPC
- Shows periodic grid updates and window counts
- Handles client commands for window assignment
- Uses minimal WinEvent callbacks to prevent deadlocks
- Performs heavy processing in main server loop (every 2 seconds)

#### 2. Start the Client (Grid Visualization & Control)
```bash
# In a separate terminal
cargo run --bin grid_client_demo
```
This starts the intelligent client that:
- Automatically requests window data from server on startup
- Displays real-time grid updates as windows move
- Shows detailed window information and grid state
- Demonstrates efficient event-driven synchronization
- Uses throttled display updates to prevent UI flooding
- Implements non-blocking grid state checks with try_lock

#### 3. Legacy Interactive Demo
```bash
cargo run --bin ipc_demo
```
Original combined server/client for interactive window assignment.

## 🎯 Focus Event Tracking

E-Grid now includes comprehensive focus event tracking integrated directly into the main server. This allows applications to monitor window focus changes in real-time without needing a separate focus tracking server.

### Features

- **✅ Complete Focus Coverage**: Both FOCUSED and DEFOCUSED events
- **✅ Real-Time Detection**: Uses Windows WinEvent hooks for instant focus change detection  
- **✅ Rich Event Data**: Process ID, window title, app hash, timestamps
- **✅ Multi-Client Support**: Up to 8 simultaneous focus event subscribers
- **✅ Production Ready**: Integrated into main e_grid server infrastructure
- **✅ Hash-Based Filtering**: Efficient app and window identification

### Focus Event Structure

```rust
pub struct WindowFocusEvent {
    pub event_type: u8,           // 0 = FOCUSED, 1 = DEFOCUSED
    pub hwnd: u64,               // Window handle
    pub process_id: u32,         // Process ID
    pub timestamp: u64,          // Unix timestamp
    pub app_name_hash: u64,      // Hash of "Process_{pid}" for identification
    pub window_title_hash: u64,  // Hash of window title for identification
    pub reserved: [u8; 2],       // Future expansion
}
```

### Quick Start - Focus Tracking

#### Method 1: Using Main Server (Recommended)
```bash
# Terminal 1: Start main server with focus events
cargo run --bin ipc_server_demo

# Terminal 2: Start focus demo client  
cargo run --example simple_focus_demo
```

#### Method 2: Using Test Scripts
```bash
# Windows - Automated setup
test_focus_defocus.bat

# Or comprehensive integration test
test_focus_integration.bat
```

### Focus Event Examples

The system includes several focus tracking examples:

- **`simple_focus_demo`**: Basic focus event monitoring with clear output
- **`comprehensive_focus_demo`**: Advanced focus tracking with filtering and statistics
- **`focus_tracking_demo`**: Demonstrates focus event callback patterns
- **`focus_music_demo`**: Example integration with MIDI/music applications

### Integration with Applications

```rust
use e_grid::GridClient;

let mut client = GridClient::new()?;

// Set up focus callback
client.set_focus_callback(|focus_event| {
    match focus_event.event_type {
        0 => println!("Window {} gained focus", focus_event.hwnd),
        1 => println!("Window {} lost focus", focus_event.hwnd),
        _ => {}
    }
})?;

// Start monitoring
client.start_background_monitoring()?;
```


**Key Architecture Features:**
- **🎯 NEW: Focus Event Integration**: Real-time focus/defocus event publishing
- **Deadlock Prevention**: WinEvent callbacks only log events, no lock acquisition
- **Main Loop Processing**: All heavy work done in server main loop (every 2 seconds)
- **Multi-Service IPC**: Separate channels for different event types
- **Non-blocking Client**: Uses try_lock and display throttling for responsive UI
- **Event-Driven Updates**: Server publishes incremental changes, not full state dumps
- **Efficient IPC**: Large buffer sizes (64KB) prevent message loss

## 📡 IPC Services

The e_grid server provides multiple IPC services for different types of communication:

| Service | Purpose | Message Type | Description |
|---------|---------|--------------|-------------|
| **Grid Events** | Window lifecycle | `WindowEvent` | Window creation, destruction, movement |
| **Window Details** | Window information | `WindowDetails` | Position, size, grid coordinates, titles |
| **🎯 Focus Events** | Focus tracking | `WindowFocusEvent` | Focus/defocus with process info ⭐ |
| **Commands** | Client requests | `WindowCommand` | Window assignment, grid requests |
| **Responses** | Server replies | `WindowResponse` | Command acknowledgments, data |
| **Layout** | Grid layouts | `GridLayoutMessage` | Save/restore window arrangements |
| **Animations** | Window animations | `AnimationCommand` | Smooth window transitions |

**Multi-Client Support**: Each service supports up to 8 concurrent subscribers with individual message buffers.

## 🛠️ Available Demos & Tools

### Server Applications
```bash
# Main server (recommended for all use cases)
cargo run --bin ipc_server_demo

# Legacy interactive server/client combo
cargo run --bin ipc_demo
```

### Grid Client Applications  
```bash
# Real-time grid visualization
cargo run --bin grid_client_demo

# Enhanced grid client with better error handling
cargo run --example enhanced_grid_client

# Robust grid client with reconnection
cargo run --example robust_grid_client
```

### 🎯 Focus Tracking Applications
```bash
# Simple focus event monitoring (great for testing)
cargo run --example simple_focus_demo

# Comprehensive focus tracking with filtering
cargo run --example comprehensive_focus_demo

# Focus event callback patterns
cargo run --example focus_tracking_demo

# Focus integration with music/MIDI applications
cargo run --example focus_music_demo

# Focus callback demonstration
cargo run --example focus_callback_example
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

## 🎯 Focus Event Tracking Examples

### Simple Focus Demo (`simple_focus_demo`)
**Perfect for testing and learning focus events**

```bash
cargo run --example simple_focus_demo
```

**Output Example:**
```
🎯 Simple Focus Event Demo
==========================
🟢 FOCUSED - Window: 123456 (PID: 5678) at timestamp: 1640995200
   📱 App Hash: 0x8a2f3c1b5d4e6789
   🪟 Title Hash: 0x1b2c3d4e5f6a7890
   ─────────────────────────────

🔴 DEFOCUSED - Window: 123456 (PID: 5678) at timestamp: 1640995205
   📱 App Hash: 0x8a2f3c1b5d4e6789
   🪟 Title Hash: 0x1b2c3d4e5f6a7890
   ─────────────────────────────
```

### Comprehensive Focus Demo (`comprehensive_focus_demo`)
**Advanced focus tracking with filtering and statistics**

- Process-based filtering
- Focus duration tracking
- Application switching patterns
- Statistical analysis

### Focus Music Demo (`focus_music_demo`)
**Integration example for music/MIDI applications**

- Demonstrates focus event integration with audio applications
- Shows how to use app hash filtering
- Perfect template for e_midi integration

### Focus Tracking Architecture

```
Windows Focus Event
         ↓
EVENT_SYSTEM_FOREGROUND (WinEvent)
         ↓
GridIpcServer::handle_window_event()
         ↓
1. Send DEFOCUSED for previous window
2. Update last_focused_window tracking  
3. Send FOCUSED for current window
         ↓
GRID_FOCUS_EVENTS_SERVICE (IPC)
         ↓
Client Applications (up to 8 simultaneous)
```

## 🖥️ Server Commands

The server provides these interactive commands:

- **`g`** / **`grid`** - Display all monitor grids
- **`r`** / **`rescan`** - Force rescan of all windows
- **`e`** / **`event`** - Publish demo IPC event
- **`c`** / **`commands`** - Process demo IPC commands
- **`h`** / **`help`** - Show available commands
- **`q`** / **`quit`** - Exit the server


## 🎬 Event-Driven Comprehensive Demo

The `test_event_driven_demo` showcases E-Grid's event-driven architecture with real-time window management:

### Key Features
- **No Polling**: Uses Windows event hooks (SetWinEventHook) for true real-time window detection
- **Extensible Callbacks**: Event system supports multiple callbacks for window events
- **IPC Integration**: Demonstrates server/client communication for grid commands
- **Main-Thread Safety**: All WinEvent processing on main thread to avoid HWND issues
- **Manageable Window Filtering**: Only shows windows that can be meaningfully managed

### Demo Phases
1. **Animated Grid Layouts**: IPC commands to arrange windows in 2x2 grid with animations
2. **Dynamic Window Rotation**: Windows rotate through grid positions with smooth transitions
3. **Real-time Event Monitoring**: 30-second live monitoring of window create/move/destroy events

### Running The Demo
```bash
# Build and run the event-driven demo
cargo run --bin test_event_driven_demo

# Or use the convenient batch file
run_event_driven_demo.bat
```

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

NOTICE: This repository is in an interesting state;  the examples and functionality may or may not be implemented.
Have a look at the [e_midi](https://crates.io/crates/e_midi) which includes an example of playing a midi sound using the focus and defocused events.
Aside from that; Feel free to take a look at the LLM cruft, it will give you an idea of some of the directions this project could go.

Dave Horner 6/25 MIT/Apache License
