use crate::{WindowEventCallback, WindowInfo, WindowTracker};
use std::ptr;
use std::sync::{Arc, Mutex};
use winapi::shared::windef::HWND;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::{
    SetWinEventHook, UnhookWinEvent, CHILDID_SELF, EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY,
    EVENT_OBJECT_HIDE, EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT,
};

// Add the missing type aliases
type HWINEVENTHOOK = winapi::shared::windef::HWINEVENTHOOK;
type DWORD = winapi::shared::minwindef::DWORD;
type LONG = winapi::shared::ntdef::LONG;

/// Configuration for window events with optional callbacks
pub struct WindowEventConfig {
    pub tracker: Arc<Mutex<WindowTracker>>,
    pub focus_callback: Option<Box<dyn Fn(u64, bool) + Send + Sync>>, // hwnd, is_focused
    pub heartbeat_reset: Option<Box<dyn Fn() + Send + Sync>>,
    pub event_callback: Option<Box<dyn Fn(crate::ipc_protocol::GridEvent) + Send + Sync>>, // NEW: event publishing callback
    pub debug_mode: bool,
    // --- UPDATED: For move/resize tracking ---
    pub move_resize_event_queue:
        Option<Arc<crossbeam_queue::SegQueue<(isize, crate::MoveResizeEventType)>>>,
    pub move_resize_states: Option<Arc<dashmap::DashMap<isize, crate::MoveResizeState>>>,
}

impl WindowEventConfig {
    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Self {
        Self {
            tracker,
            focus_callback: None,
            heartbeat_reset: None,
            event_callback: None, // NEW
            debug_mode: false,
            move_resize_event_queue: None,
            move_resize_states: None,
        }
    }

    pub fn with_focus_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(u64, bool) + Send + Sync + 'static,
    {
        self.focus_callback = Some(Box::new(callback));
        self
    }

    pub fn with_heartbeat_reset<F>(mut self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.heartbeat_reset = Some(Box::new(callback));
        self
    }

    pub fn with_event_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(crate::ipc_protocol::GridEvent) + Send + Sync + 'static,
    {
        self.event_callback = Some(Box::new(callback));
        self
    }

    pub fn with_debug(mut self, enabled: bool) -> Self {
        self.debug_mode = enabled;
        self
    }
}

// Debug callback implementation
pub struct DebugEventCallback;

impl WindowEventCallback for DebugEventCallback {
    fn on_window_created(&self, hwnd: u64, window_info: &WindowInfo) {
        // Only show manageable windows in debug output
        if WindowTracker::is_manageable_window(hwnd) {
            println!("🔔 WINDOW EVENT: CREATED (Manageable)");
            let title_str = {
                // Convert &[u16; 256] to String, trimming at null terminator
                let nul_pos = window_info
                    .title
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(window_info.title.len());
                String::from_utf16_lossy(&window_info.title[..nul_pos])
            };
            println!(
                "   Window: {}",
                if title_str.is_empty() {
                    "<No Title>"
                } else {
                    &title_str
                }
            );
            println!("   HWND: {:?}", hwnd);
        }
    }

    fn on_window_destroyed(&self, hwnd: u64) {
        println!("🔔 WINDOW EVENT: DESTROYED");
        println!("   HWND: {:?}", hwnd);
        println!();
    }

    fn on_window_moved(&self, hwnd: u64, window_info: &WindowInfo) {
        // Only show manageable windows in debug output
        if WindowTracker::is_manageable_window(hwnd) {
            println!("🔔 WINDOW EVENT: MOVED/RESIZED (Manageable)");
            let title_str = {
                let nul_pos = window_info
                    .title
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(window_info.title.len());
                String::from_utf16_lossy(&window_info.title[..nul_pos])
            };
            println!(
                "   Window: {}",
                if title_str.is_empty() {
                    "<No Title>"
                } else {
                    &title_str
                }
            );
            println!("   HWND: {:?}", hwnd);
            println!();
        }
    }

