# Softwane

Softwane is a non-intrusive rest reminder system based on progressive visual erosion. Instead of interrupting the user with popups, sounds, or modal overlays, it gradually reduces screen saturation, shifts the display toward a warmer color temperature, and approaches grayscale as a work session nears its end.

The goal is simple: make "it is time to take a break" feel like an ambient environmental change rather than a forced interruption.

The intended loop is also simple: the user notices the screen has visibly drifted, leaves to rest, and later comes back to press `Reset`. softwane does not try to manage the whole break lifecycle; it focuses on making the boundary perceptible.

## Repository Scope

This repository currently contains:

- A desktop app built with `Tauri + Rust + Native OS APIs`
- A Vue 3 control and settings UI for sessions, preview, visual channels, shortcuts, and startup behavior
- A product and technical specification derived from the original concept document

## Product Philosophy

- Softwane is an ambient cue, not a break workflow manager
- The product's job is to make "it is time to leave the screen" perceptible without a popup
- The core path is: `color shift becomes noticeable -> user leaves -> user returns later and presses Reset`
- Because of that, the current product direction does not include a separate `Recovery` state

## Project Structure

- `docs/spec.md`: Product and technical specification
- `src/`: Vue 3 frontend source
- `src-tauri/`: Tauri backend in Rust

## Prerequisites

Before running the project, install:

- `Rust` and `cargo` with toolchain `1.89.0` or newer
- `Node.js` 20+ and either `pnpm` or `npm`
- Tauri system prerequisites for your OS

Recommended checks:

```bash
rustc --version
cargo --version
node --version
npm --version
pnpm --version
```

Platform notes:

- On macOS, install Xcode Command Line Tools if they are missing: `xcode-select --install`
- On Windows, WebView2 must be available because Tauri uses the system webview

## Install Dependencies

Preferred:

From the repository root:

```bash
pnpm install
```

The first Rust build will also download crates from `crates.io`.

Alternative:

```bash
npm install
```

## Rust Toolchain

This repository includes a pinned Rust toolchain file:

- `rust-toolchain.toml` -> `1.89.0`

If your local Cargo version is older, update Rust before building:

```bash
rustup toolchain install 1.89.0
rustup override set 1.89.0
rustup default 1.89.0
```

If you prefer tracking the latest stable release instead:

```bash
rustup update stable
rustup default stable
```

## Run In Development

From the repository root:

```bash
pnpm tauri:dev
```

This launches the Tauri desktop app using:

- the Vite-powered Vue frontend
- the Rust backend in `src-tauri/`

## Build A Desktop Bundle

From the repository root:

```bash
pnpm tauri:build
```

This produces a packaged desktop build through Tauri.

Alternative:

```bash
npm run tauri:dev
npm run tauri:build
```

## Frontend Preview Only

If you only want to inspect the current phase preview UI in a browser without building the desktop app, run:

```bash
pnpm dev
```

Then open the local Vite URL shown in the terminal.

This is useful for reviewing the visual phase model without applying system-wide display changes.

## Current Implementation Status

Implemented:

- Session configuration model with persisted timer, channel, shortcut, and startup settings
- Runtime state model: `Idle`, `Preview`, `Progress`, `Settling`, `Rest`, and `Reverse`
- Sigmoid-based transition curve
- Real session timer and state machine
- Tray/menu bar presence with start, break, stop, reset, open, and quit controls
- Frontend progress channel for live timer and preview updates
- Configurable quick-start durations
- Configurable visual channels for saturation, warmth, brightness, cue timing, final intensity, and ramp shape
- Configurable global shortcuts
- Launch-at-login and silent-start controls
- Crash capture and acknowledgement flow
- Tray/menu bar progress status updates
- macOS native display adapter using `Core Graphics` transfer tables for warmth/brightness and a ColorSync saturation filter
- Windows native display adapter using the `Magnification API`
- Vue 3 control, preview, and settings UI
- Local structured tracing logs for runtime diagnostics
- Product and technical spec

Not implemented yet:

- Configurable loop behavior
- Dedicated structured observability event log with stable event categories
- Broader automated coverage for platform-specific display adapters

## Verification Notes

Local validation completed:

- `cargo test`
- `pnpm build`
- `pnpm tauri build --debug`

## Observability Logs

Runtime diagnostics are written through `tracing` as JSON logs in release builds.

Path details for this project:

- Bundle identifier: `com.softwane.app`
- Log filename prefix: `softwane.log`

Default log locations:

- macOS: `~/Library/Logs/com.softwane.app/softwane.log.YYYY-MM-DD`
- Windows: `%LOCALAPPDATA%\\com.softwane.app\\logs\\softwane.log.YYYY-MM-DD`

The backend resolves the path through Tauri's app log directory first and falls back to the app local data directory only if the log directory cannot be resolved.

Current logs include runtime diagnostics such as:

- Session transitions
- Manual user actions
- Platform apply failures
- Platform recovery attempts and recovery failures
- Startup, shortcut, window, and crash-recovery diagnostics

## Troubleshooting

### `feature edition2024 is required`

Your local Rust/Cargo is too old for the dependency graph being resolved.

Fix:

```bash
rustup toolchain install 1.89.0
rustup override set 1.89.0
pnpm install
pnpm tauri:dev
```

### `error: no such command: tauri`

Make sure JavaScript dependencies are installed first:

```bash
pnpm install
```

Then use:

```bash
pnpm tauri:dev
```

## Suggested Next Steps

1. Add configurable loop behavior
2. Add a dedicated structured observability event log with stable event names
3. Tune the transition parameters with manual visual testing on real hardware
4. Expand automated coverage for timer, shortcut, config, and platform adapter behavior
