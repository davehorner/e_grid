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
    pub monitor_list: Option<MonitorList>,
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

// Re-export all event/data types and constants needed for IPC
pub use crate::ipc::{
    WindowDetails, WindowEvent, WindowFocusEvent, HeartbeatMessage,
    GRID_EVENTS_SERVICE, GRID_COMMANDS_SERVICE, GRID_RESPONSE_SERVICE, GRID_WINDOW_DETAILS_SERVICE, GRID_FOCUS_EVENTS_SERVICE, GRID_LAYOUT_SERVICE, GRID_CELL_ASSIGNMENTS_SERVICE, ANIMATION_COMMANDS_SERVICE, ANIMATION_STATUS_SERVICE, GRID_HEARTBEAT_SERVICE,
    AnimationCommand, AnimationStatus, GridLayoutMessage, GridCellAssignment, GridEvent
};

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
        let list = MonitorList { monitors: vec![grid] };
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
