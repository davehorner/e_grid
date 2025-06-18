@echo off
echo E-Grid Demo Launcher
echo ===================

echo.
echo Choose a demo to run:
echo.
echo 1. Server Demo (non-interactive)
echo 2. Client Demo (non-interactive) 
echo 3. Both Demos (recommended)
echo 4. Build Release Version
echo.

set /p choice=Enter your choice (1-4): 

if "%choice%"=="1" (
    echo Starting Server Demo...
    cargo run --bin ipc_server_demo
) else if "%choice%"=="2" (
    echo Starting Client Demo...
    echo Note: Make sure server is running first!
    cargo run --bin grid_client_demo
) else if "%choice%"=="3" (
    echo Starting both demos...
    start "E-Grid Server Demo" cmd /k "cargo run --bin ipc_server_demo"
    timeout /t 2 /nobreak >nul
    start "E-Grid Client Demo" cmd /k "cargo run --bin grid_client_demo"
    echo Both demos started in separate windows!
) else if "%choice%"=="4" (
    echo Building release versions...
    cargo build --release --bin ipc_server_demo --bin grid_client_demo
    echo.
    echo Release binaries built! You can run them with:
    echo   target\release\ipc_server_demo.exe
    echo   target\release\grid_client_demo.exe
) else (
    echo Invalid choice!
)

echo.
pause
