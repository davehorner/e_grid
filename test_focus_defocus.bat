@echo off
echo Testing FOCUSED and DEFOCUSED Events
echo ====================================

echo.
echo This will test the main server's ability to send both FOCUSED and DEFOCUSED events.
echo.

echo Building both server and client...
cargo build --bin ipc_server_demo --example simple_focus_demo

echo.
echo Starting Server with Focus Tracking...
start "Main Server (Focus+Defocus)" cmd /k "cargo run --bin ipc_server_demo"

echo.
echo Waiting 3 seconds for server to initialize...
timeout /t 3 /nobreak >nul

echo Starting Focus Demo Client...
start "Focus Events Client" cmd /k "cargo run --example simple_focus_demo"

echo.
echo Test Instructions:
echo =================
echo 1. Watch the "Focus Events Client" window for event notifications
echo 2. Click on different windows (Notepad, Explorer, etc.)
echo 3. You should see both FOCUSED and DEFOCUSED events
echo.
echo - FOCUSED events when you click on a window
echo - DEFOCUSED events when you click away from that window
echo.
echo Try opening Notepad and clicking between it and other windows!
echo.
echo Press any key to exit this launcher...
pause >nul
