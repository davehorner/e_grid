@echo off
echo Testing window positioning debug output...
echo This will run the demo for a short time to capture monitor debug info.
echo.
cd c:\w\c\e_grid
(echo. & echo. & echo. & echo. & echo.) | cargo run --bin test_event_driven_demo 2>&1 | findstr /i "Monitor Target primary bounds rect All Moving Calculated"
pause
