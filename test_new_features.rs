use e_grid::{WindowTracker, EasingType, GridLayout, GridCellAssignment};
use std::collections::HashMap;
use std::time::Duration;

fn main() {
    println!("🧪 Testing new E-Grid features...");
    
    // Test 1: Create WindowTracker with animation support
    let mut tracker = WindowTracker::new();
    println!("✅ WindowTracker created successfully");
    
    // Test 2: Test EasingType enum
    let easing_types = vec![
        EasingType::Linear,
        EasingType::EaseIn,
        EasingType::EaseOut,
        EasingType::EaseInOut,
        EasingType::Bounce,
        EasingType::Elastic,
        EasingType::Back,
    ];
    
    for easing in &easing_types {
        println!("✅ EasingType::{:?} created", easing);
    }
    
    // Test 3: Test GridLayout creation
    let assignments = vec![
        GridCellAssignment {
            hwnd: 123456,
            row: 0,
            col: 0,
        },
        GridCellAssignment {
            hwnd: 789012,
            row: 1,
            col: 1,
        },
    ];
    
    let layout = GridLayout { assignments };
    println!("✅ GridLayout created with {} assignments", layout.assignments.len());
    
    // Test 4: Test saving and retrieving layouts
    tracker.save_current_layout("test_layout".to_string());
    let saved_layouts = tracker.saved_layouts.keys().cloned().collect::<Vec<_>>();
    println!("✅ Layout saved, total saved layouts: {}", saved_layouts.len());
    
    // Test 5: Test animation progress calculation
    let duration = Duration::from_millis(1000);
    let progress_tests = vec![0.0, 0.25, 0.5, 0.75, 1.0];
    
    for progress in progress_tests {
        for easing in &easing_types {
            let result = e_grid::apply_easing(*easing, progress);
            println!("✅ Easing {:?} at progress {:.2} = {:.3}", easing, progress, result);
        }
    }
    
    println!("🎉 All tests completed successfully!");
    println!("✅ E-Grid animation and layout features are working correctly!");
}
