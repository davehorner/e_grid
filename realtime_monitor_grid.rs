use e_grid::{GridClient, ipc};
use e_grid::ipc_client::{ClientCellState};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use log::{info, debug, warn, error};

const MAX_LOG_ENTRIES: usize = 1000;
const LOG_UPDATE_INTERVAL: Duration = Duration::from_millis(100);
const GRID_UPDATE_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone)]
struct LogEntry {
    timestamp: Instant,
    message: String,
    level: LogLevel,
}

#[derive(Clone, Copy)]
enum LogLevel {
    Info,
    Debug,
    Warning,
    Error,
    Event,
}

impl LogLevel {
    fn color(&self) -> Color {
        match self {
            LogLevel::Info => Color::Green,
            LogLevel::Debug => Color::Cyan,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
            LogLevel::Event => Color::Magenta,
        }
    }
    
    fn prefix(&self) -> &'static str {
        match self {
            LogLevel::Info => "ℹ️ ",
            LogLevel::Debug => "🔍",
            LogLevel::Warning => "⚠️ ",
            LogLevel::Error => "❌",
            LogLevel::Event => "📡",
        }
    }
}

struct AppState {
    logs: Arc<Mutex<VecDeque<LogEntry>>>,
    grid_client: Option<GridClient>,
    monitors: Vec<MonitorState>,
    current_monitor: usize,
    last_grid_update: Instant,
    last_log_update: Instant,
    connection_status: String,
    total_windows: u32,
    total_events: u64,
    should_quit: bool,
    auto_scroll: bool,
    show_help: bool,
}

#[derive(Clone)]
struct MonitorState {
    id: u32,
    name: String,
    width: u32,
    height: u32,
    grid_rows: usize,
    grid_cols: usize,
    windows: Vec<WindowGridState>,
    last_update: Instant,
}

#[derive(Clone)]
struct WindowGridState {
    hwnd: u64,
    title: String,
    grid_top_left_row: u32,
    grid_top_left_col: u32,
    grid_bottom_right_row: u32,
    grid_bottom_right_col: u32,
    real_x: i32,
    real_y: i32,
    real_width: u32,
    real_height: u32,
    last_event_type: u8,
}

