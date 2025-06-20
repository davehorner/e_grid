# Lock-Free Windows Storage Implementation Guide

## Benefits of Lock-Free Windows Data Structure

### **Current Issues with Mutex:**
```rust
// Current implementation - blocking
let tracker = tracker_arc.lock().unwrap(); // BLOCKS HERE
tracker.add_window(hwnd);  // All other threads wait
```

### **Lock-Free Solution with DashMap:**
```rust
// Lock-free implementation - never blocks
tracker.add_window_lockfree(hwnd);  // Multiple threads can run concurrently
```

## **Implementation Strategy**

### 1. **Replace HashMap with DashMap for Windows**

```rust
// Before (current)
pub struct WindowTracker {
    pub windows: HashMap<HWND, WindowInfo>,  // Requires mutex protection
    // ... other fields
}

// After (lock-free)
pub struct WindowTracker {
    pub windows: DashMap<HWND, WindowInfo>,  // No mutex needed!
    // ... other fields
}
```

### 2. **Lock-Free Window Operations**

```rust
impl WindowTracker {
    // Lock-free window addition
    pub fn add_window_lockfree(&self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let window_info = WindowInfo { /* ... */ };
            
            // This operation is lock-free and atomic
            self.windows.insert(hwnd, window_info);
            return true;
        }
        false
    }
    
    // Lock-free window removal
    pub fn remove_window_lockfree(&self, hwnd: HWND) -> bool {
        self.windows.remove(&hwnd).is_some()
    }
    
    // Lock-free window updates
    pub fn update_window_lockfree(&self, hwnd: HWND) -> bool {
        if let Some(mut entry) = self.windows.get_mut(&hwnd) {
            // Update fields directly - no global lock needed
            entry.rect = new_rect;
            return true;
        }
        false
    }
    
    // Lock-free iteration
    pub fn for_each_window<F>(&self, mut f: F) 
    where F: FnMut(HWND, &WindowInfo) {
        for entry in self.windows.iter() {
            f(*entry.key(), entry.value());
        }
    }
}
```

### 3. **Integration with Existing Code**

```rust
// In window_events.rs - WinEvent callback
unsafe extern "system" fn win_event_proc(
    hook: HWINEVENTHOOK,
    event: DWORD,
    hwnd: HWND,
    obj: LONG,
    child: LONG,
    thread: DWORD,
    time: DWORD,
) {
    // No more blocking on tracker lock!
    if let Some(tracker) = get_window_tracker() {
        match event {
            EVENT_OBJECT_CREATE => {
                tracker.add_window_lockfree(hwnd);  // Non-blocking
            }
            EVENT_OBJECT_DESTROY => {
                tracker.remove_window_lockfree(hwnd);  // Non-blocking
            }
            EVENT_OBJECT_LOCATIONCHANGE => {
                tracker.update_window_lockfree(hwnd);  // Non-blocking
            }
            _ => {}
        }
    }
}
```

### 4. **IPC Server Integration**

```rust
// In ipc_server.rs
impl GridIpcServer {
    fn handle_move_window_command(&mut self, hwnd: HWND, row: usize, col: usize) -> Result<(), String> {
        // No blocking on tracker access
        let tracker = self.tracker_ref(); // Get reference, no lock needed
        
        // This operation is mostly lock-free
        tracker.move_window_to_cell_lockfree(hwnd, row, col)
    }
    
    fn get_window_list(&self) -> Vec<WindowInfo> {
        let mut windows = Vec::new();
        
        // Lock-free iteration - no blocking
        self.tracker.for_each_window(|_hwnd, window_info| {
            windows.push(window_info.clone());
        });
        
        windows
    }
}
```

## **Performance Improvements**

### **Before (with Mutex):**
- ❌ WinEvent callbacks block waiting for tracker lock
- ❌ IPC commands serialize on tracker access
- ❌ Window movement operations block other window operations
- ❌ Grid updates block all window access

### **After (with DashMap):**
- ✅ WinEvent callbacks never block on window operations
- ✅ Multiple IPC commands can run concurrently
- ✅ Window movements don't block window lookups
- ✅ Only grid updates need brief synchronization

## **Gradual Migration Strategy**

### **Step 1: Add DashMap dependency**
```toml
[dependencies]
dashmap = "5.5"
```

### **Step 2: Add lock-free methods alongside existing ones**
```rust
impl WindowTracker {
    // Keep existing methods for compatibility
    pub fn add_window(&mut self, hwnd: HWND) -> bool { ... }
    
    // Add new lock-free methods
    pub fn add_window_lockfree(&self, hwnd: HWND) -> bool { ... }
}
```

### **Step 3: Update WinEvent callbacks to use lock-free methods**
```rust
// Replace blocking operations with lock-free ones
tracker.add_window_lockfree(hwnd);      // Instead of tracker.lock().add_window()
tracker.remove_window_lockfree(hwnd);   // Instead of tracker.lock().remove_window()
```

### **Step 4: Update IPC server to use lock-free methods**
```rust
// Replace mutex-protected operations
self.tracker.move_window_to_cell_lockfree(hwnd, row, col);
```

### **Step 5: Eventually replace HashMap with DashMap in the struct**
```rust
pub struct WindowTracker {
    pub windows: DashMap<HWND, WindowInfo>,  // Final migration
    // ... other fields
}
```

## **Expected Results**

1. **Eliminated Contention**: No more "Failed to lock tracker - might be busy" messages
2. **Better Responsiveness**: WinEvent callbacks process without blocking
3. **Improved Throughput**: Multiple window operations can run simultaneously
4. **Reduced Latency**: Window movement commands execute without waiting for locks

## **Still Need Synchronization For:**
- Grid state updates (brief locks acceptable)
- Event callbacks (order matters)
- Monitor configuration changes

But the hot path (window storage access) becomes completely lock-free!
