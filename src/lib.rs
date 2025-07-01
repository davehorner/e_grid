use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use ringbuf::wrap::{Cons, Prod};
use ringbuf::{traits::*, HeapRb};
use std::collections::HashMap;
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::*;

// Import our error handling module
pub mod grid_client_errors;
pub use grid_client_errors::{
    retry_with_backoff, safe_arc_lock, safe_lock, validate_grid_coordinates, GridClientError,
    GridClientResult, RetryConfig,
};

// Import the centralized grid display module
pub mod grid_display;

pub mod config;
pub mod display;
pub mod grid;
pub mod grid_client_config;
pub mod monitor;
pub mod performance_monitor;
pub mod util;
pub mod window;
use crate::grid::GridConfig;
pub use crate::grid_client_config::GridClientConfig;
pub use crate::performance_monitor::{EventType, OperationTimer, PerformanceMonitor};
pub use crate::window::WindowInfo;
pub use crate::window_tracker::WindowTracker;
pub use grid::animation::EasingType;

// Import the heartbeat service module
pub mod heartbeat;
pub use heartbeat::HeartbeatService;

// Import window events module with unified hook management
pub mod window_events;
pub use window_events::{cleanup_hooks, setup_window_events, WindowEventConfig};

// Coverage threshold: percentage of cell area that must be covered by window
// to consider the window as occupying that cell (0.0 to 1.0)
const COVERAGE_THRESHOLD: f32 = 0.3; // 30% coverage required

// Animation and Tweening System
// #[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
// pub enum EasingType {
//     Linear,        // Constant speed
//     EaseIn,        // Slow start, fast end
//     EaseOut,       // Fast start, slow end
//     EaseInOut,     // Slow start and end, fast middle
//     Bounce,        // Bouncing effect at the end
//     Elastic,       // Elastic/spring effect
//     Back,          // Slight overshoot then settle
// }

#[derive(Clone)]
pub struct WindowAnimation {
    pub hwnd: u64,
    pub start_rect: RECT,
    pub target_rect: RECT,
    pub start_time: Instant,
    pub duration: Duration,
    pub easing: EasingType,
    pub completed: bool,
}

impl WindowAnimation {
    pub fn new(
        hwnd: u64,
        start_rect: RECT,
        target_rect: RECT,
        duration: Duration,
        easing: EasingType,
    ) -> Self {
        Self {
            hwnd,
            start_rect,
            target_rect,
            start_time: Instant::now(),
            duration,
            easing,
            completed: false,
        }
    }

    pub fn get_current_rect(&self) -> RECT {
        if self.completed {
            return self.target_rect;
        }

        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            return self.target_rect;
        }

        let progress = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let eased_progress = self.apply_easing(progress);

        RECT {
            left: self.lerp(self.start_rect.left, self.target_rect.left, eased_progress),
            top: self.lerp(self.start_rect.top, self.target_rect.top, eased_progress),
            right: self.lerp(
                self.start_rect.right,
                self.target_rect.right,
                eased_progress,
            ),
            bottom: self.lerp(
                self.start_rect.bottom,
                self.target_rect.bottom,
                eased_progress,
            ),
        }
    }

    pub fn is_completed(&self) -> bool {
        self.completed || self.start_time.elapsed() >= self.duration
    }

    pub fn get_progress(&self) -> f32 {
        if self.completed {
            return 1.0;
        }

        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            1.0
        } else {
            elapsed.as_secs_f32() / self.duration.as_secs_f32()
        }
    }

    fn lerp(&self, start: i32, end: i32, t: f32) -> i32 {
        (start as f32 + (end - start) as f32 * t) as i32
    }

    pub fn apply_easing(&self, t: f32) -> f32 {
        match self.easing {
            EasingType::Linear => t,
            EasingType::EaseIn => t * t,
            EasingType::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            EasingType::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                }
            }
            EasingType::Bounce => {
                if t < 1.0 / 2.75 {
                    7.5625 * t * t
                } else if t < 2.0 / 2.75 {
                    let t = t - 1.5 / 2.75;
                    7.5625 * t * t + 0.75
                } else if t < 2.5 / 2.75 {
                    let t = t - 2.25 / 2.75;
                    7.5625 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / 2.75;
                    7.5625 * t * t + 0.984375
                }
            }
            EasingType::Elastic => {
                if t == 0.0 || t == 1.0 {
                    t
                } else {
                    let p = 0.3;
                    let s = p / 4.0;
                    -((2.0_f32).powf(10.0 * (t - 1.0))
                        * ((t - 1.0 - s) * (2.0 * std::f32::consts::PI) / p).sin())
                }
            }
            EasingType::Back => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
        }
    }
}

