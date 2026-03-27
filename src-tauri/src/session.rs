use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    config::SessionConfig,
    engine::{calculate_snapshot, EffectSnapshot},
    platform::{apply_preview, CueStyle, ManagedDisplayEffectApplier},
};

const SESSION_EVENT: &str = "timer-session-updated";
const DEFAULT_WORK_DURATION_MINUTES: u32 = 50;
const DEFAULT_PAUSE_TIMEOUT_MINUTES: u32 = 10;
const MIN_SUPPORTED_WORK_DURATION_MINUTES: u32 = 2;
const MAX_WORK_DURATION_MINUTES: u32 = 120;
const LOOP_INTERVAL_MS: u64 = 250;
const EARLY_END_LOOP_INTERVAL_MS: u64 = 33;
const EFFECT_TRANSITION_LOOP_INTERVAL_MS: u64 = 33;
const EARLY_END_TRANSITION_MS: u64 = 2_800;
const EFFECT_TRANSITION_MS: u64 = 2_800;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatePayload {
    pub session_stage: String,
    pub session_status: String,
    pub work_duration_minutes: u32,
    pub remaining_seconds: u64,
    pub pause_remaining_seconds: u64,
    pub is_early_ending: bool,
    pub is_cue_transitioning: bool,
    pub snapshot: EffectSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionStage {
    Idle,
    Work,
    Break,
}

impl SessionStage {
    fn as_label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Work => "Work",
            Self::Break => "Break",
        }
    }
}

#[derive(Debug)]
struct SessionRuntimeState {
    stage: SessionStage,
    work_duration_minutes: u32,
    cue_style: CueStyle,
    auto_resume_enabled: bool,
    pause_timeout_minutes: u32,
    is_paused: bool,
    work_end_at_ms: Option<u64>,
    paused_remaining_ms: u64,
    pause_resume_at_ms: Option<u64>,
    early_end_started_at_ms: Option<u64>,
    early_end_finishes_at_ms: Option<u64>,
    early_end_from_remaining_ms: u64,
    effect_snapshot: EffectSnapshot,
    effect_target_snapshot: EffectSnapshot,
    effect_transition_started_at_ms: Option<u64>,
    effect_transition_finishes_at_ms: Option<u64>,
    effect_transition_from_snapshot: EffectSnapshot,
}

impl Default for SessionRuntimeState {
    fn default() -> Self {
        let neutral_snapshot = neutral_snapshot();
        Self {
            stage: SessionStage::Idle,
            work_duration_minutes: DEFAULT_WORK_DURATION_MINUTES,
            cue_style: CueStyle::Warm,
            auto_resume_enabled: true,
            pause_timeout_minutes: DEFAULT_PAUSE_TIMEOUT_MINUTES,
            is_paused: false,
            work_end_at_ms: None,
            paused_remaining_ms: 0,
            pause_resume_at_ms: None,
            early_end_started_at_ms: None,
            early_end_finishes_at_ms: None,
            early_end_from_remaining_ms: 0,
            effect_snapshot: neutral_snapshot.clone(),
            effect_target_snapshot: neutral_snapshot.clone(),
            effect_transition_started_at_ms: None,
            effect_transition_finishes_at_ms: None,
            effect_transition_from_snapshot: neutral_snapshot,
        }
    }
}

#[derive(Default)]
pub struct ManagedSessionController {
    state: Mutex<SessionRuntimeState>,
    loop_started: AtomicBool,
}

impl ManagedSessionController {
    pub fn start(&self, app_handle: AppHandle) {
        if self.loop_started.swap(true, Ordering::SeqCst) {
            return;
        }

        tauri::async_runtime::spawn(async move {
            let mut last_emitted: Option<SessionStatePayload> = None;
            let mut last_applied: Option<AppliedEffect> = None;

            loop {
                let next_interval_ms =
                    tick_session(&app_handle, &mut last_emitted, &mut last_applied);
                tokio::time::sleep(Duration::from_millis(next_interval_ms)).await;
            }
        });
    }
}

#[derive(Debug, Clone, PartialEq)]
struct AppliedEffect {
    cue_style: CueStyle,
    snapshot: EffectSnapshot,
}

