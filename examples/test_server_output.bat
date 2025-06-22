@echo off
echo Starting server for 10 seconds to check output...
echo.

echo Building server...
cargo build --release --bin ipc_server_demo

echo.
echo Starting server (will run for 10 seconds)...
echo.

rem Run server with timeout and capture output
target\release\ipc_server_demo.exe > server_output.txt 2>&1 &
set SERVER_PID=%ERRORLEVEL%

rem Wait 10 seconds
timeout /t 10 /nobreak >nul

rem Kill the server process
taskkill /f /im ipc_server_demo.exe >nul 2>&1

echo.
echo Server output:
echo ==========================================
type server_output.txt
echo ==========================================
echo.
echo Done. Server output saved to server_output.txt
