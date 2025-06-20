#!/bin/bash

# Focus Tracking Demonstration Script
# This script demonstrates the complete focus tracking system with server and client examples

echo "🎯 e_grid Focus Tracking Complete Demonstration"
echo "=============================================="
echo ""
echo "This demonstration shows the complete focus tracking system:"
echo "• Focus Demo Server provides real-time focus events via IPC"
echo "• Focus tracking examples receive and process these events"
echo "• Multiple client examples demonstrate different use cases"
echo ""

echo "📋 Available Focus Tracking Examples:"
echo "1. focus_demo_server     - The IPC server that detects and broadcasts focus events"
echo "2. simple_focus_demo     - Basic focus event logging"
echo "3. focus_tracking_demo   - Statistics and history tracking"
echo "4. focus_music_demo      - Music control simulation"
echo "5. comprehensive_focus_demo - All features combined (RECOMMENDED)"
echo ""

# Function to run server in background
start_server() {
    echo "🚀 Starting Focus Demo Server..."
    echo "================================"
    cargo run --example focus_demo_server &
    SERVER_PID=$!
    echo "Server started with PID: $SERVER_PID"
    echo "Waiting 3 seconds for server to initialize..."
    sleep 3
    echo ""
}

# Function to stop server
stop_server() {
    echo ""
    echo "🛑 Stopping Focus Demo Server..."
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null
        echo "Server stopped (PID: $SERVER_PID)"
    fi
}

# Trap to ensure server is stopped on script exit
trap stop_server EXIT

echo "⚡ Quick Demo (Recommended):"
echo "  Run the server and comprehensive demo automatically"
echo ""
echo "🎮 Manual Mode:"
echo "  Choose which client example to run with the server"
echo ""

read -p "Choose mode - [Q]uick demo or [M]anual mode (Q/M): " mode

case $mode in
    [Qq]* )
        echo ""
        echo "🚀 Starting Quick Demo..."
        echo "========================"
        echo ""
        
        # Start server
        start_server
        
        echo "🎯 Running Comprehensive Focus Demo..."
        echo "======================================"
        echo "This combines all focus tracking features:"
        echo "• Real-time focus event monitoring"
        echo "• Statistical analysis and rankings"
        echo "• Music control simulation"
        echo "• Comprehensive reporting"
        echo ""
        echo "💡 Switch between different applications to see focus events!"
        echo "⌨️  Press Ctrl+C to stop the demonstration"
        echo ""
        
        # Run comprehensive demo
        cargo run --example comprehensive_focus_demo
        ;;
        
    [Mm]* )
        echo ""
        echo "🎮 Manual Mode Selected"
        echo "======================"
        echo ""
        
        # Start server
        start_server
        
        echo "📋 Available Client Examples:"
        echo "1. Simple Focus Demo (basic event logging)"
        echo "2. Focus Tracking Demo (statistics and history)"
        echo "3. Focus Music Demo (music control simulation)"
        echo "4. Comprehensive Focus Demo (all features)"
        echo ""
        
        read -p "Select example (1-4): " choice
        
        case $choice in
            1)
                echo "Running Simple Focus Demo..."
                cargo run --example simple_focus_demo
                ;;
            2)
                echo "Running Focus Tracking Demo..."
                cargo run --example focus_tracking_demo
                ;;
            3)
                echo "Running Focus Music Demo..."
                cargo run --example focus_music_demo
                ;;
            4)
                echo "Running Comprehensive Focus Demo..."
                cargo run --example comprehensive_focus_demo
                ;;
            *)
                echo "Invalid choice. Running Comprehensive Focus Demo by default..."
                cargo run --example comprehensive_focus_demo
                ;;
        esac
        ;;
        
    * )
        echo "Invalid choice. Running Quick Demo by default..."
        
        # Start server
        start_server
        
        echo "🎯 Running Comprehensive Focus Demo..."
        cargo run --example comprehensive_focus_demo
        ;;
esac

echo ""
echo "👋 Focus tracking demonstration completed!"
echo "=========================================="
echo ""
echo "🔄 To run again:"
echo "  ./focus_demo.sh"
echo ""
echo "🚀 To run individual components:"
echo "  Server:  cargo run --example focus_demo_server"
echo "  Client:  cargo run --example comprehensive_focus_demo"
echo ""
