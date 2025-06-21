// Animation Grid - Grid that supports smooth window transitions and animations
// Incorporates functionality similar to test_animated_transitions.rs

use crate::config::GridConfig;
use crate::display::format_hwnd_display;
use crate::grid::traits::{AnimatableGrid, CellDisplay, GridError, GridResult, GridTrait};
use crate::window::{WindowAnimation, WindowInfo};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EasingType {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bounce,
    Elastic,
    Back,
}

#[derive(Debug, Clone)]
pub enum AnimationCellState {
    Empty,
    Occupied(HWND),
    Animating {
        hwnd: HWND,
        target_cell: (usize, usize),
        progress: f32,
    },
    OffScreen,
}

pub struct AnimationGrid {
    config: GridConfig,
    grid: Vec<Vec<AnimationCellState>>,
    windows: HashMap<HWND, WindowInfo>,
    active_animations: HashMap<HWND, WindowAnimation>,
    monitor_bounds: (i32, i32, i32, i32), // (left, top, right, bottom)
    animation_fps: u64,                   // Target FPS for animations
}

impl AnimationGrid {
    pub fn new(config: GridConfig, monitor_bounds: (i32, i32, i32, i32)) -> Self {
        let grid = vec![vec![AnimationCellState::Empty; config.cols]; config.rows];

        Self {
            config,
            grid,
            windows: HashMap::new(),
            active_animations: HashMap::new(),
            monitor_bounds,
            animation_fps: 60,
        }
    }

    pub fn set_monitor_bounds(&mut self, bounds: (i32, i32, i32, i32)) {
        self.monitor_bounds = bounds;
    }

    pub fn set_animation_fps(&mut self, fps: u64) {
        self.animation_fps = fps.max(30).min(120); // Clamp between 30-120 FPS
    }

    /// Calculate the screen position for a grid cell
    fn calculate_cell_position(&self, row: usize, col: usize) -> GridResult<RECT> {
        self.validate_coordinates(row, col)?;

        let (monitor_left, monitor_top, monitor_right, monitor_bottom) = self.monitor_bounds;
        let monitor_width = monitor_right - monitor_left;
        let monitor_height = monitor_bottom - monitor_top;

        let cell_width = monitor_width / self.config.cols as i32;
        let cell_height = monitor_height / self.config.rows as i32;

        let left = monitor_left + (col as i32 * cell_width);
        let top = monitor_top + (row as i32 * cell_height);
        let right = left + cell_width - 30; // Margin for visibility
        let bottom = top + cell_height - 30; // Margin for visibility

        Ok(RECT {
            left,
            top,
            right,
            bottom,
        })
    }

