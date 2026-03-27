# Erode App Spec And Todo

## 1. Product Definition

### 1.1 Working Name

- Public name: `Erode App`
- Internal concept label: `Erosion Mode`

### 1.2 Problem Statement

Most break reminder tools rely on popups, alarms, vibration, or aggressive overlays. They may force attention, but they also break deep focus and can feel harsh for sensitive users.

Erode App reframes a reminder from a discrete event into a progressive state. Instead of demanding attention, it gradually changes the visual environment of the screen so the user notices through peripheral vision that time is running out and a rest boundary is approaching.

### 1.3 Design Philosophy

- Shift from `interruption-based` reminders to `ambient awareness`
- Replace "stop now" commands with environmental change
- Use gradual visual degradation instead of sudden sensory spikes
- Treat `gentleness` as a first-class interaction rule, not just a visual style
- Make every state change feel like a fade or drift, never a snap or pop

### 1.4 Goals

- Increase break reminder awareness without startling the user
- Reduce compulsive attachment to on-screen content near the end of a work session
- Offer a gentler reminder model for users who dislike noisy notifications
- Provide a cross-platform desktop architecture with Windows and macOS as priority targets

### 1.5 Non-Goals

- No AI-driven rhythm optimization in the MVP
- No team collaboration, account system, or cloud sync in the MVP
- No browser-extension-based implementation for the core experience
- No Electron-based desktop shell

### 1.6 Target Users

- Knowledge workers who spend long sessions coding, writing, designing, or researching
- Users who are sensitive to abrupt reminders
- People who already use focus timers but dislike hard interruptions

### 1.7 Core Scenarios

- A user runs a work session from `2` to `90` minutes and needs a gentle break cue
- A user is presenting and needs an immediate pause or reset hotkey
- A user wants a subtle "time to stop soon" signal instead of a modal notification

### 1.8 Product Principles

- `Zero startle`: no sounds, flashing, or sudden high-contrast transitions
- `Slow change`: use easing curves instead of linear shifts
- `Reversible`: any reminder state must recover smoothly
- `Controllable`: the user must be able to pause, end the work phase early into rest, or reset
- `Low presence`: the product should live mostly in the tray/menu bar, not in the workflow
- `Gentle everywhere`: dialogs, overlays, control responses, and resets must enter and leave softly; no abrupt UI jumps are acceptable

### 1.9 User Stories

- As a person in deep focus, I want most of my work session to remain completely undisturbed, with cueing compressed toward the end.
- As a user who hates popups, I want reminders to arrive through environmental change rather than direct interruption.
- As a sensitive user, I want the display to recover gradually when I choose to stop, instead of snapping back instantly.
- As a frequent presenter, I want a hotkey that can immediately pause or reset the effect.

## 2. Experience Model

### 2.1 Four-Phase Model

| Phase | Time Window | Visual Behavior | Experience Goal |
| --- | --- | --- | --- |
| Stable | Dynamic remainder before cue windows | No visible change | Preserve deep focus |
| JND Phase | Dynamic prewarm window, defaulting to the last `10%` of the session and capped once session duration exceeds `50` minutes | Saturation decreases at a very low slope | Reach the just-noticeable threshold without starting too early |
| Evolution | Dynamic final window, defaulting to the last `10%` of the session and capped once session duration exceeds `50` minutes | Warm shift becomes obvious and saturation drops faster | Communicate a clear "day is ending" cue |
| Statue | Work session has ended | Strong warm tone with near-grayscale output | Maximize friction for continued work |

User-facing language should avoid hard internal terms. For example, `JND` should be shown as `Soft shift`, and `Statue` should be shown with softer labels such as `Rest` or `Settled`. These remain internal phase names.

### 2.2 State Machine

`Idle -> Work(Stable/JND/Evolution) -> Break(Statue) -> Recovery -> Idle`

Additional states:

- `Paused`: the user disables all visual effects temporarily
- `EmergencyReset`: the effect is terminated immediately and the session is frozen

### 2.3 Mathematical Model

#### Grayscale Blend

`C_out = (1 - alpha) * C_in + alpha * Gray(C_in)`

#### Easing Function

`f(x) = 1 / (1 + e^(-k * (x - 0.5)))`

Use a sigmoid / S-curve instead of a linear function so the early change remains almost imperceptible and the mid-to-late change accelerates gently.

#### Session-Length Scaling

The cue windows must be derived from the current session length instead of fixed minute values.

- Minimum supported work session: `2 minutes`
- Sessions shorter than `2 minutes` are out of scope and should be treated as unsupported when entered
- Default session length remains `50 minutes`
- `prewarm_duration = min(session_duration * 0.10, 5 minutes)`
- `evolution_duration = min(session_duration * 0.10, 5 minutes)`
- `stable_duration = max(0, session_duration - prewarm_duration - evolution_duration)`

This means:

- Sessions at or below `50 minutes` scale cue timing proportionally
- Sessions above `50 minutes` keep the same maximum cue windows instead of letting prewarm and evolution keep growing
- A `25 minute` session should not inherit a `15 + 5` minute cue model
- A `2 minute` session is valid and should still pass through `Stable -> JND -> Evolution -> Statue`, albeit in a compressed form

### 2.4 Default Parameters

| Parameter | Default |
| --- | --- |
| work_duration_minutes | 50 |
| min_supported_work_duration_minutes | 2 |
| prewarm_ratio_of_session | 0.10 |
| evolution_ratio_of_session | 0.10 |
| prewarm_cap_minutes | 5 |
| evolution_cap_minutes | 5 |
| pause_timeout_minutes | 10 |
| target_warmth_kelvin | 2500 |
| recovery_duration_seconds | 30 |

