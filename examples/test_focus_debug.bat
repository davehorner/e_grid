@echo off
echo Testing Focus Events...
echo.
echo 1. Start the server in one window
echo 2. Start the grid_client_demo in another window  
echo 3. Switch between different windows to generate focus events
echo.
echo You should see:
echo   - Server: 🎯 Published FOCUSED/DEFOCUSED events
echo   - Client: 🎯 [FOCUS EVENT] messages + 🟢 FOCUSED/🔴 DEFOCUSED from callback
echo.
echo If you only see server events but no client events, there's an IPC issue
echo If you see client [FOCUS EVENT] but no 🟢/🔴 messages, the callback isn't working
echo.
pause
echo Starting server and client for focus testing...
echo.

start "E-Grid Server" cmd /k "cargo run --bin e_grid"
timeout /t 3
start "Grid Client Demo" cmd /k "cargo run --bin grid_client_demo"

echo.
echo Both processes started in separate windows.
echo Switch between different applications to test focus events.
echo.
pause
