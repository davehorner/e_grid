@echo off
echo Testing event-driven window detection...
echo.
echo Instructions:
echo 1. The demo will start
echo 2. Open/close notepad or other windows 
echo 3. Watch for "WINDOW EVENT RECEIVED!" messages
echo 4. Press Ctrl+C to stop when done testing
echo.
pause
cargo run --bin test_event_driven_demo
