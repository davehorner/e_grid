//! IPC Protocol types for E-Grid

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
// Add ZeroCopySend and repr(C) for iceoryx2 compatibility
use iceoryx2::prelude::ZeroCopySend;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub enum IpcCommandType {
    GetGridState,
    GetMonitorList,
    GetWindowList,
    MoveWindow,
    FocusWindow,
    AnimateWindow,
    AssignToVirtualCell,
    AssignToMonitorCell,
    // Add any other variants needed by client/server
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct IpcCommand {
    pub command_type: IpcCommandType,
    pub hwnd: Option<u64>,
    pub target_row: Option<u32>,
    pub target_col: Option<u32>,
    pub monitor_id: Option<u32>,
    pub layout_id: Option<u32>,
    pub animation_duration_ms: Option<u32>,
    pub easing_type: Option<u8>,
    pub protocol_version: u32,
}
unsafe impl ZeroCopySend for IpcCommand {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub enum IpcResponseType {
    GridState,
    MonitorList,
    WindowList,
    Ack,
    Error,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct IpcResponse {
    pub response_type: IpcResponseType,
    pub grid_state: Option<GridState>,
    pub monitor_list: Option<MonitorList>,
    pub window_list: Option<Vec<crate::grid::WindowInfo>>,
    pub error_message: Option<String>,
    pub protocol_version: u32,
}
unsafe impl ZeroCopySend for IpcResponse {}

// // Dummy types for illustration; replace with real ones from your codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridState;
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct MonitorInfo;
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct WindowInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub enum GridType {
    Physical,
    Virtual,
    Dynamic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorGridInfo {
    pub id: u32,
    pub grid_type: GridType,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
    pub rows: u32,
    pub cols: u32,
    pub name: Option<String>,
    pub grid: Vec<Vec<Option<u64>>>, // Grid of window handles
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorList {
    pub monitors: Vec<MonitorGridInfo>, // 0..N = physical, N+1 = virtual, N+2+ = dynamic
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientGridRequest {
    pub monitor_id: u32,
    pub rows: u32,
    pub cols: u32,
    pub grid_type: GridType,
    pub name: Option<String>,
}

// Service names for iceoryx2 communication
pub const GRID_EVENTS_SERVICE: &str = "e_grid_events";
pub const GRID_COMMANDS_SERVICE: &str = "e_grid_commands";
pub const GRID_RESPONSE_SERVICE: &str = "e_grid_responses";
pub const GRID_WINDOW_LIST_SERVICE: &str = "e_grid_window_list"; // Deprecated - chunked approach
pub const GRID_WINDOW_DETAILS_SERVICE: &str = "e_grid_window_details"; // Individual window details
pub const GRID_LAYOUT_SERVICE: &str = "e_grid_layouts"; // Grid layout transfer
pub const GRID_CELL_ASSIGNMENTS_SERVICE: &str = "e_grid_cell_assignments"; // Cell assignments for layouts
pub const ANIMATION_COMMANDS_SERVICE: &str = "e_grid_animations"; // Animation control
pub const ANIMATION_STATUS_SERVICE: &str = "e_grid_animation_status"; // Animation status updates
pub const GRID_FOCUS_EVENTS_SERVICE: &str = "e_grid_focus_events"; // Window focus/defocus events
pub const GRID_HEARTBEAT_SERVICE: &str = "e_grid_heartbeat"; // Server heartbeat messages

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_monitor_grid_info_serialization() {
        let info = MonitorGridInfo {
            id: 1,
            grid_type: GridType::Physical,
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
            rows: 8,
            cols: 12,
            name: Some("Primary".to_string()),
            grid: vec![vec![Some(123), None], vec![None, Some(456)]],
        };
        let json = serde_json::to_string(&info).unwrap();
        let de: MonitorGridInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info.id, de.id);
        assert_eq!(info.grid_type, de.grid_type);
        assert_eq!(info.width, de.width);
        assert_eq!(info.height, de.height);
        assert_eq!(info.x, de.x);
        assert_eq!(info.y, de.y);
        assert_eq!(info.rows, de.rows);
        assert_eq!(info.cols, de.cols);
        assert_eq!(info.name, de.name);
        assert_eq!(info.grid, de.grid);
    }

    #[test]
    fn test_monitor_list_roundtrip() {
        let grid = MonitorGridInfo {
            id: 2,
            grid_type: GridType::Virtual,
            width: 3840,
            height: 1080,
            x: 0,
            y: 0,
            rows: 8,
            cols: 24,
            name: Some("Virtual".to_string()),
            grid: vec![vec![None; 24]; 8],
        };
        let list = MonitorList {
            monitors: vec![grid],
        };
        let json = serde_json::to_string(&list).unwrap();
        let de: MonitorList = serde_json::from_str(&json).unwrap();
        assert_eq!(list.monitors.len(), de.monitors.len());
        assert_eq!(list.monitors[0].grid_type, de.monitors[0].grid_type);
    }

    #[test]
    fn test_client_grid_request_serialization() {
        let req = ClientGridRequest {
            monitor_id: 1,
            rows: 10,
            cols: 20,
            grid_type: GridType::Dynamic,
            name: Some("CustomGrid".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let de: ClientGridRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req.monitor_id, de.monitor_id);
        assert_eq!(req.rows, de.rows);
        assert_eq!(req.cols, de.cols);
        assert_eq!(req.grid_type, de.grid_type);
        assert_eq!(req.name, de.name);
    }
}

// Zero-copy compatible data types for iceoryx2
// Using only basic types that work with iceoryx2's zero-copy requirements

// Heartbeat message to keep client connection alive during idle periods
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct HeartbeatMessage {
    pub timestamp: u64,
    pub server_iteration: u64,
    pub uptime_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowEvent {
    pub event_type: u8, // 0=created, 1=destroyed, 2=moved, 3=state_changed, 4=move_start, 5=move_stop, 6=resize_start, 7=resize_stop
    pub hwnd: u64,
    pub row: u32,
    pub col: u32,
    pub old_row: u32,
    pub old_col: u32,
    pub timestamp: u64,
    pub total_windows: u32,
    pub occupied_cells: u32,
    // NEW: Enhanced position data for better grid sync
    pub grid_top_left_row: u32, // Grid coordinates (top-left corner)
    pub grid_top_left_col: u32,
    pub grid_bottom_right_row: u32, // Grid coordinates (bottom-right corner)
    pub grid_bottom_right_col: u32,
    pub real_x: i32, // Real window bounds
    pub real_y: i32,
    pub real_width: u32,
    pub real_height: u32,
    pub monitor_id: u32, // Which monitor this window is on
}

impl Default for WindowEvent {
    fn default() -> Self {
        Self {
            event_type: 0,
            hwnd: 0,
            row: 0,
            col: 0,
            old_row: 0,
            old_col: 0,
            timestamp: 0,
            total_windows: 0,
            occupied_cells: 0,
            grid_top_left_row: 0,
            grid_top_left_col: 0,
            grid_bottom_right_row: 0,
            grid_bottom_right_col: 0,
            real_x: 0,
            real_y: 0,
            real_width: 0,
            real_height: 0,
            monitor_id: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct WindowCommand {
    pub command_type: u8, // 0=move_window, 1=get_state, 2=get_windows, 3=assign_window_virtual, 4=assign_window_monitor, 5=apply_grid_layout, 6=save_layout, 7=get_layouts
    pub hwnd: u64,
    pub target_row: u32,
    pub target_col: u32,
    pub monitor_id: u32, // Monitor index for per-monitor assignment (ignored for virtual grid)
    pub layout_id: u32,  // Layout ID for grid operations
    pub animation_duration_ms: u32, // Animation duration in milliseconds
    pub easing_type: u8, // Easing function type
}
unsafe impl ZeroCopySend for WindowCommand {}

impl Default for WindowCommand {
    fn default() -> Self {
        Self {
            command_type: 0,
            hwnd: 0,
            target_row: 0,
            target_col: 0,
            monitor_id: 0,
            layout_id: 0,
            animation_duration_ms: 1000,
            easing_type: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowResponse {
    pub response_type: u8, // 0=success, 1=error, 2=window_list_count, 3=individual_window
    pub error_code: u32,
    pub window_count: u32,
    pub data: [u64; 4], // Generic data payload
}

impl Default for WindowResponse {
    fn default() -> Self {
        Self {
            response_type: 0,
            error_code: 0,
            window_count: 0,
            data: [0; 4],
        }
    }
}

// NEW: Focus event structure for e_midi integration
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowFocusEvent {
    pub event_type: u8,         // 0=focused, 1=defocused
    pub hwnd: u64,              // Window handle
    pub process_id: u32,        // Process ID
    pub timestamp: u64,         // Event timestamp
    pub app_name_hash: u64,     // Hash of application name for quick comparison
    pub window_title_hash: u64, // Hash of window title for quick comparison
    pub reserved: [u32; 2],     // Reserved for future use
}

impl Default for WindowFocusEvent {
    fn default() -> Self {
        Self {
            event_type: 0,
            hwnd: 0,
            process_id: 0,
            timestamp: 0,
            app_name_hash: 0,
            window_title_hash: 0,
            reserved: [0; 2],
        }
    }
}

// Individual window information with position data
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowPositionInfo {
    pub hwnd: u64,
    pub top_left_row: u32,
    pub top_left_col: u32,
    pub bottom_right_row: u32,
    pub bottom_right_col: u32,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

impl Default for WindowPositionInfo {
    fn default() -> Self {
        Self {
            hwnd: 0,
            top_left_row: 0,
            top_left_col: 0,
            bottom_right_row: 0,
            bottom_right_col: 0,
            width: 0,
            height: 0,
            x: 0,
            y: 0,
        }
    }
}

// Zero-copy compatible individual window information for IPC
// Based on the WindowInfo from lib.rs but optimized for IPC
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct WindowDetails {
    pub hwnd: u64,
    pub x: i32, // Window rectangle coordinates
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub virtual_row_start: u32, // Top-left grid position in virtual grid
    pub virtual_col_start: u32,
    pub virtual_row_end: u32, // Bottom-right grid position in virtual grid
    pub virtual_col_end: u32,
    pub monitor_id: u32,        // Which monitor this window is primarily on
    pub monitor_row_start: u32, // Top-left grid position in monitor grid
    pub monitor_col_start: u32,
    pub monitor_row_end: u32, // Bottom-right grid position in monitor grid
    pub monitor_col_end: u32,
    pub title_len: u32, // Length of title (for separate title transmission)
}

impl Default for WindowDetails {
    fn default() -> Self {
        Self {
            hwnd: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            virtual_row_start: 0,
            virtual_col_start: 0,
            virtual_row_end: 0,
            virtual_col_end: 0,
            monitor_id: 0,
            monitor_row_start: 0,
            monitor_col_start: 0,
            monitor_row_end: 0,
            monitor_col_end: 0,
            title_len: 0,
        }
    }
}

// Higher-level enum types for external API compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GridEvent {
    WindowCreated {
        hwnd: u64,
        title: String,
        row: usize,
        col: usize,
        // Enhanced position data
        grid_top_left_row: usize,
        grid_top_left_col: usize,
        grid_bottom_right_row: usize,
        grid_bottom_right_col: usize,
        real_x: i32,
        real_y: i32,
        real_width: u32,
        real_height: u32,
        monitor_id: u32,
    },
    WindowDestroyed {
        hwnd: u64,
        title: String,
    },
    WindowMoved {
        hwnd: u64,
        title: String,
        old_row: usize,
        old_col: usize,
        new_row: usize,
        new_col: usize,
        // Enhanced position data
        grid_top_left_row: usize,
        grid_top_left_col: usize,
        grid_bottom_right_row: usize,
        grid_bottom_right_col: usize,
        real_x: i32,
        real_y: i32,
        real_width: u32,
        real_height: u32,
        monitor_id: u32,
    },
    // NEW: Move tracking events for better client sync
    WindowMoveStart {
        hwnd: u64,
        title: String,
        current_row: usize,
        current_col: usize,
        grid_top_left_row: usize,
        grid_top_left_col: usize,
        grid_bottom_right_row: usize,
        grid_bottom_right_col: usize,
        real_x: i32,
        real_y: i32,
        real_width: u32,
        real_height: u32,
        monitor_id: u32,
    },
    WindowMoveStop {
        hwnd: u64,
        title: String,
        final_row: usize,
        final_col: usize,
        grid_top_left_row: usize,
        grid_top_left_col: usize,
        grid_bottom_right_row: usize,
        grid_bottom_right_col: usize,
        real_x: i32,
        real_y: i32,
        real_width: u32,
        real_height: u32,
        monitor_id: u32,
    },
    // NEW: Resize tracking events
    WindowResizeStart {
        hwnd: u64,
        title: String,
        current_row: usize,
        current_col: usize,
        grid_top_left_row: usize,
        grid_top_left_col: usize,
        grid_bottom_right_row: usize,
        grid_bottom_right_col: usize,
        real_x: i32,
        real_y: i32,
        real_width: u32,
        real_height: u32,
        monitor_id: u32,
    },
    WindowResizeStop {
        hwnd: u64,
        title: String,
        final_row: usize,
        final_col: usize,
        grid_top_left_row: usize,
        grid_top_left_col: usize,
        grid_bottom_right_row: usize,
        grid_bottom_right_col: usize,
        real_x: i32,
        real_y: i32,
        real_width: u32,
        real_height: u32,
        monitor_id: u32,
    },
    GridStateChanged {
        timestamp: u64,
        total_windows: usize,
        occupied_cells: usize,
    },
}

#[derive(Debug, Clone, ZeroCopySend)]
#[repr(C)]
pub enum GridCommand {
    MoveWindowToCell {
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    },
    AssignWindowToVirtualCell {
        hwnd: u64,
        target_row: usize,
        target_col: usize,
    },
    AssignWindowToMonitorCell {
        hwnd: u64,
        target_row: usize,
        target_col: usize,
        monitor_id: usize,
    },
    ApplyGridLayout {
        layout_name: [u8; 64], // Fixed-size byte array for zero-copy compatibility
        duration_ms: u32,
        easing_type: crate::EasingType,
    },
    SaveCurrentLayout {
        layout_name: [u8; 64], // Fixed-size byte array for zero-copy compatibility
    },
    GetSavedLayouts,
    StartAnimation {
        hwnd: u64,
        target_x: i32,
        target_y: i32,
        target_width: u32,
        target_height: u32,
        duration_ms: u32,
        easing_type: crate::EasingType,
    },
    GetAnimationStatus {
        hwnd: u64, // 0 for all windows
    },
    GetGridState,
    GetGridConfig,
    GetWindowList,
}

#[derive(Debug, Clone)]
pub enum GridResponse {
    Success,
    Error(String),
    WindowList {
        windows: Vec<crate::grid::WindowInfo>,
    },
    GridState {
        total_windows: usize,
        occupied_cells: usize,
        grid_summary: String,
    },
    GridConfig(crate::grid::GridConfig),
    SavedLayouts {
        layout_names: Vec<String>,
    },
    AnimationStatus {
        statuses: Vec<(u64, bool, f32)>, // (hwnd, is_active, progress)
    },
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct WindowInfo {
//     pub hwnd: u64,
//     pub title: String,
//     pub x: i32,
//     pub y: i32,
//     pub grid_cells: Vec<(usize, usize)>,
//     pub monitor_cells: HashMap<usize, Vec<(usize, usize)>>, // For individual monitor grids
//     pub width: i32,
//     pub height: i32,
// }

// Grid Layout Transfer - Compact representation of grid state
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct GridLayoutMessage {
    pub message_type: u8, // 0=apply_layout, 1=save_current_layout, 2=get_saved_layouts
    pub layout_id: u32,   // Unique ID for this layout
    pub animation_duration_ms: u32, // Animation duration in milliseconds
    pub easing_type: u8,  // 0=Linear, 1=EaseIn, 2=EaseOut, 3=EaseInOut, 4=Bounce, 5=Elastic, 6=Back
    pub grid_rows: u8,    // Number of rows in the grid
    pub grid_cols: u8,    // Number of columns in the grid
    pub total_cells: u16, // Total number of cells with windows
    pub layout_name_hash: u64, // Hash of layout name for identification
}

impl Default for GridLayoutMessage {
    fn default() -> Self {
        let default_config = crate::GridConfig::default();
        Self {
            message_type: 0,
            layout_id: 0,
            animation_duration_ms: 1000, // Default 1 second
            easing_type: 0,              // Linear
            grid_rows: default_config.rows as u8,
            grid_cols: default_config.cols as u8,
            total_cells: 0,
            layout_name_hash: 0,
        }
    }
}

// Grid Cell Assignment - Individual cell data for layout transfer
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct GridCellAssignment {
    pub row: u8,
    pub col: u8,
    pub hwnd: u64,         // Window handle assigned to this cell (0 if empty)
    pub monitor_id: u8,    // Monitor ID for per-monitor layouts (255 for virtual grid)
    pub reserved: [u8; 5], // Padding for alignment
}

impl Default for GridCellAssignment {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            hwnd: 0,
            monitor_id: 255, // Default to virtual grid
            reserved: [0; 5],
        }
    }
}

// Animation Control Message
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct AnimationCommand {
    pub command_type: u8, // 0=start_animation, 1=stop_animation, 2=pause_animation, 3=resume_animation, 4=get_status
    pub hwnd: u64,        // Target window (0 for all windows)
    pub duration_ms: u32, // Animation duration in milliseconds
    pub easing_type: u8,  // Easing function type
    pub target_x: i32,    // Target X position
    pub target_y: i32,    // Target Y position
    pub target_width: u32, // Target width
    pub target_height: u32, // Target height
}

impl Default for AnimationCommand {
    fn default() -> Self {
        Self {
            command_type: 0,
            hwnd: 0,
            duration_ms: 1000,
            easing_type: 0,
            target_x: 0,
            target_y: 0,
            target_width: 0,
            target_height: 0,
        }
    }
}

// Animation Status Response
#[derive(Debug, Clone, Copy, PartialEq, ZeroCopySend)]
#[repr(C)]
pub struct AnimationStatus {
    pub hwnd: u64,
    pub is_active: u8,     // 1 if animation is active, 0 if not
    pub progress: u8,      // Animation progress (0-100)
    pub elapsed_ms: u32,   // Elapsed time in milliseconds
    pub remaining_ms: u32, // Remaining time in milliseconds
    pub current_x: i32,    // Current X position
    pub current_y: i32,    // Current Y position
    pub reserved: [u8; 8], // Padding for future use
}

impl Default for AnimationStatus {
    fn default() -> Self {
        Self {
            hwnd: 0,
            is_active: 0,
            progress: 0,
            elapsed_ms: 0,
            remaining_ms: 0,
            current_x: 0,
            current_y: 0,
            reserved: [0; 8],
        }
    }
}
