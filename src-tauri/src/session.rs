use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex,
    },
    time::Duration,
};

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    channel::{
        Channel, ChannelConfig, ChannelType, ChannelValue, TickContext,
    },
    compositor::{compose, CompositeFrame},
    observability,
    phase::SessionPhase,
    platform::{apply_frame, ManagedPlatformAdapter},
};

const SESSION_EVENT: &str = "session-updated";
const DEFAULT_WORK_DURATION_MINUTES: u32 = 50;
const DEFAULT_REVERSE_MAX_DURATION_MS: u64 = 30_000;
const MAX_SETTLING_MS: u64 = 15_000;
const MIN_SUPPORTED_WORK_DURATION_MINUTES: u32 = 2;
const MAX_WORK_DURATION_MINUTES: u32 = 120;
const LOOP_INTERVAL_MS: u64 = 250;
const FAST_LOOP_INTERVAL_MS: u64 = 33;
const ALL_CHANNELS_THRESHOLD: f32 = 0.99;
const ZERO_THRESHOLD: f32 = 0.01;

// ---------------------------------------------------------------------------
// Payload types sent to the frontend
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatePayload {
    pub phase: SessionPhase,
    pub work_duration_minutes: u32,
    pub elapsed_seconds: u64,
    pub target_duration_seconds: u64,
    pub channels: Vec<ChannelStatePayload>,
    pub frame: CompositeFrame,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatePayload {
    pub channel_type: ChannelType,
    pub current_intensity: f32,
}

// ---------------------------------------------------------------------------
// Engine runtime state
// ---------------------------------------------------------------------------

struct SessionEngine {
    phase: SessionPhase,
    channels: Vec<Box<dyn Channel>>,
    channel_configs: Vec<ChannelConfig>,
    work_duration_minutes: u32,
    reverse_max_duration_ms: u64,
    phase_started_at_ms: u64,
}

impl Default for SessionEngine {
    fn default() -> Self {
        Self {
            phase: SessionPhase::Idle,
            channels: Vec::new(),
            channel_configs: Vec::new(),
            work_duration_minutes: DEFAULT_WORK_DURATION_MINUTES,
            reverse_max_duration_ms: DEFAULT_REVERSE_MAX_DURATION_MS,
            phase_started_at_ms: 0,
        }
    }
}

impl SessionEngine {
    // TODO: This should not be such a complex function.
    // Maybe many logic should be in Channel.
    fn advance_phase(&mut self, now_ms: u64) {
        let elapsed_ms = now_ms.saturating_sub(self.phase_started_at_ms);

        match &self.phase {
            SessionPhase::Forward {
                target_duration_ms, ..
            } => {
                let target = *target_duration_ms;
                if elapsed_ms >= target {
                    self.enter_sabi(now_ms);
                } else {
                    self.phase = SessionPhase::Forward {
                        elapsed_ms,
                        target_duration_ms: target,
                    };
                }
            }
            SessionPhase::Settling { .. } => {
                let incomplete: Vec<_> = self
                    .channels
                    .iter()
                    .enumerate()
                    .filter(|(_, ch)| ch.current_intensity() < ALL_CHANNELS_THRESHOLD)
                    .collect();
                if elapsed_ms >= MAX_SETTLING_MS {
                    if incomplete.is_empty() {
                        self.enter_sabi(now_ms);
                    } else {
                        panic!(
                            "Elapsed time >= the max settling time but the following channels did not reach max intensity (threshold={}): {:?}",
                            ALL_CHANNELS_THRESHOLD,
                            incomplete
                                .into_iter()
                                .map(|(idx, ch)| (idx, ch.channel_type(), ch.current_intensity()))
                                .collect::<Vec<_>>()
                        )
                    }
                } else {
                    self.phase = SessionPhase::Settling { elapsed_ms };
                }
            }
            SessionPhase::Reverse {
                max_duration_ms, ..
            } => {
                let max = *max_duration_ms;
                // TODO: 一个channel有没有完成应当由自己判断，而不应当让引擎来判断
                // 因为intensity除了用于判断是否完成只用于Payload，故它是否有用存疑
                let incomplete: Vec<_> = self
                    .channels
                    .iter()
                    .enumerate()
                    .filter(|(_, ch)| ch.current_intensity() > ZERO_THRESHOLD)
                    .collect();

                if incomplete.is_empty() {
                    self.enter_idle(now_ms);
                } else if elapsed_ms >= max {
                    panic!(
                        "Elapsed time >= the max reverse time but the following channels did not reach zero intensity (threshold={}): {:?}",
                        ZERO_THRESHOLD,
                        incomplete
                            .into_iter()
                            .map(|(idx, ch)| (idx, ch.channel_type(), ch.current_intensity()))
                            .collect::<Vec<_>>()
                    );
                } else {
                    self.phase = SessionPhase::Reverse {
                        elapsed_ms,
                        max_duration_ms: max,
                    };
                }
            }
            SessionPhase::Idle | SessionPhase::Sabi => {}
        }
    }

    // TODO: Compositor的逻辑散步在tick和tick_session里面
    fn tick(&mut self, now_ms: u64) -> CompositeFrame {
        let elapsed_ms = now_ms.saturating_sub(self.phase_started_at_ms);
        self.advance_phase(now_ms);

        let dt_ms = LOOP_INTERVAL_MS.min(elapsed_ms);
        let ctx = TickContext {
            phase: &self.phase,
            dt_ms,
        };

        let mut values = Vec::with_capacity(self.channels.len());
        for channel in &mut self.channels {
            channel.tick(&ctx);
            values.push(channel.current_value());
        }

        self.channels
            .retain(|ch| ch.current_intensity() > 0.0 || !matches!(self.phase, SessionPhase::Idle));

        // TODO: 通道分开是对的，输出就应当是语义化的；但是不同sensory
        // （全屏、暗角、声音）应当接收后将其整合成标准化格式。
        // e.g. 全屏整合成标准5*5颜色矩阵，声音整合成电平
        // TODO: 先调整饱和度，再调整色温，不然色温直接灰了
        compose(&values)
    }

    fn build_payload(&self) -> SessionStatePayload {
        let (elapsed_seconds, target_duration_seconds) = match &self.phase {
            SessionPhase::Forward {
                elapsed_ms,
                target_duration_ms,
            } => (elapsed_ms / 1000, target_duration_ms / 1000),
            SessionPhase::Settling { elapsed_ms } => (*elapsed_ms / 1000, 0),
            SessionPhase::Reverse {
                elapsed_ms,
                max_duration_ms,
            } => (elapsed_ms / 1000, max_duration_ms / 1000),
            SessionPhase::Sabi | SessionPhase::Idle => (0, 0),
        };

        let channel_payloads: Vec<ChannelStatePayload> = self
            .channels
            .iter()
            .map(|ch| ChannelStatePayload {
                channel_type: ch.channel_type(),
                current_intensity: ch.current_intensity(),
            })
            .collect();

        let values: Vec<ChannelValue> = self.channels.iter().map(|ch| ch.current_value()).collect();
        let frame = compose(&values);

        SessionStatePayload {
            phase: self.phase.clone(),
            work_duration_minutes: self.work_duration_minutes,
            elapsed_seconds,
            target_duration_seconds,
            channels: channel_payloads,
            frame,
        }
    }

    fn enter_forward(&mut self, work_duration_minutes: u32, now_ms: u64) {
        self.work_duration_minutes = work_duration_minutes;
        let target_duration_ms = u64::from(work_duration_minutes) * 60_000;
        self.phase = SessionPhase::Forward {
            elapsed_ms: 0,
            target_duration_ms,
        };
        self.phase_started_at_ms = now_ms;
    }

    fn enter_settling(&mut self, now_ms: u64) {
        snapshot_all_channels_for_phase_entry(&mut self.channels);
        self.phase = SessionPhase::Settling { elapsed_ms: 0 };
        self.phase_started_at_ms = now_ms;
    }

    fn enter_sabi(&mut self, now_ms: u64) {
        self.phase = SessionPhase::Sabi;
        self.phase_started_at_ms = now_ms;
    }

    fn enter_reverse(&mut self, now_ms: u64) {
        snapshot_all_channels_for_phase_entry(&mut self.channels);
        self.phase = SessionPhase::Reverse {
            elapsed_ms: 0,
            max_duration_ms: self.reverse_max_duration_ms,
        };
        self.phase_started_at_ms = now_ms;
    }

    fn enter_idle(&mut self, _now_ms: u64) {
        self.phase = SessionPhase::Idle;
        self.channels.clear();
    }

    fn rebuild_channels(&mut self) {
        self.channels = self
            .channel_configs
            .iter()
            .map(|cfg| cfg.create_channel())
            .collect();
    }

    fn needs_fast_tick(&self) -> bool {
        matches!(
            self.phase,
            SessionPhase::Settling { .. } | SessionPhase::Reverse { .. }
        )
    }
}

fn snapshot_all_channels_for_phase_entry(channels: &mut [Box<dyn Channel>]) {
    for ch in channels.iter_mut() {
        ch.snapshot_intensity_for_phase_entry();
    }
}

// ---------------------------------------------------------------------------
// Managed controller (thread-safe wrapper)
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct ManagedSessionController {
    engine: Mutex<SessionEngine>,
    loop_started: AtomicBool,
}

impl ManagedSessionController {
    pub fn start(&self, app_handle: AppHandle) {
        if self.loop_started.swap(true, Ordering::SeqCst) {
            return;
        }

        tauri::async_runtime::spawn(async move {
            let mut last_emitted: Option<SessionStatePayload> = None;
            let mut last_frame: Option<CompositeFrame> = None;

            loop {
                let next_interval =
                    tick_session(&app_handle, &mut last_emitted, &mut last_frame);
                tokio::time::sleep(Duration::from_millis(next_interval)).await;
            }
        });
    }
}

fn tick_session(
    app_handle: &AppHandle,
    last_emitted: &mut Option<SessionStatePayload>,
    last_frame: &mut Option<CompositeFrame>,
) -> u64 {
    let controller = app_handle.state::<ManagedSessionController>();
    let now_ms = unix_time_ms();

    let (payload, frame, needs_fast_tick) = {
        let mut engine = controller
            .engine
            .lock()
            .expect("session engine lock poisoned");
        let frame = engine.tick(now_ms);
        let payload = engine.build_payload();
        let fast = engine.needs_fast_tick();
        (payload, frame, fast)
    };

    if last_frame.as_ref() != Some(&frame) {
        let apply_result = apply_effect_on_main_thread(app_handle, &frame);
        if apply_result.error.is_some() || apply_result.recovery_attempted {
            log_platform_apply_result(app_handle, "session_tick", &frame, &apply_result);
        }
        if apply_result.applied {
            *last_frame = Some(frame);
        }
    }

    if last_emitted.as_ref() != Some(&payload) {
        log_phase_transition(app_handle, last_emitted.as_ref(), &payload);
        update_system_ui(app_handle, &payload);
        let _ = app_handle.emit(SESSION_EVENT, payload.clone());
        *last_emitted = Some(payload);
    }

    if needs_fast_tick {
        FAST_LOOP_INTERVAL_MS
    } else {
        LOOP_INTERVAL_MS
    }
}

fn apply_effect_on_main_thread(
    app_handle: &AppHandle,
    frame: &CompositeFrame,
) -> crate::platform::ApplyResult {
    let (sender, receiver) = mpsc::channel();
    let frame = frame.clone();
    let handle_for_apply = app_handle.clone();

    if let Err(error) = app_handle.run_on_main_thread(move || {
        let adapter = handle_for_apply.state::<ManagedPlatformAdapter>();
        let result = apply_frame(adapter, &frame);
        let _ = sender.send(result);
    }) {
        return crate::platform::ApplyResult {
            applied: false,
            backend: "run-on-main-thread",
            error: Some(error.to_string()),
            recovery_attempted: false,
            recovery_succeeded: false,
            recovery_error: None,
        };
    }

    receiver
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or(crate::platform::ApplyResult {
            applied: false,
            backend: "run-on-main-thread",
            error: Some("timed out waiting for platform apply".to_string()),
            recovery_attempted: false,
            recovery_succeeded: false,
            recovery_error: None,
        })
}

// ---------------------------------------------------------------------------
// System UI helpers
// ---------------------------------------------------------------------------

fn update_system_ui(app_handle: &AppHandle, payload: &SessionStatePayload) {
    let progress = match &payload.phase {
        SessionPhase::Forward {
            elapsed_ms,
            target_duration_ms,
        } => {
            let target = (*target_duration_ms).max(1);
            ((elapsed_ms * 100) / target).min(100)
        }
        _ => 0,
    };

    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.set_progress_bar(tauri::window::ProgressBarState {
            progress: Some(progress),
            status: None,
        });
    }

    let tray_title = match &payload.phase {
        SessionPhase::Forward {
            elapsed_ms,
            target_duration_ms,
        } => {
            let remaining_s = target_duration_ms.saturating_sub(*elapsed_ms) / 1000;
            format!("{}:{:02}", remaining_s / 60, remaining_s % 60)
        }
        SessionPhase::Settling { .. } => "Settling...".to_string(),
        SessionPhase::Sabi => "Sabi".to_string(),
        SessionPhase::Reverse { .. } => "Recovering".to_string(),
        SessionPhase::Idle => "Idle".to_string(),
    };
    crate::tray::update_tray_title(app_handle, &tray_title);

    let _ = crate::tray::update_tray_menu(app_handle, payload.phase.label());
}

