@echo off
echo ==============================================
echo Testing Focus Event Fix
echo ==============================================
echo.
echo The fix added process_focus_events() to the main server loop.
echo This should now publish focus events via IPC.
echo.
echo To test:
echo 1. Run this server: cargo run --bin e_grid
echo 2. In another terminal: cargo run --bin grid_client_demo
echo 3. Switch between windows to generate focus events
echo.
echo Expected: Client should now show focus debug messages
echo.
pause
