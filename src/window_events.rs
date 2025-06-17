use crate::WindowTracker;
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

// Global state for event hooks - necessary due to Windows API callback constraints
// These are accessed only from the main thread and properly synchronized
// SAFETY: These statics are only accessed from a single thread (main thread)
// and proper cleanup is ensured through the cleanup_hooks function
static mut WINDOW_TRACKER: Option<Arc<Mutex<WindowTracker>>> = None;
static mut EVENT_HOOKS: Vec<winapi::shared::windef::HWINEVENTHOOK> = Vec::new();

// WinEvent hook procedure
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
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF {
        return;
    }

    // Skip if window handle is null
    if hwnd.is_null() {
        return;
    }

    // Safe access to static without creating references
    let tracker_opt = ptr::addr_of!(WINDOW_TRACKER).read();
    if let Some(tracker_arc) = tracker_opt {
        if let Ok(mut tracker) = tracker_arc.try_lock() {
            let window_title = WindowTracker::get_window_title(hwnd);
            let event_name = match event {
                EVENT_OBJECT_CREATE => "CREATED",
                EVENT_OBJECT_DESTROY => "DESTROYED", 
                EVENT_OBJECT_LOCATIONCHANGE => "MOVED/RESIZED",
                EVENT_SYSTEM_FOREGROUND => "ACTIVATED",
                EVENT_SYSTEM_MINIMIZESTART => "MINIMIZED",
                EVENT_SYSTEM_MINIMIZEEND => "RESTORED",
                _ => "OTHER"
            };

            println!("🔔 WINDOW EVENT RECEIVED!");
            println!("   Event: {} | Window: {}", 
                event_name,
                if window_title.is_empty() { "<No Title>" } else { &window_title }
            );

            match event {
                EVENT_OBJECT_CREATE => {
                    println!("   → Checking if window is manageable...");
                    if WindowTracker::is_manageable_window(hwnd) {
                        println!("   → Window IS manageable, adding to tracker...");
                        // Small delay to ensure window is fully initialized
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        if tracker.add_window(hwnd) {
                            println!("   ✅ Window created and added: {}", window_title);
                            println!("   📊 Updating grid displays...");
                            tracker.print_all_grids();
                        } else {
                            println!("   ❌ Failed to add window");
                        }
                    } else {
                        println!("   → Window is NOT manageable, ignoring");
                    }
                }
                EVENT_OBJECT_DESTROY => {
                    println!("   → Removing window from tracker...");
                    if tracker.remove_window(hwnd) {
                        println!("   ✅ Window destroyed and removed");
                        println!("   📊 Updating grid displays...");
                        tracker.print_all_grids();
                    } else {
                        println!("   → Window was not being tracked");
                    }
                }
                EVENT_OBJECT_LOCATIONCHANGE | EVENT_SYSTEM_FOREGROUND => {
                    println!("   → Checking if window is manageable...");
                    if WindowTracker::is_manageable_window(hwnd) {
                        println!("   → Window IS manageable, updating position...");
                        if tracker.update_window(hwnd) {
                            println!("   ✅ Window updated: {}", window_title);
                            println!("   📊 Updating grid displays...");
                            tracker.print_all_grids();
                        } else {
                            println!("   → No significant position change detected");
                        }
                    } else {
                        println!("   → Window is NOT manageable, ignoring");
                    }
                }
                EVENT_SYSTEM_MINIMIZESTART => {
                    println!("   → Window minimized, removing from grid...");
                    if tracker.remove_window(hwnd) {
                        println!("   ✅ Minimized window removed from grid");
                        println!("   📊 Updating grid displays...");
                        tracker.print_all_grids();
                    }
                }
                EVENT_SYSTEM_MINIMIZEEND => {
                    println!("   → Window restored, checking if should be tracked...");
                    if WindowTracker::is_manageable_window(hwnd) {
                        if tracker.add_window(hwnd) {
                            println!("   ✅ Restored window added back to grid");
                            println!("   📊 Updating grid displays...");
                            tracker.print_all_grids();
                        }
                    }
                }
                _ => {
                    println!("   → Unhandled event type: {}", event);
                }
            }
            println!(); // Add blank line for readability
        }
    }
}

pub fn setup_window_events(tracker: Arc<Mutex<WindowTracker>>) -> Result<(), String> {
    unsafe {
        // Use raw pointer access to avoid static mut ref warnings
        ptr::addr_of_mut!(WINDOW_TRACKER).write(Some(tracker.clone()));
        ptr::addr_of_mut!(EVENT_HOOKS).write(Vec::new());

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
            );

            if hook.is_null() {
                let error = GetLastError();
                println!("❌ Failed to set up hook for {}: error {}", description, error);
            } else {
                // Safely add hook to static vector
                let hooks_ptr = ptr::addr_of_mut!(EVENT_HOOKS);
                (*hooks_ptr).push(hook);
                println!("✅ Successfully set up hook for {}", description);
            }
        }

        // Check if we have any hooks
        let hooks_len = ptr::addr_of!(EVENT_HOOKS).read().len();
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
        // Use raw pointer iteration to avoid warnings
        let hooks_ptr = ptr::addr_of!(EVENT_HOOKS);
        for hook in &(*hooks_ptr) {
            UnhookWinEvent(*hook);
        }
        ptr::addr_of_mut!(EVENT_HOOKS).write(Vec::new());
        println!("🧹 Cleaned up all event hooks");
    }
}
