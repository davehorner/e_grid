use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Optional heartbeat service for keeping clients connected during idle periods
pub struct HeartbeatService {
    last_reset: Arc<Mutex<Instant>>,
    timeout_duration: Duration,
    enabled: bool,
}

impl HeartbeatService {
    /// Create a new heartbeat service with specified timeout
    pub fn new(timeout_duration: Duration) -> Self {
        Self {
            last_reset: Arc::new(Mutex::new(Instant::now())),
            timeout_duration,
            enabled: true,
        }
    }

    /// Create a disabled heartbeat service (no-op)
    pub fn disabled() -> Self {
        Self {
            last_reset: Arc::new(Mutex::new(Instant::now())),
            timeout_duration: Duration::from_secs(0),
            enabled: false,
        }
    }

    /// Reset the heartbeat timer (called on window events or other activity)
    pub fn reset(&self) {
        if self.enabled {
            if let Ok(mut last_reset) = self.last_reset.lock() {
                *last_reset = Instant::now();
            }
        }
    }

    /// Check if the heartbeat has timed out
    pub fn has_timed_out(&self) -> bool {
        if !self.enabled {
            return false;
        }

        if let Ok(last_reset) = self.last_reset.lock() {
            last_reset.elapsed() > self.timeout_duration
        } else {
            false
        }
    }

    /// Get time since last reset
    pub fn time_since_reset(&self) -> Duration {
        if let Ok(last_reset) = self.last_reset.lock() {
            last_reset.elapsed()
        } else {
            Duration::from_secs(0)
        }
    }

    /// Get a closure that can be used as a heartbeat reset callback
    pub fn reset_callback(&self) -> Box<dyn Fn() + Send + Sync> {
        let last_reset = self.last_reset.clone();
        let enabled = self.enabled;
        
        Box::new(move || {
            if enabled {
                if let Ok(mut reset_time) = last_reset.lock() {
                    *reset_time = Instant::now();
                }
            }
        })
    }

    /// Check if heartbeat is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the configured timeout duration
    pub fn timeout_duration(&self) -> Duration {
        self.timeout_duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_heartbeat_reset() {
        let heartbeat = HeartbeatService::new(Duration::from_millis(100));
        
        // Should not timeout immediately
        assert!(!heartbeat.has_timed_out());
        
        // Wait and check timeout
        thread::sleep(Duration::from_millis(150));
        assert!(heartbeat.has_timed_out());
        
        // Reset should clear timeout
        heartbeat.reset();
        assert!(!heartbeat.has_timed_out());
    }

    #[test]
    fn test_disabled_heartbeat() {
        let heartbeat = HeartbeatService::disabled();
        
        // Disabled heartbeat should never timeout
        assert!(!heartbeat.has_timed_out());
        thread::sleep(Duration::from_millis(50));
        assert!(!heartbeat.has_timed_out());
    }
}
