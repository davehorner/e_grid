use e_grid::{GridClient, GridClientResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Demonstrates focus tracking capabilities of GridClient
/// This example shows how applications can listen for window focus events
/// and maintain state based on which windows are currently focused.

struct FocusTracker {
    // Track which windows have been focused and when
    focus_history: Arc<Mutex<Vec<FocusEvent>>>,
    // Track current focused window
    current_focused: Arc<Mutex<Option<u64>>>,
    // Count focus events per application
    app_focus_counts: Arc<Mutex<HashMap<u64, u32>>>,
    // Statistics
    total_focus_events: Arc<Mutex<u32>>,
}

#[derive(Debug, Clone)]
struct FocusEvent {
    hwnd: u64,
    timestamp: u64,
    event_type: String, // "FOCUSED" or "DEFOCUSED"
    app_name_hash: u64,
    process_id: u32,
}

impl FocusTracker {
    fn new() -> Self {
        Self {
            focus_history: Arc::new(Mutex::new(Vec::new())),
            current_focused: Arc::new(Mutex::new(None)),
            app_focus_counts: Arc::new(Mutex::new(HashMap::new())),
            total_focus_events: Arc::new(Mutex::new(0)),
        }
    }

    fn handle_focus_event(&self, focus_event: e_grid::ipc::WindowFocusEvent) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let event_type = if focus_event.event_type == 0 {
            "FOCUSED"
        } else {
            "DEFOCUSED"
        };

        println!(
            "🎯 [FOCUS TRACKER] {} - Window: {} (PID: {}) Hash: {:x} Time: {}",
            event_type,
            focus_event.hwnd,
            focus_event.process_id,
            focus_event.app_name_hash,
            timestamp
        );

        // Create our internal focus event
        let internal_event = FocusEvent {
            hwnd: focus_event.hwnd,
            timestamp,
            event_type: event_type.to_string(),
            app_name_hash: focus_event.app_name_hash,
            process_id: focus_event.process_id,
        };

        // Update focus history
        if let Ok(mut history) = self.focus_history.lock() {
            history.push(internal_event);
            // Keep only last 100 events to prevent memory growth
            if history.len() > 100 {
                history.remove(0);
            }
        }

        // Update current focused window
        if let Ok(mut current) = self.current_focused.lock() {
            if focus_event.event_type == 0 {
                // FOCUSED
                *current = Some(focus_event.hwnd);
            } else if Some(focus_event.hwnd) == *current {
                // DEFOCUSED and it was the current
                *current = None;
            }
        }

        // Update app focus counts
        if focus_event.event_type == 0 {
            // Only count focus events, not defocus
            if let Ok(mut counts) = self.app_focus_counts.lock() {
                *counts.entry(focus_event.app_name_hash).or_insert(0) += 1;
            }
        }

        // Update total count
        if let Ok(mut total) = self.total_focus_events.lock() {
            *total += 1;
        }
    }

    fn print_statistics(&self) {
        println!("\n📊 === FOCUS TRACKING STATISTICS ===");

        // Current focused window
        if let Ok(current) = self.current_focused.lock() {
            match *current {
                Some(hwnd) => println!("🔍 Currently Focused: Window {}", hwnd),
                None => println!("🔍 Currently Focused: None"),
            }
        }

        // Total events
        if let Ok(total) = self.total_focus_events.lock() {
            println!("📈 Total Focus Events: {}", *total);
        }

        // Top applications by focus count
        if let Ok(counts) = self.app_focus_counts.lock() {
            if !counts.is_empty() {
                println!("🏆 Top Applications by Focus Count:");
                let mut sorted_counts: Vec<_> = counts.iter().collect();
                sorted_counts.sort_by(|a, b| b.1.cmp(a.1));

                for (i, (app_hash, count)) in sorted_counts.iter().take(5).enumerate() {
                    println!("   {}. App {:x}: {} focus events", i + 1, app_hash, count);
                }
            }
        }

        // Recent focus history
        if let Ok(history) = self.focus_history.lock() {
            if !history.is_empty() {
                println!("📜 Recent Focus Events (last 10):");
                for event in history.iter().rev().take(10) {
                    println!(
                        "   {} Window {} (App {:x}) at {}",
                        event.event_type, event.hwnd, event.app_name_hash, event.timestamp
                    );
                }
            }
        }

        println!("=====================================\n");
    }

    fn get_focus_summary(&self) -> String {
        let current = self.current_focused.lock().unwrap();
        let total = self.total_focus_events.lock().unwrap();
        let app_count = self.app_focus_counts.lock().unwrap().len();

        format!(
            "Focus Status: {} events, {} apps, current: {:?}",
            *total, app_count, *current
        )
    }
}