// Grid Layout for transferring complete grid states
#[derive(Clone, Debug)]
pub struct GridLayout {
    pub name: String,
    pub config: GridConfig,
    pub virtual_grid: Vec<Vec<Option<u64>>>,
    pub monitor_grids: Vec<MonitorGridLayout>,
    pub created_at: Instant,
}

#[derive(Clone, Debug)]
pub struct MonitorGridLayout {
    pub monitor_id: usize,
    pub config: GridConfig,
    pub grid: Vec<Vec<Option<u64>>>,
}

impl GridLayout {
    pub fn new(name: String) -> Self {
        Self::new_with_config(name, GridConfig::default())
    }

    pub fn new_with_config(name: String, config: GridConfig) -> Self {
        let virtual_grid = vec![vec![None; config.cols]; config.rows];
        Self {
            name,
            config,
            virtual_grid,
            monitor_grids: Vec::new(),
            created_at: Instant::now(),
        }
    }

    pub fn from_current_state(tracker: &WindowTracker, name: String) -> Self {
        let mut layout = Self::new_with_config(name, tracker.config.clone());

        // Extract virtual grid layout
        for row in 0..tracker.config.rows {
            for col in 0..tracker.config.cols {
                if let CellState::Occupied(hwnd) = tracker.grid[row][col] {
                    layout.virtual_grid[row][col] = Some(hwnd);
                }
            }
        }

        // Extract monitor grid layouts
        for monitor_grid in &tracker.monitor_grids {
            let mut monitor_layout = MonitorGridLayout {
                monitor_id: monitor_grid.monitor_id,
                config: monitor_grid.config.clone(),
                grid: vec![vec![None; monitor_grid.config.cols]; monitor_grid.config.rows],
            };

            for row in 0..monitor_grid.config.rows {
                for col in 0..monitor_grid.config.cols {
                    if let CellState::Occupied(hwnd) = monitor_grid.grid[row][col] {
                        monitor_layout.grid[row][col] = Some(hwnd);
                    }
                }
            }

            layout.monitor_grids.push(monitor_layout);
        }

        layout
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellState {
    Empty,         // No window (on-screen area)
    Occupied(u64), // Window present (now thread-safe)
    OffScreen,     // Off-screen area (outside actual monitor bounds)
}

// #[derive(Clone)]
// pub struct WindowInfo {
//     pub hwnd: HWND,
//     pub title: String,
//     pub rect: RECT,
//     pub grid_cells: Vec<(usize, usize)>, // For virtual grid
//     pub monitor_cells: HashMap<usize, Vec<(usize, usize)>>, // For individual monitor grids
// }

// Move/resize detection state (per window)
pub struct MoveResizeState {
    pub last_event: Instant,
    pub in_progress: bool,
    pub last_rect: RECT, // Track last known window rectangle
    pub last_type: Option<MoveResizeEventType>, // Track last event type
}

// Move/resize tracker (shared across threads)
pub struct MoveResizeTracker {
    pub states: Arc<DashMap<isize, MoveResizeState>>, // Use Arc for sharing
    pub timeout: Duration,
    pub event_queue: Arc<SegQueue<(isize, MoveResizeEventType)>>,
}

impl MoveResizeTracker {
    pub fn new(
        timeout: Duration,
        states: Arc<DashMap<isize, MoveResizeState>>,
        event_queue: Arc<SegQueue<(isize, MoveResizeEventType)>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            states: states.clone(),
            timeout,
            event_queue: event_queue.clone(),
        })
    }

