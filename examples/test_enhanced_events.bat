@echo off
echo 🧪 Testing Enhanced Move/Resize Events
echo =======================================
echo.
echo This test will verify the new enhanced event structure:
echo   • Grid coordinates (top-left, bottom-right)
echo   • Real window bounds (x, y, width, height)  
echo   • Move/resize start/stop events (event types 4-7)
echo   • Monitor ID tracking
echo.
echo IMPORTANT: All output will be contained within the ratatui interface!
echo No more breaking out of frames - everything is displayed in panels.
echo.
echo Starting server in background...
start "E-Grid Server" cmd /c "cargo run --bin e_grid server"

echo Waiting for server to initialize...
timeout /t 5 /nobreak > nul

echo.
echo Starting real-time monitor with enhanced UI...
echo Press Ctrl+C to stop both server and monitor
echo All grid and event data will be shown in the interface panels.
echo.
cargo run --bin realtime_monitor_grid