fn tick_session(
    app_handle: &AppHandle,
    last_emitted: &mut Option<SessionStatePayload>,
    last_applied: &mut Option<AppliedEffect>,
) -> u64 {
    let controller = app_handle.state::<ManagedSessionController>();
    let now_ms = unix_time_ms();
    let (payload, cue_style) = {
        let mut state = controller.state.lock().expect("session state lock poisoned");
        advance_state(&mut state, now_ms);
        sync_effect_snapshot(&mut state, now_ms);
        let payload = build_payload(&state, now_ms);
        (payload, state.cue_style)
    };

    let next_effect = AppliedEffect {
        cue_style,
        snapshot: payload.snapshot.clone(),
    };
    let is_early_ending = payload.is_early_ending;
    let is_cue_transitioning = payload.is_cue_transitioning;

    if last_applied.as_ref() != Some(&next_effect) {
        // MagSetFullscreenColorEffect (Windows Magnification API) requires the
        // calling thread to run a Win32 message loop. Tokio worker threads do not
        // have one, so we must dispatch to the main thread.
        let snapshot = next_effect.snapshot.clone();
        let cue_style = next_effect.cue_style;
        let handle_for_apply = app_handle.clone();
        let _ = app_handle.run_on_main_thread(move || {
            let applier = handle_for_apply.state::<ManagedDisplayEffectApplier>();
            apply_preview(applier, &snapshot, cue_style);
        });
        *last_applied = Some(next_effect);
    }

    if last_emitted.as_ref() != Some(&payload) {
        let _ = app_handle.emit(SESSION_EVENT, payload.clone());
        *last_emitted = Some(payload);
    }

    if is_early_ending {
        EARLY_END_LOOP_INTERVAL_MS
    } else if is_cue_transitioning {
        EFFECT_TRANSITION_LOOP_INTERVAL_MS
    } else {
        LOOP_INTERVAL_MS
    }
}

fn mutate_session(
    app_handle: &AppHandle,
    controller: State<'_, ManagedSessionController>,
    mutate: impl FnOnce(&mut SessionRuntimeState, u64),
) -> SessionStatePayload {
    let now_ms = unix_time_ms();

    let payload = {
        let mut state = controller.state.lock().expect("session state lock poisoned");
        advance_state(&mut state, now_ms);
        mutate(&mut state, now_ms);
        advance_state(&mut state, now_ms);
        sync_effect_snapshot(&mut state, now_ms);
        build_payload(&state, now_ms)
    };

    let _ = app_handle.emit(SESSION_EVENT, payload.clone());
    payload
}

fn build_payload(state: &SessionRuntimeState, now_ms: u64) -> SessionStatePayload {
    let remaining_ms = current_remaining_ms(state, now_ms);
    let pause_remaining_seconds = if state.is_paused && state.auto_resume_enabled {
        state
            .pause_resume_at_ms
            .map(|resume_at_ms| ceil_seconds(remaining_ms_until(resume_at_ms, now_ms)))
            .unwrap_or(0)
    } else {
        0
    };

    SessionStatePayload {
        session_stage: state.stage.as_label().to_string(),
        session_status: current_status_label(state).to_string(),
        work_duration_minutes: state.work_duration_minutes,
        remaining_seconds: ceil_seconds(remaining_ms),
        pause_remaining_seconds,
        is_early_ending: state.early_end_finishes_at_ms.is_some(),
        is_cue_transitioning: state.effect_transition_finishes_at_ms.is_some(),
        snapshot: state.effect_snapshot.clone(),
    }
}

fn current_status_label(state: &SessionRuntimeState) -> &'static str {
    if state.is_paused {
        return "Paused";
    }

    if state.early_end_finishes_at_ms.is_some() {
        return "EndingEarly";
    }

    match state.stage {
        SessionStage::Work => "Running",
        SessionStage::Break => "Break",
        SessionStage::Idle => "Idle",
    }
}

fn advance_state(state: &mut SessionRuntimeState, now_ms: u64) {
    if state.stage != SessionStage::Work {
        return;
    }

    if state.is_paused {
        if state.auto_resume_enabled {
            if let Some(resume_at_ms) = state.pause_resume_at_ms {
                if now_ms >= resume_at_ms {
                    if state.paused_remaining_ms == 0 {
                        enter_break(state);
                    } else {
                        state.is_paused = false;
                        state.work_end_at_ms = Some(now_ms + state.paused_remaining_ms);
                        state.pause_resume_at_ms = None;
                    }
                }
            }
        }

        return;
    }

    if let Some(finish_at_ms) = state.early_end_finishes_at_ms {
        if now_ms >= finish_at_ms {
            enter_break(state);
            return;
        }
    }

    if current_remaining_ms(state, now_ms) == 0 {
        enter_break(state);
    }
}

