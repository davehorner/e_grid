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

// Global state for event hooks - necessary due to Windows API callback constraints
// These are accessed only from the main thread and properly synchronized
static mut WINDOW_TRACKER: Option<Arc<Mutex<WindowTracker>>> = None;
static mut EVENT_HOOKS: Vec<winapi::shared::windef::HWINEVENTHOOK> = Vec::new();

// WinEvent hook procedure - simplified to use WindowTracker callbacks
pub unsafe extern "system" fn win_event_proc(
    _h_winevent_hook: winapi::shared::windef::HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _dw_event_thread: u32,
    _dw_ms_event_time: u32,
) {
    // Debug: Always log that we received an event
    println!("🔍 WinEvent received: event={}, hwnd={:?}, obj={}, child={}", 
        event, hwnd, id_object, id_child);
        
    // Only process window-level events (not child objects)
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF {
        println!("🔍 Skipping non-window event");
        return;
    }

    // Skip if window handle is null
    if hwnd.is_null() {
        println!("🔍 Skipping null HWND");
        return;
    }    println!("🔍 Processing window event for HWND {:?}", hwnd);
    
    // Safer access to static tracker
    let tracker_arc = unsafe {
        WINDOW_TRACKER.as_ref()
    };
    
    if let Some(tracker_arc) = tracker_arc {
        println!("🔍 Tracker found, attempting to lock...");
        if let Ok(mut tracker) = tracker_arc.try_lock() {
            println!("🔍 Tracker locked successfully, processing event {}", event);
            match event {                EVENT_OBJECT_CREATE => {
                    if WindowTracker::is_manageable_window(hwnd) {
                        println!("🔍 Processing CREATE event for manageable window {:?}", hwnd);
                        // Remove the sleep - it can block other events
                        // std::thread::sleep(std::time::Duration::from_millis(100));
                        tracker.add_window(hwnd); // This will trigger callbacks
                    } else {
                        println!("🔍 Skipping CREATE event for non-manageable window {:?}", hwnd);
                    }
                }                EVENT_OBJECT_DESTROY => {
                    println!("🔍 Processing DESTROY event for window {:?}", hwnd);
                    tracker.remove_window(hwnd); // This will trigger callbacks
                }
                EVENT_OBJECT_LOCATIONCHANGE | EVENT_SYSTEM_FOREGROUND => {
                    if WindowTracker::is_manageable_window(hwnd) {
                        println!("🔍 Processing MOVE/FOREGROUND event for manageable window {:?}", hwnd);
                        tracker.update_window(hwnd); // This will trigger callbacks
                    } else {
                        println!("🔍 Skipping MOVE/FOREGROUND event for non-manageable window {:?}", hwnd);
                    }
                }
                EVENT_SYSTEM_MINIMIZESTART => {
                    println!("🔍 Processing MINIMIZE START event for window {:?}", hwnd);
                    tracker.remove_window(hwnd); // This will trigger callbacks
                }
                EVENT_SYSTEM_MINIMIZEEND => {
                    if WindowTracker::is_manageable_window(hwnd) {
                        println!("🔍 Processing MINIMIZE END event for manageable window {:?}", hwnd);
                        tracker.add_window(hwnd); // This will trigger callbacks
                    } else {
                        println!("🔍 Skipping MINIMIZE END event for non-manageable window {:?}", hwnd);
                    }
                }_ => {
                    // Unhandled event type
                    println!("🔍 Unhandled event type: {}", event);
                }
            }
        } else {
            println!("🔍 Failed to lock tracker - might be busy");
        }
    } else {
        println!("🔍 No tracker found in static");
    }
}

pub fn setup_window_events(tracker: Arc<Mutex<WindowTracker>>) -> Result<(), String> {
    unsafe {
        // Simple assignment instead of ptr operations
        WINDOW_TRACKER = Some(tracker.clone());
        EVENT_HOOKS = Vec::new();

        println!("🔧 Setting up WinEvent hooks...");

        // Set up hooks for different window events
        let events_to_hook = [
            (EVENT_OBJECT_CREATE, "Window Creation"),
            (EVENT_OBJECT_DESTROY, "Window Destruction"), 
            (EVENT_OBJECT_LOCATIONCHANGE, "Window Move/Resize"),
            (EVENT_SYSTEM_FOREGROUND, "Window Activation"),
            (EVENT_SYSTEM_MINIMIZESTART, "Window Minimize"),
            (EVENT_SYSTEM_MINIMIZEEND, "Window Restore"),
        ];

        for (event, description) in &events_to_hook {
            let hook = SetWinEventHook(
                *event,
                *event,
                ptr::null_mut(), // No specific module
                Some(win_event_proc),
                0, // All processes
                0, // All threads
                WINEVENT_OUTOFCONTEXT, // Out-of-context (more reliable)
            );            if hook.is_null() {
                let error = GetLastError();
                println!("❌ Failed to set up hook for {}: error {}", description, error);
            } else {
                // Simple push to static vector
                EVENT_HOOKS.push(hook);
                println!("✅ Successfully set up hook for {}", description);
            }
        }        // Check if we have any hooks
        let hooks_len = EVENT_HOOKS.len();
        if hooks_len == 0 {
            return Err("Failed to set up any event hooks".to_string());
        }

        println!("🚀 Successfully set up {} WinEvent hooks!", hooks_len);
        println!("📢 Now listening for real-time window events across all monitors!");
        println!();

        Ok(())
    }
}

pub fn cleanup_hooks() {
    unsafe {
        // Simple iteration and cleanup
        for hook in &EVENT_HOOKS {
            UnhookWinEvent(*hook);
        }
        EVENT_HOOKS = Vec::new();
        println!("🧹 Cleaned up all event hooks");
    }
}