// ---------------------------------------------------------------------------
// Observability
// ---------------------------------------------------------------------------

fn log_phase_transition(
    app_handle: &AppHandle,
    previous: Option<&SessionStatePayload>,
    next: &SessionStatePayload,
) {
    let changed = previous
        .map(|last| last.phase.label() != next.phase.label())
        .unwrap_or(true);

    if !changed {
        return;
    }

    observability::log_event(
        app_handle,
        "phase_transition",
        json!({
            "from": previous.map(|p| p.phase.label()),
            "to": next.phase.label(),
            "elapsedSeconds": next.elapsed_seconds,
        }),
    );
}

fn log_platform_apply_result(
    app_handle: &AppHandle,
    source: &'static str,
    frame: &CompositeFrame,
    result: &crate::platform::ApplyResult,
) {
    observability::log_event(
        app_handle,
        "platform_apply",
        json!({
            "source": source,
            "backend": result.backend,
            "frame": frame,
            "applied": result.applied,
            "error": result.error,
            "recoveryAttempted": result.recovery_attempted,
            "recoverySucceeded": result.recovery_succeeded,
            "recoveryError": result.recovery_error,
        }),
    );
}

// ---------------------------------------------------------------------------
// Mutation helper
// ---------------------------------------------------------------------------

fn mutate_engine(
    app_handle: &AppHandle,
    controller: State<'_, ManagedSessionController>,
    action: &'static str,
    action_payload: serde_json::Value,
    mutate: impl FnOnce(&mut SessionEngine, u64),
) -> SessionStatePayload {
    let now_ms = unix_time_ms();

    let payload = {
        let mut engine = controller
            .engine
            .lock()
            .expect("session engine lock poisoned");
        mutate(&mut engine, now_ms);
        engine.tick(now_ms);
        engine.build_payload()
    };

    observability::log_event(
        app_handle,
        "user_action",
        json!({
            "action": action,
            "input": action_payload,
            "result": {
                "phase": payload.phase.label(),
                "elapsedSeconds": payload.elapsed_seconds,
            }
        }),
    );

    update_system_ui(app_handle, &payload);
    let _ = app_handle.emit(SESSION_EVENT, payload.clone());
    payload
}

