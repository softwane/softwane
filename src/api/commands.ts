import { invoke, Channel } from "@tauri-apps/api/core";
import type {
  ChannelType,
  ChannelValue,
  CurveParameters,
  StoredConfig,
  ProgressPayload,
  ShortcutBindings,
} from "./types";

// ── Timer ────────────────────────────────────────────────────────────

export const startSession = (targetDurationMs: number) =>
  invoke<void>("start_session", { targetDurationMs });

export const takeBreakNow = () => invoke<void>("take_break_now");

export const stopSession = () => invoke<void>("stop_session");

export const enterPreview = () => invoke<void>("enter_preview");

export const exitPreview = () => invoke<void>("exit_preview");

export const updatePreviewProgress = (progress: number) =>
  invoke<void>("update_preview_progress", { progress });

export const updateSettlingDuration = (durationMs: number) =>
  invoke<void>("update_settling_duration", { durationMs });

export const updateReverseDuration = (durationMs: number) =>
  invoke<void>("update_reverse_duration", { durationMs });

// ── 通道 ──────────────────────────────────────────────────────────────

export const toggleChannelSwitch = (channelType: ChannelType, switchOn: boolean) =>
  invoke<void>("toggle_channel_switch", { channelType, switchOn });

export const updateTargetChannelValue = (targetChannelValue: ChannelValue) =>
  invoke<void>("update_target_channel_value", { targetChannelValue });

export const updateProgressBeginRatio = (channelType: ChannelType, progressBeginRatio: number) =>
  invoke<void>("update_progress_begin_ratio", { channelType, progressBeginRatio });

export const updateProgressCurveParams = (channelType: ChannelType, curveParameters: CurveParameters) =>
  invoke<void>("update_progress_curve_params", { channelType, curveParameters });

export const updateSettlingCurveParams = (channelType: ChannelType, curveParameters: CurveParameters) =>
  invoke<void>("update_settling_curve_params", { channelType, curveParameters });

export const updateReverseCurveParams = (channelType: ChannelType, curveParameters: CurveParameters) =>
  invoke<void>("update_reverse_curve_params", { channelType, curveParameters });

// ── 系统 ──────────────────────────────────────────────────────────────

export const forceReset = () => invoke<void>("force_reset");

export const getAvailableStoredConfig = () =>
  invoke<StoredConfig>("get_available_stored_config");

export const getPresetSessionDurations = () =>
  invoke<number[]>("get_preset_session_durations");

export const updatePresetSessionDurations = (durations: [number, number, number]) =>
  invoke<void>("update_preset_session_durations", { durations });

export const getLastCrash = () =>
  invoke<Record<string, unknown> | null>("get_last_crash");

export const acknowledgeCrash = () => invoke<void>("acknowledge_crash");

export const setAutostartEnabled = (enabled: boolean) =>
  invoke<void>("set_autostart_enabled", { enabled });

export const isAutostartEnabled = () => invoke<boolean>("is_autostart_enabled");

// ── Silent start ───────────────────────────────────────────────────────

export const getSilentStart = () => invoke<boolean>("get_silent_start");

export const setSilentStart = (enabled: boolean) =>
  invoke<void>("set_silent_start", { enabled });

// ── Progress channel ──────────────────────────────────────────────────

export function registerProgressChannel(
  onTick: (payload: ProgressPayload) => void
): void {
  const ch = new Channel<ProgressPayload>();
  ch.onmessage = onTick;
  invoke<void>("register_progress_channel", { channel: ch });
}

// ── 快捷键 ────────────────────────────────────────────────────────────

export const getShortcutBindings = () =>
  invoke<ShortcutBindings>("get_shortcut_bindings");

export const updateShortcutBindings = (bindings: ShortcutBindings) =>
  invoke<void>("update_shortcut_bindings", { bindings });
