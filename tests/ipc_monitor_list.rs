//! Integration test: server-client monitor list exchange
use e_grid::ipc_protocol::{GridCommand, IpcCommand, IpcCommandType, IpcResponseType};
use e_grid::ipc_server::GridIpcServer;
use e_grid::WindowTracker;
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use iceoryx2::service::ipc::Service;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Add this import for the serial attribute macro
// Iceoryx2 IPC services are process-wide singletons.
// If you try to create the same service (with the same name/type) more than once in a process,
// you get a ServiceInCorruptedState error.
use serial_test::serial;

fn setup_server_and_client() -> (
    GridIpcServer,
    Node<Service>,
    Publisher<Service, IpcCommand, ()>,
    Subscriber<Service, e_grid::ipc_protocol::IpcResponse, ()>,
) {
    let tracker = Arc::new(Mutex::new(WindowTracker::new()));
    let windows = {
        let tracker_guard = tracker.lock().unwrap();
        tracker_guard.windows.clone()
    };
    let mut server = GridIpcServer::new(tracker.clone(), Arc::new(windows)).unwrap();
    server.setup_services().unwrap();
    let node = NodeBuilder::new().create::<Service>().unwrap();
    let command_service = node
        .service_builder(&ServiceName::new(e_grid::ipc_protocol::GRID_COMMANDS_SERVICE).unwrap())
        .publish_subscribe::<IpcCommand>()
        .open_or_create()
        .unwrap();
    let command_publisher = command_service.publisher_builder().create().unwrap();
    let response_service = node
        .service_builder(&ServiceName::new(e_grid::ipc_protocol::GRID_RESPONSE_SERVICE).unwrap())
        .publish_subscribe::<e_grid::ipc_protocol::IpcResponse>()
        .open_or_create()
        .unwrap();
    let response_subscriber = response_service.subscriber_builder().create().unwrap();
    std::panic::set_hook(Box::new(|info| {
        println!("Panic occurred: {:?}", info);
        println!("{:?}", std::backtrace::Backtrace::capture());
    }));
    (server, node, command_publisher, response_subscriber)
}

#[test]
#[serial]
fn test_monitor_list_exchange() {
    let (mut server, _node, command_publisher, response_subscriber) = setup_server_and_client();
    // Send GetMonitorList command
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
    command_publisher.send_copy(cmd).unwrap();
    // Process one command in the server
    server.process_commands().unwrap();
    // Wait for response
    let mut got_response = false;
    for _ in 0..10 {
        if let Some(sample) = response_subscriber.receive().unwrap() {
            let resp = sample.clone();
            if resp.response_type == IpcResponseType::MonitorList {
                // let list = &resp.monitor_list;
                // assert!(
                //     !list.monitors.is_empty(),
                //     "Monitor list should not be empty"
                // );
                // let m = &list.monitors[0];
                // assert_eq!(m.grid_type as u8, 0, "First monitor should be Physical");
                got_response = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        got_response,
        "Did not receive MonitorList response from server"
    );
}

#[test]
#[serial]
fn test_error_response_on_unsupported_command() {
    let (mut server, _node, command_publisher, response_subscriber) = setup_server_and_client();
    // Send MoveWindow command with no hwnd (should be an error or Ack, depending on server logic)
    let cmd = IpcCommand {
        command_type: IpcCommandType::MoveWindow,
        hwnd: None,
        target_row: Some(0),
        target_col: Some(0),
        monitor_id: None,
        layout_id: None,
        animation_duration_ms: None,
        easing_type: None,
        protocol_version: 1,
    };
    command_publisher.send_copy(cmd).unwrap();
    // Process one command in the server
    server.process_commands().unwrap();
    // Wait for response
    let mut got_response = false;
    for _ in 0..10 {
        if let Some(sample) = response_subscriber.receive().unwrap() {
            let resp = sample.clone();
            if resp.response_type == IpcResponseType::Error
                || resp.response_type == IpcResponseType::Ack
            {
                got_response = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        got_response,
        "Did not receive error or ack response from server"
    );
}

#[test]
#[serial]
fn test_multiple_monitor_list_requests() {
    let (mut server, _node, command_publisher, response_subscriber) = setup_server_and_client();
    // Send GetMonitorList command twice
    for _ in 0..2 {
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
        command_publisher.send_copy(cmd).unwrap();
        server.process_commands().unwrap();
        let mut got_response = false;
        for _ in 0..10 {
            if let Some(sample) = response_subscriber.receive().unwrap() {
                let resp = sample.clone();
                if resp.response_type == IpcResponseType::MonitorList {
                    // let list = &resp.monitor_list;

                    
                    // assert!(
                    //     !list.monitors.is_empty(),
                    //     "Monitor list should not be empty"
                    // );
                    got_response = true;
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            got_response,
            "Did not receive MonitorList response from server"
        );
    }
}