    fn on_window_activated(&self, hwnd: u64, window_info: &WindowInfo) {
        // Only show manageable windows in debug output
        if WindowTracker::is_manageable_window(hwnd) {
            println!("🔔 WINDOW EVENT: ACTIVATED (Manageable)");
            let title_str = {
                // Convert &[u16; 256] to String, trimming at null terminator
                let nul_pos = window_info
                    .title
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(window_info.title.len());
                String::from_utf16_lossy(&window_info.title[..nul_pos])
            };
            println!(
                "   Window: {}",
                if title_str.is_empty() {
                    "<No Title>"
                } else {
                    &title_str
                }
            );
            println!("   HWND: {:?}", hwnd);
            println!();
        }
    }

    fn on_window_minimized(&self, hwnd: u64) {
        println!("🔔 WINDOW EVENT: MINIMIZED");
        println!("   HWND: {:?}", hwnd);
        println!();
    }

    fn on_window_restored(&self, hwnd: u64, window_info: &WindowInfo) {
        println!("🔔 WINDOW EVENT: RESTORED");
        let title_str = {
            // Convert &[u16; 256] to String, trimming at null terminator
            let nul_pos = window_info
                .title
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(window_info.title.len());
            String::from_utf16_lossy(&window_info.title[..nul_pos])
        };
        println!(
            "   Window: {}",
            if title_str.is_empty() {
                "<No Title>"
            } else {
                &title_str
            }
        );
        println!("   HWND: {:?}", hwnd);
        println!();
    }
}

// Global state for event hooks and configuration
// These are accessed only from the main thread and properly synchronized
static mut WINDOW_EVENT_CONFIG: Option<WindowEventConfig> = None;
static mut EVENT_HOOKS: Vec<winapi::shared::windef::HWINEVENTHOOK> = Vec::new();
static mut LAST_FOCUSED_WINDOW: Option<u64> = None;

