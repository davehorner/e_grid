//! Tests for the new IPC protocol types
use e_grid::ipc_protocol::*;

#[test]
fn test_command_serialization_roundtrip() {
    let cmd = IpcCommand {
        command_type: IpcCommandType::MoveWindow,
        hwnd: Some(0x1234),
        target_row: Some(2),
        target_col: Some(3),
        monitor_id: Some(1),
        layout_id: Some(42),
        animation_duration_ms: Some(250),
        easing_type: Some(1),
        protocol_version: 1,
    };
    let bytes = bincode::serialize(&cmd).unwrap();
    let decoded: IpcCommand = bincode::deserialize(&bytes).unwrap();
    assert_eq!(cmd, decoded);
}

#[test]
fn test_response_error() {
    let resp = IpcResponse {
        response_type: IpcResponseType::Error,
        grid_state: None,
        monitor_list: None,
        window_list: None,
        error_message: Some("Something went wrong".to_string()),
        protocol_version: 1,
    };
    let bytes = bincode::serialize(&resp).unwrap();
    let decoded: IpcResponse = bincode::deserialize(&bytes).unwrap();
    assert_eq!(resp.response_type, IpcResponseType::Error);
    assert!(resp.error_message.is_some());
    assert_eq!(resp, decoded);
}
