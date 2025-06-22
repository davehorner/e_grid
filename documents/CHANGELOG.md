# Changelog

All notable changes to the e_grid project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - Focus Event Tracking System
- **🎯 Complete Focus Event Integration**: Main e_grid server (`ipc_server_demo`) now publishes comprehensive focus events
  - **FOCUSED events** (event_type: 0) when windows gain focus
  - **DEFOCUSED events** (event_type: 1) when windows lose focus
  - Real-time focus change detection via Windows WinEvent hooks
  - Process ID, window title, and hash-based identification
  - Timestamp tracking for event ordering

- **🏗️ IPC Service Infrastructure**: 
  - New `GRID_FOCUS_EVENTS_SERVICE` for publishing `WindowFocusEvent` messages
  - Multi-client support (up to 8 simultaneous subscribers)
  - Message history and buffering for late-joining clients
  - Graceful client connect/disconnect handling

- **📡 Server-Side Focus Tracking**:
  - Added `focus_publisher` field to `GridIpcServer` struct
  - Added `last_focused_window` tracking for focus transition detection
  - Enhanced `handle_window_event()` to publish both focus and defocus events
  - Automatic focus state management with Windows API integration

- **🧪 Testing and Demo Infrastructure**:
  - Created `test_focus_defocus.bat` for easy end-to-end testing
  - Created `test_focus_integration.bat` for server-client validation
  - Enhanced existing focus demo examples to work with main server
  - Updated `examples/README.md` with comprehensive focus tracking documentation

### Enhanced
- **🔧 Server Architecture**: Enhanced main e_grid server to support focus events alongside existing grid management features
- **📋 Service Registration**: Updated service initialization to include focus events service
- **🖥️ Multi-Client Support**: All IPC services now support up to 8 concurrent clients with individual buffering

### Technical Details
- **Focus Event Structure**: `WindowFocusEvent` with event_type, hwnd, process_id, timestamp, app_name_hash, window_title_hash
- **Hash-Based Identification**: Simple but effective string hashing for app and window identification
- **Thread-Safe Operations**: Focus event publishing from WinEvent callbacks with minimal processing
- **Performance Optimized**: Lightweight focus detection with no expensive operations in event callbacks

### Breaking Changes
- None - All changes are additive and backward compatible

### Migration Guide
- **Existing Users**: No changes required - all existing functionality remains unchanged
- **Focus Tracking Users**: Can now use main server (`cargo run --bin ipc_server_demo`) instead of standalone `focus_demo_server`
- **New Users**: Recommended to use main server for all use cases as it provides complete e_grid functionality plus focus tracking

---

## [0.1.0] - Initial Release

### Added
- Core window grid management system
- Real-time window tracking with Windows API integration
- IPC communication system using iceoryx2
- Window positioning and layout management
- Multi-monitor support
- Animation system for smooth window transitions
- Grid layout saving and restoration
- Comprehensive client-server architecture
- Multiple demo applications and examples

### Core Features
- **Grid Management**: Virtual grid overlay for window positioning
- **Window Tracking**: Real-time window creation, movement, and destruction tracking
- **IPC Services**: High-performance inter-process communication
- **Multi-Monitor**: Support for multiple monitor configurations
- **Animations**: Smooth window animations with various easing functions
- **Layouts**: Save and restore window arrangements
- **Client Examples**: Multiple demo applications showing different use cases

### Architecture
- **Server**: Main window tracking and management server
- **Client**: Lightweight client library for applications
- **IPC**: Zero-copy message passing via iceoryx2
- **Windows API**: Native Windows integration for window management
