@echo off
echo 🎯 e_grid Full Server + Focus Demo Test
echo =======================================
echo.
echo This script starts the full e_grid server and tests focus tracking.
echo.

REM Kill any existing processes
taskkill /f /im grid_tracker.exe >nul 2>&1
taskkill /f /im simple_focus_demo.exe >nul 2>&1

echo 🚀 Building server and client...
cargo build --bin grid_tracker --example simple_focus_demo
if %ERRORLEVEL% neq 0 (
    echo ❌ Build failed!
    pause
    exit /b 1
)
echo ✅ Build successful!

echo.
echo 🎯 Starting full e_grid server...
echo (A new window will open with the grid tracker server)
start "e_grid Server" cmd /k "echo 🎯 e_grid Server && echo Use Ctrl+C to stop or close this window && echo. && cargo run --bin grid_tracker"

echo.
echo ⏳ Waiting for server to initialize...
timeout /t 5 /nobreak >nul

echo.
echo 📱 Now testing focus demo client...
echo (This should connect successfully to the server)
echo.
pause

echo.
echo 🧪 Running simple focus demo...
cargo run --example simple_focus_demo

echo.
echo 🛑 Stopping server...
taskkill /f /im grid_tracker.exe >nul 2>&1

echo.
echo ✅ Test completed!
pause
