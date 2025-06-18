@echo off
echo 🎬 E-GRID ANIMATED TRANSITIONS
echo =============================
echo.
echo This demo showcases smooth animated window transitions through:
echo   • 2x2 grid with Bounce animation
echo   • Cell rotation within grids  
echo   • 4x4 grid with Elastic animation
echo   • 8x8 grid with Back animation
echo   • Smooth return transitions
echo   • 60 FPS real-time animations
echo.
echo ⚠️  This will animate your windows - save your work first!
echo.
set /p start="Ready to start the animated demo? (y/N): "
if /i "%start%" neq "y" exit /b

echo.
echo 🚀 Launching animated grid transitions...
echo.
cargo run --bin test_animated_transitions

echo.
echo 🎉 Demo complete! Your windows should be back to normal.
pause