    pub fn update_event(
        producer: &mut Prod<Arc<HeapRb<(isize, bool)>>>,
        states: &Arc<DashMap<isize, MoveResizeState>>,
        hwnd: HWND,
    ) {
        unsafe {
            if GetParent(hwnd).is_null()
                && window_tracker::WindowTracker::is_manageable_window(hwnd as u64)
            {
                let hwnd_val = hwnd as isize;
                let mut entry = states.entry(hwnd_val).or_insert(MoveResizeState {
                    last_event: Instant::now(),
                    in_progress: false,
                    last_rect: RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    }, // Initialize last_rect
                    last_type: None, // Initialize last_type
                });
                entry.last_event = Instant::now();
                if !entry.in_progress {
                    // Print class and title for debug
                    let mut class_buf = [0u16; 256];
                    let class_len =
                        GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
                    let class = if class_len > 0 {
                        String::from_utf16_lossy(&class_buf[..class_len as usize])
                    } else {
                        String::from("")
                    };
                    let title = WindowTracker::get_window_title(hwnd as u64);
                    println!(
                        "[MoveResizeTracker] Detected move/resize START for HWND={:?} [class='{}', title='{}']",
                        hwnd, class, title
                    );
                    entry.in_progress = true; // <-- Set before pushing event
                    let _ = producer.try_push((hwnd_val, true));
                }
            } else {
                // Debug: filtered out
                // let title = WindowTracker::get_window_title(hwnd);
                // println!("[MoveResizeTracker] Ignored HWND={:?} (not top-level or not manageable), title='{}'", hwnd, title);
            }
        }
    }
}

// Event callback system for WindowTracker
pub trait WindowEventCallback: Send + Sync {
    fn on_window_created(&self, hwnd: u64, window_info: &WindowInfo);
    fn on_window_destroyed(&self, hwnd: u64);
    fn on_window_moved(&self, hwnd: u64, window_info: &WindowInfo);
    fn on_window_activated(&self, hwnd: u64, window_info: &WindowInfo);
    fn on_window_minimized(&self, hwnd: u64);
    fn on_window_restored(&self, hwnd: u64, window_info: &WindowInfo);
    // New: Move/Resize start/stop events
    fn on_window_move_resize_start(&self, hwnd: u64, window_info: &WindowInfo) {}
    fn on_window_move_resize_stop(&self, hwnd: u64, window_info: &WindowInfo) {}
}

// Box wrapper for dynamic dispatch
pub type WindowEventCallbackBox = Box<dyn WindowEventCallback>;

// iceoryx2 IPC integration for command and control
pub mod ipc;
pub mod ipc_manager;
/// Protocol definitions and message types for IPC communication
pub mod ipc_protocol;
pub mod window_tracker;
// Client module for real-time grid reconstruction and monitoring
pub mod ipc_client;
pub use ipc_client::GridClient;
pub mod monitor_grid;

// Server module for IPC server functionality
pub mod ipc_server;
pub use crate::ipc_server::start_server;

