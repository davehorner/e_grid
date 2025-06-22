@echo off
echo ==========================================
echo  TESTING EVENT-DRIVEN DEMO WITH WINDOW MOVEMENT
echo ==========================================
echo.
echo This will test if windows actually move when commands are sent
echo Please have some windows open (like notepad, calculator, etc.)
echo.
pause

cd /d "%~dp0"
echo Starting demo...
cargo run --bin test_event_driven_demo

echo.
pause
