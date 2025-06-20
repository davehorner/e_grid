// Example: Adding lock-free operations to existing WindowTracker

// Add this to lib.rs imports
use dashmap::DashMap;
use once_cell::sync::Lazy;

// Global lock-free windows storage (can coexist with existing HashMap)
static LOCKFREE_WINDOWS: Lazy<DashMap<HWND, WindowInfo>> = Lazy::new(|| DashMap::new());

impl WindowTracker {
    // Add these new methods alongside existing ones
    
    /// Add window using lock-free storage - never blocks
    pub fn add_window_lockfree(&self, hwnd: HWND) -> bool {
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

            // Lock-free insertion - never blocks other threads
            LOCKFREE_WINDOWS.insert(hwnd, window_info.clone());
            
            // Also update the regular HashMap for compatibility (if needed)
            // self.windows.insert(hwnd, window_info); // This would need the mutex
            
            // Trigger callbacks
            self.trigger_window_created(hwnd, &window_info);
            
            return true;
        }
        false
    }
    
    /// Remove window using lock-free storage - never blocks
    pub fn remove_window_lockfree(&self, hwnd: HWND) -> bool {
        let removed = LOCKFREE_WINDOWS.remove(&hwnd).is_some();
        if removed {
            self.trigger_window_destroyed(hwnd);
        }
        removed
    }
    
    /// Update window using lock-free storage - never blocks
    pub fn update_window_lockfree(&self, hwnd: HWND) -> bool {
        if let Some(rect) = Self::get_window_rect(hwnd) {
            let grid_cells = self.window_to_grid_cells(&rect);
            let monitor_cells = self.calculate_monitor_cells(&rect);
            
            // Lock-free update
            if let Some(mut entry) = LOCKFREE_WINDOWS.get_mut(&hwnd) {
                entry.rect = rect;
                entry.grid_cells = grid_cells;
                entry.monitor_cells = monitor_cells;
                
                let window_info = entry.clone();
                drop(entry); // Release the reference early
                
                self.trigger_window_moved(hwnd, &window_info);
                return true;
            }
        }
        false
    }
    
    /// Get window from lock-free storage - never blocks
    pub fn get_window_lockfree(&self, hwnd: HWND) -> Option<WindowInfo> {
        LOCKFREE_WINDOWS.get(&hwnd).map(|entry| entry.value().clone())
    }
    
    /// Get window count from lock-free storage - never blocks
    pub fn window_count_lockfree(&self) -> usize {
        LOCKFREE_WINDOWS.len()
    }
    
    /// Iterate over windows from lock-free storage - never blocks
    pub fn for_each_window_lockfree<F>(&self, mut f: F) 
    where F: FnMut(HWND, &WindowInfo) {
        for entry in LOCKFREE_WINDOWS.iter() {
            f(*entry.key(), entry.value());
        }
    }
    
    /// Move window using mostly lock-free operations
    pub fn move_window_to_cell_lockfree(&self, hwnd: HWND, target_row: usize, target_col: usize) -> Result<(), String> {
        // All validation is lock-free
        if target_row >= self.config.rows || target_col >= self.config.cols {
            return Err(format!("Invalid grid coordinates: ({}, {})", target_row, target_col));
        }
        
        unsafe {
            if IsWindow(hwnd) == 0 {
                return Err(format!("Invalid window handle: {:?}", hwnd));
            }
        }
        
        if !Self::is_manageable_window(hwnd) {
            return Err(format!("Window {:?} is not manageable", hwnd));
        }
        
        // Calculate target position - no locking
        if let Some(target_rect) = self.primary_monitor_cell_to_rect(target_row, target_col) {
            println!("🎯 Moving window {:?} to cell ({}, {}) - LOCK-FREE MODE", hwnd, target_row, target_col);
            
            // Move window - Windows API call
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
            
            // Update window info in lock-free storage
            if let Some(mut entry) = LOCKFREE_WINDOWS.get_mut(&hwnd) {
                entry.grid_cells.clear();
                entry.grid_cells.push((target_row, target_col));
                
                let window_info = entry.clone();
                drop(entry); // Release early
                
                println!("✅ Successfully moved window {:?} using LOCK-FREE operations", hwnd);
                
                // Only the grid state update needs a brief lock
                if let Ok(()) = self.assign_window_to_virtual_cell(hwnd, target_row, target_col) {
                    self.trigger_window_moved(hwnd, &window_info);
                }
            }
            
            Ok(())
        } else {
            Err(format!("Could not calculate target rectangle for cell ({}, {})", target_row, target_col))
        }
    }
    
    /// Migrate existing windows to lock-free storage
    pub fn migrate_to_lockfree(&self) {
        for (hwnd, window_info) in &self.windows {
            LOCKFREE_WINDOWS.insert(*hwnd, window_info.clone());
        }
        println!("🔄 Migrated {} windows to lock-free storage", self.windows.len());
    }
    
    /// Sync lock-free storage back to HashMap (for compatibility)
    pub fn sync_from_lockfree(&mut self) {
        self.windows.clear();
        for entry in LOCKFREE_WINDOWS.iter() {
            self.windows.insert(*entry.key(), entry.value().clone());
        }
        println!("🔄 Synced {} windows from lock-free storage", LOCKFREE_WINDOWS.len());
    }
}