// ---------------------------------------------------------------------------
// Tauri commands (IPC)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_session_state(
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    let now_ms = unix_time_ms();
    let mut engine = controller
        .engine
        .lock()
        .expect("session engine lock poisoned");
    engine.advance_phase(now_ms);
    engine.build_payload()
}

#[tauri::command]
pub fn start_session(
    work_duration_minutes: u32,
    channels: Vec<ChannelConfig>,
    reverse_max_duration_ms: Option<u64>,
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_engine(
        &app_handle,
        controller,
        "start_session",
        json!({
            "workDurationMinutes": work_duration_minutes,
            "channelCount": channels.len(),
        }),
        |engine, now_ms| {
            let clamped_duration =
                work_duration_minutes.clamp(MIN_SUPPORTED_WORK_DURATION_MINUTES, MAX_WORK_DURATION_MINUTES);

            if work_duration_minutes < MIN_SUPPORTED_WORK_DURATION_MINUTES {
                engine.enter_idle(now_ms);
                return;
            }

            engine.channel_configs = channels;
            engine.reverse_max_duration_ms =
                reverse_max_duration_ms.unwrap_or(DEFAULT_REVERSE_MAX_DURATION_MS);
            engine.rebuild_channels();
            engine.enter_forward(clamped_duration, now_ms);
        },
    )
}

#[tauri::command]
pub fn take_break_now(
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_engine(
        &app_handle,
        controller,
        "take_break_now",
        json!({}),
        |engine, now_ms| {
            if matches!(engine.phase, SessionPhase::Forward { .. }) {
                engine.enter_settling(now_ms);
            }
        },
    )
}

#[tauri::command]
pub fn start_reverse(
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_engine(
        &app_handle,
        controller,
        "start_reverse",
        json!({}),
        |engine, now_ms| {
            if matches!(
                engine.phase,
                SessionPhase::Forward { .. } | SessionPhase::Settling { .. } | SessionPhase::Sabi { .. }
            ) {
                engine.enter_reverse(now_ms);
            }
        },
    )
}

#[tauri::command]
pub fn update_channels(
    channels: Vec<ChannelConfig>,
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_engine(
        &app_handle,
        controller,
        "update_channels",
        json!({ "channelCount": channels.len() }),
        |engine, _now_ms| {
            engine.channel_configs = channels;
            engine.rebuild_channels();
        },
    )
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock went backwards")
        .as_millis() as u64
}
