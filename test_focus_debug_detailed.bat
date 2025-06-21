@echo off
echo =============================================
echo Testing Focus Event Publishing Debug
echo =============================================
echo.
echo This will run the server with debug logging.
echo You should see debug messages showing:
echo - How many focus events are processed
echo - Whether the focus publisher is available
echo - If the publishing succeeds or fails
echo.
echo Run this, then switch between windows to generate focus events.
echo.
pause
echo Starting server with debug logging...
cargo run --bin e_grid
