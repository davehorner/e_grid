@echo off
echo ================================================
echo Testing Focus Events After Server Restart
echo ================================================
echo.
echo This test demonstrates focus/defocus events working 
echo correctly after the server shuts down and restarts.
echo.
echo Instructions:
echo 1. The server will start first
echo 2. The client will connect 
echo 3. You can test focus events (click different windows)
echo 4. Close the server window to test restart
echo 5. Start a new server manually
echo 6. Check that focus events continue to work
echo.

echo 1. Starting e_grid server...
start "E-Grid Server" cmd /c "cd /d c:\w\e_grid && cargo run --bin ipc_server_demo"

echo 2. Waiting 3 seconds for server to initialize...
timeout /t 3 /nobreak >nul

echo 3. Starting e_grid client...
start "E-Grid Client" cmd /c "cd /d c:\w\e_grid && cargo run --bin grid_client_demo"

echo.
echo ✅ Both processes started!
echo.
echo 🧪 Test Procedure:
echo ==================
echo 1. Both windows should be running now
echo 2. Try clicking different windows - you should see focus events in both consoles
echo 3. Close the SERVER window (click [X])
echo 4. The client should detect the disconnect
echo 5. Start a new server: cargo run --bin ipc_server_demo
echo 6. The client should reconnect automatically
echo 7. Try clicking windows again - focus events should still work!
echo.
echo 📋 Expected Results:
echo - Focus events work initially ✓
echo - Server shuts down gracefully ✓  
echo - Client detects disconnect ✓
echo - Client reconnects to new server ✓
echo - Focus events work after reconnection ✓
echo.
echo Press any key to continue...
pause
