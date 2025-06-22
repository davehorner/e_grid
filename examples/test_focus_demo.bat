@echo off
echo 🧪 Focus Demo End-to-End Test
echo ===============================
echo.
echo Testing the focus tracking system...
echo.

echo 🚀 Step 1: Building examples...
cargo build --example focus_demo_server --example simple_focus_demo
if %ERRORLEVEL% neq 0 (
    echo ❌ Build failed!
    exit /b 1
)

echo ✅ Build successful!
echo.

echo 🎯 Step 2: Starting focus demo server...
echo (Server will run for a few seconds to demonstrate functionality)
echo.

REM Start server in background for a few seconds
start /b cmd /c "cargo run --example focus_demo_server > server_output.txt 2>&1"

REM Wait a moment for server to initialize
timeout /t 3 /nobreak > nul

echo 📡 Step 3: Server is running, checking if simple focus demo can connect...
echo.

REM Run client for a few seconds
timeout /t 5 cargo run --example simple_focus_demo > client_output.txt 2>&1

echo.
echo 📊 Results:
echo.

echo 🖥️  Server Output:
type server_output.txt | findstr /C:"✅" /C:"🎯" /C:"📡" /C:"📊"
echo.

echo 📱 Client Output:
type client_output.txt | findstr /C:"Connected" /C:"Received" /C:"Focus" /C:"Error"
echo.

echo 🛑 Stopping server...
taskkill /f /im focus_demo_server.exe > nul 2>&1

echo.
echo ✅ Test completed! Check outputs above to verify the connection works.
echo.
echo 💡 To run the full interactive demo:
echo    cargo run --example focus_demo_server
echo    (in another terminal) cargo run --example simple_focus_demo
