use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Performance metrics for GridClient operations
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    /// Total events processed
    pub total_events_processed: u64,
    /// Total focus events processed  
    pub total_focus_events_processed: u64,
    /// Total window details processed
    pub total_window_details_processed: u64,
    /// Average event processing time
    pub avg_event_processing_time: Duration,
    /// Peak event processing time
    pub peak_event_processing_time: Duration,
    /// Events processed per second (current rate)
    pub events_per_second: f64,
    /// Current memory usage estimate (bytes)
    pub estimated_memory_usage: usize,
    /// Number of active windows being tracked
    pub active_window_count: usize,
    /// Background thread health status
    pub background_thread_healthy: bool,
    /// Last activity timestamp
    pub last_activity_time: Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_events_processed: 0,
            total_focus_events_processed: 0,
            total_window_details_processed: 0,
            avg_event_processing_time: Duration::from_nanos(0),
            peak_event_processing_time: Duration::from_nanos(0),
            events_per_second: 0.0,
            estimated_memory_usage: 0,
            active_window_count: 0,
            background_thread_healthy: false,
            last_activity_time: Instant::now(),
        }
    }
}

/// Performance monitor for tracking GridClient operations
pub struct PerformanceMonitor {
    metrics: Arc<Mutex<PerformanceMetrics>>,
    event_times: Arc<Mutex<VecDeque<Instant>>>,
    processing_times: Arc<Mutex<VecDeque<Duration>>>,
    start_time: Instant,
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(PerformanceMetrics::default())),
            event_times: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            processing_times: Arc::new(Mutex::new(VecDeque::with_capacity(1000))),
            start_time: Instant::now(),
        }
    }

    /// Record an event being processed
    pub fn record_event(&self, event_type: EventType, processing_time: Duration) {
        if let Ok(mut metrics) = self.metrics.lock() {
            let now = Instant::now();

            // Update counters
            match event_type {
                EventType::WindowEvent => metrics.total_events_processed += 1,
                EventType::FocusEvent => metrics.total_focus_events_processed += 1,
                EventType::WindowDetails => metrics.total_window_details_processed += 1,
            }

            // Update timing metrics
            if processing_time > metrics.peak_event_processing_time {
                metrics.peak_event_processing_time = processing_time;
            }

            metrics.last_activity_time = now;
            metrics.background_thread_healthy = true;
        }

        // Track event timing for rate calculation
        if let Ok(mut event_times) = self.event_times.lock() {
            event_times.push_back(Instant::now());

            // Keep only recent events (last 60 seconds)
            let cutoff = Instant::now() - Duration::from_secs(60);
            while let Some(&front_time) = event_times.front() {
                if front_time < cutoff {
                    event_times.pop_front();
                } else {
                    break;
                }
            }
        }

        // Track processing times for average calculation
        if let Ok(mut processing_times) = self.processing_times.lock() {
            processing_times.push_back(processing_time);

            // Keep only recent processing times (last 1000 events)
            if processing_times.len() > 1000 {
                processing_times.pop_front();
            }
        }

        // Update calculated metrics
        self.update_calculated_metrics();
    }

    /// Update metrics that require calculation
    fn update_calculated_metrics(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            // Calculate events per second
            if let Ok(event_times) = self.event_times.lock() {
                let now = Instant::now();
                let one_second_ago = now - Duration::from_secs(1);
                let recent_events = event_times
                    .iter()
                    .filter(|&&time| time >= one_second_ago)
                    .count();
                metrics.events_per_second = recent_events as f64;
            }

            // Calculate average processing time
            if let Ok(processing_times) = self.processing_times.lock() {
                if !processing_times.is_empty() {
                    let total: Duration = processing_times.iter().sum();
                    metrics.avg_event_processing_time = total / processing_times.len() as u32;
                }
            }
        }
    }

    /// Update window count
    pub fn update_window_count(&self, count: usize) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.active_window_count = count;
        }
    }

    /// Update memory usage estimate
    pub fn update_memory_usage(&self, estimated_bytes: usize) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.estimated_memory_usage = estimated_bytes;
        }
    }

    /// Mark background thread as unhealthy
    pub fn mark_unhealthy(&self) {
        if let Ok(mut metrics) = self.metrics.lock() {
            metrics.background_thread_healthy = false;
        }
    }

    /// Get current performance metrics
    pub fn get_metrics(&self) -> PerformanceMetrics {
        self.update_calculated_metrics();

        if let Ok(metrics) = self.metrics.lock() {
            metrics.clone()
        } else {
            PerformanceMetrics::default()
        }
    }

    /// Get uptime
    pub fn uptime(&self) -> Duration {
        Instant::now() - self.start_time
    }

    /// Generate a performance report
    pub fn generate_report(&self) -> String {
        let metrics = self.get_metrics();
        let uptime = self.uptime();

        format!(
            r#"
🔥 GridClient Performance Report
================================
Uptime: {:.2?}
Background Thread: {}

Event Processing:
  Total Events: {}
  Focus Events: {}  
  Window Details: {}
  Events/sec: {:.1}
  
Timing:
  Avg Processing: {:.2?}
  Peak Processing: {:.2?}
  
System:
  Active Windows: {}
  Memory Usage: {:.1} KB
  Last Activity: {:.2?} ago

Health: {}
"#,
            uptime,
            if metrics.background_thread_healthy {
                "✅ Healthy"
            } else {
                "❌ Unhealthy"
            },
            metrics.total_events_processed,
            metrics.total_focus_events_processed,
            metrics.total_window_details_processed,
            metrics.events_per_second,
            metrics.avg_event_processing_time,
            metrics.peak_event_processing_time,
            metrics.active_window_count,
            metrics.estimated_memory_usage as f64 / 1024.0,
            Instant::now() - metrics.last_activity_time,
            if metrics.background_thread_healthy
                && Instant::now() - metrics.last_activity_time < Duration::from_secs(10)
            {
                "🟢 Excellent"
            } else if metrics.background_thread_healthy {
                "🟡 Good"
            } else {
                "🔴 Poor"
            }
        )
    }

    /// Check if performance is degraded
    pub fn is_performance_degraded(&self) -> bool {
        let metrics = self.get_metrics();

        // Check various performance indicators
        metrics.avg_event_processing_time > Duration::from_millis(100) ||
        metrics.events_per_second > 100.0 || // Too many events
        Instant::now() - metrics.last_activity_time > Duration::from_secs(30) ||
        !metrics.background_thread_healthy
    }
}

/// Types of events that can be processed
#[derive(Debug, Clone, Copy)]
pub enum EventType {
    WindowEvent,
    FocusEvent,
    WindowDetails,
}

/// RAII timer for measuring operation duration
pub struct OperationTimer {
    start_time: Instant,
    monitor: Arc<PerformanceMonitor>,
    event_type: EventType,
}

impl OperationTimer {
    pub fn new(monitor: Arc<PerformanceMonitor>, event_type: EventType) -> Self {
        Self {
            start_time: Instant::now(),
            monitor,
            event_type,
        }
    }
}

impl Drop for OperationTimer {
    fn drop(&mut self) {
        let duration = Instant::now() - self.start_time;
        self.monitor.record_event(self.event_type, duration);
    }
}
