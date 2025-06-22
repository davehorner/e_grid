@echo off
echo Testing Client Reconnection Logic
echo ================================
echo.
echo This test will:
echo 1. Start the server
echo 2. Start a focus demo client 
echo 3. Stop the server to simulate disconnection
echo 4. Restart the server to test reconnection
echo.
echo Press any key to start the test...
pause > nul

echo.
echo Step 1: Starting e_grid server...
echo.
start "E-Grid Server" cmd /k "cargo run --bin e_grid server"

echo Waiting 5 seconds for server to initialize...
timeout /t 5 /nobreak > nul

echo.
echo Step 2: Starting focus demo client...
echo.
start "Focus Demo Client" cmd /k "cargo run --example simple_focus_demo"

echo.
echo *** TEST INSTRUCTIONS ***
echo 1. Wait for the client to connect and show focus events
echo 2. Click on different windows to generate focus events 
echo 3. Close the server window (Step 1) to simulate disconnection
echo 4. Observe that the client detects the disconnection
echo 5. Restart the server by running: cargo run --bin e_grid server
echo 6. Observe that the client automatically reconnects
echo.
echo The test demonstrates the new reconnection logic!
echo.
pause
