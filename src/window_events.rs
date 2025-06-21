use crate::{WindowTracker, WindowEventCallback, WindowInfo};
use std::ptr;
use std::sync::{Arc, Mutex};
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::winuser::{
    SetWinEventHook, UnhookWinEvent, 
    EVENT_OBJECT_CREATE, EVENT_OBJECT_DESTROY, EVENT_OBJECT_LOCATIONCHANGE,
    EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND,
    WINEVENT_OUTOFCONTEXT, OBJID_WINDOW, CHILDID_SELF
};
use winapi::shared::windef::HWND;
use log::{info, debug, warn};

/// Configuration for window events with optional callbacks
pub struct WindowEventConfig {
    pub tracker: Arc<Mutex<WindowTracker>>,
    pub focus_callback: Option<Box<dyn Fn(HWND, bool) + Send + Sync>>, // hwnd, is_focused
    pub heartbeat_reset: Option<Box<dyn Fn() + Send + Sync>>,
    pub event_callback: Option<Box<dyn Fn(crate::ipc_protocol::GridEvent) + Send + Sync>>, // NEW: event publishing callback
    pub debug_mode: bool,
}

impl WindowEventConfig {
    pub fn new(tracker: Arc<Mutex<WindowTracker>>) -> Self {
        Self {
            tracker,
            focus_callback: None,
            heartbeat_reset: None,
            event_callback: None, // NEW
            debug_mode: false,
        }
    }
    
    pub fn with_focus_callback<F>(mut self, callback: F) -> Self 
    where F: Fn(HWND, bool) + Send + Sync + 'static {
        self.focus_callback = Some(Box::new(callback));
        self
    }
    
    pub fn with_heartbeat_reset<F>(mut self, callback: F) -> Self 
    where F: Fn() + Send + Sync + 'static {
        self.heartbeat_reset = Some(Box::new(callback));
        self
    }
    
    pub fn with_event_callback<F>(mut self, callback: F) -> Self 
    where F: Fn(crate::ipc_protocol::GridEvent) + Send + Sync + 'static {
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
    fn on_window_created(&self, hwnd: HWND, window_info: &WindowInfo) {
        // Only show manageable windows in debug output
        if WindowTracker::is_manageable_window(hwnd) {
            println!("🔔 WINDOW EVENT: CREATED (Manageable)");
            println!("   Window: {}", 
                if window_info.title.is_empty() { "<No Title>" } else { &window_info.title }
            );
            println!("   HWND: {:?}", hwnd);
            println!();
        }
    }
    
    fn on_window_destroyed(&self, hwnd: HWND) {
        println!("🔔 WINDOW EVENT: DESTROYED");
        println!("   HWND: {:?}", hwnd);
        println!();
    }
    
    fn on_window_moved(&self, hwnd: HWND, window_info: &WindowInfo) {
        // Only show manageable windows in debug output
        if WindowTracker::is_manageable_window(hwnd) {
            println!("🔔 WINDOW EVENT: MOVED/RESIZED (Manageable)");
            println!("   Window: {}", 
                if window_info.title.is_empty() { "<No Title>" } else { &window_info.title }
            );
            println!("   HWND: {:?}", hwnd);
            println!();
        }
    }
    
    fn on_window_activated(&self, hwnd: HWND, window_info: &WindowInfo) {
        // Only show manageable windows in debug output
        if WindowTracker::is_manageable_window(hwnd) {
            println!("🔔 WINDOW EVENT: ACTIVATED (Manageable)");
            println!("   Window: {}", 
                if window_info.title.is_empty() { "<No Title>" } else { &window_info.title }
            );
            println!("   HWND: {:?}", hwnd);
            println!();
        }
    }
    
    fn on_window_minimized(&self, hwnd: HWND) {
        println!("🔔 WINDOW EVENT: MINIMIZED");
        println!("   HWND: {:?}", hwnd);
        println!();
    }
    
    fn on_window_restored(&self, hwnd: HWND, window_info: &WindowInfo) {
        println!("🔔 WINDOW EVENT: RESTORED");
        println!("   Window: {}", 
            if window_info.title.is_empty() { "<No Title>" } else { &window_info.title }
        );
        println!("   HWND: {:?}", hwnd);
        println!();
    }
}

