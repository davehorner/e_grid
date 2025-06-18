@echo off
echo Starting E-Grid IPC Server and Client Test...
echo.

echo Starting server...
start "E-Grid Server" cmd /c "target\release\ipc_server_demo.exe"

echo Waiting 3 seconds for server to initialize...
timeout /t 3 /nobreak >nul

echo Starting client...
start "E-Grid Client" cmd /c "target\release\grid_client_demo.exe"

echo.
echo Both server and client are now running in separate windows.
echo Press any key to exit this launcher...
pause >nul
