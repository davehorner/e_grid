@echo off
echo Starting server for 15 seconds to check window publishing...
echo.

echo Starting server...
start "Server Test" cmd /c "target\release\ipc_server_demo.exe & timeout /t 15 /nobreak >nul & exit"

echo Waiting for server to run and publish windows...
timeout /t 18 /nobreak >nul

echo.
echo Server test completed. Check the server window for publishing output.
pause
