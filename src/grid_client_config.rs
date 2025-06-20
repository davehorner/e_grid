use crate::GridConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Comprehensive configuration for GridClient
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridClientConfig {
    pub grid: GridConfig,
    pub display: DisplayConfig,
    pub performance: PerformanceConfig,
    pub ipc: IpcConfig,
    pub focus_events: FocusEventConfig,
}

impl Default for GridClientConfig {
    fn default() -> Self {
        Self {
            grid: GridConfig::default(),
            display: DisplayConfig::default(),
            performance: PerformanceConfig::default(),
            ipc: IpcConfig::default(),
            focus_events: FocusEventConfig::default(),
        }
    }
}

/// Display and output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    /// Enable automatic grid display on updates
    pub auto_display: bool,
    /// Minimum time between automatic displays (ms)
    pub throttle_ms: u64,
    /// Maximum lines to output in grid display
    pub max_output_lines: usize,
    /// Show debug information
    pub show_debug_info: bool,
    /// Show performance metrics
    pub show_performance_metrics: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            auto_display: true,
            throttle_ms: 1000,
            max_output_lines: 50,
            show_debug_info: false,
            show_performance_metrics: false,
        }
    }
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Maximum events to process in one batch
    pub event_batch_size: usize,
    /// Processing interval for background thread (ms)  
    pub processing_interval_ms: u64,
    /// Idle processing interval when no activity (ms)
    pub idle_interval_ms: u64,
    /// Maximum event history to retain
    pub max_event_history: usize,
    /// Status reporting interval (seconds)
    pub status_report_interval_secs: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            event_batch_size: 10,
            processing_interval_ms: 100,
            idle_interval_ms: 500,
            max_event_history: 1000,
            status_report_interval_secs: 30,
        }
    }
}

/// IPC communication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    /// Enable retry logic for IPC operations
    pub enable_retry: bool,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Base delay for retry backoff (ms)
    pub retry_base_delay_ms: u64,
    /// Backoff multiplier for retries
    pub retry_backoff_multiplier: f32,
    /// Timeout for IPC operations (ms)
    pub operation_timeout_ms: u64,
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            enable_retry: true,
            max_retry_attempts: 3,
            retry_base_delay_ms: 100,
            retry_backoff_multiplier: 2.0,
            operation_timeout_ms: 5000,
        }
    }
}

/// Focus event system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusEventConfig {
    /// Enable focus event processing
    pub enabled: bool,
    /// Maximum focus events to process in one batch
    pub batch_size: usize,
    /// Enable focus event history
    pub enable_history: bool,
    /// Maximum focus events to keep in history
    pub max_history_size: usize,
    /// Minimum time between focus events for same window (ms)
    pub debounce_ms: u64,
}

impl Default for FocusEventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            batch_size: 5,
            enable_history: true,
            max_history_size: 100,
            debounce_ms: 250,
        }
    }
}

impl GridClientConfig {
    /// Load configuration from file
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: GridClientConfig = serde_json::from_str(&contents)?;
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save_to_file(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }
    
    /// Load configuration from environment variables with fallback to defaults
    pub fn from_env() -> Self {
        let mut config = Self::default();
        
        // Grid configuration
        if let Ok(rows) = std::env::var("GRID_ROWS") {
            if let Ok(rows) = rows.parse::<usize>() {
                config.grid.rows = rows;
            }
        }
        if let Ok(cols) = std::env::var("GRID_COLS") {
            if let Ok(cols) = cols.parse::<usize>() {
                config.grid.cols = cols;
            }
        }
        
        // Display configuration
        if let Ok(auto_display) = std::env::var("GRID_AUTO_DISPLAY") {
            config.display.auto_display = auto_display.to_lowercase() == "true";
        }
        if let Ok(debug) = std::env::var("GRID_DEBUG") {
            config.display.show_debug_info = debug.to_lowercase() == "true";
        }
        
        // Performance configuration
        if let Ok(batch_size) = std::env::var("GRID_BATCH_SIZE") {
            if let Ok(batch_size) = batch_size.parse::<usize>() {
                config.performance.event_batch_size = batch_size;
            }
        }
        
        // Focus event configuration
        if let Ok(focus_enabled) = std::env::var("GRID_FOCUS_EVENTS_ENABLED") {
            config.focus_events.enabled = focus_enabled.to_lowercase() == "true";
        }
        
        config
    }
    
    /// Get processing interval as Duration
    pub fn processing_interval(&self) -> Duration {
        Duration::from_millis(self.performance.processing_interval_ms)
    }
    
    /// Get idle interval as Duration
    pub fn idle_interval(&self) -> Duration {
        Duration::from_millis(self.performance.idle_interval_ms)
    }
    
    /// Get throttle interval as Duration
    pub fn display_throttle(&self) -> Duration {
        Duration::from_millis(self.display.throttle_ms)
    }
    
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.grid.rows == 0 || self.grid.cols == 0 {
            return Err("Grid dimensions must be positive".into());
        }
        
        if self.performance.event_batch_size == 0 {
            return Err("Event batch size must be positive".into());
        }
        
        if self.ipc.max_retry_attempts == 0 {
            return Err("Max retry attempts must be positive".into());
        }
        
        if self.focus_events.batch_size == 0 {
            return Err("Focus event batch size must be positive".into());
        }
        
        Ok(())
    }
}
