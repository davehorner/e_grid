// Example demonstrating the different grid types
// This shows how to use BasicGrid, ZOrderGrid, AnimationGrid, and LayoutGrid

use e_grid::grid::traits::{
    AnimatableGrid, GridTrait, LayoutGrid as LayoutGridTrait, ZOrderGrid as ZOrderGridTrait,
};
use e_grid::*;
use winapi::shared::windef::{HWND, RECT};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎯 E-GRID MODULAR DEMONSTRATION");
    println!("🎯 E-GRID MODULAR DEMONSTRATION");
    println!("=================================");
    println!("Demonstrating different grid types and their capabilities\n");

    // Example window data
    let monitor_bounds = (0, 0, 1920, 1080);
    let windows = create_example_windows();

    // 1. Basic Grid Example
    println!("📋 1. BASIC GRID EXAMPLE");
    println!("========================");
    demonstrate_basic_grid(&windows, monitor_bounds)?;

    // 2. Z-Order Grid Example
    println!("\n🔍 2. Z-ORDER GRID EXAMPLE");
    println!("==========================");
    demonstrate_zorder_grid(&windows, monitor_bounds)?;

    // 3. Animation Grid Example
    println!("\n🎬 3. ANIMATION GRID EXAMPLE");
    println!("============================");
    demonstrate_animation_grid(&windows, monitor_bounds)?;

    // 4. Layout Grid Example
    println!("\n💾 4. LAYOUT GRID EXAMPLE");
    println!("=========================");
    demonstrate_layout_grid(&windows, monitor_bounds)?;

    println!("\n🎉 DEMONSTRATION COMPLETE!");
    println!("All grid types have been demonstrated successfully.");

    Ok(())
}

fn create_example_windows() -> Vec<(HWND, WindowInfo)> {
    let mut windows = Vec::new();

    // Create some example windows
    for i in 0..6 {
        let hwnd = (1000 + i) as HWND;
        let rect = RECT {
            left: i * 300,
            top: i * 200,
            right: (i + 1) * 300,
            bottom: (i + 1) * 200,
        };

        let window_info = WindowInfo::new(hwnd, format!("Window {}", i + 1), rect);

        windows.push((hwnd, window_info));
    }

    windows
}

fn demonstrate_basic_grid(
    windows: &[(HWND, WindowInfo)],
    monitor_bounds: (i32, i32, i32, i32),
) -> GridResult<()> {
    let config = GridConfig::new(4, 6);
    let mut grid = BasicGrid::with_monitor_bounds(config, monitor_bounds);

    println!("Basic grid shows simple window occupancy in a grid layout.");
    println!("Each cell can contain one window, displayed by its handle.");

    // Add windows to the grid
    for (hwnd, window_info) in windows {
        grid.add_window(*hwnd, window_info.clone())?;
    }

    grid.print_grid();

    println!("Grid statistics:");
    println!("  Total cells: {}", grid.config().cell_count());
    println!("  Occupied cells: {}", grid.occupied_cells());
    println!("  Windows tracked: {}", grid.get_all_windows().len());

    Ok(())
}

fn demonstrate_zorder_grid(
    windows: &[(HWND, WindowInfo)],
    monitor_bounds: (i32, i32, i32, i32),
) -> GridResult<()> {
    let config = GridConfig::new(4, 6);
    let mut grid = ZOrderGrid::new(config);

    println!("Z-Order grid shows window stacking and visibility.");
    println!("Multiple windows can occupy the same cell, with visibility calculations.");

    // Add windows to the grid with overlapping positions
    for (hwnd, window_info) in windows {
        grid.add_window(*hwnd, window_info.clone(), monitor_bounds)?;
    }

    grid.print_zorder_grid();
    grid.print_detailed_zorder();
    // Demonstrate z-order manipulation
    if let Some((first_hwnd, _)) = windows.first() {
        println!("🔄 Bringing first window to front...");
        grid.bring_to_front(*first_hwnd)?;

        println!("Updated z-order:");
        grid.print_zorder_grid();
    }

    Ok(())
}