// Global state for event hooks and configuration
// These are accessed only from the main thread and properly synchronized
static mut WINDOW_EVENT_CONFIG: Option<WindowEventConfig> = None;
static mut EVENT_HOOKS: Vec<winapi::shared::windef::HWINEVENTHOOK> = Vec::new();
static mut LAST_FOCUSED_WINDOW: Option<HWND> = None;

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
    };    if config.debug_mode {
        let event_name = match event {
            3 => "EVENT_SYSTEM_FOREGROUND (FOCUS)",
            32768 => "EVENT_OBJECT_SHOW",
            32769 => "EVENT_OBJECT_HIDE",
            32779 => "EVENT_OBJECT_LOCATIONCHANGE (MOVE/RESIZE)",
            _ => "OTHER",
        };
        println!("🔍 WinEvent: event={} ({}), hwnd={:?}", event, event_name, hwnd);
    }

    // Always try to reset heartbeat if callback is available
    if let Some(ref heartbeat_reset) = config.heartbeat_reset {
        heartbeat_reset();
    }

    // Handle focus events specially
    if event == EVENT_SYSTEM_FOREGROUND {
        if let Some(ref focus_callback) = config.focus_callback {
            // Send DEFOCUSED for previous window if it exists
            if let Some(prev_hwnd) = LAST_FOCUSED_WINDOW {
                if prev_hwnd != hwnd && !prev_hwnd.is_null() {
                    if config.debug_mode {
                        println!("🎯 Focus: DEFOCUSED HWND {:?}", prev_hwnd);
                    }
                    focus_callback(prev_hwnd, false); // false = DEFOCUSED
                }
            }
            
            // Update last focused window and send FOCUSED
            LAST_FOCUSED_WINDOW = Some(hwnd);
            if config.debug_mode {
                println!("🎯 Focus: FOCUSED HWND {:?}", hwnd);
            }
            focus_callback(hwnd, true); // true = FOCUSED
        }
    }

    // Update window tracker
    if let Ok(mut tracker) = config.tracker.try_lock() {
        if config.debug_mode {
            println!("🔍 Processing event {} for window {:?}", event, hwnd);
        }
        
        match event {
            EVENT_OBJECT_CREATE => {
                if WindowTracker::is_manageable_window(hwnd) {
                    tracker.add_window(hwnd);
                }
            }
            EVENT_OBJECT_DESTROY => {
                tracker.remove_window(hwnd);
            }
            EVENT_OBJECT_LOCATIONCHANGE => {
                if WindowTracker::is_manageable_window(hwnd) {
                    tracker.update_window(hwnd);
                    // NEW: Publish move event
                    if let Some(ref event_callback) = config.event_callback {
                        if let Some(window_info) = tracker.windows.get(&hwnd) {
                            let event = crate::ipc_protocol::GridEvent::WindowMoved {
                                hwnd: hwnd as u64,
                                title: window_info.title.clone(),
                                old_row: 0, // TODO: track previous row/col if needed
                                old_col: 0,
                                new_row: 0, // TODO: fill with actual grid info
                                new_col: 0,
                                grid_top_left_row: 0,
                                grid_top_left_col: 0,
                                grid_bottom_right_row: 0,
                                grid_bottom_right_col: 0,
                                real_x: window_info.rect.left,
                                real_y: window_info.rect.top,
                                real_width: (window_info.rect.right - window_info.rect.left) as u32,
                                real_height: (window_info.rect.bottom - window_info.rect.top) as u32,
                                monitor_id: 0,
                            };
                            event_callback(event);
                        }
                    }
                }
            }
            EVENT_SYSTEM_MINIMIZESTART => {
                tracker.remove_window(hwnd);
            }
            EVENT_SYSTEM_MINIMIZEEND => {
                if WindowTracker::is_manageable_window(hwnd) {
                    tracker.add_window(hwnd);
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

        // Set up hooks for different window events
        let events_to_hook = [
            (EVENT_OBJECT_CREATE, "Window Creation"),
            (EVENT_OBJECT_DESTROY, "Window Destruction"), 
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
                println!("❌ Failed to set up hook for {}: error {}", description, error);
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
        // Clean up hooks
        for hook in &EVENT_HOOKS {
            UnhookWinEvent(*hook);
        }
        EVENT_HOOKS = Vec::new();
        
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
            PeekMessageW, TranslateMessage, DispatchMessageW, MSG, PM_REMOVE, WM_QUIT
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
    F: FnMut() -> bool 
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
