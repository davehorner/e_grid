@echo off
echo Testing Focus Event Integration
echo ===============================

echo.
echo This will start the server with focus event publishing and a focus demo client.
echo.
echo - Server: Main IPC server with integrated focus event publishing
echo - Client: Simple focus demo that listens for focus events
echo.

echo Starting Server Demo...
start "E-Grid Server (Focus Enabled)" cmd /k "cargo run --bin ipc_server_demo"

echo.
echo Waiting 3 seconds for server to initialize...
timeout /t 3 /nobreak >nul

echo Starting Focus Demo Client...
start "Focus Events Client" cmd /k "cargo run --example simple_focus_demo"

echo.
echo Both components started!
echo.
echo Windows:
echo - "E-Grid Server (Focus Enabled)" - Server with focus event publishing
echo - "Focus Events Client" - Client listening for focus events
echo.
echo Try switching between different windows to generate focus events!
echo The client should display real-time focus change notifications.
echo.
echo Press any key to exit this launcher...
pause >nul
