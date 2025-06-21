@echo off
echo 🔍 Log Level Review and Optimization Test
echo ==========================================
echo.
echo This test reviews and validates log level assignments
echo across the E-Grid codebase for optimal verbosity.
echo.

echo 📋 Analyzing log level usage in key files...
echo.

echo 🔧 IPC Client (src\ipc_client.rs):
echo -----------------------------------
findstr /N /C:"debug!" src\ipc_client.rs 2>nul
findstr /N /C:"info!" src\ipc_client.rs 2>nul  
findstr /N /C:"warn!" src\ipc_client.rs 2>nul
findstr /N /C:"error!" src\ipc_client.rs 2>nul
echo.

echo 🔧 IPC Core (src\ipc.rs):
echo --------------------------
findstr /N /C:"debug!" src\ipc.rs 2>nul
findstr /N /C:"info!" src\ipc.rs 2>nul
findstr /N /C:"warn!" src\ipc.rs 2>nul  
findstr /N /C:"error!" src\ipc.rs 2>nul
echo.

echo 🔧 Real-time Monitor (realtime_monitor_grid.rs):
echo ------------------------------------------------
findstr /N /C:"debug!" realtime_monitor_grid.rs 2>nul
findstr /N /C:"info!" realtime_monitor_grid.rs 2>nul
findstr /N /C:"warn!" realtime_monitor_grid.rs 2>nul
findstr /N /C:"error!" realtime_monitor_grid.rs 2>nul
echo.

echo 📊 Log Level Testing with Different Verbosity:
echo ==============================================
echo.

echo Testing with RUST_LOG=error (minimal output):
set RUST_LOG=error
start /min "Log-Test-Error" cmd /c "timeout /t 10 >nul & taskkill /F /IM ipc_server_demo.exe >nul 2>&1"
start /min "Log-Test-Error-Server" cmd /c "cargo run --bin ipc_server_demo > log_test_error_server.txt 2>&1"
timeout /t 5 >nul
cargo run --bin grid_client_demo > log_test_error_client.txt 2>&1 &
timeout /t 8 >nul
taskkill /F /IM cargo.exe >nul 2>&1

echo.
echo Testing with RUST_LOG=warn (moderate output):
set RUST_LOG=warn  
start /min "Log-Test-Warn" cmd /c "timeout /t 10 >nul & taskkill /F /IM ipc_server_demo.exe >nul 2>&1"
start /min "Log-Test-Warn-Server" cmd /c "cargo run --bin ipc_server_demo > log_test_warn_server.txt 2>&1"
timeout /t 5 >nul
cargo run --bin grid_client_demo > log_test_warn_client.txt 2>&1 &
timeout /t 8 >nul
taskkill /F /IM cargo.exe >nul 2>&1

echo.
echo Testing with RUST_LOG=info (verbose output):
set RUST_LOG=info
start /min "Log-Test-Info" cmd /c "timeout /t 10 >nul & taskkill /F /IM ipc_server_demo.exe >nul 2>&1"
start /min "Log-Test-Info-Server" cmd /c "cargo run --bin ipc_server_demo > log_test_info_server.txt 2>&1"
timeout /t 5 >nul
cargo run --bin grid_client_demo > log_test_info_client.txt 2>&1 &
timeout /t 8 >nul
taskkill /F /IM cargo.exe >nul 2>&1

echo.
echo Testing with RUST_LOG=debug (full output):
set RUST_LOG=debug
start /min "Log-Test-Debug" cmd /c "timeout /t 10 >nul & taskkill /F /IM ipc_server_demo.exe >nul 2>&1"
start /min "Log-Test-Debug-Server" cmd /c "cargo run --bin ipc_server_demo > log_test_debug_server.txt 2>&1"
timeout /t 5 >nul
cargo run --bin grid_client_demo > log_test_debug_client.txt 2>&1 &
timeout /t 8 >nul
taskkill /F /IM cargo.exe >nul 2>&1

echo.
echo 📈 Log Level Analysis Results:
echo ==============================
echo.

echo Error level output size:
for %%F in (log_test_error_*.txt) do (
    echo %%F: 
    for /f %%A in ('type "%%F" 2^>nul ^| find /c /v ""') do echo   %%A lines
)

echo.
echo Warn level output size:
for %%F in (log_test_warn_*.txt) do (
    echo %%F:
    for /f %%A in ('type "%%F" 2^>nul ^| find /c /v ""') do echo   %%A lines  
)

echo.
echo Info level output size:
for %%F in (log_test_info_*.txt) do (
    echo %%F:
    for /f %%A in ('type "%%F" 2^>nul ^| find /c /v ""') do echo   %%A lines
)

echo.
echo Debug level output size:
for %%F in (log_test_debug_*.txt) do (
    echo %%F:
    for /f %%A in ('type "%%F" 2^>nul ^| find /c /v ""') do echo   %%A lines
)

echo.
echo 🎯 Recommendations:
echo ===================
echo For production TUI use: RUST_LOG=error (minimal disruption)
echo For development/debug: RUST_LOG=info (balanced visibility)
echo For deep troubleshooting: RUST_LOG=debug (full verbosity)
echo.
echo Generated log level test files:
echo - log_test_error_*.txt (error level)
echo - log_test_warn_*.txt (warn level)  
echo - log_test_info_*.txt (info level)
echo - log_test_debug_*.txt (debug level)
echo.

rem Cleanup
taskkill /F /IM cargo.exe >nul 2>&1
taskkill /F /IM ipc_server_demo.exe >nul 2>&1

echo 🎉 Log level testing complete!
pause