fn enter_break(state: &mut SessionRuntimeState) {
    state.stage = SessionStage::Break;
    state.is_paused = false;
    state.work_end_at_ms = None;
    state.paused_remaining_ms = 0;
    state.pause_resume_at_ms = None;
    state.early_end_started_at_ms = None;
    state.early_end_finishes_at_ms = None;
    state.early_end_from_remaining_ms = 0;
}

fn current_remaining_ms(state: &SessionRuntimeState, now_ms: u64) -> u64 {
    if state.stage != SessionStage::Work {
        return 0;
    }

    if state.is_paused {
        return state.paused_remaining_ms;
    }

    if let Some(remaining_ms) = current_early_end_remaining_ms(state, now_ms) {
        return remaining_ms;
    }

    state
        .work_end_at_ms
        .map(|work_end_at_ms| remaining_ms_until(work_end_at_ms, now_ms))
        .unwrap_or(0)
}

fn current_early_end_remaining_ms(state: &SessionRuntimeState, now_ms: u64) -> Option<u64> {
    let started_at_ms = state.early_end_started_at_ms?;
    let finishes_at_ms = state.early_end_finishes_at_ms?;

    if now_ms >= finishes_at_ms {
        return Some(0);
    }

    let total_duration_ms = finishes_at_ms.saturating_sub(started_at_ms).max(1);
    let elapsed_ms = now_ms.saturating_sub(started_at_ms).min(total_duration_ms);
    let progress = elapsed_ms as f64 / total_duration_ms as f64;
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let remaining_ms = ((1.0 - eased) * state.early_end_from_remaining_ms as f64).round() as u64;

    Some(remaining_ms)
}

fn sync_effect_snapshot(state: &mut SessionRuntimeState, now_ms: u64) {
    let target_snapshot = target_effect_snapshot(state, now_ms);
    let current_snapshot = current_effect_snapshot(state, now_ms);

    if state.effect_target_snapshot != target_snapshot {
        if state.effect_transition_finishes_at_ms.is_some()
            && should_transition_effect(&state.effect_transition_from_snapshot, &target_snapshot)
        {
            state.effect_snapshot = current_snapshot;
            state.effect_target_snapshot = target_snapshot;
            return;
        }

        if should_transition_effect(&state.effect_target_snapshot, &target_snapshot) {
            begin_effect_transition(state, current_snapshot, target_snapshot, now_ms);
        } else {
            finish_effect_transition(state, target_snapshot);
        }

        return;
    }

    state.effect_snapshot = current_snapshot;
}

fn target_effect_snapshot(state: &SessionRuntimeState, now_ms: u64) -> EffectSnapshot {
    if state.stage == SessionStage::Idle || state.is_paused {
        return neutral_snapshot();
    }

    calculate_snapshot(
        &SessionConfig::default(),
        state.work_duration_minutes as f32,
        current_remaining_ms(state, now_ms) as f32 / 60_000.0,
    )
}

fn current_effect_snapshot(state: &mut SessionRuntimeState, now_ms: u64) -> EffectSnapshot {
    let Some(started_at_ms) = state.effect_transition_started_at_ms else {
        return state.effect_snapshot.clone();
    };
    let Some(finishes_at_ms) = state.effect_transition_finishes_at_ms else {
        return state.effect_snapshot.clone();
    };

    if now_ms >= finishes_at_ms {
        let target_snapshot = state.effect_target_snapshot.clone();
        finish_effect_transition(state, target_snapshot.clone());
        return target_snapshot;
    }

    let total_duration_ms = finishes_at_ms.saturating_sub(started_at_ms).max(1);
    let elapsed_ms = now_ms.saturating_sub(started_at_ms).min(total_duration_ms);
    let progress = elapsed_ms as f32 / total_duration_ms as f32;
    let eased = ease_in_out(progress);
    let snapshot = interpolate_snapshot(
        &state.effect_transition_from_snapshot,
        &state.effect_target_snapshot,
        eased,
    );
    state.effect_snapshot = snapshot.clone();
    snapshot
}