// WinEvent hook procedure - unified event handling with optional callbacks
pub unsafe extern "system" fn win_event_proc(
    _h_winevent_hook: winapi::shared::windef::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _dw_event_thread: u32,
    _dw_ms_event_time: u32,
) {
    // Only process window-level events (not child objects)
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF || hwnd.is_null() {
        return;
    }

    // Get the global configuration
    let config = match WINDOW_EVENT_CONFIG.as_ref() {
        Some(config) => config,
        None => return, // No configuration available
    };

    // Always try to reset heartbeat if callback is available
    if let Some(ref heartbeat_reset) = config.heartbeat_reset {
        heartbeat_reset();
    }

    // Handle focus events specially
    if event == EVENT_SYSTEM_FOREGROUND {
        let hwnd_u64 = hwnd as u64;
        if !WindowTracker::is_manageable_window(hwnd_u64) {
            return;
        }
        if let Some(ref focus_callback) = config.focus_callback {
            // Send DEFOCUSED for previous window if it exists
            if let Some(prev_hwnd) = LAST_FOCUSED_WINDOW {
                if prev_hwnd != hwnd as u64 {
                    if config.debug_mode {
                        let class_name = WindowTracker::get_window_class(prev_hwnd);
                        let title = WindowTracker::get_window_title(prev_hwnd);
                        println!(
                            "🎯 Focus: DEFOCUSED HWND {:?} {class_name} {title}",
                            prev_hwnd
                        );
                    }
                    focus_callback(prev_hwnd, false); // false = DEFOCUSED
                }
            }

            // Update last focused window and send FOCUSED
            LAST_FOCUSED_WINDOW = Some(hwnd as u64);
            if config.debug_mode {
                let class_name = WindowTracker::get_window_class(hwnd as u64);
                let title = WindowTracker::get_window_title(hwnd as u64);
                println!("🎯 Focus: FOCUSED HWND {:?} {class_name} {title}", hwnd);
            }
            focus_callback(hwnd as u64, true); // true = FOCUSED
        }
    }

    // NEW: Handle create/destroy events with callbacks
    match event {
        EVENT_OBJECT_CREATE => {
            let hwnd_u64 = hwnd as u64;
            if WindowTracker::is_manageable_window(hwnd_u64) {
                println!("🆕 [WINEVENT] Window CREATED: HWND 0x{:X}", hwnd_u64);

                if let Some(ref callback) = config.event_callback {
                    let title = WindowTracker::get_window_title(hwnd_u64);
                    let event = crate::ipc_protocol::GridEvent::WindowCreated {
                        hwnd: hwnd_u64,
                        title,
                        row: 0,
                        col: 0,
                        grid_top_left_row: 0,
                        grid_top_left_col: 0,
                        grid_bottom_right_row: 0,
                        grid_bottom_right_col: 0,
                        real_x: 0,
                        real_y: 0,
                        real_width: 0,
                        real_height: 0,
                        monitor_id: 0,
                    };
                    callback(event);
                }
            }
        }
        EVENT_OBJECT_DESTROY => {
            let hwnd_u64 = hwnd as u64;

            // Check if the window is currently tracked as manageable before proceeding
            if let Ok(tracker) = config.tracker.lock() {
                if !tracker.windows.contains_key(&hwnd_u64) {
                    // Not a managed/tracked window, skip further processing
                    return;
                }
            }
            // Always send destroy events - let the main.rs event handler determine
            // if this was a manageable window using the tracking
            let class_name = WindowTracker::get_window_class_name(hwnd_u64);
            let title = WindowTracker::get_window_title(hwnd_u64);

            println!(
                "💀 [WINEVENT] Window DESTROYED: HWND 0x{:X} {class_name} {title}",
                hwnd_u64
            );

            if let Some(ref callback) = config.event_callback {
                let title = if title.is_empty() {
                    "(destroyed)".to_string()
                } else {
                    title
                };
                let event = crate::ipc_protocol::GridEvent::WindowDestroyed {
                    hwnd: hwnd_u64,
                    title,
                };
                callback(event);
            }
        }
        EVENT_OBJECT_SHOW => {
            let hwnd_u64 = hwnd as u64;
            if WindowTracker::is_manageable_window(hwnd_u64) {
                let class = WindowTracker::get_window_class(hwnd_u64);
                if class == "Windows.UI.Composition.DesktopWindowContentBridge" {
                    // Skip hidden windows that are part of the Windows UI framework
                    return;
                }
                println!("👁️ [WINEVENT] Window SHOWN: HWND 0x{:X}", hwnd_u64);

                if let Some(ref callback) = config.event_callback {
                    let title = WindowTracker::get_window_title(hwnd_u64);
                    let event = crate::ipc_protocol::GridEvent::WindowCreated {
                        hwnd: hwnd_u64,
                        title,
                        row: 0,
                        col: 0,
                        grid_top_left_row: 0,
                        grid_top_left_col: 0,
                        grid_bottom_right_row: 0,
                        grid_bottom_right_col: 0,
                        real_x: 0,
                        real_y: 0,
                        real_width: 0,
                        real_height: 0,
                        monitor_id: 0,
                    };
                    callback(event);
                }
            }
        }
        EVENT_OBJECT_HIDE => {
            let hwnd_u64 = hwnd as u64;
            if let Some(ref callback) = config.event_callback {
                if WindowTracker::is_manageable_window(hwnd_u64) {
                    println!("🙈 [WINEVENT] Window HIDDEN: HWND 0x{:X}", hwnd_u64);
                    let title = WindowTracker::get_window_title(hwnd_u64);
                    let event = crate::ipc_protocol::GridEvent::WindowDestroyed {
                        hwnd: hwnd_u64,
                        title,
                    };
                    callback(event);
                }
            }
        }
        _ => {
            // Handle other events (LOCATIONCHANGE, etc.) - existing code
        }
    }

    // Update window tracker - existing code for LOCATIONCHANGE, etc.
    if let Ok(mut tracker) = config.tracker.try_lock() {
        match event {
            EVENT_OBJECT_CREATE => {
                if WindowTracker::is_manageable_window(hwnd as u64) {
                    tracker.add_window(hwnd as u64);
                }
            }
            EVENT_OBJECT_DESTROY => {
                tracker.remove_window(hwnd as u64);
            }
            EVENT_OBJECT_LOCATIONCHANGE => {
                if WindowTracker::is_manageable_window(hwnd as u64) {
                    // Quick validation: check if we can get a valid rect before processing
                    if let Some(rect) = WindowTracker::get_window_rect(hwnd as u64) {
                        // Ensure rect is reasonable (not zero-sized or negative)
                        if rect.right > rect.left && rect.bottom > rect.top {
                            // Ensure window is tracked before updating
                            if !tracker.windows.contains_key(&(hwnd as u64)) {
                                tracker.add_window(hwnd as u64);
                            }
                            tracker.update_window(hwnd as u64);

                            // Re-enabled: Move/Resize tracking - feed directly into global queue
                            if let (Some(_event_queue), Some(states)) = (
                                config.move_resize_event_queue.as_ref(),
                                config.move_resize_states.as_ref(),
                            ) {
                                let hwnd_val = hwnd as isize;
                                if let Some(window_info) = tracker.windows.get(&(hwnd as u64)) {
                                    let mut entry =
                                        states.entry(hwnd_val).or_insert(crate::MoveResizeState {
                                            last_event: std::time::Instant::now(),
                                            in_progress: std::sync::atomic::AtomicBool::new(false),
                                            last_rect: window_info.window_rect.0,
                                            last_type: None,
                                        });

                                    let now = std::time::Instant::now();
                                    let prev_rect = entry.last_rect;
                                    let current_rect = window_info.window_rect.0;

                                    let moved = prev_rect.left != current_rect.left
                                        || prev_rect.top != current_rect.top;
                                    let resized = (prev_rect.right - prev_rect.left
                                        != current_rect.right - current_rect.left)
                                        || (prev_rect.bottom - prev_rect.top
                                            != current_rect.bottom - current_rect.top);

                                    use crate::MoveResizeEventType::*;

                                    if !entry.in_progress.load(std::sync::atomic::Ordering::Relaxed)
                                    {
                                        // Start detection - only if not already in progress
                                        if moved && !resized {
                                            println!("[MOVE/RESIZE] Gesture detected: MoveStart for HWND {:?}", hwnd);
                                            crate::GLOBAL_EVENT_QUEUE.push((hwnd_val, MoveStart));
                                            entry.last_type = Some(MoveStart);
                                        } else if resized && !moved {
                                            println!("[MOVE/RESIZE] Gesture detected: ResizeStart for HWND {:?}", hwnd);
                                            crate::GLOBAL_EVENT_QUEUE.push((hwnd_val, ResizeStart));
                                            entry.last_type = Some(ResizeStart);
                                        } else if moved && resized {
                                            println!("[MOVE/RESIZE] Gesture detected: BothStart for HWND {:?}", hwnd);
                                            crate::GLOBAL_EVENT_QUEUE.push((hwnd_val, BothStart));
                                            entry.last_type = Some(BothStart);
                                        }
                                    }

                                    // Update tracking state only if rect has changed
                                    if prev_rect.left != current_rect.left
                                        || prev_rect.top != current_rect.top
                                        || prev_rect.right != current_rect.right
                                        || prev_rect.bottom != current_rect.bottom
                                    {
                                        entry.last_event = now;
                                        entry.last_rect = current_rect;

                                        // Emit continuous move/resize events during gesture
                                        if entry.in_progress.load(std::sync::atomic::Ordering::Relaxed) {
                                            // Determine which event to emit
                                            if moved && !resized {
                                                // Continuous move
                                                if let Some(ref callback) = config.event_callback {
                                                    let title = WindowTracker::get_window_title(hwnd as u64);
                                                    let event = crate::ipc_protocol::GridEvent::WindowMoved {
                                                        hwnd: hwnd as u64,
                                                        title,
                                                        old_row: 0, // You may want to track previous row/col if needed
                                                        old_col: 0,
                                                        new_row: 0,
                                                        new_col: 0,
                                                        grid_top_left_row: 0,
                                                        grid_top_left_col: 0,
                                                        grid_bottom_right_row: 0,
                                                        grid_bottom_right_col: 0,
                                                        real_x: current_rect.left,
                                                        real_y: current_rect.top,
                                                        real_width: (current_rect.right - current_rect.left) as u32,
                                                        real_height: (current_rect.bottom - current_rect.top) as u32,
                                                        monitor_id: 0,
                                                    };
                                                    callback(event);
                                                }
                                            } else if resized && !moved {
                                                // Continuous resize
                                                if let Some(ref callback) = config.event_callback {
                                                    let title = WindowTracker::get_window_title(hwnd as u64);
                                                    let event = crate::ipc_protocol::GridEvent::WindowResize {
                                                        hwnd: hwnd as u64,
                                                        title,
                                                        grid_top_left_row: 0,
                                                        grid_top_left_col: 0,
                                                        grid_bottom_right_row: 0,
                                                        grid_bottom_right_col: 0,
                                                        real_x: current_rect.left,
                                                        real_y: current_rect.top,
                                                        real_width: (current_rect.right - current_rect.left) as u32,
                                                        real_height: (current_rect.bottom - current_rect.top) as u32,
                                                        monitor_id: 0,
                                                        old_width: (prev_rect.right - prev_rect.left) as u32,
                                                        old_height: (prev_rect.bottom - prev_rect.top) as u32,
                                                        new_width: (current_rect.right - current_rect.left) as u32,
                                                        new_height: (current_rect.bottom - current_rect.top) as u32,
                                                    };
                                                    callback(event);
                                                }
                                            } else if moved && resized {
                                                // Both move and resize
                                                if let Some(ref callback) = config.event_callback {
                                                    let title = WindowTracker::get_window_title(hwnd as u64);
                                                    let event = crate::ipc_protocol::GridEvent::WindowMoved {
                                                        hwnd: hwnd as u64,
                                                        title,
                                                        old_row: 0,
                                                        old_col: 0,
                                                        new_row: 0,
                                                        new_col: 0,
                                                        grid_top_left_row: 0,
                                                        grid_top_left_col: 0,
                                                        grid_bottom_right_row: 0,
                                                        grid_bottom_right_col: 0,
                                                        real_x: current_rect.left,
                                                        real_y: current_rect.top,
                                                        real_width: (current_rect.right - current_rect.left) as u32,
                                                        real_height: (current_rect.bottom - current_rect.top) as u32,
                                                        monitor_id: 0,
                                                    };
                                                    callback(event);
                                                }
                                                if let Some(ref callback) = config.event_callback {
                                                    let title = WindowTracker::get_window_title(hwnd as u64);
                                                    let event = crate::ipc_protocol::GridEvent::WindowResize {
                                                        hwnd: hwnd as u64,
                                                        title,
                                                        grid_top_left_row: 0,
                                                        grid_top_left_col: 0,
                                                        grid_bottom_right_row: 0,
                                                        grid_bottom_right_col: 0,
                                                        real_x: current_rect.left,
                                                        real_y: current_rect.top,
                                                        real_width: (current_rect.right - current_rect.left) as u32,
                                                        real_height: (current_rect.bottom - current_rect.top) as u32,
                                                        monitor_id: 0,
                                                        old_width: (prev_rect.right - prev_rect.left) as u32,
                                                        old_height: (prev_rect.bottom - prev_rect.top) as u32,
                                                        new_width: (current_rect.right - current_rect.left) as u32,
                                                        new_height: (current_rect.bottom - current_rect.top) as u32,
                                                    };
                                                    callback(event);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            EVENT_SYSTEM_MINIMIZESTART => {
                tracker.remove_window(hwnd as u64);
            }
            EVENT_SYSTEM_MINIMIZEEND => {
                if WindowTracker::is_manageable_window(hwnd as u64) {
                    tracker.add_window(hwnd as u64);
                }
            }
            _ => {} // Unhandled event types
        }
    } else if config.debug_mode {
        println!("🔍 Failed to lock tracker - might be busy");
    }
}

pub fn setup_window_events(config: WindowEventConfig) -> Result<(), String> {
    unsafe {
        // Store the configuration globally
        WINDOW_EVENT_CONFIG = Some(config);
        EVENT_HOOKS = Vec::new();
        LAST_FOCUSED_WINDOW = None;

        println!("🔧 Setting up unified WinEvent hooks...");

        // Set up hooks for different window events - UPDATED to include CREATE/DESTROY
        let events_to_hook = [
            (EVENT_OBJECT_CREATE, "Window Creation"),
            (EVENT_OBJECT_DESTROY, "Window Destruction"),
            (EVENT_OBJECT_SHOW, "Window Show"),
            (EVENT_OBJECT_HIDE, "Window Hide"),
            (EVENT_OBJECT_LOCATIONCHANGE, "Window Move/Resize"),
            (EVENT_SYSTEM_FOREGROUND, "Window Activation/Focus"),
            (EVENT_SYSTEM_MINIMIZESTART, "Window Minimize"),
            (EVENT_SYSTEM_MINIMIZEEND, "Window Restore"),
        ];

        for (event, description) in &events_to_hook {
            let hook = SetWinEventHook(
                *event,
                *event,
                ptr::null_mut(),
                Some(win_event_proc),
                0, // All processes
                0, // All threads
                WINEVENT_OUTOFCONTEXT,
            );

            if hook.is_null() {
                let error = GetLastError();
                println!(
                    "❌ Failed to set up hook for {}: error {}",
                    description, error
                );
            } else {
                EVENT_HOOKS.push(hook);
                println!("✅ Successfully set up hook for {}", description);
            }
        }

        let hooks_len = EVENT_HOOKS.len();
        if hooks_len == 0 {
            return Err("Failed to set up any event hooks".to_string());
        }

        // Display what features are enabled
        let config_ref = WINDOW_EVENT_CONFIG.as_ref().unwrap();
        println!("🚀 Successfully set up {} WinEvent hooks!", hooks_len);
        println!("📢 Features enabled:");
        println!("   • Window tracking: ✓");
        if config_ref.focus_callback.is_some() {
            println!("   • Focus event callbacks: ✓");
        }
        if config_ref.heartbeat_reset.is_some() {
            println!("   • Heartbeat reset: ✓");
        }
        if config_ref.debug_mode {
            println!("   • Debug logging: ✓");
        }
        println!();

        Ok(())
    }
}

pub fn cleanup_hooks() {
    unsafe {
        let len = EVENT_HOOKS.len();
        // Clean up hooks
        for i in 0..len {
            UnhookWinEvent(EVENT_HOOKS[i]);
        }
        EVENT_HOOKS.clear();

        // Clear state
        WINDOW_EVENT_CONFIG = None;
        LAST_FOCUSED_WINDOW = None;

        println!("🧹 Cleaned up all WinEvent hooks and state");
    }
}

/// Process Windows messages for WinEvent hooks
///
/// This function processes the Windows message queue, which is required for WinEvent hooks
/// to function properly. It should be called regularly in the application's main loop.
///
/// Returns `Ok(true)` if messages were processed normally, `Ok(false)` if a WM_QUIT message
/// was received (indicating the application should shut down), or an `Err` if there was an error.
pub fn process_windows_messages() -> Result<bool, String> {
    unsafe {
        use winapi::um::winuser::{
            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_QUIT,
        };

        let mut msg = MSG {
            hwnd: std::ptr::null_mut(),
            message: 0,
            wParam: 0,
            lParam: 0,
            time: 0,
            pt: winapi::shared::windef::POINT { x: 0, y: 0 },
        };

        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == WM_QUIT {
                return Ok(false); // Signal that the application should shut down
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        Ok(true) // Continue processing
    }
}

/// Run a complete message loop that processes Windows messages until WM_QUIT is received
///
/// This is a convenience function for applications that want a simple blocking message loop.
/// It will process messages and call the provided callback function in each iteration.
///
/// # Arguments
/// * `callback` - Function called in each loop iteration. Should return `false` to exit the loop.
///
/// # Returns
/// `Ok(())` when the loop exits normally, or an error if message processing fails.
pub fn run_message_loop<F>(mut callback: F) -> Result<(), String>
where
    F: FnMut() -> bool,
{
    loop {
        // Process Windows messages
        match process_windows_messages()? {
            false => {
                // WM_QUIT received, exit the loop
                break;
            }
            true => {
                // Continue processing
            }
        }

        // Call the application callback
        if !callback() {
            break; // Application requested to exit
        }

        // Small delay to prevent busy waiting
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    Ok(())
}

// Add helper function to check if window is toplevel
pub unsafe fn is_toplevel_window(hwnd: HWND) -> bool {
    use winapi::um::winuser::{GetParent, GetWindow, GW_OWNER};

    // Check if window has a parent (not desktop)
    let parent = GetParent(hwnd);
    if !parent.is_null() {
        return false;
    }

    // Check if window has an owner
    let owner = GetWindow(hwnd, GW_OWNER);
    if !owner.is_null() {
        return false;
    }

    // Additional check: ensure it's a manageable window
    WindowTracker::is_manageable_window(hwnd as u64)
}
