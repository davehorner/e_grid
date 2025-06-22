@echo off
echo Starting E-Grid IPC Demo Test
echo ================================

echo.
echo Starting Server (will auto-spawn client in new terminal)...
start "E-Grid Server" cmd /k "cargo run --bin ipc_demo_new"

echo.
echo Server started!
echo - Server window: "E-Grid Server"
echo - Client window: New CMD window (auto-spawned)
echo.
echo In the server window, type 'e' to send events, 'g' to show grid
echo In the client window, type 'g' to request grid state, 'w' for windows
echo.
pause
