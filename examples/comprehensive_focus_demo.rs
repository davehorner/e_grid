use e_grid::{GridClient, GridClientResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Comprehensive Focus Tracking Demonstration
/// This example showcases all the focus tracking capabilities of e_grid's GridClient
/// It combines statistics, music control simulation, and detailed event logging

struct ComprehensiveFocusTracker {
    // Event tracking
    focus_history: Arc<Mutex<Vec<FocusEvent>>>,
    current_focused: Arc<Mutex<Option<FocusedWindow>>>,

    // Statistics
    app_focus_counts: Arc<Mutex<HashMap<u64, u32>>>,
    app_focus_time: Arc<Mutex<HashMap<u64, u64>>>, // Total seconds focused per app
    total_focus_events: Arc<Mutex<u32>>,
    session_start_time: u64,

    // Music simulation (like e_midi integration would use)
    app_music_map: Arc<Mutex<HashMap<u64, String>>>,
    current_song: Arc<Mutex<Option<String>>>,
    song_changes: Arc<Mutex<u32>>,

    // Application identification
    app_names: Arc<Mutex<HashMap<u64, String>>>, // For display purposes
}

#[derive(Debug, Clone)]
struct FocusEvent {
    hwnd: u64,
    timestamp: u64,
    event_type: String,
    app_name_hash: u64,
    window_title_hash: u64,
    process_id: u32,
}

#[derive(Debug, Clone)]
struct FocusedWindow {
    hwnd: u64,
    app_name_hash: u64,
    process_id: u32,
    focus_start_time: u64,
}

impl ComprehensiveFocusTracker {
    fn new() -> Self {
        let session_start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            focus_history: Arc::new(Mutex::new(Vec::new())),
            current_focused: Arc::new(Mutex::new(None)),
            app_focus_counts: Arc::new(Mutex::new(HashMap::new())),
            app_focus_time: Arc::new(Mutex::new(HashMap::new())),
            total_focus_events: Arc::new(Mutex::new(0)),
            session_start_time: session_start,
            app_music_map: Arc::new(Mutex::new(HashMap::new())),
            current_song: Arc::new(Mutex::new(None)),
            song_changes: Arc::new(Mutex::new(0)),
            app_names: Arc::new(Mutex::new(HashMap::new())),
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

        // Create our internal focus event
        let internal_event = FocusEvent {
            hwnd: focus_event.hwnd,
            timestamp,
            event_type: event_type.to_string(),
            app_name_hash: focus_event.app_name_hash,
            window_title_hash: focus_event.window_title_hash,
            process_id: focus_event.process_id,
        };

        // Print real-time event with enhanced formatting
        self.print_realtime_event(&internal_event);

        // Update focus history
        self.update_focus_history(internal_event);

        // Handle focus/defocus logic
        if focus_event.event_type == 0 {
            self.handle_window_focused(focus_event, timestamp);
        } else {
            self.handle_window_defocused(focus_event, timestamp);
        }

        // Update total event count
        if let Ok(mut total) = self.total_focus_events.lock() {
            *total += 1;
        }
    }

    fn print_realtime_event(&self, event: &FocusEvent) {
        let emoji = if event.event_type == "FOCUSED" {
            "🟢"
        } else {
            "🔴"
        };
        let app_name = self.get_app_display_name(event.app_name_hash);

        println!(
            "{} {} - {} | PID: {} | HWND: {} | Time: {}",
            emoji, event.event_type, app_name, event.process_id, event.hwnd, event.timestamp
        );

        // Show hashes for technical users
        if event.app_name_hash != 0 {
            println!(
                "   📱 App Hash: 0x{:x} | Title Hash: 0x{:x}",
                event.app_name_hash, event.window_title_hash
            );
        }

        println!("   ─────────────────────────────────────");
    }

    fn get_app_display_name(&self, app_hash: u64) -> String {
        if let Ok(app_names) = self.app_names.lock() {
            if let Some(name) = app_names.get(&app_hash) {
                return name.clone();
            }
        }

        // Generate a readable name based on hash
        let app_names = [
            "Code Editor",
            "Web Browser",
            "Terminal",
            "File Manager",
            "Music Player",
            "Video Player",
            "Chat App",
            "Email Client",
            "System Tool",
            "Game",
            "Office App",
            "Design Tool",
            "Developer Tool",
            "System Monitor",
            "Desktop",
        ];

        let index = (app_hash % app_names.len() as u64) as usize;
        let display_name = format!("{} (0x{:x})", app_names[index], app_hash);

        // Cache it for future use
        if let Ok(mut names) = self.app_names.lock() {
            names.insert(app_hash, display_name.clone());
        }

        display_name
    }

    fn update_focus_history(&self, event: FocusEvent) {
        if let Ok(mut history) = self.focus_history.lock() {
            history.push(event);
            // Keep only last 50 events to prevent memory growth
            if history.len() > 50 {
                history.remove(0);
            }
        }
    }

    fn handle_window_focused(&self, focus_event: e_grid::ipc::WindowFocusEvent, timestamp: u64) {
        // Update current focused window
        let focused_window = FocusedWindow {
            hwnd: focus_event.hwnd,
            app_name_hash: focus_event.app_name_hash,
            process_id: focus_event.process_id,
            focus_start_time: timestamp,
        };

        if let Ok(mut current) = self.current_focused.lock() {
            *current = Some(focused_window);
        }

        // Update app focus counts
        if let Ok(mut counts) = self.app_focus_counts.lock() {
            *counts.entry(focus_event.app_name_hash).or_insert(0) += 1;
        }

        // Handle music simulation
        self.start_music_for_app(focus_event.app_name_hash);
    }

    fn handle_window_defocused(&self, focus_event: e_grid::ipc::WindowFocusEvent, timestamp: u64) {
        // Update focus time tracking
        if let Ok(mut current) = self.current_focused.lock() {
            if let Some(ref focused) = *current {
                if focused.hwnd == focus_event.hwnd {
                    let focus_duration = timestamp - focused.focus_start_time;

                    // Add to total focus time for this app
                    if let Ok(mut times) = self.app_focus_time.lock() {
                        *times.entry(focused.app_name_hash).or_insert(0) += focus_duration;
                    }

                    *current = None;
                }
            }
        }

        // Stop music
        self.stop_current_music();
    }

    fn start_music_for_app(&self, app_hash: u64) {
        let song = self.get_or_assign_song(app_hash);

        if let Ok(mut current) = self.current_song.lock() {
            if current.as_ref() != Some(&song) {
                *current = Some(song.clone());

                // Update song change counter
                if let Ok(mut changes) = self.song_changes.lock() {
                    *changes += 1;
                }

                println!("🎵 Now playing: {}", song);
            }
        }
    }

    fn stop_current_music(&self) {
        if let Ok(mut current) = self.current_song.lock() {
            if let Some(ref song) = *current {
                println!("⏸️  Paused: {}", song);
            }
            *current = None;
        }
    }

    fn get_or_assign_song(&self, app_hash: u64) -> String {
        if let Ok(mut music_map) = self.app_music_map.lock() {
            if let Some(song) = music_map.get(&app_hash) {
                return song.clone();
            }

            let song = self.generate_song_for_app(app_hash);
            music_map.insert(app_hash, song.clone());

            println!(
                "🆕 Assigned new song to {}: {}",
                self.get_app_display_name(app_hash),
                song
            );
            song
        } else {
            "🎵 Default Song".to_string()
        }
    }

    fn generate_song_for_app(&self, app_hash: u64) -> String {
        let songs = [
            "🎼 Coding Symphony in C Major",
            "🎹 Browser Blues & Scrolling Jazz",
            "🥁 Terminal Beats & Command Line Rhythms",
            "🎻 Editor's Sonata in Text Minor",
            "🎺 Communication Concerto",
            "🎸 Gaming Rock & Victory Anthems",
            "🎵 Office Orchestral Suite",
            "🎤 Creative Chorus & Design Dreams",
            "🔊 System Sounds & Process Harmony",
            "🎧 Focus Flow & Deep Work Meditation",
            "🎼 File Manager Waltz",
            "🎹 Email Ensemble & Message Melodies",
            "🥁 Video Player Soundtrack Collection",
            "🎻 Chat Application Chamber Music",
            "🎺 Desktop Ambient Soundscape",
        ];

        let index = (app_hash % songs.len() as u64) as usize;
        songs[index].to_string()
    }

    fn print_comprehensive_statistics(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let session_duration = now - self.session_start_time;

        println!("\n🔥 ===== COMPREHENSIVE FOCUS TRACKING REPORT =====");
        println!(
            "📊 Session Duration: {} seconds ({} minutes)",
            session_duration,
            session_duration / 60
        );

        // Current status
        if let Ok(current) = self.current_focused.lock() {
            match current.as_ref() {
                Some(focused) => {
                    let app_name = self.get_app_display_name(focused.app_name_hash);
                    let focus_duration = now - focused.focus_start_time;
                    println!(
                        "🔍 Currently Focused: {} (for {} seconds)",
                        app_name, focus_duration
                    );
                }
                None => println!("🔍 Currently Focused: None"),
            }
        }

        // Music status
        if let Ok(current_song) = self.current_song.lock() {
            match current_song.as_ref() {
                Some(song) => println!("🎵 Currently Playing: {}", song),
                None => println!("⏹️  Music Status: Stopped"),
            }
        }

        // Total statistics
        if let Ok(total) = self.total_focus_events.lock() {
            println!("📈 Total Focus Events: {}", *total);
        }

        if let Ok(changes) = self.song_changes.lock() {
            println!("🎶 Song Changes: {}", *changes);
        }

        // Application statistics
        self.print_app_statistics();

        // Recent history
        self.print_recent_history();

        // Music assignments
        self.print_music_assignments();

        println!("================================================\n");
    }

    fn print_app_statistics(&self) {
        println!("\n🏆 APPLICATION STATISTICS:");

        // Focus counts
        if let Ok(counts) = self.app_focus_counts.lock() {
            if !counts.is_empty() {
                println!("   📊 Focus Count Ranking:");
                let mut sorted_counts: Vec<_> = counts.iter().collect();
                sorted_counts.sort_by(|a, b| b.1.cmp(a.1));
                for (i, (app_hash, count)) in sorted_counts.iter().take(10).enumerate() {
                    let app_name = self.get_app_display_name(**app_hash);
                    println!("      {}. {}: {} times", i + 1, app_name, count);
                }
            }
        }

        // Focus time
        if let Ok(times) = self.app_focus_time.lock() {
            if !times.is_empty() {
                println!("   ⏱️  Focus Time Ranking:");
                let mut sorted_times: Vec<_> = times.iter().collect();
                sorted_times.sort_by(|a, b| b.1.cmp(a.1));
                for (i, (app_hash, time)) in sorted_times.iter().take(10).enumerate() {
                    let app_name = self.get_app_display_name(**app_hash);
                    let minutes = *time / 60;
                    let seconds = *time % 60;
                    println!("      {}. {}: {}m {}s", i + 1, app_name, minutes, seconds);
                }
            }
        }
    }

    fn print_recent_history(&self) {
        if let Ok(history) = self.focus_history.lock() {
            if !history.is_empty() {
                println!("   📜 Recent Events (last 10):");
                for event in history.iter().rev().take(10) {
                    let app_name = self.get_app_display_name(event.app_name_hash);
                    println!(
                        "      {} {} at {} (PID: {})",
                        event.event_type, app_name, event.timestamp, event.process_id
                    );
                }
            }
        }
    }

    fn print_music_assignments(&self) {
        if let Ok(music_map) = self.app_music_map.lock() {
            if !music_map.is_empty() {
                println!("   🎼 Music Assignments:");
                for (app_hash, song) in music_map.iter() {
                    let app_name = self.get_app_display_name(*app_hash);
                    println!("      {}: {}", app_name, song);
                }
            }
        }
    }

    fn get_session_summary(&self) -> String {
        let total = if let Ok(total) = self.total_focus_events.lock() {
            *total
        } else {
            0
        };
        let app_count = if let Ok(counts) = self.app_focus_counts.lock() {
            counts.len()
        } else {
            0
        };
        let song_changes = if let Ok(changes) = self.song_changes.lock() {
            *changes
        } else {
            0
        };

        let current_info = if let Ok(current) = self.current_focused.lock() {
            match current.as_ref() {
                Some(focused) => {
                    let app_name = self.get_app_display_name(focused.app_name_hash);
                    format!("focused on {}", app_name)
                }
                None => "no focus".to_string(),
            }
        } else {
            "unknown".to_string()
        };

        format!(
            "📊 Session: {} events, {} apps, {} song changes, currently {}",
            total, app_count, song_changes, current_info
        )
    }
}

fn main() -> GridClientResult<()> {
    println!("🎯 Comprehensive Focus Tracking Demonstration");
    println!("==============================================");
    println!("This demo showcases ALL focus tracking capabilities:");
    println!("• Real-time event monitoring with app identification");
    println!("• Statistical analysis (counts, time tracking, rankings)");
    println!("• Music control simulation (like e_midi integration)");
    println!("• Comprehensive reporting and history tracking");
    println!();

    // Create the comprehensive tracker
    let focus_tracker = Arc::new(ComprehensiveFocusTracker::new());

    // Create grid client
    println!("🔧 Initializing GridClient with comprehensive tracking...");
    let mut grid_client = GridClient::new()?;

    // Register our comprehensive focus callback
    println!("🎯 Registering comprehensive focus tracking callback...");
    let tracker_clone = focus_tracker.clone();
    grid_client.set_focus_callback(move |focus_event| {
        println!("🔄 Focus Event Received: {:?}", focus_event);
        tracker_clone.handle_focus_event(focus_event);
    })?;

    // Start background monitoring
    println!("📡 Starting comprehensive focus monitoring...");
    grid_client.start_background_monitoring().map_err(|e| {
        e_grid::GridClientError::InitializationError(format!("Failed to start monitoring: {}", e))
    })?;

    println!("✅ Comprehensive focus tracking is now active!");
    println!();
    println!("🎮 Interactive Features:");
    println!("   • Switch between applications to see real-time events");
    println!("   • Watch automatic music assignments for new apps");
    println!("   • Observe focus time tracking and statistics");
    println!("   • See comprehensive reports every 60 seconds");
    println!();
    println!("📊 Status updates every 10 seconds, full reports every 60 seconds");
    println!("⌨️  Press Ctrl+C to exit");
    println!("==============================================\n");

    // Main loop with different reporting intervals
    let mut iteration = 0u32;
    loop {
        thread::sleep(Duration::from_secs(10));
        iteration += 1;

        // Brief status every 10 seconds
        println!(
            "⏱️  [{}s] {}",
            iteration * 10,
            focus_tracker.get_session_summary()
        );

        // Comprehensive report every 60 seconds
        if iteration % 6 == 0 {
            focus_tracker.print_comprehensive_statistics();
        }

        // Usage reminders
        if iteration % 12 == 0 {
            // Every 2 minutes
            println!("💡 Tip: Try switching between different applications to see the full power of focus tracking!");
        }

        if iteration % 18 == 0 {
            // Every 3 minutes
            println!("🎵 Notice how each application gets its own unique song assignment!");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comprehensive_tracker_creation() {
        let tracker = ComprehensiveFocusTracker::new();
        assert!(tracker.session_start_time > 0);

        let summary = tracker.get_session_summary();
        assert!(summary.contains("0 events"));
        assert!(summary.contains("0 apps"));
    }

    #[test]
    fn test_app_display_name_generation() {
        let tracker = ComprehensiveFocusTracker::new();

        let name1 = tracker.get_app_display_name(0x12345);
        let name2 = tracker.get_app_display_name(0x12345); // Same hash
        let name3 = tracker.get_app_display_name(0x67890); // Different hash

        assert_eq!(name1, name2); // Same hash should give same name
        assert_ne!(name1, name3); // Different hash should give different name
        assert!(name1.contains("0x12345")); // Should include hash
    }

    #[test]
    fn test_song_assignment() {
        let tracker = ComprehensiveFocusTracker::new();

        let song1 = tracker.get_or_assign_song(0xABCDEF);
        let song2 = tracker.get_or_assign_song(0xABCDEF); // Same app
        let song3 = tracker.get_or_assign_song(0x123456); // Different app

        assert_eq!(song1, song2); // Same app should get same song
        assert_ne!(song1, song3); // Different apps should get different songs
    }

    #[test]
    fn test_focus_event_handling() {
        let tracker = ComprehensiveFocusTracker::new();
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

        // Check that tracking state is updated
        let current = tracker.current_focused.lock().unwrap();
        assert!(current.is_some());
        assert_eq!(current.as_ref().unwrap().hwnd, 12345);

        let total = tracker.total_focus_events.lock().unwrap();
        assert_eq!(*total, 1);
    }
}
