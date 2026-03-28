# Erode App

Erode App is a non-intrusive rest reminder system based on progressive visual erosion. Instead of interrupting the user with popups, sounds, or modal overlays, it gradually reduces screen saturation, shifts the display toward a warmer color temperature, and approaches grayscale as a work session nears its end.

The goal is simple: make "it is time to take a break" feel like an ambient environmental change rather than a forced interruption.

The intended loop is also simple: the user notices the screen has visibly drifted, leaves to rest, and later comes back to press `Reset`. Erode does not try to manage the whole break lifecycle; it focuses on making the boundary perceptible.

## Repository Scope

This repository currently contains:

- A desktop app scaffold built for `Tauri + Rust + Native OS APIs`
- A Vue 3 preview UI for the reminder phases
- A product and technical specification derived from the original concept document

## Product Philosophy

- Erode is an ambient cue, not a break workflow manager
- The product's job is to make "it is time to leave the screen" perceptible without a popup
- The core path is: `color shift becomes noticeable -> user leaves -> user returns later and presses Reset`
- Because of that, the current product direction does not include a separate `Recovery` state

## Project Structure

- `docs/spec.md`: Product and technical specification
- `src/`: Vue 3 frontend source
- `src-tauri/`: Tauri backend scaffold in Rust

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

- Session configuration model
- Phase model: `Stable`, `JND`, `Evolution`, `Statue`
- Sigmoid-based transition curve
- Real session timer and state machine with `Idle`, `Work`, `Paused`, and `Break`
- Tray/menu bar presence with basic controls
- Native shell progress feedback
- macOS native display adapter using `Core Graphics` transfer tables as a system-wide fallback
- Windows native display adapter using the `Magnification API`
- Vue 3 preview UI
- Product and technical spec

Not implemented yet:

- Global hotkeys
- Auto-launch
- Cue-window and intensity tuning beyond the current defaults
- Local observability and platform-failure reporting

## Verification Notes

Local validation completed:

- `pnpm build`
- `pnpm tauri build --debug`

Not fully validated in this environment:

- `cargo test`

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

1. Add global hotkeys for pause and reset
2. Expose cue-window and intensity tuning beyond the current defaults
3. Add local observability for transitions, user actions, and platform failures
4. Tune the transition parameters with manual visual testing on real hardware

