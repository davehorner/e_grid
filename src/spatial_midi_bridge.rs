//! E-GRID Integration Bridge for MIDI Control
//!
//! This module implements the e_grid side of the grid-music integration,
//! translating window events into spatial MIDI events and handling
//! configuration for the integration.

use crate::{WindowEvent, WindowCommand, WindowResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Integration events sent from e_grid to e_midi
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpatialMidiEvent {
    /// Grid-based events (from e_grid to e_midi)
    GridCellOccupied {
        row: u32,
        col: u32,
        window_id: u64,
        app_name: String,
        timestamp: u64,
    },
    GridCellFreed {
        row: u32,
        col: u32,
        window_id: u64,
        timestamp: u64,
    },
    GridPatternDetected {
        pattern_id: String,
        cells: Vec<(u32, u32)>,
        confidence: f32,
        timestamp: u64,
    },
    
    /// Application context events
    ApplicationFocused {
        app_name: String,
        window_title: String,
        grid_position: Option<(u32, u32)>,
        timestamp: u64,
    },
    WorkflowDetected {
        workflow_type: String, // "coding", "creative", "research", etc.
        active_apps: Vec<String>,
        timestamp: u64,
    },
    
    /// Activity-based events
    DesktopActivityLevel {
        level: f32, // 0.0 = idle, 1.0 = very active
        window_count: u32,
        recent_changes: u32,
        timestamp: u64,
    },
}

/// Commands received from e_midi for visual feedback
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpatialMidiCommand {
    /// Visual feedback commands
    HighlightMusicCells {
        cells: Vec<(u32, u32)>,
        color: String,
        duration_ms: u32,
    },
    ShowMusicState {
        is_playing: bool,
        song_name: String,
        intensity: f32,
    },
    SetCellMusicMapping {
        row: u32,
        col: u32,
        song_index: usize,
        enabled: bool,
    },
}

/// Grid-side integration manager
pub struct GridIntegrationBridge {
    enabled: bool,
    activity_tracker: ActivityTracker,
    pattern_detector: PatternDetector,
    midi_publisher: Option<iceoryx2::port::publisher::Publisher<SpatialMidiEvent>>,
    command_subscriber: Option<iceoryx2::port::subscriber::Subscriber<SpatialMidiCommand>>,
    cell_mappings: HashMap<(u32, u32), CellMapping>,
}

#[derive(Debug, Clone)]
struct CellMapping {
    song_index: Option<usize>,
    app_filter: Option<String>,
    enabled: bool,
}

struct ActivityTracker {
    window_events: Vec<TimestampedEvent>,
    last_calculation: Instant,
    current_level: f32,
}

struct PatternDetector {
    known_patterns: HashMap<String, Vec<(u32, u32)>>,
    recent_occupations: Vec<(u32, u32, Instant)>,
}

#[derive(Debug, Clone)]
struct TimestampedEvent {
    timestamp: Instant,
    event_type: String,
}

impl GridIntegrationBridge {
    pub fn new() -> Self {
        Self {
            enabled: false,
            activity_tracker: ActivityTracker::new(),
            pattern_detector: PatternDetector::new(),
            midi_publisher: None,
            command_subscriber: None,
            cell_mappings: HashMap::new(),
        }
    }

    pub fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize iceoryx2 services for spatial MIDI communication
        let node = iceoryx2::node::NodeBuilder::new()
            .name("e_grid_midi_bridge")
            .create::<iceoryx2::service::zero_copy::Service>()?;

        // Set up publisher for spatial MIDI events
        let service = node.service_builder("spatial_midi_events")
            .publish_subscribe::<SpatialMidiEvent>()
            .create()?;

        let publisher = service.publisher_builder().create()?;
        self.midi_publisher = Some(publisher);

        // Set up subscriber for MIDI commands (visual feedback)
        let cmd_service = node.service_builder("spatial_midi_commands")
            .publish_subscribe::<SpatialMidiCommand>()
            .create()?;

        let subscriber = cmd_service.subscriber_builder().create()?;
        self.command_subscriber = Some(subscriber);

        self.enabled = true;
        println!("🌉 Grid-MIDI integration bridge initialized");
        Ok(())
    }

    pub fn process_window_event(&mut self, event: WindowEvent) -> Result<(), Box<dyn std::error::Error>> {
        if !self.enabled {
            return Ok(());
        }

        // Track activity
        self.activity_tracker.add_event(&format!("window_{}", event.event_type));

        // Convert window event to spatial MIDI event
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

        match event.event_type {
            0 => { // Window created/moved to cell
                let spatial_event = SpatialMidiEvent::GridCellOccupied {
                    row: event.row,
                    col: event.col,
                    window_id: event.hwnd,
                    app_name: self.get_app_name_from_hwnd(event.hwnd),
                    timestamp,
                };
                
                self.publish_spatial_event(spatial_event)?;
                
                // Update pattern detector
                self.pattern_detector.add_occupation(event.row, event.col);
                
                // Check for patterns
                if let Some(pattern) = self.pattern_detector.detect_pattern() {
                    let pattern_event = SpatialMidiEvent::GridPatternDetected {
                        pattern_id: pattern.0,
                        cells: pattern.1,
                        confidence: pattern.2,
                        timestamp,
                    };
                    self.publish_spatial_event(pattern_event)?;
                }
            },
            
            1 => { // Window destroyed/moved from cell
                let spatial_event = SpatialMidiEvent::GridCellFreed {
                    row: event.old_row,
                    col: event.old_col,
                    window_id: event.hwnd,
                    timestamp,
                };
                
                self.publish_spatial_event(spatial_event)?;
            },
            
            _ => {}
        }

        // Calculate and publish activity level
        let activity_level = self.activity_tracker.calculate_activity_level();
        if activity_level != self.activity_tracker.current_level {
            let activity_event = SpatialMidiEvent::DesktopActivityLevel {
                level: activity_level,
                window_count: event.total_windows,
                recent_changes: self.activity_tracker.window_events.len() as u32,
                timestamp,
            };
            
            self.publish_spatial_event(activity_event)?;
        }

        Ok(())
    }

    pub fn process_midi_commands(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref subscriber) = self.command_subscriber {
            while let Some(sample) = subscriber.receive()? {
                match &sample.payload {
                    SpatialMidiCommand::HighlightMusicCells { cells, color, duration_ms } => {
                        println!("🎨 Highlighting cells {:?} with {} for {}ms", cells, color, duration_ms);
                        // Implement visual highlighting in grid display
                        self.highlight_grid_cells(cells, color, *duration_ms)?;
                    },
                    
                    SpatialMidiCommand::ShowMusicState { is_playing, song_name, intensity } => {
                        println!("🎵 Music state: {} - {} (intensity: {})", 
                                 if *is_playing { "Playing" } else { "Stopped" }, 
                                 song_name, intensity);
                        // Update grid display with music state
                    },
                    
                    SpatialMidiCommand::SetCellMusicMapping { row, col, song_index, enabled } => {
                        let mapping = CellMapping {
                            song_index: Some(*song_index),
                            app_filter: None,
                            enabled: *enabled,
                        };
                        self.cell_mappings.insert((*row, *col), mapping);
                        println!("🎼 Cell ({},{}) mapped to song {}", row, col, song_index);
                    }
                }
            }
        }
        Ok(())
    }

    fn publish_spatial_event(&self, event: SpatialMidiEvent) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(ref publisher) = self.midi_publisher {
            let sample = publisher.loan_uninit()?;
            let sample = sample.write_payload(event);
            sample.send()?;
        }
        Ok(())
    }

    fn get_app_name_from_hwnd(&self, hwnd: u64) -> String {
        // This is a simplified version - real implementation would query the window
        // for its process name or executable path
        format!("app_{}", hwnd % 1000) // Placeholder
    }

    fn highlight_grid_cells(&self, _cells: &[(u32, u32)], _color: &str, _duration_ms: u32) -> Result<(), Box<dyn std::error::Error>> {
        // Implement grid cell highlighting
        // This would integrate with the terminal display system
        Ok(())
    }

    pub fn shutdown(&mut self) {
        self.enabled = false;
        self.midi_publisher = None;
        self.command_subscriber = None;
        println!("🌉 Grid-MIDI integration bridge shutdown");
    }
}

