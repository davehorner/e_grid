@echo off
echo Starting e_grid server and client debug test...
echo.

REM Start the server in background
echo Starting e_grid server...
start "E-Grid Server" cmd /c "cd /d %~dp0 && cargo run --bin e_grid"

REM Wait for server to initialize
echo Waiting 3 seconds for server to start...
timeout /t 3 >nul

REM Run the debug client
echo Running debug client...
cargo run --bin debug_monitor_coords

echo.
echo Cleaning up - stopping server...
taskkill /F /IM e_grid.exe >nul 2>&1

echo Test completed.
pause
