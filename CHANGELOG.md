# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
