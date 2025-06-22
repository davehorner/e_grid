@echo off
setlocal

REM Focus Tracking Demonstration Script for Windows
REM This script demonstrates the complete focus tracking system with server and client examples

echo 🎯 e_grid Focus Tracking Complete Demonstration
echo ==============================================
echo.
echo This demonstration shows the complete focus tracking system:
echo • Focus Demo Server provides real-time focus events via IPC
echo • Focus tracking examples receive and process these events
echo • Multiple client examples demonstrate different use cases
echo.

echo 📋 Available Focus Tracking Examples:
echo 1. focus_demo_server     - The IPC server that detects and broadcasts focus events
echo 2. simple_focus_demo     - Basic focus event logging
echo 3. focus_tracking_demo   - Statistics and history tracking
echo 4. focus_music_demo      - Music control simulation
echo 5. comprehensive_focus_demo - All features combined (RECOMMENDED)
echo.

echo ⚡ Quick Demo (Recommended):
echo   Run the server and comprehensive demo automatically
echo.
echo 🎮 Manual Mode:
echo   Choose which client example to run with the server
echo.

set /p mode="Choose mode - [Q]uick demo or [M]anual mode (Q/M): "

if /i "%mode%"=="Q" goto quick_demo
if /i "%mode%"=="M" goto manual_mode
goto quick_demo

:quick_demo
echo.
echo 🚀 Starting Quick Demo...
echo ========================
echo.

echo 🚀 Step 1: Starting Focus Demo Server...
echo ========================================
echo Starting server in background (you'll see server output)...
echo Press Ctrl+C in the server window to stop the server when done.
echo.
start "Focus Demo Server" cmd /c "cargo run --example focus_demo_server"

echo Waiting 5 seconds for server to initialize...
timeout /t 5 >nul

echo.
echo 🎯 Step 2: Running Comprehensive Focus Demo...
echo ==============================================
echo This combines all focus tracking features:
echo • Real-time focus event monitoring
echo • Statistical analysis and rankings  
echo • Music control simulation
echo • Comprehensive reporting
echo.
echo 💡 Switch between different applications to see focus events!
echo ⌨️  Press Ctrl+C to stop the client demonstration
echo.

REM Run comprehensive demo
cargo run --example comprehensive_focus_demo

goto end

:manual_mode
echo.
echo 🎮 Manual Mode Selected
echo ======================
echo.

echo 🚀 Starting Focus Demo Server...
echo ================================
echo Starting server in background...
start "Focus Demo Server" cmd /c "cargo run --example focus_demo_server"

echo Waiting 5 seconds for server to initialize...
timeout /t 5 >nul

echo.
echo 📋 Available Client Examples:
echo 1. Simple Focus Demo (basic event logging)
echo 2. Focus Tracking Demo (statistics and history)
echo 3. Focus Music Demo (music control simulation)
echo 4. Comprehensive Focus Demo (all features)
echo.

set /p choice="Select example (1-4): "

if "%choice%"=="1" (
    echo Running Simple Focus Demo...
    cargo run --example simple_focus_demo
) else if "%choice%"=="2" (
    echo Running Focus Tracking Demo...
    cargo run --example focus_tracking_demo
) else if "%choice%"=="3" (
    echo Running Focus Music Demo...
    cargo run --example focus_music_demo
) else if "%choice%"=="4" (
    echo Running Comprehensive Focus Demo...
    cargo run --example comprehensive_focus_demo
) else (
    echo Invalid choice. Running Comprehensive Focus Demo by default...
    cargo run --example comprehensive_focus_demo
)

:end
echo.
echo 👋 Focus tracking demonstration completed!
echo ==========================================
echo.
echo 🔄 To run again:
echo   focus_demo.bat
echo.
echo 🚀 To run individual components:
echo   Server:  cargo run --example focus_demo_server
echo   Client:  cargo run --example comprehensive_focus_demo
echo.
echo 📝 Note: Remember to stop the server window when you're done!
echo.

pause
