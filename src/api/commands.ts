import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  ChannelType,
  ChannelValue,
  CurveParameters,
  StoredConfig,
  ProgressPayload,
  ShortcutBindings,
  EngineCommand,
  CrashPayload,
} from "./types";

// TODO: There should be fallback paths for promise's error.

// ── Wrapper: Timer ────────────────────────────────────────────────────────────

export const startSession = (targetDurationMs: number) =>
  commandEngine({ category: "state", content: { command: "start_session", target_duration_ms: targetDurationMs } });

export const takeBreakNow = () =>
  commandEngine({ category: "state", content: { command: "take_break_now" } });

export const stopSession = () =>
  commandEngine({ category: "state", content: { command: "stop_session" } });

export const enterPreview = () =>
  commandEngine({ category: "state", content: { command: "enter_preview" } });

export const exitPreview = () =>
  commandEngine({ category: "state", content: { command: "exit_preview" } });

export const updatePreviewProgress = (progress: number) =>
  commandEngineNowait({ category: "state", content: { command: "update_preview_progress", progress } });

export const updateSettlingDuration = (durationMs: number) =>
  commandEngine({ category: "state", content: { command: "update_settling_duration", duration_ms: durationMs } });

export const updateReverseDuration = (durationMs: number) =>
  commandEngine({ category: "state", content: { command: "update_reverse_duration", duration_ms: durationMs } });

// ── Wrapper: Channel ──────────────────────────────────────────────────────────────

export const toggleChannelSwitch = (channelType: ChannelType, switchOn: boolean) =>
  commandEngine({ category: "channel", content: { command: "toggle_switch", channel_type: channelType, switch_on: switchOn } });

export const updateTargetChannelValue = (targetChannelValue: ChannelValue) => {
  commandEngineNowait({ category: "channel", content: { command: "update_target_channel_value", target_channel_value: targetChannelValue } });
  console.log(targetChannelValue);
}

export const updateProgressBeginRatio = (channelType: ChannelType, progressBeginRatio: number) =>
  commandEngineNowait({ category: "channel", content: { command: "update_progress_begin_ratio", channel_type: channelType, progress_begin_ratio: progressBeginRatio } });

export const updateProgressCurveParams = (channelType: ChannelType, curveParameters: CurveParameters) =>
  commandEngineNowait({ category: "channel", content: { command: "update_progress_curve_paras", channel_type: channelType, curve_parameters: curveParameters } });

export const updateSettlingCurveParams = (channelType: ChannelType, curveParameters: CurveParameters) =>
  commandEngineNowait({ category: "channel", content: { command: "update_settling_curve_paras", channel_type: channelType, curve_parameters: curveParameters } });

export const updateReverseCurveParams = (channelType: ChannelType, curveParameters: CurveParameters) =>
  commandEngineNowait({ category: "channel", content: { command: "update_reverse_curve_paras", channel_type: channelType, curve_parameters: curveParameters } });

export const forceReset = () =>
  commandEngine({ category: "force_reset" });

// ── Wrapper: Progress channel ──────────────────────────────────────────────────

export const registerProgressChannel = (onTick: (payload: ProgressPayload) => void) => {
  const ch = new Channel<ProgressPayload>();
  ch.onmessage = onTick;
  commandEngine({ category: "progress", content: { command: "register_channel", channel: ch } });
}

// ── Engine ──────────────────────────────────────────────────────────────

const commandEngine = (command: EngineCommand) =>
  invoke<void>("command_engine", { command });

const commandEngineNowait = (command: EngineCommand) =>
  invoke<void>("command_engine_nowait", { command });

export const getAvailableStoredConfig = () =>
  invoke<StoredConfig>("get_available_stored_config");

// ── Misc ──────────────────────────────────────────────────────────────

export const getPresetSessionDurations = () =>
  invoke<number[]>("get_preset_session_durations");

export const updatePresetSessionDurations = (durations: [number, number, number]) =>
  invoke<void>("update_preset_session_durations", { durations });

export const getLastCrash = () =>
  invoke<CrashPayload | null>("get_last_crash");

export const acknowledgeCrash = () => invoke<void>("acknowledge_crash");

export const setAutostartEnabled = (enabled: boolean) =>
  invoke<void>("set_autostart_enabled", { enabled });

export const isAutostartEnabled = () => invoke<boolean>("is_autostart_enabled");

// ── Shortcuts ────────────────────────────────────────────────────────────

export const getShortcutBindings = () =>
  invoke<ShortcutBindings>("get_shortcut_bindings");

export const updateShortcutBindings = (bindings: ShortcutBindings) =>
  invoke<void>("update_shortcut_bindings", { bindings });

// ── Window silent start ───────────────────────────────────────────────────────

export const getSilentStart = () => invoke<boolean>("get_silent_start");

export const setSilentStart = (enabled: boolean) =>
  invoke<void>("set_silent_start", { enabled });

export const getAppLocale = () => invoke<string>("get_app_locale");

export const getResolvedAppLocale = () => invoke<string>("get_resolved_app_locale");

export const setAppLocale = (locale: string) =>
  invoke<void>("set_app_locale", { locale });
