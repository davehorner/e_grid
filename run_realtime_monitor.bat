@echo off
echo 📊 Starting E-Grid Real-time Monitor with enhanced move/resize tracking...
echo.
echo 🎯 Features:
echo   • Enhanced move/resize events with grid + real coordinates
echo   • Start/stop move events for better client sync  
echo   • Multi-monitor grid visualization
echo   • Real-time event log with ratatui interface
echo   • ALL OUTPUT CONTAINED IN PANELS - no frame breaking!
echo.
echo 🎮 Controls:
echo   • Press 'h' in the monitor for help
echo   • Press 'q' to quit
echo   • Use ←/→ to switch between monitors
echo   • Press 'c' to clear logs
echo.
cargo run --bin realtime_monitor_grid
