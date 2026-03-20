# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Seriust is a TUI serial port monitor written in Rust. It uses ratatui for the terminal UI, crossterm for terminal event handling, serialport for serial communication, and clap for CLI argument parsing. Rust edition 2024.

## Build & Run

```bash
cargo build
cargo run
cargo run -- --port COM3 --baud 9600
```

## Architecture

- **`main.rs`** — CLI argument parsing (clap derive) and app entry point. Defines `Args` struct with serial config options (port, baud, data bits, stop bits, parity, flow control).
- **`app.rs`** — Core application state machine (`App` struct). Three states: `Capturing` (normal serial I/O), `Options` (popup menu), `PortList` (port selection). Handles keyboard events and manages the main loop. `Ctrl+A` toggles the options menu.
- **`ui.rs`** — All ratatui rendering. Layout has three vertical sections: output area, input line, status bar. Menu and port list render as centered popup overlays using `Clear` widget.
- **`serial.rs`** — `SerialConfig` struct wrapping serialport types with defaults (115200/8N1/no flow control).

## Key Patterns

- The app uses `ratatui::run()` which handles terminal setup/teardown automatically.
- UI is fully immediate-mode: `ui::draw()` receives `&App` and renders the entire frame each tick.
- The main loop in `App::run()` polls available ports every frame before drawing.
