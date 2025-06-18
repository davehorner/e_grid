@echo off
echo 🎬 E-GRID ANIMATED DYNAMIC TRANSITION TEST
echo ==========================================
echo 📋 This test will animate your windows through different grid configurations:
echo    1. 2x2 grid arrangement with Bounce animation
echo    2. Cell rotation within 2x2 grid  
echo    3. 4x4 grid expansion with Elastic animation
echo    4. Cell rotation within 4x4 grid
echo    5. 8x8 grid expansion with Back animation
echo    6. 4x4 grid return with EaseInOut animation
echo    7. 2x2 grid return with EaseOut animation
echo    8. Original position restoration
echo.
echo 🎭 Features smooth 60 FPS animations with multiple easing functions!
echo ⚠️  WARNING: This will animate your open windows around the screen!
echo    Make sure to save your work before proceeding.
echo.
set /p confirm="Are you ready to see the animated transitions? (y/N): "
if /i "%confirm%" neq "y" (
    echo Test cancelled.
    exit /b 0
)

echo.
echo 🚀 Starting animated dynamic grid transition test...
cargo run --bin test_animated_transitions

echo.
echo ✅ Animated test completed!
pause
