@echo off
echo Testing New E-Grid Primary Binary
echo ==================================

echo.
echo This tests the new unified e_grid binary that:
echo - Auto-detects if server is running
echo - Starts server + detached client if no server
echo - Starts interactive client if server exists
echo.

echo Building e_grid binary...
cargo build --bin e_grid

echo.
echo Testing help command...
cargo run --bin e_grid help

echo.
echo Press any key to test auto-detection mode...
pause >nul

echo.
echo Starting e_grid in auto-detection mode...
echo This will start the server and a detached client.
cargo run --bin e_grid

echo.
echo E-Grid test completed!
pause
