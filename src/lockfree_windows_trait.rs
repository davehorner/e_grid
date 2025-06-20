use dashmap::DashMap;
use winapi::shared::windef::HWND;
use crate::{WindowInfo, WindowTracker};

/// Extension trait to add lock-free methods to existing WindowTracker
pub trait LockFreeWindowOps {
    /// Add window without blocking - can be called from any thread
    fn add_window_lockfree(&self, hwnd: HWND) -> bool;
    
    /// Remove window without blocking
    fn remove_window_lockfree(&self, hwnd: HWND) -> bool;
    
    /// Update window without blocking
    fn update_window_lockfree(&self, hwnd: HWND) -> bool;
    
    /// Get window info without blocking
    fn get_window_lockfree(&self, hwnd: HWND) -> Option<WindowInfo>;
    
    /// Iterate over all windows without blocking
    fn for_each_window_lockfree<F>(&self, f: F) where F: FnMut(HWND, &WindowInfo);
    
    /// Get window count without blocking
    fn window_count_lockfree(&self) -> usize;
}

/// Lock-free windows storage - can be used alongside existing WindowTracker
pub struct LockFreeWindows {
    windows: DashMap<HWND, WindowInfo>,
}

impl LockFreeWindows {
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
        }
    }
    
    /// Populate from existing HashMap (for migration)
    pub fn from_hashmap(windows: &std::collections::HashMap<HWND, WindowInfo>) -> Self {
        let lockfree = Self::new();
        for (hwnd, window_info) in windows {
            lockfree.windows.insert(*hwnd, window_info.clone());
        }
        lockfree
    }
    
    pub fn insert(&self, hwnd: HWND, window_info: WindowInfo) {
        self.windows.insert(hwnd, window_info);
    }
    
    pub fn remove(&self, hwnd: HWND) -> bool {
        self.windows.remove(&hwnd).is_some()
    }
    
    pub fn get(&self, hwnd: HWND) -> Option<WindowInfo> {
        self.windows.get(&hwnd).map(|entry| entry.value().clone())
    }
    
    pub fn update<F>(&self, hwnd: HWND, updater: F) -> bool 
    where F: FnOnce(&mut WindowInfo) {
        if let Some(mut entry) = self.windows.get_mut(&hwnd) {
            updater(entry.value_mut());
            true
        } else {
            false
        }
    }
    
    pub fn len(&self) -> usize {
        self.windows.len()
    }
    
    pub fn for_each<F>(&self, mut f: F) 
    where F: FnMut(HWND, &WindowInfo) {
        for entry in self.windows.iter() {
            f(*entry.key(), entry.value());
        }
    }
    
    /// Convert back to HashMap (for compatibility)
    pub fn to_hashmap(&self) -> std::collections::HashMap<HWND, WindowInfo> {
        let mut map = std::collections::HashMap::new();
        for entry in self.windows.iter() {
            map.insert(*entry.key(), entry.value().clone());
        }
        map
    }
}

// Example of how to integrate with window_events.rs
pub static LOCKFREE_WINDOWS: once_cell::sync::Lazy<LockFreeWindows> = 
    once_cell::sync::Lazy::new(|| LockFreeWindows::new());

// Usage in WinEvent callback (completely non-blocking):
/*
unsafe extern "system" fn win_event_proc(
    _hook: HHOOK,
    event: DWORD,
    hwnd: HWND,
    obj: LONG,
    _child: LONG,
    _thread: DWORD,
    _time: DWORD,
) {
    if obj != 0 || hwnd.is_null() {
        return;
    }
    
    match event {
        EVENT_OBJECT_CREATE => {
            if WindowTracker::is_manageable_window(hwnd) {
                if let Some(rect) = WindowTracker::get_window_rect(hwnd) {
                    let title = WindowTracker::get_window_title(hwnd);
                    let window_info = WindowInfo {
                        hwnd,
                        title,
                        rect,
                        grid_cells: vec![], // Calculate as needed
                        monitor_cells: std::collections::HashMap::new(),
                    };
                    
                    // This never blocks!
                    LOCKFREE_WINDOWS.insert(hwnd, window_info);
                }
            }
        }
        EVENT_OBJECT_DESTROY => {
            // This never blocks!
            LOCKFREE_WINDOWS.remove(hwnd);
        }
        EVENT_OBJECT_LOCATIONCHANGE => {
            // This never blocks!
            if let Some(new_rect) = WindowTracker::get_window_rect(hwnd) {
                LOCKFREE_WINDOWS.update(hwnd, |window_info| {
                    window_info.rect = new_rect;
                    // Update grid_cells as needed
                });
            }
        }
        _ => {}
    }
}
*/
