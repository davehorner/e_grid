@echo off
REM Start the e_grid real-time TUI monitor client

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
REM Build the server and client (debug mode for dev, change to --release for release)
cargo build --bin e_grid --bin realtime_monitor_grid || goto :error

REM Start the e_grid server in the background
start "e_grid server" target\debug\e_grid.exe

REM Wait a moment for the server to initialize
ping 127.0.0.1 -n 3 > nul

REM Run the TUI client
target\debug\realtime_monitor_grid.exe

exit /b
:error
echo Build or run failed.
pause
exit /b 1
