@echo off
echo 🧪 Testing TUI Output Containment After Log Conversion
echo ======================================================
echo.
echo This test checks if the log-based approach properly contains
echo all output within the TUI panels without breaking frames.
echo.
echo Starting server in background...
start "E-Grid Server" cmd /c "cargo run --bin e_grid server"

echo Waiting for server to initialize...
timeout /t 3 /nobreak > nul

echo.
echo Starting real-time monitor with log filtering...
echo The TUI should now contain all output within panels.
echo.
cargo run --bin realtime_monitor_grid