impl AppState {
    fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(VecDeque::new())),
            grid_client: None,
            monitors: Vec::new(),
            current_monitor: 0,
            last_grid_update: Instant::now(),
            last_log_update: Instant::now(),
            connection_status: "Disconnected".to_string(),
            total_windows: 0,
            total_events: 0,
            should_quit: false,
            auto_scroll: true,
            show_help: false,
        }
    }

    fn add_log(&mut self, level: LogLevel, message: String) {
        let entry = LogEntry {
            timestamp: Instant::now(),
            message,
            level,
        };
          if let Ok(mut logs) = self.logs.lock() {
            logs.push_back(entry);
            if logs.len() > MAX_LOG_ENTRIES {
                logs.pop_front();
            }
        }
    }fn try_connect(&mut self) {
        if self.grid_client.is_none() {
            match GridClient::new() {
                Ok(mut client) => {
                    self.add_log(LogLevel::Info, "Connected to e_grid server".to_string());
                    
                    // Start background monitoring
                    if let Err(e) = client.start_background_monitoring() {
                        self.add_log(LogLevel::Warning, format!("Failed to start monitoring: {}", e));
                    } else {
                        self.add_log(LogLevel::Info, "Background monitoring started".to_string());
                    }
                    
                    self.grid_client = Some(client);
                    self.connection_status = "Connected".to_string();
                },
                Err(e) => {
                    if self.connection_status != "Reconnecting..." {
                        self.add_log(LogLevel::Warning, format!("Failed to connect: {}", e));
                        self.connection_status = "Reconnecting...".to_string();
                    }
                }
            }
        }
    }    fn update_grid_state(&mut self) {
        if let Some(ref mut client) = self.grid_client {
            // Request current grid state from server
            match client.request_grid_state() {
                Ok(_) => {
                    self.add_log(LogLevel::Info, "Grid state requested from server".to_string());
                },
                Err(e) => {
                    self.add_log(LogLevel::Error, format!("Grid state request failed: {}", e));
                    // Reset connection to trigger reconnection
                    self.grid_client = None;
                    self.connection_status = "Disconnected".to_string();
                    return;
                }
            }
            
            // Update with real data after the request
            self.update_from_real_server_data();
        } else {
            // If not connected, show placeholder data
            if self.monitors.is_empty() {
                self.create_placeholder_monitors();
            }
        }
    }    // Use REAL server data instead of simulated data
    fn update_from_real_server_data(&mut self) {
        let (server_monitors, server_windows, virtual_grid, has_recent_data) = if let Some(ref client) = self.grid_client {
            (
                client.get_monitor_data(),
                client.get_window_data(),
                client.get_virtual_grid_state(),
                client.has_recent_data()
            )
        } else {
            return;
        };
        
        // Add debug logging to see what data we're getting
        let monitor_count = server_monitors.len();
        let window_count = server_windows.len();
        self.add_log(LogLevel::Info, format!("Server data check: {} monitors, {} windows", 
            monitor_count, window_count));
        
        if !server_monitors.is_empty() {
            self.add_log(LogLevel::Info, format!("Received {} monitors from server", monitor_count));
            
            // Convert server monitor data to TUI monitor data
            self.monitors.clear();
            for server_monitor in server_monitors {
                let mut monitor = MonitorState {
                    id: server_monitor.monitor_id,
                    name: format!("Monitor {}", server_monitor.monitor_id),
                    width: server_monitor.width.max(0) as u32,
                    height: server_monitor.height.max(0) as u32,
                    grid_rows: server_monitor.grid.len(),
                    grid_cols: server_monitor.grid.get(0).map(|row| row.len()).unwrap_or(0),
                    windows: Vec::new(),
                    last_update: Instant::now(),
                };
                debug!("Mapping monitor: id={} name={} size={}x{} grid={}x{}", monitor.id, monitor.name, monitor.width, monitor.height, monitor.grid_rows, monitor.grid_cols);
                let mut seen_hwnds = std::collections::HashSet::new();
                for (row_idx, row) in server_monitor.grid.iter().enumerate() {
                    for (col_idx, cell) in row.iter().enumerate() {
                        if let Some(hwnd) = cell {
                            if seen_hwnds.insert(*hwnd) {
                                if let Some(window_info) = server_windows.get(hwnd) {
                                    monitor.windows.push(WindowGridState {
                                        hwnd: *hwnd,
                                        title: format!("HWND {}", hwnd), // Replace with real title if available
                                        grid_top_left_row: window_info.monitor_row_start,
                                        grid_top_left_col: window_info.monitor_col_start,
                                        grid_bottom_right_row: window_info.monitor_row_end,
                                        grid_bottom_right_col: window_info.monitor_col_end,
                                        real_x: window_info.x,
                                        real_y: window_info.y,
                                        real_width: window_info.width.max(0) as u32,
                                        real_height: window_info.height.max(0) as u32,
                                        last_event_type: 0, // TODO: Use real event type if available
                                    });
                                    debug!("  Window HWND={} grid=({},{}-{},{}), real=({},{} {}x{})", hwnd, window_info.monitor_row_start, window_info.monitor_col_start, window_info.monitor_row_end, window_info.monitor_col_end, window_info.x, window_info.y, window_info.width, window_info.height);
                                }
                            }
                        }
                    }
                }
                self.monitors.push(monitor);
            }
            
            // Update total window count from real data
            self.total_windows = window_count as u32;
            
            self.add_log(LogLevel::Info, format!("Updated with real server data: {} monitors, {} windows", 
                self.monitors.len(), self.total_windows));
        } else if !virtual_grid.is_empty() {
            self.add_log(LogLevel::Info, format!("Using virtual grid data: {}x{} cells", 
                virtual_grid.len(), virtual_grid.get(0).map(|row| row.len()).unwrap_or(0)));
            
            // Create a single virtual monitor from virtual grid data
            let mut virtual_monitor = MonitorState {
                id: 999, // Virtual monitor ID
                name: "Virtual Monitor".to_string(),
                width: 1920, // Default size
                height: 1080,
                grid_rows: virtual_grid.len(),
                grid_cols: virtual_grid.get(0).map(|row| row.len()).unwrap_or(0),
                windows: Vec::new(),
                last_update: Instant::now(),
            };
            
            // Convert virtual grid to windows
            for (row_idx, row) in virtual_grid.iter().enumerate() {
                for (col_idx, cell) in row.iter().enumerate() {
                    if let ClientCellState::Occupied(hwnd) = cell {
                        if let Some(window_info) = server_windows.get(hwnd) {
                            virtual_monitor.windows.push(WindowGridState {
                                hwnd: *hwnd,
                                title: format!("Window {}", hwnd), // Use hwnd as title
                                grid_top_left_row: row_idx as u32,
                                grid_top_left_col: col_idx as u32,
                                grid_bottom_right_row: row_idx as u32,
                                grid_bottom_right_col: col_idx as u32,
                                real_x: window_info.x,
                                real_y: window_info.y,
                                real_width: window_info.width.max(0) as u32,
                                real_height: window_info.height.max(0) as u32,
                                last_event_type: 0,
                            });
                        }
                    }
                }
            }
            
            self.monitors = vec![virtual_monitor];
            self.total_windows = window_count as u32;
        } else if has_recent_data {
            self.add_log(LogLevel::Warning, "Client has recent data but no virtual grid".to_string());
            if self.monitors.is_empty() {
                self.create_placeholder_monitors();
            }
        } else {
            // No real data available yet - client is still connecting or no data
            self.add_log(LogLevel::Warning, "No monitor data received from server yet".to_string());
            if self.monitors.is_empty() {
                self.create_placeholder_monitors();
            }
        }
    }

    // Create placeholder monitor data for demonstration (fallback when no server data)
    fn create_placeholder_monitors(&mut self) {
        self.monitors.clear();
        
        // Create 2 example monitors
        for i in 0..2 {
            let mut monitor = MonitorState {
                id: i,
                name: format!("Monitor {}", i + 1),
                width: 1920,
                height: 1080,
                grid_rows: 8,
                grid_cols: 12,
                windows: Vec::new(),
                last_update: Instant::now(),
            };
            
            // Add some example windows
            if i == 0 {
                monitor.windows.push(WindowGridState {
                    hwnd: 12345,
                    title: "Example Window 1".to_string(),
                    grid_top_left_row: 1,
                    grid_top_left_col: 2,
                    grid_bottom_right_row: 3,
                    grid_bottom_right_col: 5,
                    real_x: 100,
                    real_y: 100,
                    real_width: 800,
                    real_height: 600,
                    last_event_type: 0, // Created
                });
            }
            
            self.monitors.push(monitor);
        }
        
        self.total_windows = self.monitors.iter().map(|m| m.windows.len() as u32).sum();
        self.add_log(LogLevel::Info, format!("Created {} placeholder monitors", self.monitors.len()));
    }

    // Simulate monitor data updates for demonstration
    fn simulate_monitor_data(&mut self) {
        if self.monitors.is_empty() {
            self.create_placeholder_monitors();
        }
        
        // Simulate window events
        if self.total_events % 10 == 0 {
            let event_types = ["CREATED", "MOVED", "MOVE_START", "MOVE_STOP"];
            let event_type = event_types[self.total_events as usize % event_types.len()];
            
            self.add_log(
                LogLevel::Event,
                format!("Simulated {} event - HWND:54321 Grid:(2,3) to (4,6) Real:400x300+200+150 Monitor:0", event_type)
            );
        }
    }    fn process_events(&mut self) {
        if let Some(ref mut _client) = self.grid_client {
            // The GridClient handles events automatically in background monitoring
            // For now, we'll use simulated data to demonstrate the enhanced event structure
            // In a full implementation, this would process actual events from the client
            
            // Increment event counter to show activity
            self.total_events += 1;
            
            // Update monitor data periodically to show activity
            if self.total_events % 5 == 0 {
                self.simulate_monitor_data();
            }
        }
    }

    fn process_window_event(&mut self, event: &ipc::WindowEvent) {
        let event_type_str = match event.event_type {
            0 => "CREATED",
            1 => "DESTROYED", 
            2 => "MOVED",
            3 => "STATE_CHANGED",
            4 => "MOVE_START",
            5 => "MOVE_STOP", 
            6 => "RESIZE_START",
            7 => "RESIZE_STOP",
            _ => "UNKNOWN",
        };

        self.add_log(
            LogLevel::Event,
            format!(
                "{} - HWND:{} Grid:({},{}) to ({},{}) Real:{}x{}+{}+{} Monitor:{}",
                event_type_str,
                event.hwnd,
                event.grid_top_left_row,
                event.grid_top_left_col,
                event.grid_bottom_right_row,
                event.grid_bottom_right_col,
                event.real_width,
                event.real_height,
                event.real_x,
                event.real_y,
                event.monitor_id
            )
        );

        // Update monitor state based on event
        self.update_monitor_from_event(event);
    }

    fn process_focus_event(&mut self, event: &ipc::WindowFocusEvent) {
        let event_type_str = if event.event_type == 0 { "FOCUSED" } else { "DEFOCUSED" };
        
        self.add_log(
            LogLevel::Event,
            format!(
                "{} - HWND:{} PID:{}",
                event_type_str,
                event.hwnd,
                event.process_id
            )
        );
    }

    fn update_monitor_from_event(&mut self, event: &ipc::WindowEvent) {
        let monitor_id = event.monitor_id as usize;
        
        // Ensure we have enough monitor entries
        while self.monitors.len() <= monitor_id {
            self.monitors.push(MonitorState {
                id: self.monitors.len() as u32,
                name: format!("Monitor {}", self.monitors.len()),
                width: 1920, // Default, should get from system
                height: 1080,
                grid_rows: 8, // Default grid size
                grid_cols: 12,
                windows: Vec::new(),
                last_update: Instant::now(),
            });
        }

        let monitor = &mut self.monitors[monitor_id];
        monitor.last_update = Instant::now();

        // Update or add window in this monitor
        if let Some(window) = monitor.windows.iter_mut().find(|w| w.hwnd == event.hwnd) {
            // Update existing window
            window.grid_top_left_row = event.grid_top_left_row;
            window.grid_top_left_col = event.grid_top_left_col;
            window.grid_bottom_right_row = event.grid_bottom_right_row;
            window.grid_bottom_right_col = event.grid_bottom_right_col;
            window.real_x = event.real_x;
            window.real_y = event.real_y;
            window.real_width = event.real_width;
            window.real_height = event.real_height;
            window.last_event_type = event.event_type;
        } else if event.event_type == 0 { // CREATED
            // Add new window
            monitor.windows.push(WindowGridState {
                hwnd: event.hwnd,
                title: format!("Window {}", event.hwnd),
                grid_top_left_row: event.grid_top_left_row,
                grid_top_left_col: event.grid_top_left_col,
                grid_bottom_right_row: event.grid_bottom_right_row,
                grid_bottom_right_col: event.grid_bottom_right_col,
                real_x: event.real_x,
                real_y: event.real_y,
                real_width: event.real_width,
                real_height: event.real_height,
                last_event_type: event.event_type,
            });
        }

        // Remove destroyed windows
        if event.event_type == 1 { // DESTROYED
            monitor.windows.retain(|w| w.hwnd != event.hwnd);
        }

        // Update total window count
        self.total_windows = self.monitors.iter().map(|m| m.windows.len() as u32).sum();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging with a custom filter to suppress stdout during TUI operation
    // Set log level to ERROR to minimize output that could break the TUI
    // The GridClient's println! statements should be converted to use log macros
    std::env::set_var("RUST_LOG", "error");
    env_logger::init();
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = AppState::new();
    app.add_log(LogLevel::Info, "Real-time Grid Monitor started".to_string());
    app.add_log(LogLevel::Info, "Press 'h' for help, 'q' to quit".to_string());

    // Main loop
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(50);

    loop {
        // Handle events
        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => {
                            app.should_quit = true;
                        },
                        KeyCode::Char('h') => {
                            app.show_help = !app.show_help;
                        },
                        KeyCode::Char('a') => {
                            app.auto_scroll = !app.auto_scroll;
                        },
                        KeyCode::Char('c') => {
                            // Clear logs
                            if let Ok(mut logs) = app.logs.lock() {
                                logs.clear();
                            }
                            app.add_log(LogLevel::Info, "Logs cleared".to_string());
                        },
                        KeyCode::Left => {
                            if app.current_monitor > 0 {
                                app.current_monitor -= 1;
                            }
                        },
                        KeyCode::Right => {
                            if app.current_monitor + 1 < app.monitors.len() {
                                app.current_monitor += 1;
                            }
                        },
                        _ => {}
                    }
                }
            }
        }

        // Update app state
        if last_tick.elapsed() >= tick_rate {
            // Try to connect if not connected
            app.try_connect();

            // Update grid state periodically
            if app.last_grid_update.elapsed() >= GRID_UPDATE_INTERVAL {
                app.update_grid_state();
                app.last_grid_update = Instant::now();
            }

            // Process events from server
            app.process_events();

            last_tick = Instant::now();
        }

        // Render UI
        terminal.draw(|f| {
            if app.show_help {
                render_help(f, &app);
            } else {
                render_main_ui(f, &app);
            }
        })?;

        // Exit condition
        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn render_main_ui(f: &mut Frame, app: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),     // Header
            Constraint::Min(10),       // Grid area
            Constraint::Length(8),     // Logs
        ])
        .split(f.area());

    // Header
    render_header(f, chunks[0], app);

    // Grid area
    render_grid_area(f, chunks[1], app);

    // Logs
    render_logs(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &AppState) {
    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(area);

    // Title and status
    let title = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("📊 E-Grid Real-time Monitor", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::White)),
            Span::styled(
                app.connection_status.as_str(),
                Style::default().fg(match app.connection_status.as_str() {
                    "Connected" => Color::Green,
                    "Reconnecting..." => Color::Yellow,
                    _ => Color::Red,
                })
            ),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title("Status"));

    f.render_widget(title, header_chunks[0]);

    // Stats
    let stats = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Windows: ", Style::default().fg(Color::White)),
            Span::styled(app.total_windows.to_string(), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("Events: ", Style::default().fg(Color::White)),
            Span::styled(app.total_events.to_string(), Style::default().fg(Color::Magenta)),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL).title("Stats"));

    f.render_widget(stats, header_chunks[1]);
}

