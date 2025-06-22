@echo off
echo Testing monitor coordinate display...
echo.

REM Start the TUI monitor briefly to see current output
echo Starting TUI monitor for 5 seconds to check monitor display...
timeout /t 5 >nul & taskkill /F /IM realtime_monitor_grid.exe >nul 2>&1 &
cargo run --bin realtime_monitor_grid

echo.
echo Test completed. Check the TUI output above to see if monitor coordinates are correct.
pause