fn begin_effect_transition(
    state: &mut SessionRuntimeState,
    from_snapshot: EffectSnapshot,
    target_snapshot: EffectSnapshot,
    now_ms: u64,
) {
    if from_snapshot == target_snapshot {
        finish_effect_transition(state, target_snapshot);
        return;
    }

    state.effect_snapshot = from_snapshot.clone();
    state.effect_target_snapshot = target_snapshot;
    state.effect_transition_started_at_ms = Some(now_ms);
    state.effect_transition_finishes_at_ms = Some(now_ms + EFFECT_TRANSITION_MS);
    state.effect_transition_from_snapshot = from_snapshot;
}

fn finish_effect_transition(state: &mut SessionRuntimeState, snapshot: EffectSnapshot) {
    state.effect_snapshot = snapshot.clone();
    state.effect_target_snapshot = snapshot.clone();
    state.effect_transition_started_at_ms = None;
    state.effect_transition_finishes_at_ms = None;
    state.effect_transition_from_snapshot = snapshot;
}

fn should_transition_effect(from_snapshot: &EffectSnapshot, to_snapshot: &EffectSnapshot) -> bool {
    is_neutral_snapshot(from_snapshot) || is_neutral_snapshot(to_snapshot)
}

fn is_neutral_snapshot(snapshot: &EffectSnapshot) -> bool {
    snapshot.phase == crate::engine::Phase::Stable
        && (snapshot.saturation - 1.0).abs() <= 0.001
        && snapshot.warmth_kelvin >= 6500
}

fn ease_in_out(progress: f32) -> f32 {
    progress * progress * (3.0 - 2.0 * progress)
}

fn interpolate_snapshot(
    from_snapshot: &EffectSnapshot,
    to_snapshot: &EffectSnapshot,
    progress: f32,
) -> EffectSnapshot {
    EffectSnapshot {
        phase: if progress >= 1.0 {
            to_snapshot.phase
        } else {
            from_snapshot.phase
        },
        saturation: from_snapshot.saturation + (to_snapshot.saturation - from_snapshot.saturation) * progress,
        warmth_kelvin: (from_snapshot.warmth_kelvin as f32
            + (to_snapshot.warmth_kelvin as f32 - from_snapshot.warmth_kelvin as f32) * progress)
            .round() as u32,
    }
}

fn remaining_ms_until(target_ms: u64, now_ms: u64) -> u64 {
    target_ms.saturating_sub(now_ms)
}

fn ceil_seconds(duration_ms: u64) -> u64 {
    if duration_ms == 0 {
        0
    } else {
        duration_ms.div_ceil(1_000)
    }
}

fn normalize_work_duration_minutes(value: u32) -> u32 {
    value.clamp(0, MAX_WORK_DURATION_MINUTES)
}

fn normalize_pause_timeout_minutes(value: u32) -> u32 {
    value.clamp(1, 120)
}

fn clamp_remaining_seconds_for_duration(remaining_seconds: u64, work_duration_minutes: u32) -> u64 {
    remaining_seconds.min(u64::from(work_duration_minutes) * 60)
}

fn unix_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock went backwards")
        .as_millis() as u64
}

fn neutral_snapshot() -> EffectSnapshot {
    EffectSnapshot {
        phase: crate::engine::Phase::Stable,
        saturation: 1.0,
        warmth_kelvin: 6500,
    }
}

#[tauri::command]
pub fn get_timer_session_state(controller: State<'_, ManagedSessionController>) -> SessionStatePayload {
    let now_ms = unix_time_ms();
    let mut state = controller.state.lock().expect("session state lock poisoned");
    advance_state(&mut state, now_ms);
    sync_effect_snapshot(&mut state, now_ms);
    build_payload(&state, now_ms)
}

#[tauri::command]
pub fn start_timer_session(
    work_duration_minutes: u32,
    cue_style: String,
    auto_resume_enabled: bool,
    pause_timeout_minutes: u32,
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_session(&app_handle, controller, |state, now_ms| {
        let work_duration_minutes = normalize_work_duration_minutes(work_duration_minutes);
        state.work_duration_minutes = work_duration_minutes;
        state.cue_style = CueStyle::from_id(&cue_style);
        state.auto_resume_enabled = auto_resume_enabled;
        state.pause_timeout_minutes = normalize_pause_timeout_minutes(pause_timeout_minutes);

        if work_duration_minutes < MIN_SUPPORTED_WORK_DURATION_MINUTES {
            state.stage = SessionStage::Idle;
            state.is_paused = false;
            state.work_end_at_ms = None;
            state.paused_remaining_ms = 0;
            state.pause_resume_at_ms = None;
            state.early_end_started_at_ms = None;
            state.early_end_finishes_at_ms = None;
            state.early_end_from_remaining_ms = 0;
            return;
        }

        state.stage = SessionStage::Work;
        state.is_paused = false;
        state.work_end_at_ms = Some(now_ms + u64::from(work_duration_minutes) * 60_000);
        state.paused_remaining_ms = 0;
        state.pause_resume_at_ms = None;
        state.early_end_started_at_ms = None;
        state.early_end_finishes_at_ms = None;
        state.early_end_from_remaining_ms = 0;
    })
}

