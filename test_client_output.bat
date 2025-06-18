@echo off
echo Starting client for 10 seconds to check output...
echo.

echo Building client...
cargo build --release --bin grid_client_demo

echo.
echo Starting client (will run for 10 seconds)...
echo.

rem Run client and capture output
target\release\grid_client_demo.exe > client_output.txt 2>&1 &

rem Wait 10 seconds
timeout /t 10 /nobreak >nul

rem Kill the client process
taskkill /f /im grid_client_demo.exe >nul 2>&1

echo.
echo Client output:
echo ==========================================
type client_output.txt
echo ==========================================
echo.
echo Done. Client output saved to client_output.txt
