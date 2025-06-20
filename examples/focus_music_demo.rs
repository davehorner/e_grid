use e_grid::{GridClient, GridClientResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Interactive Focus-Based Action Demo
/// This example demonstrates how focus events can trigger different actions
/// based on which application is focused - simulating music control behavior

struct FocusActionManager {
    // Map application hashes to "songs" (just names for demo)
    app_music_map: Arc<Mutex<HashMap<u64, String>>>,
    // Currently "playing" song
    current_song: Arc<Mutex<Option<String>>>,
    // Action history
    action_history: Arc<Mutex<Vec<String>>>,
}

impl FocusActionManager {
    fn new() -> Self {
        let mut music_map = HashMap::new();
        
        // Pre-populate with some common application patterns
        // In a real implementation, these would be learned or configured
        music_map.insert(0, "🎵 Default Desktop Theme".to_string());
        
        Self {
            app_music_map: Arc::new(Mutex::new(music_map)),
            current_song: Arc::new(Mutex::new(None)),
            action_history: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn handle_focus_event(&self, focus_event: e_grid::ipc::WindowFocusEvent) {
        if focus_event.event_type == 0 { // FOCUSED
            self.handle_window_focused(focus_event);
        } else { // DEFOCUSED
            self.handle_window_defocused(focus_event);
        }
    }

    fn handle_window_focused(&self, focus_event: e_grid::ipc::WindowFocusEvent) {
        let app_hash = focus_event.app_name_hash;
        
        println!("🔥 Window {} gained focus", focus_event.hwnd);
        
        // Get or assign a song for this application
        let song = self.get_or_assign_song(app_hash);
        
        // "Play" the song (simulate music control)
        if let Ok(mut current) = self.current_song.lock() {
            *current = Some(song.clone());
        }
        
        println!("🎵 Now playing: {}", song);
        
        // Log the action
        self.log_action(format!("STARTED: {} (Window: {}, App: 0x{:x})", 
                               song, focus_event.hwnd, app_hash));
        
        // Show some visual feedback
        self.print_playback_status();
    }

    fn handle_window_defocused(&self, focus_event: e_grid::ipc::WindowFocusEvent) {
        println!("💤 Window {} lost focus", focus_event.hwnd);
        
        // Pause current playback
        if let Ok(mut current) = self.current_song.lock() {
            if let Some(ref song) = *current {
                println!("⏸️  Paused: {}", song);
                self.log_action(format!("PAUSED: {} (Window: {})", song, focus_event.hwnd));
            }
            *current = None;
        }
        
        self.print_playback_status();
    }

    fn get_or_assign_song(&self, app_hash: u64) -> String {
        if let Ok(mut music_map) = self.app_music_map.lock() {
            // Check if we already have a song for this app
            if let Some(song) = music_map.get(&app_hash) {
                return song.clone();
            }
            
            // Assign a new song based on the app hash
            let song = self.generate_song_for_app(app_hash);
            music_map.insert(app_hash, song.clone());
            
            println!("🆕 Assigned new song to app 0x{:x}: {}", app_hash, song);
            song
        } else {
            "🎵 Default Song".to_string()
        }
    }

    fn generate_song_for_app(&self, app_hash: u64) -> String {
        // Generate a "song" based on the app hash
        // This simulates how e_midi might assign music to different applications
        
        let songs = [
            "🎼 Coding Symphony",
            "🎹 Browser Blues", 
            "🥁 Terminal Beats",
            "🎻 Editor's Sonata",
            "🎺 Communication Jazz",
            "🎸 Gaming Rock",
            "🎵 Office Orchestral",
            "🎤 Creative Chorus",
            "🔊 System Sounds",
            "🎧 Focus Flow",
        ];
        
        let index = (app_hash % songs.len() as u64) as usize;
        songs[index].to_string()
    }

    fn log_action(&self, action: String) {
        if let Ok(mut history) = self.action_history.lock() {
            history.push(action);
            // Keep only last 20 actions
            if history.len() > 20 {
                history.remove(0);
            }
        }
    }

    fn print_playback_status(&self) {
        if let Ok(current) = self.current_song.lock() {
            match current.as_ref() {
                Some(song) => println!("🎶 Status: Playing \"{}\"", song),
                None => println!("⏹️  Status: No music playing"),
            }
        }
        println!(); // Add spacing for readability
    }

    fn print_summary(&self) {
        println!("\n🎵 === FOCUS MUSIC CONTROL SUMMARY ===");
        
        // Current playback status
        if let Ok(current) = self.current_song.lock() {
            match current.as_ref() {
                Some(song) => println!("🎵 Currently Playing: {}", song),
                None => println!("⏹️  Currently Playing: Nothing"),
            }
        }
        
        // Show assigned songs
        if let Ok(music_map) = self.app_music_map.lock() {
            if music_map.len() > 1 { // More than just default
                println!("🎼 Application Music Assignments:");
                for (app_hash, song) in music_map.iter() {
                    if *app_hash != 0 { // Skip default
                        println!("   App 0x{:x}: {}", app_hash, song);
                    }
                }
            }
        }
        
        // Show recent actions
        if let Ok(history) = self.action_history.lock() {
            if !history.is_empty() {
                println!("📜 Recent Actions:");
                for action in history.iter().rev().take(5) {
                    println!("   {}", action);
                }
            }
        }
        
        println!("=====================================\n");
    }
}

fn main() -> GridClientResult<()> {
    println!("🎵 Focus-Based Music Control Demo");
    println!("==================================");
    println!("This demo simulates how e_midi could control music based on window focus.");
    println!("Each application gets assigned a unique 'song' that plays when focused.\n");

    // Create the action manager
    let action_manager = Arc::new(FocusActionManager::new());

    // Create grid client
    println!("🔧 Setting up GridClient...");
    let mut grid_client = GridClient::new()?;

    // Register the focus callback
    println!("🎯 Registering music control callback...");
    let manager_clone = action_manager.clone();
    grid_client.set_focus_callback(move |focus_event| {
        manager_clone.handle_focus_event(focus_event);
    })?;

    // Start monitoring
    println!("📡 Starting focus monitoring...");
    grid_client.start_background_monitoring()
        .map_err(|e| e_grid::GridClientError::InitializationError(format!("Failed to start monitoring: {}", e)))?;

    println!("✅ Music control is now active!");
    println!();
    println!("🎮 How to use:");
    println!("   1. Click on different applications/windows");
    println!("   2. Watch as different 'songs' start playing");
    println!("   3. Notice songs pause when you switch away");
    println!("   4. Each app gets its own unique song assignment");
    println!();
    println!("🎵 Starting focus-based music control...");
    println!("===========================================\n");

    // Main loop with periodic summaries
    let mut iteration = 0u32;
    loop {
        thread::sleep(Duration::from_secs(10));
        iteration += 1;

        // Print summary every 30 seconds
        if iteration % 3 == 0 {
            action_manager.print_summary();
        }

        // Print usage reminder every 2 minutes  
        if iteration % 12 == 0 {
            println!("💡 Tip: Try switching between different applications to hear different songs!");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_manager_creation() {
        let manager = FocusActionManager::new();
        let music_map = manager.app_music_map.lock().unwrap();
        assert!(music_map.contains_key(&0)); // Default song should exist
    }

    #[test]
    fn test_song_assignment() {
        let manager = FocusActionManager::new();
        
        // Test that same app hash gets same song
        let song1 = manager.get_or_assign_song(0x12345);
        let song2 = manager.get_or_assign_song(0x12345);
        assert_eq!(song1, song2);
        
        // Test that different app hash gets different song
        let song3 = manager.get_or_assign_song(0x67890);
        assert_ne!(song1, song3);
    }

    #[test]
    fn test_focus_event_handling() {
        let manager = FocusActionManager::new();
        
        let focus_event = e_grid::ipc::WindowFocusEvent {
            event_type: 0, // FOCUSED
            hwnd: 12345,
            process_id: 1000,
            timestamp: 1234567890,
            app_name_hash: 0xABCDEF,
            window_title_hash: 0x123456,
            reserved: [0; 2],
        };

        manager.handle_focus_event(focus_event);

        // Check that a song is now playing
        let current = manager.current_song.lock().unwrap();
        assert!(current.is_some());
    }
}
