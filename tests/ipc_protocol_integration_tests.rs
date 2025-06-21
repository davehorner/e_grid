//! Integration tests for the E-Grid IPC protocol: simulate client/server request/response flows
use e_grid::ipc_protocol::*;

fn make_test_grid_state() -> GridState {
    // Replace with real construction as needed
    GridState {}
}

fn make_test_monitor_list() -> Vec<MonitorInfo> {
    vec![MonitorInfo {}, MonitorInfo {}]
}

fn make_test_window_list() -> Vec<WindowInfo> {
    vec![WindowInfo {}, WindowInfo {}]
}

#[test]
fn integration_grid_state_flow() {
    // Simulate client sending GetGridState
    let cmd = IpcCommand {
        command_type: IpcCommandType::GetGridState,
        hwnd: None,
        target_row: None,
        target_col: None,
        monitor_id: None,
        layout_id: None,
        animation_duration_ms: None,
        easing_type: None,
        protocol_version: 1,
    };
    // Server receives and responds
    let resp = IpcResponse {
        response_type: IpcResponseType::GridState,
        grid_state: Some(make_test_grid_state()),
        monitor_list: None,
        window_list: None,
        error_message: None,
        protocol_version: 1,
    };
    // Roundtrip serialization
    let cmd_bytes = bincode::serialize(&cmd).unwrap();
    let resp_bytes = bincode::serialize(&resp).unwrap();
    let cmd2: IpcCommand = bincode::deserialize(&cmd_bytes).unwrap();
    let resp2: IpcResponse = bincode::deserialize(&resp_bytes).unwrap();
    assert_eq!(cmd.command_type, cmd2.command_type);
    assert_eq!(resp.response_type, resp2.response_type);
}

#[test]
fn integration_monitor_list_flow() {
    let cmd = IpcCommand {
        command_type: IpcCommandType::GetMonitorList,
        hwnd: None,
        target_row: None,
        target_col: None,
        monitor_id: None,
        layout_id: None,
        animation_duration_ms: None,
        easing_type: None,
        protocol_version: 1,
    };
    let resp = IpcResponse {
        response_type: IpcResponseType::MonitorList,
        grid_state: None,
        monitor_list: Some(make_test_monitor_list()),
        window_list: None,
        error_message: None,
        protocol_version: 1,
    };
    let cmd_bytes = bincode::serialize(&cmd).unwrap();
    let resp_bytes = bincode::serialize(&resp).unwrap();
    let cmd2: IpcCommand = bincode::deserialize(&cmd_bytes).unwrap();
    let resp2: IpcResponse = bincode::deserialize(&resp_bytes).unwrap();
    assert_eq!(cmd.command_type, cmd2.command_type);
    assert_eq!(resp.response_type, resp2.response_type);
    assert!(resp2.monitor_list.is_some());
}

#[test]
fn integration_window_list_flow() {
    let cmd = IpcCommand {
        command_type: IpcCommandType::GetWindowList,
        hwnd: None,
        target_row: None,
        target_col: None,
        monitor_id: None,
        layout_id: None,
        animation_duration_ms: None,
        easing_type: None,
        protocol_version: 1,
    };
    let resp = IpcResponse {
        response_type: IpcResponseType::WindowList,
        grid_state: None,
        monitor_list: None,
        window_list: Some(make_test_window_list()),
        error_message: None,
        protocol_version: 1,
    };
    let cmd_bytes = bincode::serialize(&cmd).unwrap();
    let resp_bytes = bincode::serialize(&resp).unwrap();
    let cmd2: IpcCommand = bincode::deserialize(&cmd_bytes).unwrap();
    let resp2: IpcResponse = bincode::deserialize(&resp_bytes).unwrap();
    assert_eq!(cmd.command_type, cmd2.command_type);
    assert_eq!(resp.response_type, resp2.response_type);
    assert!(resp2.window_list.is_some());
}
