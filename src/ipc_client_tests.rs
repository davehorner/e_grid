use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid_client_errors::*;
    use std::sync::Arc;

    #[test]
    fn test_coordinate_validation() {
        // Test valid coordinates
        assert!(validate_grid_coordinates(0, 0, 4, 6).is_ok());
        assert!(validate_grid_coordinates(3, 5, 4, 6).is_ok());
        
        // Test invalid coordinates
        assert!(validate_grid_coordinates(4, 0, 4, 6).is_err());
        assert!(validate_grid_coordinates(0, 6, 4, 6).is_err());
        assert!(validate_grid_coordinates(5, 7, 4, 6).is_err());
    }

    #[test]
    fn test_retry_config() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay_ms, 100);
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_safe_locking() {
        use std::sync::Mutex;
        
        let mutex = Mutex::new(42);
        let result = safe_lock(&mutex, "test context");
        assert!(result.is_ok());
        assert_eq!(*result.unwrap(), 42);
    }

    #[test]
    fn test_error_display() {
        let error = GridClientError::InvalidCoordinates { 
            row: 5, 
            col: 7, 
            max_row: 4, 
            max_col: 6 
        };
        let display = format!("{}", error);
        assert!(display.contains("Invalid grid coordinates"));
        assert!(display.contains("(5, 7)"));
        assert!(display.contains("4x6"));
    }

    #[test]
    fn test_monitor_grid_info() {
        let monitor_info = MonitorGridInfo {
            monitor_id: 0,
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
            grid: vec![vec![None; 6]; 4],
        };
        
        assert_eq!(monitor_info.monitor_id, 0);
        assert_eq!(monitor_info.width, 1920);
        assert_eq!(monitor_info.height, 1080);
        assert_eq!(monitor_info.grid.len(), 4);
        assert_eq!(monitor_info.grid[0].len(), 6);
    }

    #[test]
    fn test_client_cell_state() {
        let empty = ClientCellState::Empty;
        let occupied = ClientCellState::Occupied(12345);
        let offscreen = ClientCellState::OffScreen;
        
        assert_eq!(empty, ClientCellState::Empty);
        assert_eq!(occupied, ClientCellState::Occupied(12345));
        assert_eq!(offscreen, ClientCellState::OffScreen);
        
        // Test different occupied states
        assert_ne!(ClientCellState::Occupied(12345), ClientCellState::Occupied(67890));
    }

    #[test]
    fn test_window_info_conversion() {
        let details = ipc::WindowDetails {
            hwnd: 12345,
            x: 100,
            y: 200,
            width: 800,
            height: 600,
            virtual_row_start: 1,
            virtual_col_start: 2,
            virtual_row_end: 2,
            virtual_col_end: 4,
            monitor_id: 0,
            monitor_row_start: 1,
            monitor_col_start: 2,
            monitor_row_end: 2,
            monitor_col_end: 4,
            title_len: 10,
        };
        
        let window_info = ClientWindowInfo::from(details);
        assert_eq!(window_info.hwnd, 12345);
        assert_eq!(window_info.x, 100);
        assert_eq!(window_info.y, 200);
        assert_eq!(window_info.width, 800);
        assert_eq!(window_info.height, 600);
    }

    // Integration test for focus callback functionality
    #[test]
    fn test_focus_callback_integration() {
        use std::sync::{Arc, Mutex};
        use std::sync::atomic::{AtomicBool, Ordering};
        
        let callback_called = Arc::new(AtomicBool::new(false));
        let callback_called_clone = callback_called.clone();
        
        let focus_callback: Arc<Mutex<Option<Box<dyn Fn(ipc::WindowFocusEvent) + Send + Sync>>>> = 
            Arc::new(Mutex::new(Some(Box::new(move |_event| {
                callback_called_clone.store(true, Ordering::SeqCst);
            }))));
        
        let focus_event = ipc::WindowFocusEvent {
            hwnd: 12345,
            is_focused: true,
            timestamp: 1234567890,
            app_name: [0; 256],
            app_name_len: 0,
        };
        
        // Simulate handling the focus event
        GridClient::handle_focus_event(&focus_event, &focus_callback);
        
        // Verify callback was called
        assert!(callback_called.load(Ordering::SeqCst));
    }
}
