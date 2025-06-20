@echo off
:: Focus Tracking Demo Script for e_grid (Windows)
:: This script demonstrates all the focus tracking capabilities

echo 🎯 e_grid Focus Tracking Demonstration
echo ======================================
echo.
echo This script will show you all the focus tracking examples in e_grid.
echo Each example demonstrates different aspects of window focus monitoring.
echo.

:: Check if we're in the right directory
if not exist "Cargo.toml" (
    echo ❌ Error: Please run this script from the e_grid directory
    echo    Expected to find Cargo.toml file
    exit /b 1
)

if not exist "examples" (
    echo ❌ Error: Please run this script from the e_grid directory
    echo    Expected to find examples/ directory
    exit /b 1
)

echo 📋 Available Focus Tracking Examples:
echo    1. simple_focus_demo - Basic focus event logging
echo    2. focus_tracking_demo - Statistics and history tracking
echo    3. focus_music_demo - Music control simulation
echo    4. comprehensive_focus_demo - All features combined ⭐
echo.

:: Example 1: Simple Focus Demo
echo 🔹 Example 1: Simple Focus Demo
echo    This shows basic focus event monitoring with minimal output.
echo    You'll see focus/defocus events as you switch between windows.
pause

echo 🏃 Running: cargo run --example simple_focus_demo
echo    💡 Switch between different applications to see focus events
echo    ⏹️  Press Ctrl+C to stop the demo when you've seen enough
echo.

cargo run --example simple_focus_demo
echo ✅ Simple focus demo completed
pause

:: Example 2: Focus Tracking Demo
echo 🔹 Example 2: Focus Tracking Demo
echo    This shows comprehensive statistics and history tracking.
echo    You'll see focus counts, application rankings, and recent history.
pause

echo 🏃 Running: cargo run --example focus_tracking_demo
echo    💡 Watch the statistics build up as you switch between applications
echo    ⏹️  Press Ctrl+C to stop the demo when you've seen enough
echo.

cargo run --example focus_tracking_demo
echo ✅ Focus tracking demo completed
pause

:: Example 3: Focus Music Demo
echo 🔹 Example 3: Focus Music Demo
echo    This simulates music control based on focus events.
echo    Each application gets assigned a unique 'song' that plays when focused.
pause

echo 🏃 Running: cargo run --example focus_music_demo
echo    💡 Notice how different applications get different songs
echo    🎵 Songs start when you focus an app and pause when you switch away
echo    ⏹️  Press Ctrl+C to stop the demo when you've seen enough
echo.

cargo run --example focus_music_demo
echo ✅ Focus music demo completed
pause

:: Example 4: Comprehensive Focus Demo
echo 🔹 Example 4: Comprehensive Focus Demo ⭐
echo    This is the ultimate demonstration combining all features:
echo    • Real-time event monitoring with smart app identification
echo    • Statistical analysis and rankings
echo    • Music control simulation
echo    • Comprehensive reporting
pause

echo 🏃 Running: cargo run --example comprehensive_focus_demo
echo    💡 This combines everything - watch for:
echo       - Real-time events with readable app names
echo       - Automatic music assignments
echo       - Focus time tracking
echo       - Periodic comprehensive reports
echo    ⏹️  Press Ctrl+C to stop the demo when you've seen enough
echo.

cargo run --example comprehensive_focus_demo
echo ✅ Comprehensive focus demo completed

echo.
echo 🎉 All Focus Tracking Demonstrations Complete!
echo ==============================================
echo.
echo 📊 Summary of what you've seen:
echo    • Basic focus event monitoring
echo    • Statistical analysis and application rankings
echo    • Music control simulation (perfect for e_midi integration)
echo    • Comprehensive tracking with time analysis
echo.
echo 🔧 Integration with e_midi:
echo    The focus tracking system is ready for e_midi integration.
echo    e_midi can use the same focus callback pattern to:
echo    • Start/stop MIDI playback based on focused applications
echo    • Assign different songs to different applications
echo    • Implement spatial audio based on window positions
echo.
echo 📚 Next Steps:
echo    1. Review the example code in examples/ directory
echo    2. Read the documentation in examples/README.md
echo    3. Integrate focus tracking into your own applications
echo    4. Connect e_midi to use these focus events for music control
echo.
echo ✨ The focus tracking system is ready for production use!
pause
