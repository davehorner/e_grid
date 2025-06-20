// Lock-Free WindowTracker Implementation Example
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use winapi::shared::windef::HWND;

// Lock-free windows storage using DashMap
pub struct LockFreeWindowTracker {
    // Lock-free concurrent hashmap for windows
    windows: DashMap<HWND, WindowInfo>,
    
    // Atomic counters for statistics
    window_count: AtomicUsize,
    
    // Grid state could also be made lock-free using atomic arrays
    // For simplicity, keeping basic config
    config: GridConfig,
    
    // Event callbacks (could be made lock-free with lockfree lists)
    event_callbacks: Vec<WindowEventCallbackBox>,
}

impl LockFreeWindowTracker {
    pub fn new() -> Self {
        Self {
            windows: DashMap::new(),
            window_count: AtomicUsize::new(0),
            config: GridConfig::default(),
            event_callbacks: Vec::new(),
        }
    }
    
    /// Add window - completely lock-free
    pub fn add_window(&self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let title = Self::get_window_title(hwnd);
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);

            let window_info = WindowInfo {
                hwnd,
                title,
                rect,
                grid_cells,
                monitor_cells,
            };

            // Lock-free insertion
            self.windows.insert(hwnd, window_info.clone());
            self.window_count.fetch_add(1, Ordering::Relaxed);
            
            // Trigger callbacks (this could be made lock-free too)
            self.trigger_window_created(hwnd, &window_info);
            
            return true;
        }
        false
    }
    
    /// Remove window - lock-free
    pub fn remove_window(&self, hwnd: HWND) -> bool {
        if self.windows.remove(&hwnd).is_some() {
            self.window_count.fetch_sub(1, Ordering::Relaxed);
            self.trigger_window_destroyed(hwnd);
            return true;
        }
        false
    }
    
    /// Update window - lock-free
    pub fn update_window(&self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);
            
            // Lock-free update using entry API
            if let Some(mut entry) = self.windows.get_mut(&hwnd) {
                entry.rect = rect;
                entry.grid_cells = grid_cells;
                entry.monitor_cells = monitor_cells;
                
                let window_info = entry.clone();
                drop(entry); // Release the lock early
                
                self.trigger_window_moved(hwnd, &window_info);
                return true;
            }
        }
        false
    }
    
    /// Get window count - atomic read
    pub fn window_count(&self) -> usize {
        self.window_count.load(Ordering::Relaxed)
    }
    
    /// Iterate over windows - lock-free iteration
    pub fn iter_windows<F>(&self, mut f: F) 
    where 
        F: FnMut(HWND, &WindowInfo)
    {
        for entry in self.windows.iter() {
            f(*entry.key(), entry.value());
        }
    }
    
    /// Get specific window - lock-free read
    pub fn get_window(&self, hwnd: HWND) -> Option<WindowInfo> {
        self.windows.get(&hwnd).map(|entry| entry.value().clone())
    }
    
    /// Move window to cell - minimized locking
    pub fn move_window_to_cell(&self, hwnd: HWND, target_row: usize, target_col: usize) -> Result<(), String> {
        // Validate coordinates first (no locking needed)
        if target_row >= self.config.rows || target_col >= self.config.cols {
            return Err(format!("Invalid grid coordinates: ({}, {})", target_row, target_col));
        }
        
        // Validate window handle (no locking)
        unsafe {
            if IsWindow(hwnd) == 0 {
                return Err(format!("Invalid window handle: {:?}", hwnd));
            }
        }
        
        // Calculate target position (no locking)
        if let Some(target_rect) = self.primary_monitor_cell_to_rect(target_row, target_col) {
            // Move the window (Windows API call)
            unsafe {
                let result = SetWindowPos(
                    hwnd,
                    ptr::null_mut(),
                    target_rect.left,
                    target_rect.top,
                    target_rect.right - target_rect.left,
                    target_rect.bottom - target_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                
                if result == 0 {
                    let error = GetLastError();
                    return Err(format!("SetWindowPos failed with error: {}", error));
                }
            }
            
            // Update tracking - lock-free
            if let Some(mut entry) = self.windows.get_mut(&hwnd) {
                // Update grid position
                entry.grid_cells.clear();
                entry.grid_cells.push((target_row, target_col));
                
                let window_info = entry.clone();
                drop(entry); // Release early
                
                self.trigger_window_moved(hwnd, &window_info);
            }
            
            Ok(())
        } else {
            Err(format!("Could not calculate target rectangle for cell ({}, {})", target_row, target_col))
        }
    }
}

// Alternative: Fully Lock-Free with Atomic Pointers
use std::sync::atomic::AtomicPtr;
use std::ptr;

pub struct AtomicWindowList {
    head: AtomicPtr<WindowNode>,
}

struct WindowNode {
    hwnd: HWND,
    window_info: WindowInfo,
    next: AtomicPtr<WindowNode>,
}

impl AtomicWindowList {
    pub fn new() -> Self {
        Self {
            head: AtomicPtr::new(ptr::null_mut()),
        }
    }
    
    pub fn insert(&self, hwnd: HWND, window_info: WindowInfo) {
        let new_node = Box::into_raw(Box::new(WindowNode {
            hwnd,
            window_info,
            next: AtomicPtr::new(ptr::null_mut()),
        }));
        
        loop {
            let head = self.head.load(Ordering::Acquire);
            unsafe {
                (*new_node).next.store(head, Ordering::Relaxed);
            }
            
            if self.head.compare_exchange_weak(
                head, 
                new_node, 
                Ordering::Release, 
                Ordering::Relaxed
            ).is_ok() {
                break;
            }
        }
    }
    
    pub fn find<F>(&self, mut predicate: F) -> Option<WindowInfo>
    where 
        F: FnMut(HWND, &WindowInfo) -> bool
    {
        let mut current = self.head.load(Ordering::Acquire);
        
        while !current.is_null() {
            unsafe {
                let node = &*current;
                if predicate(node.hwnd, &node.window_info) {
                    return Some(node.window_info.clone());
                }
                current = node.next.load(Ordering::Acquire);
            }
        }
        None
    }
}

// Usage example for integrating with existing code:
impl WindowTracker {
    pub fn convert_to_lockfree(self) -> LockFreeWindowTracker {
        let lockfree = LockFreeWindowTracker::new();
        
        // Copy existing windows to lock-free storage
        for (hwnd, window_info) in self.windows {
            lockfree.windows.insert(hwnd, window_info);
        }
        lockfree.window_count.store(lockfree.windows.len(), Ordering::Relaxed);
        
        lockfree
    }
}
