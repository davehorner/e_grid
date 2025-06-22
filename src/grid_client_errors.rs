use log::warn;
use std::fmt;

/// Custom error types for GridClient operations
#[derive(Debug)]
pub enum GridClientError {
    /// IPC communication errors
    IpcError(String),
    /// Grid state lock contention or mutex errors
    LockError(String),
    /// Invalid grid coordinates
    InvalidCoordinates {
        row: u32,
        col: u32,
        max_row: u32,
        max_col: u32,
    },
    /// Monitor detection or management errors
    MonitorError(String),
    /// Configuration errors
    ConfigError(String),
    /// Focus event callback errors
    FocusCallbackError(String),
    /// Generic initialization errors
    InitializationError(String),
}

impl fmt::Display for GridClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GridClientError::IpcError(msg) => write!(f, "IPC communication failed: {}", msg),
            GridClientError::LockError(msg) => write!(f, "Grid state lock error: {}", msg),
            GridClientError::InvalidCoordinates {
                row,
                col,
                max_row,
                max_col,
            } => {
                write!(
                    f,
                    "Invalid grid coordinates ({}, {}) - grid size is {}x{}",
                    row, col, max_row, max_col
                )
            }
            GridClientError::MonitorError(msg) => write!(f, "Monitor detection failed: {}", msg),
            GridClientError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            GridClientError::FocusCallbackError(msg) => write!(f, "Focus callback error: {}", msg),
            GridClientError::InitializationError(msg) => {
                write!(f, "Initialization failed: {}", msg)
            }
        }
    }
}

impl std::error::Error for GridClientError {}

/// Convert from generic boxed errors
impl From<Box<dyn std::error::Error>> for GridClientError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        GridClientError::InitializationError(err.to_string())
    }
}

/// Result type alias for GridClient operations
pub type GridClientResult<T> = Result<T, GridClientError>;

/// Retry configuration for IPC operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub backoff_multiplier: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 100,
            backoff_multiplier: 2.0,
        }
    }
}

/// Retry an operation with exponential backoff
pub fn retry_with_backoff<T, E, F>(mut operation: F, config: &RetryConfig) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
    E: std::fmt::Debug,
{
    let mut last_error = None;

    for attempt in 1..=config.max_attempts {
        match operation() {
            Ok(result) => return Ok(result),
            Err(error) => {
                warn!(
                    "⚠️ Operation failed on attempt {}/{}: {:?}",
                    attempt, config.max_attempts, error
                );
                last_error = Some(error);

                if attempt < config.max_attempts {
                    let delay_ms = (config.base_delay_ms as f32
                        * config.backoff_multiplier.powi((attempt - 1) as i32))
                        as u64;
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// Validate grid coordinates
pub fn validate_grid_coordinates(
    row: u32,
    col: u32,
    max_row: u32,
    max_col: u32,
) -> GridClientResult<()> {
    if row >= max_row || col >= max_col {
        Err(GridClientError::InvalidCoordinates {
            row,
            col,
            max_row,
            max_col,
        })
    } else {
        Ok(())
    }
}

/// Safe mutex lock wrapper
pub fn safe_lock<'a, T>(
    mutex: &'a std::sync::Mutex<T>,
    context: &str,
) -> GridClientResult<std::sync::MutexGuard<'a, T>> {
    mutex
        .lock()
        .map_err(|_| GridClientError::LockError(format!("Failed to acquire lock for {}", context)))
}

/// Safe Arc<Mutex<T>> lock wrapper
pub fn safe_arc_lock<'a, T>(
    arc_mutex: &'a std::sync::Arc<std::sync::Mutex<T>>,
    context: &str,
) -> GridClientResult<std::sync::MutexGuard<'a, T>> {
    arc_mutex.lock().map_err(|_| {
        GridClientError::LockError(format!("Failed to acquire arc lock for {}", context))
    })
}
