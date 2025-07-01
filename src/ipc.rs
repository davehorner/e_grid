use crate::{GridConfig, WindowTracker};
use iceoryx2::port::publisher::Publisher;
use iceoryx2::port::subscriber::Subscriber;
use iceoryx2::prelude::*;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::ptr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use winapi::shared::windef::HWND;

// Module definition for iceoryx2 service type
pub mod ipc {
    pub use iceoryx2::service::ipc::Service;
}

// Re-export protocol constants and types for compatibility
pub use crate::ipc_protocol::{
    WindowEvent,
    // ...add any other protocol types needed by downstream code...
    ANIMATION_COMMANDS_SERVICE,
    ANIMATION_STATUS_SERVICE,
    GRID_CELL_ASSIGNMENTS_SERVICE,
    GRID_COMMANDS_SERVICE,
    GRID_EVENTS_SERVICE,
    GRID_FOCUS_EVENTS_SERVICE,
    GRID_HEARTBEAT_SERVICE,
    GRID_LAYOUT_SERVICE,
    GRID_RESPONSE_SERVICE,
    GRID_WINDOW_DETAILS_SERVICE,
};
