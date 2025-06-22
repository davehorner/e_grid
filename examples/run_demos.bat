@echo off
echo Starting E-Grid Non-Interactive Demos
echo =====================================

echo.
echo This will start both the server and client demos in separate windows.
echo Both demos are non-interactive and will run automatically.
echo.
echo - Server Demo: Tracks windows and publishes events every 3 seconds
echo - Client Demo: Connects to server and displays real-time grid updates
echo.

echo Starting Server Demo...
start "E-Grid Server Demo" cmd /k "cargo run --bin e_grid"

echo.
echo Waiting 2 seconds for server to initialize...
timeout /t 2 /nobreak >nul

echo Starting Client Demo...
start "E-Grid Client Demo" cmd /k "cargo run --example grid_client_demo"

echo.
echo Both demos started!
echo.
echo Windows:
echo - "E-Grid Server Demo" - Server tracking windows and publishing events
echo - "E-Grid Client Demo" - Client displaying real-time grid updates
echo.
echo The demos will run automatically. Move windows around to see real-time updates!
echo.
echo Press any key to exit this launcher...
pause >nul
