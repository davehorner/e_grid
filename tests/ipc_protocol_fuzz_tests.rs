//! Fuzz tests for the E-Grid IPC protocol types
use e_grid::ipc_protocol::*;

#[test]
fn fuzz_invalid_command_type() {
    // Simulate a random/invalid byte stream for IpcCommandType
    let invalid_bytes = [0xFFu8];
    let result: Result<IpcCommandType, _> = bincode::deserialize(&invalid_bytes);
    assert!(result.is_err(), "Invalid enum discriminant should fail to deserialize");
}

#[test]
fn fuzz_invalid_command_struct() {
    // Simulate a random/invalid byte stream for IpcCommand
    let invalid_bytes = [0x00u8, 0xFF, 0xFF, 0xFF, 0xFF];
    let result: Result<IpcCommand, _> = bincode::deserialize(&invalid_bytes);
    assert!(result.is_err(), "Random bytes should not deserialize to a valid IpcCommand");
}

#[test]
fn fuzz_invalid_response_type() {
    let invalid_bytes = [0xFFu8];
    let result: Result<IpcResponseType, _> = bincode::deserialize(&invalid_bytes);
    assert!(result.is_err(), "Invalid enum discriminant should fail to deserialize");
}