// Window enumeration callback function
unsafe extern "system" fn enum_windows_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let tracker = &mut *(lparam as *mut WindowTracker);
    let counter = tracker.enum_counter.fetch_add(1, Ordering::SeqCst) + 1;

    if WindowTracker::is_manageable_window(hwnd as u64) {
        // let title = WindowTracker::get_window_title(hwnd);
        // println!("Checking window #{}: {}", counter,
        //     if title.is_empty() { "<No Title>" } else { &title });
        // println!("  -> Adding manageable window: {}", title);
        if tracker.add_window(hwnd as u64) {
            // println!("  -> Added successfully");
        } else {
            // println!("  -> Failed to add window");
        }
    }

    1 // Continue enumeration
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_tracker_creation() {
        let tracker = WindowTracker::new();
        assert_eq!(tracker.windows.len(), 0);
        assert_eq!(tracker.enum_counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_grid_config() {
        let config = GridConfig::default();
        assert_eq!(config.rows, 8);
        assert_eq!(config.cols, 12);
        assert_eq!(config.cell_count(), 96);

        let custom_config = GridConfig::new(4, 4);
        assert_eq!(custom_config.rows, 4);
        assert_eq!(custom_config.cols, 4);
        assert_eq!(custom_config.cell_count(), 16);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveResizeEventType {
    MoveStart,
    MoveStop,
    ResizeStart,
    ResizeStop,
    BothStart,
    BothStop,
}

pub struct WindowEventSystem {
    pub move_resize_tracker: Arc<MoveResizeTracker>,
    pub windows: Arc<DashMap<HWND, WindowInfo>>,
    pub event_callbacks: Arc<DashMap<usize, Arc<dyn WindowEventCallback>>>,
    pub event_queue: Arc<crossbeam_queue::SegQueue<(isize, MoveResizeEventType)>>,
    pub states: Arc<DashMap<isize, MoveResizeState>>,
    // Optional event callback for IPC publishing (GridEvent)
    pub event_callback: Option<Arc<dyn Fn(crate::ipc_protocol::GridEvent) + Send + Sync>>,
    // New: Separate callback registries for move/resize start and stop
    pub move_resize_start_callbacks:
        Arc<DashMap<usize, Arc<dyn Fn(HWND, &WindowInfo) + Send + Sync>>>,
    pub move_resize_stop_callbacks:
        Arc<DashMap<usize, Arc<dyn Fn(HWND, &WindowInfo) + Send + Sync>>>,
}

impl WindowEventSystem {
    pub fn new(windows: Arc<DashMap<HWND, WindowInfo>>) -> Self {
        let event_queue = Arc::new(crossbeam_queue::SegQueue::new());
        let states = Arc::new(DashMap::new());
        let move_resize_tracker = MoveResizeTracker::new(
            Duration::from_millis(200),
            states.clone(),
            event_queue.clone(),
        );
        // Spawn the background thread for move/resize stop detection, passing a reference to the event_queue
        let states_ref = states.clone();
        let event_queue_thread = event_queue.clone();
        let timeout = Duration::from_millis(300);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let now = Instant::now();
            let mut to_stop = Vec::new();
            for entry in states_ref.iter_mut() {
                if entry.in_progress && now.duration_since(entry.last_event) > timeout {
                    println!(
                        "[MoveResizeTracker] Detected move/resize STOP for HWND={:?}",
                        *entry.key()
                    );
                    // Do NOT set entry.in_progress = false here! Let main thread do it after delivering stop event.
                    to_stop.push(*entry.key());
                }
            }
            for hwnd_val in to_stop {
                if let Some(mut entry) = states_ref.get_mut(&hwnd_val) {
                    use crate::MoveResizeEventType::*;
                    let stop_event = match entry.last_type {
                        Some(MoveStart) => MoveStop,
                        Some(ResizeStart) => ResizeStop,
                        Some(BothStart) => BothStop,
                        _ => MoveStop, // fallback
                    };
                    event_queue_thread.push((hwnd_val, stop_event));
                    // Optionally update state here, or let main thread do it after event delivery
                    // entry.in_progress = false;
                    // entry.last_type = Some(stop_event);
                } else {
                    // If state is missing, fallback to MoveStop
                    event_queue_thread.push((hwnd_val, crate::MoveResizeEventType::MoveStop));
                }
            }
        });
        Self {
            move_resize_tracker,
            windows,
            event_callbacks: Arc::new(DashMap::new()),
            event_queue,
            states,
            event_callback: None,
            move_resize_start_callbacks: Arc::new(DashMap::new()),
            move_resize_stop_callbacks: Arc::new(DashMap::new()),
        }
    }

    // Set the event callback for IPC publishing
    pub fn set_event_callback<F>(&mut self, callback: F)
    where
        F: Fn(crate::ipc_protocol::GridEvent) + Send + Sync + 'static,
    {
        self.event_callback = Some(Arc::new(callback));
    }

    pub fn set_move_resize_start_callback<F>(&self, id: usize, callback: F)
    where
        F: Fn(HWND, &WindowInfo) + Send + Sync + 'static,
    {
        self.move_resize_start_callbacks
            .insert(id, Arc::new(callback));
    }

    pub fn set_move_resize_stop_callback<F>(&self, id: usize, callback: F)
    where
        F: Fn(HWND, &WindowInfo) + Send + Sync + 'static,
    {
        self.move_resize_stop_callbacks
            .insert(id, Arc::new(callback));
    }

    pub fn remove_move_resize_start_callback(&self, id: usize) {
        self.move_resize_start_callbacks.remove(&id);
    }
    pub fn remove_move_resize_stop_callback(&self, id: usize) {
        self.move_resize_stop_callbacks.remove(&id);
    }

    // Call this periodically from the main thread/event loop
    pub fn poll_move_resize_events(&mut self) {
        while let Some((hwnd_val, event_type)) = self.event_queue.pop() {
            let hwnd = hwnd_val as HWND;
            let window_exists = self.windows.get(&hwnd).is_some();
            if !window_exists {
                println!("[WindowEventSystem] Adding tracking for HWND={:?}", hwnd);
                // Lock-free add_window logic:
                if let Some(rect) = crate::WindowTracker::get_window_rect(hwnd as u64) {
                    let title = crate::WindowTracker::get_window_title(hwnd as u64);
                    let grid_cells = vec![]; // Optionally, call window_to_grid_cells if you have config
                    let monitor_cells = std::collections::HashMap::new();
                    let process_id =
                        crate::WindowTracker::get_window_process_id(hwnd as u64).unwrap_or(0);
                    let class_name = crate::WindowTracker::get_window_class_name(hwnd as u64);
                    let is_visible = crate::WindowTracker::is_window_visible(hwnd as u64);
                    let is_minimized = crate::WindowTracker::is_window_minimized(hwnd as u64);
                    let window_info = crate::WindowInfo {
                        hwnd: hwnd as u64,
                        title,
                        grid_cells,
                        monitor_cells,
                        rect,
                        is_visible,
                        is_minimized,
                        process_id,
                        class_name,
                    };
                    self.windows.insert(hwnd, window_info);
                } else {
                    println!(
                        "[WindowEventSystem] Could not get rect for HWND={:?}, skipping add.",
                        hwnd
                    );
                    continue;
                }
            }

            if let Some(window_info) = self.windows.get(&hwnd) {
                let mut should_call = false;
                use crate::MoveResizeEventType::*;
                match event_type {
                    MoveStart => {
                        for cb in self.move_resize_start_callbacks.iter() {
                            cb.value()(hwnd, &*window_info);
                        }
                    }
                    MoveStop => {
                        if let Some(mut entry) = self.states.get_mut(&hwnd_val) {
                            if entry.in_progress {
                                entry.in_progress = false;
                                should_call = true;
                            }
                        }
                        if should_call {
                            for cb in self.move_resize_stop_callbacks.iter() {
                                cb.value()(hwnd, &*window_info);
                            }
                        }
                    }
                    ResizeStart => {
                        for cb in self.move_resize_start_callbacks.iter() {
                            cb.value()(hwnd, &*window_info);
                        }
                    }
                    ResizeStop => {
                        if let Some(mut entry) = self.states.get_mut(&hwnd_val) {
                            if entry.in_progress {
                                entry.in_progress = false;
                                should_call = true;
                            }
                        }
                        if should_call {
                            for cb in self.move_resize_stop_callbacks.iter() {
                                cb.value()(hwnd, &*window_info);
                            }
                        }
                    }
                    BothStart => {
                        for cb in self.move_resize_start_callbacks.iter() {
                            cb.value()(hwnd, &*window_info);
                        }
                    }
                    BothStop => {
                        if let Some(mut entry) = self.states.get_mut(&hwnd_val) {
                            if entry.in_progress {
                                entry.in_progress = false;
                                should_call = true;
                            }
                        }
                        if should_call {
                            for cb in self.move_resize_stop_callbacks.iter() {
                                cb.value()(hwnd, &*window_info);
                            }
                        }
                    }
                }
                // Publish to IPC if callback is set
                if let Some(ref cb) = self.event_callback {
                    use crate::ipc_protocol::GridEvent;
                    let title = window_info.title.clone();
                    let (row, col) = window_info.grid_cells.get(0).cloned().unwrap_or((0, 0));
                    let grid_top_left_row = row;
                    let grid_top_left_col = col;
                    let grid_bottom_right_row = row;
                    let grid_bottom_right_col = col;
                    let real_x = window_info.rect.left;
                    let real_y = window_info.rect.top;
                    let real_width = window_info.rect.right - window_info.rect.left;
                    let real_height = window_info.rect.bottom - window_info.rect.top;
                    let monitor_id = 0; // TODO: fill with real monitor id if available
                    let event = match event_type {
                        MoveStart => {
                            log::debug!(
                                "[SERVER] Publishing WindowMoveStart: hwnd={:?} row={} col={}",
                                hwnd,
                                row,
                                col
                            );
                            GridEvent::WindowMoveStart {
                                hwnd: hwnd as u64,
                                title,
                                current_row: row,
                                current_col: col,
                                grid_top_left_row,
                                grid_top_left_col,
                                grid_bottom_right_row,
                                grid_bottom_right_col,
                                real_x,
                                real_y,
                                real_width: real_width.try_into().unwrap(),
                                real_height: real_height.try_into().unwrap(),
                                monitor_id,
                            }
                        }
                        MoveStop => {
                            log::debug!(
                                "[SERVER] Publishing WindowMoveStop: hwnd={:?} row={} col={}",
                                hwnd,
                                row,
                                col
                            );
                            GridEvent::WindowMoveStop {
                                hwnd: hwnd as u64,
                                title,
                                final_row: row,
                                final_col: col,
                                grid_top_left_row,
                                grid_top_left_col,
                                grid_bottom_right_row,
                                grid_bottom_right_col,
                                real_x,
                                real_y,
                                real_width: real_width.try_into().unwrap(),
                                real_height: real_height.try_into().unwrap(),
                                monitor_id,
                            }
                        }
                        ResizeStart | BothStart => {
                            log::debug!(
                                "[SERVER] Publishing WindowResizeStart: hwnd={:?} row={} col={}",
                                hwnd,
                                row,
                                col
                            );
                            GridEvent::WindowResizeStart {
                                hwnd: hwnd as u64,
                                title,
                                current_row: row,
                                current_col: col,
                                grid_top_left_row,
                                grid_top_left_col,
                                grid_bottom_right_row,
                                grid_bottom_right_col,
                                real_x,
                                real_y,
                                real_width: real_width.try_into().unwrap(),
                                real_height: real_height.try_into().unwrap(),
                                monitor_id,
                            }
                        }
                        ResizeStop | BothStop => {
                            log::debug!(
                                "[SERVER] Publishing WindowResizeStop: hwnd={:?} row={} col={}",
                                hwnd,
                                row,
                                col
                            );
                            GridEvent::WindowResizeStop {
                                hwnd: hwnd as u64,
                                title,
                                final_row: row,
                                final_col: col,
                                grid_top_left_row,
                                grid_top_left_col,
                                grid_bottom_right_row,
                                grid_bottom_right_col,
                                real_x,
                                real_y,
                                real_width: real_width.try_into().unwrap(),
                                real_height: real_height.try_into().unwrap(),
                                monitor_id,
                            }
                        }
                    };
                    cb(event);
                }
            } else {
                println!("DAVE [WindowEventSystem] Event for unknown HWND={:?}", hwnd);
            };
        }
    }
}
