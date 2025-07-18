# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/davehorner/e_grid/compare/v0.1.8...v0.2.0) - 2025-07-18

### Added

- *(events)* [**breaking**] add EventDispatchMode and support raw WindowEvent dispatching

## [0.1.8](https://github.com/davehorner/e_grid/compare/v0.1.7...v0.1.8) - 2025-07-17

### Other

- *(events)* add continuous move/resize event tracking and client callbacks  - Introduced new `GridEvent` variants: `WindowMove` and `WindowResize` for granular tracking of continuous gestures. - Added corresponding event type constants and utility functions for consistent encoding/decoding. - Updated `GridClient` to support `move_callback` and `resize_callback` handlers for real-time event consumption. - Enhanced `win_event_proc` to emit continuous move and resize events during window gesture operations. - Refactored server/client `grid_event_to_window_event` logic to use `grid_event_type_code` mapping. - Silenced some debug prints for cleaner logs during regular use.

## [0.1.7](https://github.com/davehorner/e_grid/compare/v0.1.6...v0.1.7) - 2025-07-13

### Added

- *(grid)* skip grid placement and animation for maximized windows

## [0.1.6](https://github.com/davehorner/e_grid/compare/v0.1.5...v0.1.6) - 2025-07-12

### Fixed

- *(e_grid_all)* stop chrome tooltips from being seen as toplevel windows.  if a window doesn't size, at least translate and set window z-order to bottom. fixed issue with focus being driven to initial focused window.

## [0.1.5](https://github.com/davehorner/e_grid/compare/v0.1.4...v0.1.5) - 2025-07-12

### Added

- *(examples)* add dynamic grid window animation demos and enhanced animation tracking

## [0.1.4](https://github.com/davehorner/e_grid/compare/v0.1.3...v0.1.4) - 2025-07-11

### Added

- *(examples)* add 4x4 grid window rotation demos with animation and movement

## [0.1.3](https://github.com/davehorner/e_grid/compare/v0.1.2...v0.1.3) - 2025-07-06

### Added

- coloring foreground and topmost in grid, still into the tangle we go.

### Other

- wip
- wip
- wip
- intermediate state

## [0.1.2](https://github.com/davehorner/e_grid/compare/v0.1.1...v0.1.2) - 2025-06-25

### Added

- add lock-free window event system with move/resize detection and crossbeam queues

### Other

- *(ipc)* add e_midi integration with move/resize and focus event callbacks  - Integrated `e_midi` as a local dependency with a new `e_midi_demo` example. - Introduced support for `move_resize_start` and `move_resize_stop` callbacks in `GridClient`. - Enhanced window event tracking to trigger MIDI playback on focus and window interaction. - Refactored `GridIpcServer` to replace println! with structured `log` macros. - Reduced console noise and centralized debug output via the `log` crate. - Improved robustness of event polling with fallback for missing HWNDs. - Added missing crates to `Cargo.lock` including ALSA, CoreMIDI, midir, wasm-bindgen, and dependencies.
- initial release.  this is a pre-release of e_grid.  developer release.

## [0.1.1](https://github.com/davehorner/e_grid/compare/v0.1.0...v0.1.1) - 2025-06-22

### Added

- *(ipc)* add e_midi integration with move/resize and focus event callbacks

### Other

- initial release.  this is a pre-release of e_grid.  developer release.

## [0.1.0](https://github.com/davehorner/e_grid/releases/tag/v0.1.0) - 2025-06-22

### Added

- initial release.  this is a pre-release of e_grid.  developer release.
- add move/resize event handling and dependencies
- ipc_protocol; move and resize events.
- enhance grid synchronization and logging with new window event structures and improved error handling
- *(e_grid)* implement console control handler for graceful shutdown and update status reporting intervals in e_grid.rs
- *(e_grid)* implement console control handler for graceful shutdown and update status reporting intervals
- focus now working with primary e_grid and grid_client_demo. using lib winevents and client ipc
- *(grid)* add dynamic grid sizing and animated transitions
- *(grid)* add dynamic grid sizing and animated transitions
- *(ipc)* add grid layout and animation system with easing and saved layouts
- Implement event-driven IPC for real-time window updates
- *(grid)* add IPC-driven window assignment with coverage-aware cell detection
- *(ipc)* add iceoryx2-based IPC system for grid coordination
- add monitor grid management and window tracking functionality in lib.rs
- add window tracking functionality with multi-monitor support and grid display

### Other

- a great restructuring of LLM noise.
- wip intermediate
- more wip
- clean this up. wip
- *(window_events)* improve static mut access safety and cleanup in lib.rs