fn main() -> GridClientResult<()> {
    println!("🎯 e_grid Focus Tracking Demonstration");
    println!("======================================");
    println!("This example demonstrates the focus tracking capabilities of GridClient.");
    println!("It will monitor window focus events and provide statistics.\n");

    // Create the focus tracker
    let focus_tracker = Arc::new(FocusTracker::new());

    // Create grid client
    println!("🔧 Creating GridClient...");
    let mut grid_client = GridClient::new()?;

    // Register our focus callback
    println!("🎯 Registering focus tracking callback...");
    let tracker_clone = focus_tracker.clone();
    grid_client.set_focus_callback(move |focus_event| {
        tracker_clone.handle_focus_event(focus_event);
    })?;

    // Start background monitoring
    println!("📡 Starting background monitoring...");
    grid_client.start_background_monitoring().map_err(|e| {
        e_grid::GridClientError::InitializationError(format!("Failed to start monitoring: {}", e))
    })?;

    println!("✅ Focus tracking is now active!");
    println!("💡 Switch between different applications/windows to see focus events");
    println!("📊 Statistics will be displayed every 30 seconds");
    println!("⌨️  Press Ctrl+C to exit\n");

    // Main loop with periodic statistics
    let mut iteration = 0u32;
    loop {
        thread::sleep(Duration::from_secs(5));
        iteration += 1;

        // Print brief status every 5 seconds
        if iteration % 1 == 0 {
            println!(
                "⏱️  [{}s] {}",
                iteration * 5,
                focus_tracker.get_focus_summary()
            );
        }

        // Print detailed statistics every 30 seconds
        if iteration % 6 == 0 {
            focus_tracker.print_statistics();
        }

        // Print reminder every 2 minutes
        if iteration % 24 == 0 {
            println!("💡 Reminder: Switch between different applications to generate focus events");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focus_tracker_creation() {
        let tracker = FocusTracker::new();
        assert_eq!(
            tracker.get_focus_summary(),
            "Focus Status: 0 events, 0 apps, current: None"
        );
    }

    #[test]
    fn test_focus_event_handling() {
        let tracker = FocusTracker::new();

        let focus_event = e_grid::ipc::WindowFocusEvent {
            event_type: 0, // FOCUSED
            hwnd: 12345,
            process_id: 1000,
            timestamp: 1234567890,
            app_name_hash: 0xABCDEF,
            window_title_hash: 0x123456,
            reserved: [0; 2],
        };

        tracker.handle_focus_event(focus_event);

        // Check that current focused window is updated
        let current = tracker.current_focused.lock().unwrap();
        assert_eq!(*current, Some(12345));

        // Check that total count is updated
        let total = tracker.total_focus_events.lock().unwrap();
        assert_eq!(*total, 1);

        // Check that app count is updated
        let app_counts = tracker.app_focus_counts.lock().unwrap();
        assert_eq!(app_counts.get(&0xABCDEF), Some(&1));
    }

    #[test]
    fn test_defocus_event() {
        let tracker = FocusTracker::new();

        // First focus a window
        let focus_event = e_grid::ipc::WindowFocusEvent {
            event_type: 0, // FOCUSED
            hwnd: 12345,
            process_id: 1000,
            timestamp: 1234567890,
            app_name_hash: 0xABCDEF,
            window_title_hash: 0x123456,
            reserved: [0; 2],
        };
        tracker.handle_focus_event(focus_event);

        // Then defocus it
        let defocus_event = e_grid::ipc::WindowFocusEvent {
            event_type: 1, // DEFOCUSED
            hwnd: 12345,
            process_id: 1000,
            timestamp: 1234567891,
            app_name_hash: 0xABCDEF,
            window_title_hash: 0x123456,
            reserved: [0; 2],
        };
        tracker.handle_focus_event(defocus_event);

        // Check that current focused window is cleared
        let current = tracker.current_focused.lock().unwrap();
        assert_eq!(*current, None);

        // Total events should be 2
        let total = tracker.total_focus_events.lock().unwrap();
        assert_eq!(*total, 2);
    }
}
