@echo off
echo ===============================================
echo Testing E-Grid Heartbeat & Graceful Shutdown
echo ===============================================
echo.
echo This test demonstrates:
echo  💓 Server sending heartbeat messages every second
echo  💓 Client receiving heartbeats to stay connected  
echo  🛑 Graceful shutdown when server console is closed
echo  📡 Client detecting server shutdown via special heartbeat
echo.

echo 1. Starting e_grid server with heartbeat support...
start "E-Grid Server with Heartbeat" cmd /c "cd /d c:\w\e_grid && cargo run --bin ipc_server_demo"

echo 2. Waiting 3 seconds for server to initialize...
timeout /t 3 /nobreak >nul

echo 3. Starting e_grid client with heartbeat handling...
start "E-Grid Client with Heartbeat" cmd /c "cd /d c:\w\e_grid && cargo run --bin grid_client_demo"

echo.
echo ✅ Both processes started!
echo.
echo 🧪 Test Instructions:
echo ==================
echo  1. Watch both consoles - you should see:
echo     - Server: "💓 Server heartbeat - iteration X" every second
echo     - Client: Staying connected (no false disconnects)
echo.
echo  2. To test graceful shutdown:
echo     - Click the [X] close button on the SERVER window
echo     - Watch the CLIENT detect the shutdown gracefully
echo.
echo  3. Expected behavior:
echo     - Server sends shutdown heartbeat before closing
echo     - Client receives shutdown signal and exits gracefully
echo     - No false "server disconnected" errors during operation
echo.
echo Press any key to continue...
pause