#[tauri::command]
pub fn toggle_pause_timer_session(
    auto_resume_enabled: bool,
    pause_timeout_minutes: u32,
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_session(&app_handle, controller, |state, now_ms| {
        state.auto_resume_enabled = auto_resume_enabled;
        state.pause_timeout_minutes = normalize_pause_timeout_minutes(pause_timeout_minutes);

        if state.early_end_finishes_at_ms.is_some() {
            return;
        }

        if state.stage != SessionStage::Work && !state.is_paused {
            return;
        }

        if state.is_paused {
            state.is_paused = false;
            state.work_end_at_ms = Some(now_ms + state.paused_remaining_ms);
            state.pause_resume_at_ms = None;
            return;
        }

        let remaining_ms = current_remaining_ms(state, now_ms);
        if remaining_ms == 0 {
            enter_break(state);
            return;
        }

        state.is_paused = true;
        state.work_end_at_ms = None;
        state.paused_remaining_ms = remaining_ms;
        state.pause_resume_at_ms = if state.auto_resume_enabled {
            Some(now_ms + u64::from(state.pause_timeout_minutes) * 60_000)
        } else {
            None
        };
    })
}

#[tauri::command]
pub fn reset_timer_session(
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_session(&app_handle, controller, |state, _| {
        state.stage = SessionStage::Idle;
        state.is_paused = false;
        state.work_end_at_ms = None;
        state.paused_remaining_ms = 0;
        state.pause_resume_at_ms = None;
        state.early_end_started_at_ms = None;
        state.early_end_finishes_at_ms = None;
        state.early_end_from_remaining_ms = 0;
    })
}

#[tauri::command]
pub fn end_timer_session_early(
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_session(&app_handle, controller, |state, now_ms| {
        if state.stage == SessionStage::Work {
            let remaining_ms = current_remaining_ms(state, now_ms);

            if remaining_ms == 0 {
                enter_break(state);
                return;
            }

            state.is_paused = false;
            state.work_end_at_ms = None;
            state.paused_remaining_ms = remaining_ms;
            state.pause_resume_at_ms = None;
            state.early_end_started_at_ms = Some(now_ms);
            state.early_end_finishes_at_ms = Some(now_ms + EARLY_END_TRANSITION_MS);
            state.early_end_from_remaining_ms = remaining_ms;
        }
    })
}

#[tauri::command]
pub fn update_timer_session_settings(
    cue_style: String,
    auto_resume_enabled: bool,
    pause_timeout_minutes: u32,
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_session(&app_handle, controller, |state, now_ms| {
        state.cue_style = CueStyle::from_id(&cue_style);
        state.auto_resume_enabled = auto_resume_enabled;
        state.pause_timeout_minutes = normalize_pause_timeout_minutes(pause_timeout_minutes);

        if state.is_paused {
            state.pause_resume_at_ms = if state.auto_resume_enabled {
                Some(now_ms + u64::from(state.pause_timeout_minutes) * 60_000)
            } else {
                None
            };
        }
    })
}

#[tauri::command]
pub fn set_timer_session_remaining_seconds(
    remaining_seconds: u64,
    app_handle: AppHandle,
    controller: State<'_, ManagedSessionController>,
) -> SessionStatePayload {
    mutate_session(&app_handle, controller, |state, now_ms| {
        if state.stage != SessionStage::Work || state.is_paused || state.early_end_finishes_at_ms.is_some() {
            return;
        }

        let remaining_seconds =
            clamp_remaining_seconds_for_duration(remaining_seconds, state.work_duration_minutes);

        if remaining_seconds == 0 {
            enter_break(state);
            return;
        }

        state.work_end_at_ms = Some(now_ms + remaining_seconds * 1_000);
        state.paused_remaining_ms = 0;
        state.pause_resume_at_ms = None;
        state.early_end_started_at_ms = None;
        state.early_end_finishes_at_ms = None;
        state.early_end_from_remaining_ms = 0;
    })
}
