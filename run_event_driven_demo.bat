@echo off
echo ==========================================
echo  E-GRID EVENT-DRIVEN COMPREHENSIVE DEMO
echo ==========================================
echo.
echo Starting event-driven window management demo...
echo This demo uses proper Windows event hooks (no polling!)
echo.
pause

cd /d "%~dp0"
cargo run --bin test_event_driven_demo

echo.
echo Demo completed!
pause