## 3. Build Status Summary

This section reflects the current repository state, not just the original intent.

### 3.1 Done

- [x] Set up the desktop app foundation with `Tauri + Rust + Vue`
- [x] Implement the core visual phase model: `Stable`, `JND`, `Evolution`, `Statue`
- [x] Implement sigmoid-based snapshot calculation in Rust
- [x] Mirror the snapshot logic in the frontend preview path
- [x] Add unit tests for the core phase transitions in Rust
- [x] Build a frontend session flow with `Idle`, `Work`, `Paused`, and `Break`
- [x] Support `Start session`
- [x] Support `Pause` and `Resume`
- [x] Support `End early`
- [x] Support `Reset`
- [x] Keep the work-session clock advancing while paused
- [x] Support pause timeout with automatic resume
- [x] Add smooth visual ramping when cue visibility is suppressed and restored
- [x] Add a settings surface for work duration, cue style, and pause timeout
- [x] Persist basic session preferences in local storage
- [x] Add a preview slider for inspecting reminder intensity by remaining time
- [x] Add a mock display-effect adapter in the Rust platform layer
- [x] Verify `pnpm build`
- [x] Verify `cargo test`

### 3.2 Partially Done

- [~] Recovery behavior is only partially represented
  - `Break / Statue` exists
  - A true `Recovery -> Idle` flow is not implemented
- [~] Visual effects exist in the app preview
  - Saturation, warmth, and grayscale are simulated in the UI
  - System-wide display control is not implemented yet
- [~] Settings exist, but only for part of the planned control surface
  - Work duration and pause timeout are configurable
  - Dynamic prewarm / evolution scaling exists in the engine, but it is not yet exposed as a user-tunable control
  - Hotkeys, loops, auto-launch, and deeper intensity tuning are not exposed yet
- [~] The product currently behaves like a focused preview app
  - The low-presence tray/menu bar product shape is still missing

### 3.3 Not Done Yet

- [ ] Implement a native Windows backend using the `Magnification API`
- [ ] Implement a native macOS backend using `CGDisplaySetDisplayTransferFunction` or an acceptable fallback
- [ ] Replace the mock platform adapter with real OS-specific display effect backends
- [ ] Apply visual effects system-wide instead of only inside the app UI
- [ ] Add tray / menu bar status presence
- [ ] Add global hotkeys for pause, reset, and emergency actions
- [ ] Add auto-launch support
- [ ] Expose cue-window and intensity tuning beyond the current defaults
- [ ] Add configurable loop behavior
- [ ] Add a real `Recovery` state with smooth transition back to neutral
- [ ] Add an explicit user action or leave-desk signal to start recovery
- [ ] Add local observability logs for transitions, user actions, config snapshots, and platform failures
- [ ] Add platform-failure handling and recovery-failure reporting
- [ ] Validate timer drift and transition smoothness under real desktop runtime conditions

## 4. Todo List

### 4.1 MVP Core

- [x] Define the product direction around ambient break reminders
- [x] Define the four-phase reminder model
- [x] Implement phase evaluation and snapshot generation
- [x] Implement a basic session timer
- [x] Implement pause, end-early, and reset controls
- [x] Scale prewarm and evolution windows by session length with caps
- [ ] Implement the real native display-effect pipeline
- [ ] Move from preview-only visuals to actual system display control

### 4.2 Control Surface

- [x] Build a basic settings UI
- [x] Show current session state and remaining time
- [x] Expose work duration control
- [x] Expose pause timeout control
- [x] Expose cue-style presets
- [ ] Expose cue-window tuning controls
- [ ] Add tray / menu bar controls
- [ ] Add hotkey configuration
- [ ] Add auto-launch configuration

### 4.3 Session And State

- [x] Support `Idle`
- [x] Support `Work`
- [x] Support `Paused`
- [x] Support `Break`
- [ ] Support `Recovery`
- [ ] Support `EmergencyReset` as a real distinct state
- [ ] Support session loops

### 4.4 Visual Behavior

- [x] Simulate saturation reduction
- [x] Simulate warmth shift
- [x] Simulate grayscale blending
- [x] Use smooth cue blending when pausing and resuming
- [ ] Match the same behavior through native OS display APIs
- [ ] Tune final effect values on real hardware

### 4.5 Reliability And Observability

- [x] Add unit coverage for core phase calculation
- [x] Confirm the frontend production build succeeds
- [ ] Log state transitions locally
- [ ] Log manual user actions locally
- [ ] Log current config snapshots locally
- [ ] Log platform application failures locally
- [ ] Validate acceptance criteria on Windows
- [ ] Validate acceptance criteria on macOS

## 5. Acceptance Checklist

### 5.1 Product

- [ ] During roughly the first 80% of a `<= 50 minute` work session, the display appears unchanged
- [ ] During the final dynamic cue window, the user can perceive change without being startled
- [ ] At the break boundary, the display becomes visibly less attractive for continued work
- [x] Sessions from `2 minutes` upward remain valid, and sessions shorter than `2 minutes` are treated as unsupported when entered
- [x] Sessions longer than `50 minutes` do not keep extending prewarm and evolution indefinitely
- [x] `End early` produces the same `Statue` state immediately rather than resetting the session
- [x] If the user leaves the app in `Paused`, the display stays neutral only until the pause timeout elapses, then ambient cueing resumes automatically from the correct current phase

### 5.2 Technical

- [ ] Timer drift remains within `1 second` in real runtime conditions
- [ ] Phase transitions do not flicker in the native implementation
- [ ] Windows native backend works reliably
- [ ] macOS native backend works reliably