fn demonstrate_animation_grid(
    windows: &[(HWND, WindowInfo)],
    monitor_bounds: (i32, i32, i32, i32),
) -> GridResult<()> {
    let config = GridConfig::new(3, 4);
    let mut grid = AnimationGrid::new(config, monitor_bounds);

    println!("Animation grid supports smooth transitions between grid configurations.");
    println!("Windows can be animated to new positions with various easing functions.");

    // Add windows to the grid
    for (hwnd, window_info) in windows {
        grid.add_window(*hwnd, window_info.clone())?;
    }

    println!("Initial 3x4 grid:");
    grid.print_animation_grid();

    // Simulate transition to 2x2 grid
    println!("🎬 Simulating transition to 2x2 grid...");
    println!("(In real usage, this would animate windows smoothly)");

    // This would normally animate the windows
    grid.animate_to_grid_size(2, 2, 1000, grid::animation::EasingType::EaseInOut)?;

    println!("Final 2x2 grid:");
    grid.print_animation_grid();

    // Demonstrate rotation
    println!("🔄 Simulating window rotation...");
    println!("(In real usage, windows would rotate through positions)");

    Ok(())
}

fn demonstrate_layout_grid(
    windows: &[(HWND, WindowInfo)],
    monitor_bounds: (i32, i32, i32, i32),
) -> GridResult<()> {
    let config = GridConfig::new(3, 4);
    let mut grid = LayoutGrid::with_monitor_bounds(config, monitor_bounds);

    println!("Layout grid supports saving and loading window arrangements.");
    println!("You can create named layouts and restore them later.");
    // Add windows to the grid manually
    let limited_windows: Vec<_> = windows.iter().take(4).collect();
    for (i, (hwnd, window_info)) in limited_windows.iter().enumerate() {
        let row = i / 2;
        let col = i % 2;
        grid.assign_window(*hwnd, row, col)?;

        // We need to track the window info for layout saving
        // (This would normally be handled by the grid's add_window method)
        println!(
            "  Placed '{}' at cell ({}, {})",
            window_info.title, row, col
        );
    }

    // Save the current layout
    grid.save_layout("demo_layout".to_string())?;
    // Clear and rearrange
    grid.clear();
    for (i, (hwnd, window_info)) in limited_windows.iter().enumerate() {
        let row = (i + 2) % 3;
        let col = (i + 1) % 4;
        grid.assign_window(*hwnd, row, col)?;
        println!("  Moved '{}' to cell ({}, {})", window_info.title, row, col);
    }

    println!("\nSaved layouts:");
    for layout_name in grid.list_layouts() {
        println!("  📋 {}", layout_name);
    }

    // Restore the saved layout
    println!("\n🔄 Restoring 'demo_layout'...");
    grid.load_layout("demo_layout")?;

    println!("Layout grid statistics:");
    println!("  Saved layouts: {}", grid.list_layouts().len());
    println!("  Occupied cells: {}", grid.occupied_cells());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modular_grid_types() {
        // Test that all grid types can be created and used
        let config = GridConfig::new(4, 4);

        let basic_grid = BasicGrid::new(config.clone());
        assert_eq!(basic_grid.config().rows, 4);
        assert_eq!(basic_grid.config().cols, 4);

        let zorder_grid = ZOrderGrid::new(config.clone());
        assert_eq!(zorder_grid.config().rows, 4);

        let animation_grid = AnimationGrid::new(config.clone(), (0, 0, 1920, 1080));
        assert_eq!(animation_grid.config().rows, 4);

        let layout_grid = LayoutGrid::new(config.clone());
        assert_eq!(layout_grid.config().rows, 4);
    }

    #[test]
    fn test_grid_traits() {
        let config = GridConfig::new(2, 2);
        let grid = BasicGrid::new(config);

        // Test trait methods
        assert_eq!(grid.config().cell_count(), 4);
        assert_eq!(grid.occupied_cells(), 0);
        assert!(grid.get_all_windows().is_empty());
    }
}
