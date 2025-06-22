@echo off
echo 🧪 E-GRID DYNAMIC SIZING INTEGRATION TEST
echo ========================================
echo.

echo 📋 This script will test the dynamic grid sizing functionality
echo    by running server and client with different configurations.
echo.

echo Step 1: Building the project...
cargo build --lib
if %ERRORLEVEL% neq 0 (
    echo ❌ Build failed. Please fix compilation errors first.
    pause
    exit /b 1
)
echo ✅ Build successful!
echo.

echo Step 2: Running dynamic grid unit tests...
cargo run --bin test_dynamic_grid
echo.
echo ⏸️  Press any key to continue to IPC testing...
pause > nul
echo.

echo Step 3: Testing IPC Server-Client Dynamic Grid Exchange
echo ========================================================
echo.
echo 🖥️  Starting IPC Server in background...
echo    (This will use a default grid configuration)
echo.

REM Start server in background (you'll need to terminate manually)
start "E-Grid Server" cmd /k "echo 🖥️ E-GRID SERVER && echo ================ && cargo run --bin ipc_server_demo_new"

echo ⏳ Waiting 3 seconds for server to start...
timeout /t 3 /nobreak > nul

echo.
echo 👥 Starting IPC Client to test config exchange...
echo    (This should request and receive grid config from server)
echo.

REM Run client
cargo run --bin ipc_demo_new

echo.
echo 🎯 TESTING COMPLETE!
echo ===================
echo.
echo ✅ What was tested:
echo    📐 Multiple grid size configurations
echo    🔄 IPC server-client config exchange  
echo    🖥️  WindowTracker with dynamic grids
echo    📦 Config serialization
echo.
echo 📊 Check the output above for:
echo    - Server announcing its grid configuration
echo    - Client requesting and receiving config
echo    - All grid operations using dynamic sizing
echo.
echo ⚠️  Note: Please manually close the server window when done.
echo.
pause
