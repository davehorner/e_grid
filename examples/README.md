# Focus Tracking Examples

This directory contains examples demonstrating the focus tracking capabilities of e_grid's GridClient. These examples show how applications can listen for window focus events and respond to them.

## Quick Start

**🚀 Want to see everything at once?** Use the demo scripts:

```bash
# Windows
focus_demo.bat

# Linux/macOS  
./focus_demo.sh
```

**Or use the main e_grid server (NEW - with integrated focus tracking):**

1. **Start the main server:**
   ```bash
   cargo run --bin ipc_server_demo    # Main server with focus events
   ```

2. **In another terminal, run a client:**
   ```bash
   cargo run --example simple_focus_demo
   ```

**Or run manually with the standalone focus server:**

1. **Start the focus demo server:**
   ```bash
   cargo run --example focus_demo_server
   ```

2. **In another terminal, run a client:**
   ```bash
   cargo run --example comprehensive_focus_demo
   ```

## 🖥️ Server Options

### Main E-Grid Server (`ipc_server_demo`) - **RECOMMENDED**

**✨ NEW: The main e_grid server now includes integrated focus tracking!**

**Features:**
- **Complete e_grid functionality** - window management, layouts, animations
- **Integrated focus tracking** - both FOCUSED and DEFOCUSED events
- **Production-ready** - full server with all IPC services
- **Real-time window events** - comprehensive window monitoring
- **Multi-client support** - up to 8 simultaneous clients per service

**Usage:**
```bash
cargo run --bin ipc_server_demo
```

**Focus Event Support:**
- ✅ **FOCUSED events** - when a window gains focus (click on it)
- ✅ **DEFOCUSED events** - when a window loses focus (click away)
- ✅ **Window details** - title, position, process info
- ✅ **Hash identification** - for app and window title matching

### Standalone Focus Demo Server (`focus_demo_server.rs`) - **FOR TESTING**

**⚠️ IMPORTANT: You must run this server first before running any focus tracking examples!**

The Focus Demo Server creates the IPC services that broadcast focus events to client examples. It monitors the Windows foreground window and publishes focus change events.

**Features:**
- **Real-time focus detection** using Windows API
- **IPC event broadcasting** via iceoryx2 services
- **Multiple client support** - many examples can connect simultaneously  
- **Graceful shutdown** with Ctrl+C handling
- **Event logging** showing focus changes as they happen

**Usage:**
```bash
cargo run --example focus_demo_server
```

**What it does:**
- Monitors the Windows foreground window for changes
- Publishes WindowFocusEvent messages when focus changes
- Provides window details and basic event information
- Enables all other focus tracking examples to work

**⚡ The server must be running for focus tracking examples to receive events!**

## Client Examples

```bash
# Windows
demo_focus_tracking.bat

# Linux/macOS  
./demo_focus_tracking.sh

# Or run directly:
cargo run --example comprehensive_focus_demo
## Client Examples

### 1. Simple Focus Demo (`simple_focus_demo.rs`)
A basic example that demonstrates the fundamental focus tracking functionality.

**Features:**
- Basic focus event listening
- Simple event logging
- Shows window HWND, process ID, and app hashes

**Usage:**
```bash
cargo run --example simple_focus_demo
```

**What it does:**
- Displays focus/defocus events as they happen
- Shows application and window title hashes
- Provides a clean, minimal interface for understanding focus events

### 2. Focus Tracking Demo (`focus_tracking_demo.rs`)
A comprehensive example with statistics and history tracking.

**Features:**
- Maintains focus event history
- Tracks current focused window
- Counts focus events per application
- Displays periodic statistics
- Shows top applications by focus count

**Usage:**
```bash
cargo run --example focus_tracking_demo
```

**What it does:**
- Tracks all focus events with timestamps
- Maintains statistics on application usage
- Shows which applications receive the most focus
- Displays recent focus history

### 3. Focus Music Demo (`focus_music_demo.rs`)
An interactive example that simulates music control based on focus events (like e_midi would use).

**Features:**
- Assigns "songs" to different applications
- Simulates music playback start/stop based on focus
- Shows action history
- Demonstrates practical application of focus events

**Usage:**
```bash
cargo run --example focus_music_demo
```

**What it does:**
- Automatically assigns a unique "song" to each application
- "Plays" the song when an application gains focus
- "Pauses" the song when the application loses focus
- Shows how e_midi could implement spatial music control

### 4. Comprehensive Focus Demo (`comprehensive_focus_demo.rs`) ⭐ **RECOMMENDED**
The ultimate focus tracking demonstration that combines all features into one comprehensive example.

**Features:**
- **Real-time event monitoring** with smart app identification
- **Statistical analysis** including focus counts, time tracking, and rankings
- **Music control simulation** with automatic song assignments (like e_midi)
- **Comprehensive reporting** with session summaries and detailed breakdowns
- **Interactive feedback** with different update intervals
- **Memory management** with automatic history cleanup

**Usage:**
```bash
cargo run --example comprehensive_focus_demo
```

**What it does:**
- Tracks focus events with enhanced real-time display
- Generates readable application names from hash values
- Assigns unique music themes to each application
- Tracks both focus count and focus time per application
- Provides periodic comprehensive reports
- Shows session-wide statistics and trends
- Demonstrates advanced focus callback usage patterns

## How Focus Tracking Works

The GridClient receives focus events from the e_grid server through IPC (Inter-Process Communication). These events contain:

- `hwnd`: Window handle
- `process_id`: Process ID of the application
- `event_type`: 0 for focused, 1 for defocused
- `app_name_hash`: Hash of the application name
- `window_title_hash`: Hash of the window title
- `timestamp`: When the event occurred

## Setting Up Focus Callbacks

All examples use the same basic pattern:

```rust
use e_grid::{GridClient, GridClientResult};

fn main() -> GridClientResult<()> {
    // Create grid client
    let mut grid_client = GridClient::new()?;
    
    // Register focus callback
    grid_client.set_focus_callback(|focus_event| {
        // Handle focus event
        if focus_event.event_type == 0 {
            println!("Window {} gained focus", focus_event.hwnd);
        } else {
            println!("Window {} lost focus", focus_event.hwnd);
        }
    })?;
    
    // Start monitoring
    grid_client.start_background_monitoring()?;
    
    // Keep running
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

## Testing the Examples

1. **Start the e_grid server** (if not already running)
2. **Run one of the examples**
3. **Switch between different applications** to generate focus events
4. **Watch the console output** to see how the events are processed

## Integration with e_midi

These examples demonstrate the foundation for e_midi integration:

- **Focus-based music control**: Start/stop music based on which application is focused
- **Application-specific songs**: Assign different music to different applications  
- **Seamless transitions**: Smooth music changes as you switch between applications
- **Spatial awareness**: Combine with window position data for spatial audio

## Error Handling

All examples use the improved error handling system:
- `GridClientResult<T>` for consistent error types
- Proper error propagation and logging
- Graceful degradation when services aren't available

## Requirements

- Windows OS (uses Windows API for focus detection)
- e_grid server running
- Rust with the required dependencies (see Cargo.toml)

## Next Steps

To integrate focus tracking into your own application:

1. Add e_grid as a dependency
2. Create a GridClient instance
3. Register your focus callback
4. Start background monitoring
5. Handle focus events in your callback

For e_midi specifically, you would:
1. Map applications to MIDI songs
2. Control playback based on focus events
3. Implement spatial audio based on window positions
4. Provide configuration for song assignments