impl ActivityTracker {
    fn new() -> Self {
        Self {
            window_events: Vec::new(),
            last_calculation: Instant::now(),
            current_level: 0.0,
        }
    }

    fn add_event(&mut self, event_type: &str) {
        self.window_events.push(TimestampedEvent {
            timestamp: Instant::now(),
            event_type: event_type.to_string(),
        });

        // Keep only recent events (last 30 seconds)
        let cutoff = Instant::now() - Duration::from_secs(30);
        self.window_events.retain(|e| e.timestamp > cutoff);
    }

    fn calculate_activity_level(&mut self) -> f32 {
        let now = Instant::now();
        if now.duration_since(self.last_calculation) < Duration::from_secs(1) {
            return self.current_level;
        }

        // Calculate activity based on recent events
        let recent_events = self.window_events.len() as f32;
        let max_events = 20.0; // Normalize to this maximum
        
        self.current_level = (recent_events / max_events).min(1.0);
        self.last_calculation = now;
        
        self.current_level
    }
}

impl PatternDetector {
    fn new() -> Self {
        let mut known_patterns = HashMap::new();
        
        // Add some common patterns
        known_patterns.insert("coding_layout".to_string(), vec![(0, 0), (0, 1), (1, 0)]); // Editor + Terminal + Browser
        known_patterns.insert("creative_suite".to_string(), vec![(1, 1), (1, 2), (2, 1)]); // Creative apps clustered
        
        Self {
            known_patterns,
            recent_occupations: Vec::new(),
        }
    }

    fn add_occupation(&mut self, row: u32, col: u32) {
        self.recent_occupations.push((row, col, Instant::now()));
        
        // Keep only recent occupations (last 10 seconds)
        let cutoff = Instant::now() - Duration::from_secs(10);
        self.recent_occupations.retain(|(_, _, t)| *t > cutoff);
    }

    fn detect_pattern(&self) -> Option<(String, Vec<(u32, u32)>, f32)> {
        let current_cells: Vec<(u32, u32)> = self.recent_occupations.iter()
            .map(|(r, c, _)| (*r, *c))
            .collect();

        for (pattern_name, pattern_cells) in &self.known_patterns {
            let matches = pattern_cells.iter()
                .filter(|cell| current_cells.contains(cell))
                .count();
            
            let confidence = matches as f32 / pattern_cells.len() as f32;
            
            if confidence > 0.7 { // 70% match threshold
                return Some((pattern_name.clone(), pattern_cells.clone(), confidence));
            }
        }

        None
    }
}

impl Default for GridIntegrationBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Service names for spatial MIDI communication
pub const SPATIAL_MIDI_EVENTS: &str = "spatial_midi_events";
pub const SPATIAL_MIDI_COMMANDS: &str = "spatial_midi_commands";
pub const GRID_MUSIC_CONFIG: &str = "grid_music_config";
