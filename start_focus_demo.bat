@echo off
echo 🎯 Focus Demo - Simple Manual Test
echo ===================================
echo.
echo This will start the focus demo server in a visible window.
echo You can then manually test it with a client.
echo.

REM Kill any existing processes first
taskkill /f /im focus_demo_server.exe >nul 2>&1

echo 🚀 Building focus demo server...
cargo build --example focus_demo_server
if %ERRORLEVEL% neq 0 (
    echo ❌ Build failed!
    pause
    exit /b 1
)

echo ✅ Build successful!
echo.
echo 🎯 Starting focus demo server...
echo (A new window will open - watch it for focus events)
echo.

start "Focus Demo Server" cmd /k "echo 🎯 Focus Demo Server && echo Use Ctrl+C to stop or close this window && echo. && cargo run --example focus_demo_server"

echo.
echo ✅ Server started in new window!
echo.
echo 📱 To test the client connection:
echo    1. Open another command prompt
echo    2. Navigate to: c:\w\demos\e_midi\e_grid
echo    3. Run: cargo run --example simple_focus_demo
echo    4. Switch between windows to see focus events in the server window
echo.
echo 🛑 To stop the server:
echo    - Use Ctrl+C in the server window, or
echo    - Close the server window, or  
echo    - Run: taskkill /f /im focus_demo_server.exe
echo.
pause
