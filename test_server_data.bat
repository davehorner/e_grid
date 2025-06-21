@echo off
echo Testing what data the e_grid server provides...
echo.

echo Starting e_grid server in background...
start /min "E-Grid-Server-Test" cmd /c "cargo run --bin e_grid server > server_output.txt 2>&1"

echo Waiting 5 seconds for server startup...
timeout /t 5 /nobreak >nul

echo Testing client connection and data retrieval...
cargo run --bin debug_monitor_coords

echo.
echo Server output (first 30 lines):
type server_output.txt | more +1 | head -30

echo.
echo Done! Check server_output.txt for full server output.
pause
