# E-Grid IPC Protocol Guide

## Overview
This document describes the recommended structure for IPC commands and responses between the E-Grid server and clients. The goal is to make the protocol explicit, extensible, and robust for real-time grid, monitor, and window management.

---

## Command Types

### Enum: `IpcCommandType`
```
pub enum IpcCommandType {
    GetGridState,
    GetMonitorList,
    GetWindowList,
    MoveWindow,
    FocusWindow,
    // ...add more as needed
}
```

### Struct: `IpcCommand`
```
pub struct IpcCommand {
    pub command_type: IpcCommandType,
    pub hwnd: Option<u64>,
    pub target_row: Option<u32>,
    pub target_col: Option<u32>,
    pub monitor_id: Option<u32>,
    pub layout_id: Option<u32>,
    pub animation_duration_ms: Option<u32>,
    pub easing_type: Option<u8>,
    // Add more fields as needed for future extensibility
}
```

---

## Response Types

### Enum: `IpcResponseType`
```
pub enum IpcResponseType {
    GridState,
    MonitorList,
    WindowList,
    Ack,
    Error,
    // ...
}
```

### Struct: `IpcResponse`
```
pub struct IpcResponse {
    pub response_type: IpcResponseType,
    pub grid_state: Option<GridState>,
    pub monitor_list: Option<Vec<MonitorInfo>>,
    pub window_list: Option<Vec<WindowInfo>>,
    pub error_message: Option<String>,
    // ...
}
```

---

## Example Flows

### 1. Client Requests Grid State
- **Client:** `IpcCommand { command_type: GetGridState, ... }`
- **Server:** `IpcResponse { response_type: GridState, grid_state: Some(...), ... }`

### 2. Client Requests Monitor List
- **Client:** `IpcCommand { command_type: GetMonitorList, ... }`
- **Server:** `IpcResponse { response_type: MonitorList, monitor_list: Some(...), ... }`

### 3. Client Moves a Window
- **Client:** `IpcCommand { command_type: MoveWindow, hwnd: Some(...), target_row: Some(...), ... }`
- **Server:** `IpcResponse { response_type: Ack, ... }` or `IpcResponse { response_type: Error, error_message: Some(...) }`

---

## Versioning
- Add a `protocol_version: u32` field to all commands and responses for compatibility.

---

## Best Practices
- Always match on `command_type`/`response_type` in your code.
- Use `Option` fields for extensibility.
- Log all IPC traffic for debugging.
- Document new commands/responses in this guide.

---

## Testing
- Unit test each command/response type.
- Integration test full request/response flows.
- Fuzz test with invalid/unknown command types.

---

## See Also
- `src/ipc.rs`, `src/ipc_client.rs`, `src/ipc_server.rs` for implementation details.
