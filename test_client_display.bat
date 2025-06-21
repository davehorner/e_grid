@echo off
echo Testing client grid display...
cd /d c:\w\demos\e_midi\e_grid

echo Starting e_grid server in background...
start /b cargo run --bin e_grid server > server_temp.log 2>&1

echo Waiting for server to start...
timeout /t 3 /nobreak > nul

echo Running client test...
cargo run --bin test_client_grid_display

echo.
echo Test completed. Server log:
type server_temp.log
del server_temp.log
