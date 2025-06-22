@echo off
echo ========================================
echo Testing DoesNotExist Error Reduction
echo ========================================
echo.
echo This test verifies that:
echo 1. "DoesNotExist" errors are reduced with better retry logic
echo 2. All retry messages are contained within TUI panels
echo 3. Output does not break the terminal/frame display
echo.
echo Starting TUI monitor with error containment...
echo Press Ctrl+C to stop the monitor when done testing.
echo.
pause

REM Set log level to warn to reduce noise but still show connection issues
set RUST_LOG=warn

echo.
echo Starting realtime monitor grid with contained output...
cargo run --bin realtime_monitor_grid

echo.
echo Test completed. Check that:
echo - Any connection retry messages appeared in TUI panels (not raw terminal)
echo - No "Operation failed on attempt X/Y DoesNotExist" messages broke the TUI
echo - Error messages were properly formatted within the application
