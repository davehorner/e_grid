@echo off
echo Testing window positioning debug output...
echo This will run the demo and automatically proceed through phases to see debug output.
echo.
cd c:\w\c\e_grid
echo | cargo run --bin test_event_driven_demo 2>&1 | findstr /C:"Monitor" /C:"Target" /C:"WARNING" /C:"primary" /C:"bounds" /C:"rect"
pause
