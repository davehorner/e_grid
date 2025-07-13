use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use ringbuf::wrap::Prod;
use ringbuf::HeapRb;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winapi::shared::minwindef::LPARAM;
use winapi::shared::windef::{HWND, RECT};
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
pub use crate::grid::GridConfig;
pub use crate::grid_client_config::GridClientConfig;
use crate::ipc_client::IpcCommand;
pub use crate::performance_monitor::{EventType, OperationTimer, PerformanceMonitor};
pub use crate::window::WindowInfo;
pub use crate::window_tracker::WindowTracker;
pub use grid::animation::EasingType;

// Import the heartbeat service module
pub mod heartbeat;
pub use heartbeat::HeartbeatService;

// Import window events module with unified hook management
pub mod window_events;
pub use window_events::{setup_window_events, WindowEventConfig};

// Coverage threshold: percentage of cell area that must be covered by window
// to consider the window as occupying that cell (0.0 to 1.0)
pub const COVERAGE_THRESHOLD: f32 = 0.01; // 30% coverage required
pub const MAX_WINDOW_GRID_CELLS: usize = 64;
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
    pub in_progress: AtomicBool, // Thread-safe atomic boolean
    pub last_rect: RECT,         // Track last known window rectangle
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
        let tracker = Arc::new(Self {
            states: states.clone(),
            timeout,
            event_queue: event_queue.clone(),
        });

        // Spawn the background thread with proper state management
        let states_ref = states.clone();
        let event_queue_thread = event_queue.clone();
        let timeout_duration = timeout;

        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let now = Instant::now();
            let mut to_stop = Vec::new();

            // Collect HWNDs that need stop events
            for entry in states_ref.iter() {
                if entry.in_progress.load(Ordering::Relaxed)
                    && now.duration_since(entry.last_event) > timeout_duration
                {
                    to_stop.push((*entry.key(), entry.last_type));
                }
            }

            // Send stop events for timed-out windows
            for (hwnd_val, last_type) in to_stop {
                // Only send stop event if still in progress (avoid race conditions)
                if let Some(entry) = states_ref.get(&hwnd_val) {
                    if entry.in_progress.load(Ordering::Relaxed) {
                        let stop_event = match last_type {
                            Some(MoveResizeEventType::MoveStart) => MoveResizeEventType::MoveStop,
                            Some(MoveResizeEventType::ResizeStart) => {
                                MoveResizeEventType::ResizeStop
                            }
                            Some(MoveResizeEventType::BothStart) => MoveResizeEventType::BothStop,
                            _ => MoveResizeEventType::MoveStop, // fallback
                        };

                        // Don't modify state here - let the main thread handle it
                        event_queue_thread.push((hwnd_val, stop_event));
                    }
                }
            }
        });

        tracker
    }

    pub unsafe fn update_event(
        _producer: &mut Prod<Arc<HeapRb<(isize, bool)>>>, // Keep parameter for compatibility but don't use
        states: &Arc<DashMap<isize, MoveResizeState>>,
        hwnd: HWND,
    ) {
        unsafe {
            if GetParent(hwnd).is_null() && WindowTracker::is_manageable_window(hwnd as u64) {
                let hwnd_val = hwnd as isize;
                let mut entry = states.entry(hwnd_val).or_insert(MoveResizeState {
                    last_event: Instant::now(),
                    in_progress: AtomicBool::new(false), // Initialize as atomic
                    last_rect: RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                    last_type: None,
                });

                // Update timestamp - but DON'T modify in_progress here
                let now = Instant::now();
                let was_in_progress = entry.in_progress.load(Ordering::Relaxed); // Atomic read
                let time_since_last = now.duration_since(entry.last_event);

                // Only update timestamp, don't modify in_progress state
                entry.last_event = now;

                // Only send START event if not already in progress AND enough time has passed
                if !was_in_progress || time_since_last > Duration::from_millis(100) {
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
                        "[MoveResizeTracker] Detected move/resize START for HWND={:?} [class='{}', title='{}'] - was_in_progress={}, time_since_last={:?}ms",
                        hwnd, class, title, was_in_progress, time_since_last.as_millis()
                    );

                    // CRITICAL FIX: Use global lock-free queue instead of ringbuf
                    GLOBAL_EVENT_QUEUE.push((hwnd_val, MoveResizeEventType::BothStart));
                    println!(
                        "[MoveResizeTracker] ✅ Pushed BothStart to global queue for HWND={:?}",
                        hwnd
                    );
                } else {
                    println!("[MoveResizeTracker] SUPPRESSING START event for HWND={:?} - was_in_progress={}, time_since_last={:?}ms", hwnd, was_in_progress, time_since_last.as_millis());
                }
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
    fn on_window_move_resize_start(&self, _hwnd: u64, _window_info: &WindowInfo) {}
    fn on_window_move_resize_stop(&self, _hwnd: u64, _window_info: &WindowInfo) {}
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
        tracker.add_window(hwnd as u64);
        //     // println!("  -> Added successfully");
        // } else {
        //     // println!("  -> Failed to add window");
        // }
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
    // New: Blacklist for problematic HWNDs that consistently fail rect retrieval
    pub blacklisted_hwnds: Arc<DashMap<HWND, std::time::Instant>>,
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

        Self {
            move_resize_tracker,
            windows,
            event_callbacks: Arc::new(DashMap::new()),
            event_queue,
            states,
            event_callback: None,
            move_resize_start_callbacks: Arc::new(DashMap::new()),
            move_resize_stop_callbacks: Arc::new(DashMap::new()),
            blacklisted_hwnds: Arc::new(DashMap::new()),
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
        // CRITICAL FIX: Process events from the global queue first
        while let Some((hwnd_val, event_type)) = GLOBAL_EVENT_QUEUE.pop() {
            println!(
                "[WindowEventSystem] 🔥 Processing global queue event: HWND={:?}, type={:?}",
                hwnd_val, event_type
            );
            self.event_queue.push((hwnd_val, event_type));
        }

        // Clean up old blacklisted HWNDs (older than 5 minutes)
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(300); // 5 minutes
        let to_remove: Vec<HWND> = self
            .blacklisted_hwnds
            .iter()
            .filter_map(|entry| {
                if now.duration_since(*entry.value()) > cleanup_threshold {
                    Some(*entry.key())
                } else {
                    None
                }
            })
            .collect();

        for hwnd in to_remove {
            self.blacklisted_hwnds.remove(&hwnd);
            println!(
                "[WindowEventSystem] Removed HWND={:?} from blacklist after cleanup",
                hwnd
            );
        }

        while let Some((hwnd_val, event_type)) = self.event_queue.pop() {
            let hwnd = hwnd_val as HWND;

            // Check blacklist first, before any processing
            if self.blacklisted_hwnds.contains_key(&hwnd) {
                continue;
            }

            // CRITICAL FIX: Properly handle state transitions with atomic operations
            let should_call_callbacks = if let Some(mut entry) = self.states.get_mut(&hwnd_val) {
                match event_type {
                    MoveResizeEventType::MoveStart
                    | MoveResizeEventType::ResizeStart
                    | MoveResizeEventType::BothStart => {
                        // Atomic read of current state
                        let current_in_progress = entry.in_progress.load(Ordering::Relaxed);
                        let time_since_last = now.duration_since(entry.last_event);
                        let is_timeout_reset = time_since_last > Duration::from_millis(500);

                        if current_in_progress && !is_timeout_reset {
                            println!("[WindowEventSystem] ❌ DUPLICATE START for HWND={:?}, event_type={:?} - currently in_progress={} (last event: {:?}ms ago)", 
                                hwnd, event_type, current_in_progress, time_since_last.as_millis());
                            false // Already in progress, skip duplicate start
                        } else {
                            if is_timeout_reset {
                                println!("[WindowEventSystem] 🔄 TIMEOUT RESET for HWND={:?} - treating as new operation", hwnd);
                            } else {
                                println!("[WindowEventSystem] 🔄 AFTER STOP for HWND={:?} - in_progress was {}, allowing new START", hwnd, current_in_progress);
                            }
                            // ATOMIC state update
                            entry.in_progress.store(true, Ordering::Relaxed);
                            entry.last_type = Some(event_type);
                            entry.last_event = now;
                            println!("[WindowEventSystem] ✅ Calling START callbacks for HWND={:?}, event_type={:?} - SET in_progress=true", hwnd, event_type);
                            true // Valid start transition
                        }
                    }
                    MoveResizeEventType::MoveStop
                    | MoveResizeEventType::ResizeStop
                    | MoveResizeEventType::BothStop => {
                        let current_in_progress = entry.in_progress.load(Ordering::Relaxed);
                        if !current_in_progress {
                            println!("[WindowEventSystem] ❌ STOP without START for HWND={:?}, event_type={:?} - in_progress={}", hwnd, event_type, current_in_progress);
                            false // Not in progress, skip duplicate stop
                        } else {
                            // ATOMIC state update
                            entry.in_progress.store(false, Ordering::Relaxed);
                            entry.last_type = Some(event_type);
                            entry.last_event = now;
                            println!("[WindowEventSystem] ✅ Calling STOP callbacks for HWND={:?}, event_type={:?} - SET in_progress=false", hwnd, event_type);
                            true // Valid stop transition
                        }
                    }
                }
            } else {
                // No state entry, only allow start events
                match event_type {
                    MoveResizeEventType::MoveStart
                    | MoveResizeEventType::ResizeStart
                    | MoveResizeEventType::BothStart => {
                        self.states.insert(
                            hwnd_val,
                            MoveResizeState {
                                last_event: now,
                                in_progress: AtomicBool::new(true), // Initialize as atomic
                                last_rect: RECT {
                                    left: 0,
                                    top: 0,
                                    right: 0,
                                    bottom: 0,
                                },
                                last_type: Some(event_type),
                            },
                        );
                        println!("[WindowEventSystem] ✅ NEW WINDOW: Calling START callbacks for HWND={:?}, event_type={:?}", hwnd, event_type);
                        true // Valid start transition
                    }
                    MoveResizeEventType::MoveStop
                    | MoveResizeEventType::ResizeStop
                    | MoveResizeEventType::BothStop => {
                        println!("[WindowEventSystem] ❌ INVALID: Can't stop what wasn't started for HWND={:?}", hwnd);
                        false // Can't stop what wasn't started
                    }
                }
            };

            if !should_call_callbacks {
                println!(
                    "[WindowEventSystem] ❌ SKIPPING callbacks for HWND={:?}, event_type={:?}",
                    hwnd, event_type
                );
                continue;
            }

            // Ensure window exists in tracking
            let window_exists = self.windows.get(&hwnd).is_some();
            if !window_exists {
                println!("[WindowEventSystem] Adding tracking for HWND={:?}", hwnd);
                if let Some(rect) = crate::WindowTracker::get_window_rect(hwnd as u64) {
                    let title = crate::WindowTracker::get_window_title(hwnd as u64);
                    let monitor_cells: std::collections::HashMap<usize, Vec<(usize, usize)>> =
                        std::collections::HashMap::new();
                    let process_id =
                        crate::WindowTracker::get_window_process_id(hwnd as u64).unwrap_or(0);
                    let class_name = crate::WindowTracker::get_window_class_name(hwnd as u64);
                    let is_visible = crate::WindowTracker::is_window_visible(hwnd as u64);
                    let is_minimized = crate::WindowTracker::is_window_minimized(hwnd as u64);
                    let is_maximized = crate::WindowTracker::is_window_maximized(hwnd as u64);
                    let window_info = crate::WindowInfo {
                        hwnd: hwnd as u64,
                        window_rect: window::info::RectWrapper(rect),
                        title: {
                            let mut title_buf = [0u16; 256];
                            let utf16: Vec<u16> = title.encode_utf16().collect();
                            let len = utf16.len().min(256);
                            title_buf[..len].copy_from_slice(&utf16[..len]);
                            title_buf
                        },
                        title_len: title.len() as u32,
                        monitor_ids: {
                            let mut arr = [0usize; 8];
                            let ids: Vec<usize> = monitor_cells.keys().cloned().collect();
                            for (i, id) in ids.iter().take(8).enumerate() {
                                arr[i] = *id;
                            }
                            arr
                        },
                        is_visible,
                        is_minimized,
                        is_maximized,
                        process_id,
                        class_name: {
                            let mut class_name_buf = [0u16; 256];
                            let utf16: Vec<u16> = class_name.encode_utf16().collect();
                            let len = utf16.len().min(256);
                            class_name_buf[..len].copy_from_slice(&utf16[..len]);
                            class_name_buf
                        },
                        class_name_len: class_name.len() as u32,
                        z_order: 0,
                    };
                    self.windows.insert(hwnd, window_info);
                } else {
                    println!(
                        "[WindowEventSystem] Could not get rect for HWND={:?}, adding to blacklist.",
                        hwnd
                    );
                    self.blacklisted_hwnds.insert(hwnd, Instant::now());
                    continue;
                }
            }

            // Call callbacks for valid state transitions
            if let Some(window_info) = self.windows.get(&hwnd) {
                use crate::MoveResizeEventType::*;
                match event_type {
                    MoveStart | ResizeStart | BothStart => {
                        println!(
                            "[WindowEventSystem] 🚀 Calling START callbacks for HWND={:?}",
                            hwnd
                        );
                        for cb in self.move_resize_start_callbacks.iter() {
                            cb.value()(hwnd, &*window_info);
                        }
                    }
                    MoveStop | ResizeStop | BothStop => {
                        println!(
                            "[WindowEventSystem] 🛑 Calling STOP callbacks for HWND={:?}",
                            hwnd
                        );
                        let is_focused_window = unsafe {
                            let fg_hwnd = winapi::um::winuser::GetForegroundWindow();
                            !fg_hwnd.is_null() && fg_hwnd == hwnd
                        };

                        if is_focused_window && matches!(event_type, ResizeStop) {
                            println!("🔥 RESIZE STOP for focused window 0x{:X}", hwnd as u64);
                        }

                        for cb in self.move_resize_stop_callbacks.iter() {
                            cb.value()(hwnd, &*window_info);
                        }
                    }
                }

                // CRITICAL: Publish to IPC if callback is set
                if let Some(ref cb) = self.event_callback {
                    use crate::ipc_protocol::GridEvent;
                    let title = String::from_utf16_lossy(
                        &window_info.title[..window_info.title_len as usize],
                    );
                    let grid_rect = crate::window::info::rect_to_grid_rect(
                        &window_info.window_rect,
                        &crate::GridConfig::default(),
                    );
                    let row = grid_rect.left;
                    let col = grid_rect.top;
                    let grid_top_left_row = row as usize;
                    let grid_top_left_col = col as usize;
                    let grid_bottom_right_row = grid_rect.bottom as usize;
                    let grid_bottom_right_col = grid_rect.right as usize;
                    let real_x = window_info.window_rect.left;
                    let real_y = window_info.window_rect.top;
                    let real_width = window_info.window_rect.right - window_info.window_rect.left;
                    let real_height = window_info.window_rect.bottom - window_info.window_rect.top;
                    let monitor_id = 0;

                    let event = match event_type {
                        MoveStart => {
                            println!("[SERVER] 📡 Publishing WindowMoveStart for HWND={:?}", hwnd);
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
                            println!("[SERVER] 📡 Publishing WindowMoveStop for HWND={:?}", hwnd);
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
                            println!(
                                "[SERVER] 📡 Publishing WindowResizeStart for HWND={:?}",
                                hwnd
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
                            println!(
                                "[SERVER] 📡 Publishing WindowResizeStop for HWND={:?}",
                                hwnd
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
                } else {
                    println!(
                        "[WindowEventSystem] ❌ NO IPC CALLBACK SET! Events will not be published!"
                    );
                }
            } else {
                println!("[WindowEventSystem] ❌ Event for unknown HWND={:?}", hwnd);
            }
        }
    }

    /// Check if an HWND can be safely processed (not blacklisted and has valid rect)
    pub fn is_hwnd_processable(&self, hwnd: HWND) -> bool {
        // Check if blacklisted
        if self.blacklisted_hwnds.contains_key(&hwnd) {
            return false;
        }

        // Check if we can get a valid rect
        crate::WindowTracker::get_window_rect(hwnd as u64).is_some()
    }

    /// Manually add an HWND to the blacklist
    pub fn blacklist_hwnd(&self, hwnd: HWND, reason: &str) {
        println!(
            "[WindowEventSystem] Blacklisting HWND={:?}, reason: {}",
            hwnd, reason
        );
        self.blacklisted_hwnds.insert(hwnd, Instant::now());
    }

    /// Remove an HWND from the blacklist
    pub fn unblacklist_hwnd(&self, hwnd: HWND) -> bool {
        if self.blacklisted_hwnds.remove(&hwnd).is_some() {
            println!("[WindowEventSystem] Removed HWND={:?} from blacklist", hwnd);
            true
        } else {
            false
        }
    }

    /// Get the number of blacklisted HWNDs
    pub fn blacklist_count(&self) -> usize {
        self.blacklisted_hwnds.len()
    }

    /// Check if a specific HWND is blacklisted
    pub fn is_blacklisted(&self, hwnd: HWND) -> bool {
        self.blacklisted_hwnds.contains_key(&hwnd)
    }

    /// Handle MoveWindowToCell command with animation
    pub fn handle_move_window_to_cell(
        &mut self,
        command: IpcCommand,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let hwnd = command.hwnd.ok_or("Missing HWND")?;
        let target_row = command.target_row.ok_or("Missing target row")?;
        let target_col = command.target_col.ok_or("Missing target col")?;
        let duration_ms = command.animation_duration_ms.unwrap_or(500);
        let easing_type = command.easing_type.unwrap_or(crate::EasingType::Linear);

        // Calculate target position based on grid
        let screen_width =
            unsafe { winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_CXSCREEN) };
        let screen_height =
            unsafe { winapi::um::winuser::GetSystemMetrics(winapi::um::winuser::SM_CYSCREEN) };

        // Assuming 8x12 grid (or get from config)
        let grid_rows = 8;
        let grid_cols = 12;

        let cell_width = screen_width / grid_cols;
        let cell_height = screen_height / grid_rows;

        let target_x = target_col as i32 * cell_width;
        let target_y = target_row as i32 * cell_height;
        let target_width = cell_width;
        let target_height = cell_height;

        // Move and resize window with animation
        let target_rect = winapi::shared::windef::RECT {
            left: target_x,
            top: target_y,
            right: target_x + target_width,
            bottom: target_y + target_height,
        };

        // Start the animation
        self.start_window_animation(
            hwnd,
            target_rect,
            std::time::Duration::from_millis(duration_ms as u64),
            easing_type,
        )?;

        Ok(())
    }

    /// Start animation for a window to move to target rectangle
    pub fn start_window_animation(
        &self,
        hwnd: u64,
        target_rect: RECT,
        duration: Duration,
        easing_type: EasingType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Get current window rectangle
        let current_rect = if let Some(rect) = crate::WindowTracker::get_window_rect(hwnd) {
            rect
        } else {
            return Err("Could not get current window rectangle".into());
        };

        // Create animation
        let animation =
            WindowAnimation::new(hwnd, current_rect, target_rect, duration, easing_type);

        // Store animation (you'll need to add an animations field to WindowEventSystem)
        // For now, just move the window directly
        unsafe {
            winapi::um::winuser::SetWindowPos(
                hwnd as HWND,
                std::ptr::null_mut(),
                target_rect.left,
                target_rect.top,
                target_rect.right - target_rect.left,
                target_rect.bottom - target_rect.top,
                winapi::um::winuser::SWP_NOZORDER | winapi::um::winuser::SWP_NOACTIVATE,
            );
        }

        Ok(())
    }
}

// Replace the mutex HashMap with a simple lock-free global queue
lazy_static::lazy_static! {
    static ref GLOBAL_EVENT_QUEUE: crossbeam_queue::SegQueue<(isize, MoveResizeEventType)> = crossbeam_queue::SegQueue::new();
}
