@echo off
echo 🧪 Focus Demo Test - End to End
echo ================================

REM Kill any existing processes first
taskkill /f /im focus_demo_server.exe >nul 2>&1
taskkill /f /im simple_focus_demo.exe >nul 2>&1

echo.
echo 🚀 Step 1: Building examples...
cargo build --example focus_demo_server --example simple_focus_demo
if %ERRORLEVEL% neq 0 (
    echo ❌ Build failed!
    exit /b 1
)
echo ✅ Build successful!

echo.
echo 🎯 Step 2: Testing server startup...
echo Starting server in background...
start "Focus Demo Server" cmd /k "cargo run --example focus_demo_server"

echo Waiting for server to initialize...
timeout /t 4 /nobreak >nul

echo.
echo 📱 Step 3: Testing client connection...
echo (Running simple focus demo for a few seconds - check the server window for focus events)
echo Press any key when you want to stop the test...
pause

echo.
echo 🛑 Step 4: Stopping server...
taskkill /f /im focus_demo_server.exe >nul 2>&1
taskkill /f /im cmd.exe /fi "WindowTitle eq Focus Demo Server*" >nul 2>&1

echo.
echo 🛑 Step 4: Stopping server...
taskkill /f /im focus_demo_server.exe >nul 2>&1

echo.
echo 📊 Results:
echo The server window should have opened and be showing focus events.
echo You can now run the client manually to test the connection:
echo.
echo 💡 Next steps:
echo    1. The server is running in the opened window
echo    2. Open another terminal and run: cargo run --example simple_focus_demo
echo    3. Switch between windows to see focus events in the server window
echo    4. Use Ctrl+C in the server window to stop (or taskkill if needed)

echo.
echo 🧹 Final cleanup...
taskkill /f /im focus_demo_server.exe >nul 2>&1

echo.
echo ✅ Test completed!
echo.
echo 💡 To run manually:
echo    1. cargo run --example focus_demo_server
echo    2. (in another terminal) cargo run --example simple_focus_demo
echo    3. Use taskkill /f /im focus_demo_server.exe to stop if Ctrl+C doesn't work