    /// Apply easing function to animation progress
    fn apply_easing(t: f32, easing: &EasingType) -> f32 {
        match easing {
            EasingType::Linear => t,
            EasingType::EaseIn => t * t * t,
            EasingType::EaseOut => {
                let u = 1.0 - t;
                1.0 - (u * u * u)
            }
            EasingType::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let u = 1.0 - t;
                    1.0 - 4.0 * u * u * u
                }
            }
            EasingType::Bounce => {
                if t < 1.0 / 2.75 {
                    7.5625 * t * t
                } else if t < 2.0 / 2.75 {
                    let t = t - 1.5 / 2.75;
                    7.5625 * t * t + 0.75
                } else if t < 2.5 / 2.75 {
                    let t = t - 2.25 / 2.75;
                    7.5625 * t * t + 0.9375
                } else {
                    let t = t - 2.625 / 2.75;
                    7.5625 * t * t + 0.984375
                }
            }
            EasingType::Elastic => {
                if t == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    let c4 = (2.0 * std::f32::consts::PI) / 3.0;
                    -(2.0_f32.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * c4).sin()
                }
            }
            EasingType::Back => {
                let c1 = 1.70158;
                let c3 = c1 + 1.0;
                c3 * t * t * t - c1 * t * t
            }
        }
    }

    /// Animate all windows to a new grid configuration
    pub fn animate_to_grid_size(
        &mut self,
        new_rows: usize,
        new_cols: usize,
        duration_ms: u64,
        easing: EasingType,
    ) -> GridResult<()> {
        println!(
            "🎬 Animating from {}x{} to {}x{} grid",
            self.config.rows, self.config.cols, new_rows, new_cols
        );

        // Create new configuration
        let new_config = GridConfig::new(new_rows, new_cols);
        let max_windows = self.windows.len().min(new_config.cell_count());

        // Calculate target positions for all windows
        let mut target_positions = HashMap::new();
        let mut window_index = 0;

        for (&hwnd, _) in self.windows.iter().take(max_windows) {
            let target_row = window_index / new_cols;
            let target_col = window_index % new_cols;

            if let Ok(target_rect) =
                self.calculate_cell_position_for_config(&new_config, target_row, target_col)
            {
                target_positions.insert(hwnd, (target_row, target_col, target_rect));
                window_index += 1;
            }
        }

        // Start animations for all windows
        self.start_batch_animation(target_positions, duration_ms, easing)?;

        // Update grid configuration
        self.config = new_config;
        self.grid = vec![vec![AnimationCellState::Empty; new_cols]; new_rows];

        Ok(())
    }

    /// Calculate cell position for a specific configuration (used during transitions)
    fn calculate_cell_position_for_config(
        &self,
        config: &GridConfig,
        row: usize,
        col: usize,
    ) -> GridResult<RECT> {
        if row >= config.rows || col >= config.cols {
            return Err(GridError::InvalidCoordinates {
                row,
                col,
                max_row: config.rows - 1,
                max_col: config.cols - 1,
            });
        }

        let (monitor_left, monitor_top, monitor_right, monitor_bottom) = self.monitor_bounds;
        let monitor_width = monitor_right - monitor_left;
        let monitor_height = monitor_bottom - monitor_top;

        let cell_width = monitor_width / config.cols as i32;
        let cell_height = monitor_height / config.rows as i32;

        let left = monitor_left + (col as i32 * cell_width);
        let top = monitor_top + (row as i32 * cell_height);
        let right = left + cell_width - 30;
        let bottom = top + cell_height - 30;

        Ok(RECT {
            left,
            top,
            right,
            bottom,
        })
    }

    /// Start a batch animation for multiple windows
    fn start_batch_animation(
        &mut self,
        targets: HashMap<HWND, (usize, usize, RECT)>,
        duration_ms: u64,
        easing: EasingType,
    ) -> GridResult<()> {
        for (hwnd, (target_row, target_col, target_rect)) in targets {
            if let Some(window_info) = self.windows.get(&hwnd) {
                let animation = WindowAnimation::new(
                    hwnd,
                    window_info.rect,
                    target_rect,
                    Duration::from_millis(duration_ms),
                    easing.clone(),
                );

                self.active_animations.insert(hwnd, animation);

                // Update grid state to show animation
                if let Some(current_pos) = self.find_window_position(hwnd) {
                    self.grid[current_pos.0][current_pos.1] = AnimationCellState::Animating {
                        hwnd,
                        target_cell: (target_row, target_col),
                        progress: 0.0,
                    };
                }

                println!(
                    "  🎬 Started animation for window {:?} to cell ({}, {})",
                    hwnd, target_row, target_col
                );
            }
        }

        Ok(())
    }

    /// Find the current grid position of a window
    fn find_window_position(&self, hwnd: HWND) -> Option<(usize, usize)> {
        for (row, grid_row) in self.grid.iter().enumerate() {
            for (col, cell) in grid_row.iter().enumerate() {
                match cell {
                    AnimationCellState::Occupied(cell_hwnd)
                    | AnimationCellState::Animating {
                        hwnd: cell_hwnd, ..
                    } => {
                        if *cell_hwnd == hwnd {
                            return Some((row, col));
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Run the animation loop until all animations complete
    pub fn run_animation_loop(&mut self) -> GridResult<()> {
        let frame_duration = Duration::from_millis(1000 / self.animation_fps);

        while self.has_active_animations() {
            let frame_start = Instant::now();

            // Update all animations
            let completed = self.update_animations()?;

            if completed.is_empty() {
                break; // All animations completed
            }

            // Sleep to maintain target FPS
            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }

        println!("✅ All animations completed");
        Ok(())
    }

    /// Rotate windows through grid positions (like in test_animated_transitions.rs)
    pub fn rotate_windows(
        &mut self,
        rotation_steps: usize,
        step_duration_ms: u64,
    ) -> GridResult<()> {
        let max_windows = self.windows.len().min(self.config.cell_count());

        for step in 0..rotation_steps {
            println!("🔄 ROTATION STEP {} of {}", step + 1, rotation_steps);

            let mut target_positions = HashMap::new();
            let mut window_index = 0;

            // Calculate rotated positions
            for (&hwnd, _) in self.windows.iter().take(max_windows) {
                let current_index = window_index;
                let next_index = (current_index + 1) % self.config.cell_count();

                let new_row = next_index / self.config.cols;
                let new_col = next_index % self.config.cols;

                if let Ok(target_rect) = self.calculate_cell_position(new_row, new_col) {
                    target_positions.insert(hwnd, (new_row, new_col, target_rect));
                }

                window_index += 1;
            }

            // Start rotation animations
            self.start_batch_animation(target_positions, step_duration_ms, EasingType::EaseInOut)?;
            self.run_animation_loop()?;

            println!("   ✅ Rotation step {} complete", step + 1);

            // Small pause between steps
            if step < rotation_steps - 1 {
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        Ok(())
    }

    /// Add a window to the grid
    pub fn add_window(&mut self, hwnd: HWND, window_info: WindowInfo) -> GridResult<()> {
        self.windows.insert(hwnd, window_info);

        // Find an empty cell to place the window
        for row in 0..self.config.rows {
            for col in 0..self.config.cols {
                if matches!(self.grid[row][col], AnimationCellState::Empty) {
                    self.grid[row][col] = AnimationCellState::Occupied(hwnd);
                    return Ok(());
                }
            }
        }

        // If no empty cells, add to off-screen
        println!("⚠️ Grid full, window {:?} placed off-screen", hwnd);
        Ok(())
    }

    /// Print the current animation grid state
    pub fn print_animation_grid(&self) {
        println!(
            "=== ANIMATION GRID ({} x {}) ===",
            self.config.rows, self.config.cols
        );

        // Print column headers
        print!("    ");
        for col in 0..self.config.cols {
            print!(" {:2}", col);
        }
        println!();

        // Print grid rows
        for (row, grid_row) in self.grid.iter().enumerate() {
            print!("{:2}: ", row);

            for cell in grid_row {
                match cell {
                    AnimationCellState::Empty => print!(" . "),
                    AnimationCellState::OffScreen => print!(" - "),
                    AnimationCellState::Occupied(hwnd) => {
                        let display = format_hwnd_display(*hwnd as u64);
                        print!("{:>3}", display);
                    }
                    AnimationCellState::Animating { hwnd, progress, .. } => {
                        let display = format_hwnd_display(*hwnd as u64);
                        let progress_char = if *progress < 0.5 { "~" } else { ">" };
                        print!("{}{}", display, progress_char);
                    }
                }
            }
            println!();
        }

        if self.has_active_animations() {
            println!("🎬 Active animations: {}", self.active_animations.len());
        }
        println!();
    }
}

impl GridTrait for AnimationGrid {
    fn config(&self) -> &GridConfig {
        &self.config
    }

    fn update(&mut self) -> GridResult<()> {
        // Update animations and window positions
        self.update_animations()?;
        Ok(())
    }

    fn clear(&mut self) {
        self.grid = vec![vec![AnimationCellState::Empty; self.config.cols]; self.config.rows];
        self.windows.clear();
        self.active_animations.clear();
    }

    fn occupied_cells(&self) -> usize {
        self.grid
            .iter()
            .flat_map(|row| row.iter())
            .filter(|cell| !matches!(cell, AnimationCellState::Empty))
            .count()
    }

    fn is_cell_occupied(&self, row: usize, col: usize) -> GridResult<bool> {
        self.validate_coordinates(row, col)?;
        Ok(!matches!(self.grid[row][col], AnimationCellState::Empty))
    }

    fn get_cell_windows(&self, row: usize, col: usize) -> GridResult<Vec<HWND>> {
        self.validate_coordinates(row, col)?;

        match &self.grid[row][col] {
            AnimationCellState::Occupied(hwnd) => Ok(vec![*hwnd]),
            AnimationCellState::Animating { hwnd, .. } => Ok(vec![*hwnd]),
            _ => Ok(Vec::new()),
        }
    }

    fn assign_window(&mut self, hwnd: HWND, row: usize, col: usize) -> GridResult<()> {
        self.validate_coordinates(row, col)?;

        // Clear the window from its current position
        self.remove_window(hwnd)?;

        // Assign to new position
        self.grid[row][col] = AnimationCellState::Occupied(hwnd);

        Ok(())
    }

    fn remove_window(&mut self, hwnd: HWND) -> GridResult<()> {
        self.windows.remove(&hwnd);
        self.active_animations.remove(&hwnd);

        // Remove from grid
        for row in &mut self.grid {
            for cell in row {
                match cell {
                    AnimationCellState::Occupied(cell_hwnd)
                    | AnimationCellState::Animating {
                        hwnd: cell_hwnd, ..
                    } => {
                        if *cell_hwnd == hwnd {
                            *cell = AnimationCellState::Empty;
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn get_all_windows(&self) -> Vec<HWND> {
        self.windows.keys().copied().collect()
    }
}

impl AnimatableGrid for AnimationGrid {
    fn animate_to_layout(
        &mut self,
        target_layout: &HashMap<HWND, (usize, usize)>,
        duration_ms: u64,
    ) -> GridResult<()> {
        let mut targets = HashMap::new();

        for (&hwnd, &(row, col)) in target_layout {
            let target_rect = self.calculate_cell_position(row, col)?;
            targets.insert(hwnd, (row, col, target_rect));
        }

        self.start_batch_animation(targets, duration_ms, EasingType::EaseInOut)
    }
    fn update_animations(&mut self) -> GridResult<Vec<HWND>> {
        let mut completed_animations = Vec::new();
        let mut window_updates = Vec::new();
        let mut grid_updates = Vec::new();

        // First pass: collect all position data to avoid borrowing conflicts
        let position_data: Vec<_> = self
            .active_animations
            .keys()
            .filter_map(|&hwnd| self.find_window_position(hwnd).map(|pos| (hwnd, pos)))
            .collect();

        // Collect animation updates without borrowing conflicts
        for (&hwnd, animation) in &mut self.active_animations {
            if animation.completed {
                completed_animations.push(hwnd);
                continue;
            }

            // Get current animation frame
            let current_rect = animation.get_current_rect();

            // Move the window
            unsafe {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    current_rect.left,
                    current_rect.top,
                    current_rect.right - current_rect.left,
                    current_rect.bottom - current_rect.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }

            window_updates.push((hwnd, current_rect));

            // Collect grid position and progress for later update
            if let Some((_hwnd, pos)) = position_data.iter().find(|(h, _)| *h == hwnd) {
                grid_updates.push((pos.0, pos.1, animation.get_progress()));
            }
        }

        // Apply window updates
        for (hwnd, rect) in window_updates {
            if let Some(window_info) = self.windows.get_mut(&hwnd) {
                window_info.update_rect(rect);
            }
        }

        // Apply grid updates
        for (row, col, progress) in grid_updates {
            if let AnimationCellState::Animating {
                progress: cell_progress,
                ..
            } = &mut self.grid[row][col]
            {
                *cell_progress = progress;
            }
        }

        // Remove completed animations
        for hwnd in &completed_animations {
            self.active_animations.remove(hwnd);

            // Update grid state to occupied
            if let Some(pos) = self.find_window_position(*hwnd) {
                self.grid[pos.0][pos.1] = AnimationCellState::Occupied(*hwnd);
            }
        }

        Ok(completed_animations)
    }

    fn has_active_animations(&self) -> bool {
        !self.active_animations.is_empty()
    }

    fn stop_all_animations(&mut self) {
        self.active_animations.clear();

        // Update grid state to remove animation markers
        for row in &mut self.grid {
            for cell in row {
                if let AnimationCellState::Animating { hwnd, .. } = cell {
                    *cell = AnimationCellState::Occupied(*hwnd);
                }
            }
        }
    }
}

impl CellDisplay for AnimationCellState {
    fn display_cell(&self) -> &str {
        match self {
            AnimationCellState::Empty => " .",
            AnimationCellState::Occupied(_) => "", // Use get_hwnd for display
            AnimationCellState::Animating { .. } => "", // Use get_hwnd for display
            AnimationCellState::OffScreen => " -",
        }
    }

    fn get_hwnd(&self) -> Option<u64> {
        match self {
            AnimationCellState::Occupied(hwnd) => Some(*hwnd as u64),
            AnimationCellState::Animating { hwnd, .. } => Some(*hwnd as u64),
            _ => None,
        }
    }
}
