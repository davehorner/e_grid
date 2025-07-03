use std::collections::HashMap;

use winapi::shared::windef::RECT;

// Helper function to check if window coverage of a cell meets the threshold
pub fn meets_coverage_threshold(window_rect: &RECT, cell_rect: &RECT) -> bool {
    return true;
    let intersection_area = calculate_intersection_area(window_rect, cell_rect);
    let cell_area = (cell_rect.right - cell_rect.left) * (cell_rect.bottom - cell_rect.top);

    if cell_area <= 0 {
        return false;
    }

    let coverage_ratio = intersection_area as f32 / cell_area as f32;
    coverage_ratio >= crate::COVERAGE_THRESHOLD
}

// Helper function to calculate intersection area between two rectangles
pub fn calculate_intersection_area(rect1: &RECT, rect2: &RECT) -> i32 {
    let left = rect1.left.max(rect2.left);
    let top = rect1.top.max(rect2.top);
    let right = rect1.right.min(rect2.right);
    let bottom = rect1.bottom.min(rect2.bottom);

    if left < right && top < bottom {
        (right - left) * (bottom - top)
    } else {
        0
    }
}

pub fn get_hwnd_z_order_map() -> HashMap<u64, usize> {
    use winapi::um::winuser::{GetTopWindow, GetWindow, GW_HWNDNEXT};
    let mut z_map = HashMap::new();
    unsafe {
        let mut hwnd = GetTopWindow(std::ptr::null_mut());
        let mut z = 0;
        while !hwnd.is_null() {
            z_map.insert(hwnd as u64, z);
            hwnd = GetWindow(hwnd, GW_HWNDNEXT);
            z += 1;
        }
    }
    z_map
}

/// Fill all grid cells in a monitor grid that intersect with the given window rectangle.
/// This ensures the window fills a rectangle of cells, not just a line or a single cell.
pub fn fill_monitor_grid_rect(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: i32,
    monitor_height: i32,
    grid_rows: usize,
    grid_cols: usize,
    wx0: i32,
    wy0: i32,
    wx1: i32,
    wy1: i32,
    grid: &mut Vec<Vec<Option<u64>>>,
    hwnd: u64,
) {
    // Compute intersection
    let mx0 = monitor_x;
    let my0 = monitor_y;
    let mx1 = monitor_x + monitor_width;
    let my1 = monitor_y + monitor_height;
    let ix0 = wx0.max(mx0);
    let iy0 = wy0.max(my0);
    let ix1 = wx1.min(mx1);
    let iy1 = wy1.min(my1);
    if ix0 >= ix1 || iy0 >= iy1 {
        return; // No intersection
    }
    // Convert intersection rectangle to grid indices
    let grid_row_start = (((iy0 - my0) as f32 / monitor_height as f32) * grid_rows as f32).floor() as isize;
    let grid_row_end = (((iy1 - my0) as f32 / monitor_height as f32) * grid_rows as f32).ceil() as isize - 1;
    let grid_col_start = (((ix0 - mx0) as f32 / monitor_width as f32) * grid_cols as f32).floor() as isize;
    let grid_col_end = (((ix1 - mx0) as f32 / monitor_width as f32) * grid_cols as f32).ceil() as isize - 1;
    // Clamp to grid bounds
    let row_start = grid_row_start.clamp(0, (grid_rows - 1) as isize) as usize;
    let row_end = grid_row_end.clamp(0, (grid_rows - 1) as isize) as usize;
    let col_start = grid_col_start.clamp(0, (grid_cols - 1) as isize) as usize;
    let col_end = grid_col_end.clamp(0, (grid_cols - 1) as isize) as usize;
    // Fill all cells in the rectangle
    for row in row_start..=row_end {
        for col in col_start..=col_end {
            if let Some(row_vec) = grid.get_mut(row) {
                if let Some(cell) = row_vec.get_mut(col) {
                    *cell = Some(hwnd);
                }
            }
        }
    }
}