fn render_grid_area(f: &mut Frame, area: Rect, app: &AppState) {
    if app.monitors.is_empty() {
        let placeholder = Paragraph::new("No monitor data available. Connect to e_grid server to see live grids.")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title("Grid View"))
            .wrap(Wrap { trim: true });
        f.render_widget(placeholder, area);
        return;
    }

    // Split area for multiple monitors horizontally
    let monitor_count = app.monitors.len();
    let constraints: Vec<Constraint> = (0..monitor_count)
        .map(|_| Constraint::Percentage(100 / monitor_count as u16))
        .collect();

    let monitor_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    // Render each monitor
    for (i, monitor) in app.monitors.iter().enumerate() {
        if i < monitor_chunks.len() {
            render_monitor_grid(f, monitor_chunks[i], monitor, i == app.current_monitor);
        }
    }
}

fn render_monitor_grid(f: &mut Frame, area: Rect, monitor: &MonitorState, is_selected: bool) {
    let title = format!("Monitor {} ({}x{}) - {} windows", 
                       monitor.id, monitor.grid_cols, monitor.grid_rows, monitor.windows.len());
    
    let border_style = if is_selected {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Create grid representation
    let grid_height = inner.height.saturating_sub(2);
    let grid_width = inner.width.saturating_sub(2);
    
    if grid_height < monitor.grid_rows as u16 || grid_width < monitor.grid_cols as u16 {
        // Too small to display grid properly
        let msg = Paragraph::new("Area too small for grid display")
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(msg, inner);
        return;
    }

    // Calculate cell dimensions
    let cell_height = grid_height / monitor.grid_rows as u16;
    let cell_width = grid_width / monitor.grid_cols as u16;    // Render grid cells
    for row in 0..monitor.grid_rows {
        for col in 0..monitor.grid_cols {
            let cell_area = Rect {
                x: inner.x + col as u16 * cell_width,
                y: inner.y + row as u16 * cell_height,
                width: cell_width.max(1),
                height: cell_height.max(1),
            };

            // Check if any window occupies this cell
            let window_in_cell = monitor.windows.iter().find(|w| {
                row as u32 >= w.grid_top_left_row && row as u32 <= w.grid_bottom_right_row &&
                col as u32 >= w.grid_top_left_col && col as u32 <= w.grid_bottom_right_col
            });

            let (cell_char, cell_style) = if let Some(window) = window_in_cell {
                // Cell has a window - show enhanced event type colors
                let color = match window.last_event_type {
                    4 | 6 => Color::Yellow,  // Move/resize start
                    5 | 7 => Color::Green,   // Move/resize stop
                    2 => Color::Cyan,        // Moved
                    0 => Color::Blue,        // Created
                    1 => Color::Red,         // Destroyed
                    _ => Color::White,       // Default
                };
                  // Use different characters for different window parts
                let char = if row as u32 == window.grid_top_left_row && col as u32 == window.grid_top_left_col {
                    "┌" // Top-left corner
                } else if row as u32 == window.grid_top_left_row && col as u32 == window.grid_bottom_right_col {
                    "┐" // Top-right corner
                } else if row as u32 == window.grid_bottom_right_row && col as u32 == window.grid_top_left_col {
                    "└" // Bottom-left corner
                } else if row as u32 == window.grid_bottom_right_row && col as u32 == window.grid_bottom_right_col {
                    "┘" // Bottom-right corner
                } else if row as u32 == window.grid_top_left_row || row as u32 == window.grid_bottom_right_row {
                    "─" // Horizontal border
                } else if col as u32 == window.grid_top_left_col || col as u32 == window.grid_bottom_right_col {
                    "│" // Vertical border
                } else {
                    "█" // Filled area
                };
                
                (char, Style::default().fg(color))
            } else {
                // Empty cell
                ("·", Style::default().fg(Color::DarkGray))
            };

            if cell_area.width > 0 && cell_area.height > 0 {
                let cell_widget = Paragraph::new(cell_char).style(cell_style);
                f.render_widget(cell_widget, cell_area);
            }
        }
    }

    // Add a window info panel below the grid if we have windows
    if !monitor.windows.is_empty() {
        let info_area = Rect {
            x: inner.x,
            y: inner.y + (monitor.grid_rows as u16 * cell_height).min(inner.height - 3),
            width: inner.width,
            height: 3,
        };
        
        let window_info: Vec<Line> = monitor.windows.iter().take(2).map(|w| {
            Line::from(format!("HWND:{} {}x{} at ({},{}) Grid:({},{}-{},{})", 
                w.hwnd, w.real_width, w.real_height, w.real_x, w.real_y,
                w.grid_top_left_row, w.grid_top_left_col, 
                w.grid_bottom_right_row, w.grid_bottom_right_col))
        }).collect();
        
        if !window_info.is_empty() {
            let info_widget = Paragraph::new(window_info)
                .style(Style::default().fg(Color::Gray))
                .wrap(Wrap { trim: true });
            f.render_widget(info_widget, info_area);
        }
    }
}

fn render_logs(f: &mut Frame, area: Rect, app: &AppState) {
    if let Ok(logs) = app.logs.lock() {
        let items: Vec<ListItem> = logs
            .iter()
            .map(|entry| {
                let time_str = format!("{:.1}s", entry.timestamp.elapsed().as_secs_f32());
                let line = Line::from(vec![
                    Span::styled(time_str, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(entry.level.prefix(), Style::default().fg(entry.level.color())),
                    Span::raw(" "),
                    Span::raw(&entry.message),
                ]);
                ListItem::new(line)
            })
            .collect();

        let logs_widget = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("Event Log"))
            .style(Style::default().fg(Color::White));

        f.render_widget(logs_widget, area);
    }
}

fn render_help(f: &mut Frame, _app: &AppState) {
    let area = f.area();
    
    // Clear the background
    f.render_widget(Clear, area);
    
    // Create help content
    let help_text = vec![
        Line::from("📊 E-Grid Real-time Monitor - Help"),
        Line::from(""),
        Line::from("Keyboard Commands:"),
        Line::from("  q          - Quit application"),
        Line::from("  h          - Toggle this help screen"),
        Line::from("  a          - Toggle auto-scroll for logs"),
        Line::from("  c          - Clear event logs"),
        Line::from("  ←/→        - Switch between monitors"),
        Line::from(""),
        Line::from("Grid Legend:"),
        Line::from(vec![
            Span::styled("  █ ", Style::default().fg(Color::Blue)),
            Span::raw("Created window"),
        ]),
        Line::from(vec![
            Span::styled("  █ ", Style::default().fg(Color::Cyan)),
            Span::raw("Moved window"),
        ]),
        Line::from(vec![
            Span::styled("  █ ", Style::default().fg(Color::Yellow)),
            Span::raw("Moving/Resizing"),
        ]),
        Line::from(vec![
            Span::styled("  █ ", Style::default().fg(Color::Green)),
            Span::raw("Move/Resize completed"),
        ]),
        Line::from(vec![
            Span::styled("  · ", Style::default().fg(Color::DarkGray)),
            Span::raw("Empty cell"),
        ]),
        Line::from(""),
        Line::from("Features:"),
        Line::from("  • Real-time grid state updates"),
        Line::from("  • Multi-monitor support"),
        Line::from("  • Window move/resize event tracking"),
        Line::from("  • Focus event monitoring"),
        Line::from("  • Enhanced position data (grid + real coordinates)"),
        Line::from(""),
        Line::from("Press 'h' again to close this help screen"),
    ];

    let help_widget = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help").title_style(Style::default().fg(Color::Cyan)))
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(Color::White));

    // Center the help dialog
    let popup_area = centered_rect(70, 80, area);
    f.render_widget(help_widget, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
