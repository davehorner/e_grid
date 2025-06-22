ORIGINAL ipc_client.rs backup before fixing static mut and lock contention issues

The original file had these problems:
1. static mut variables for timing (unsafe)
2. Excessive lock holding during complex operations
3. Potential deadlocks from multiple lock acquisitions
4. Busy waiting patterns

Key static mut variables to replace:
- LAST_STATUS_TIME and STATUS_INITIALIZED
- LAST_EVENT_DISPLAY and EVENT_DISPLAY_INITIALIZED  
- LAST_AUTO_DISPLAY and AUTO_DISPLAY_INITIALIZED

All need to be converted to safe local variables or Arc<Mutex<T>> shared state.
