@echo off
echo Testing E-Grid IPC Communication
echo ================================

echo.
echo Starting server in background...
start /min "E-Grid Server Test" cmd /c "cargo run --bin ipc_server_demo > server_output.txt 2>&1"

echo Waiting 5 seconds for server to start...
timeout /t 5 /nobreak >nul

echo.
echo Starting client for 15 seconds...
timeout /t 15 /nobreak | cargo run --bin grid_client_demo

echo.
echo Stopping server...
taskkill /F /FI "WINDOWTITLE eq E-Grid Server Test" >nul 2>&1

echo.
echo Server output:
type server_output.txt 2>nul

echo.
echo Test complete. Check the output above to see if IPC is working.
pause
