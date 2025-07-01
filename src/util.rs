use std::collections::HashMap;

use winapi::shared::windef::RECT;

// Helper function to check if window coverage of a cell meets the threshold
pub fn meets_coverage_threshold(window_rect: &RECT, cell_rect: &RECT) -> bool {
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
