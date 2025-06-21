//! IPC Protocol types for E-Grid

use serde::{Serialize, Deserialize};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct IpcResponse {
    pub response_type: IpcResponseType,
    pub grid_state: Option<GridState>,
    pub monitor_list: Option<Vec<MonitorInfo>>,
    pub window_list: Option<Vec<WindowInfo>>,
    pub error_message: Option<String>,
    pub protocol_version: u32,
}
unsafe impl ZeroCopySend for IpcResponse {}

// Dummy types for illustration; replace with real ones from your codebase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridState;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorInfo;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo;

// Re-export all event/data types and constants needed for IPC
pub use crate::ipc::{
    WindowDetails, WindowEvent, WindowFocusEvent, HeartbeatMessage,
    GRID_EVENTS_SERVICE, GRID_COMMANDS_SERVICE, GRID_RESPONSE_SERVICE, GRID_WINDOW_DETAILS_SERVICE, GRID_FOCUS_EVENTS_SERVICE, GRID_LAYOUT_SERVICE, GRID_CELL_ASSIGNMENTS_SERVICE, ANIMATION_COMMANDS_SERVICE, ANIMATION_STATUS_SERVICE, GRID_HEARTBEAT_SERVICE,
    AnimationCommand, AnimationStatus, GridLayoutMessage, GridCellAssignment, GridEvent
};
