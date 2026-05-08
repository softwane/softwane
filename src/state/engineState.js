import { reactive, ref } from "vue";
import * as api from "../api/commands";

// ── Reactive state ────────────────────────────────────────────────────

/** @type {import('vue').Ref<[ChannelType, ChannelConfig][]>} */
export const channelConfigs = ref([]);

export const timerConfig = ref({ settling_duration_ms: 5000, reverse_duration_ms: 2000 });
export const presetDurations = ref([25 * 60_000, 50 * 60_000, 90 * 60_000]);

/** @type {import('vue').Ref<import('../api/types').TimerStateSnapshot|null>} */
export const timerState = ref(null);

/** Bump on every progress push so reactive chains depending on elapsed/time see updates. */
export const epoch = ref(0);

/** @type {import('vue').Ref<Record<string,unknown>|null>} */
export const lastCrash = ref(null);

/**
 * Authoritative shortcut bindings, populated by `init()` and replaced
 * after a successful `saveShortcutBindings` round-trip.  `null` means
 * "not yet loaded".
 *
 * @type {import('vue').Ref<import('../api/types').ShortcutBindings|null>}
 */
export const shortcutBindings = ref(null);

// ── Preview — optimistic local, reconciled by progress push ───────────

export const previewProgress = ref(0);

// ── Has init been called? ─────────────────────────────────────────────

let _initialized = false;

// ── Init (call once in App.vue onMounted) ─────────────────────────────

export async function init() {
  if (_initialized) return;
  _initialized = true;

  const [config, durations, crash, shortcuts] = await Promise.all([
    api.getAvailableStoredConfig().catch(() => null),
    api.getPresetSessionDurations().catch(() => null),
    api.getLastCrash().catch(() => null),
    api.getShortcutBindings().catch(() => null),
  ]);

  if (config) {
    // channel_configs is [ChannelType, ChannelConfig][] — convert to reactive friendly shape
    channelConfigs.value = config.channel_configs;
    timerConfig.value = config.timer_config;
  }
  if (durations) {
    presetDurations.value = [...durations];
  }
  if (crash) {
    lastCrash.value = crash;
  }
  if (shortcuts) {
    shortcutBindings.value = shortcuts;
  }

  api.registerProgressChannel(onProgress);
}

function onProgress(payload) {
  if (!payload) return;
  timerState.value = payload.timer_state;

  // Reconcile preview state: engine is authoritative
  const ts = payload.timer_state;
  if (ts && ts.state === "preview" && "progress" in ts) {
    previewProgress.value = ts.progress;
  }

  epoch.value++;
}

// ── Re-fetch config from engine (call when settings opens) ────────────

let _refreshRunning = false;

export async function refreshConfig() {
  if (_refreshRunning) return;
  _refreshRunning = true;
  try {
    const config = await api.getAvailableStoredConfig().catch(() => null);
    if (config) {
      channelConfigs.value = config.channel_configs;
      timerConfig.value = config.timer_config;
    }
  } finally {
    _refreshRunning = false;
  }
}

// ── Actions — optimistic local update + fire-and-forget command ───────

export async function startSession(targetDurationMs) {
  api.startSession(targetDurationMs);
}

export function takeBreakNow() {
  api.takeBreakNow();
}

export function stopSession() {
  api.stopSession();
}

/**
 * @param {import('../api/types').ChannelType} channelType
 * @param {boolean} switchOn
 */
export function toggleChannel(channelType, switchOn) {
  // Optimistic: update local config array in place
  const cfgs = channelConfigs.value;
  for (let i = 0; i < cfgs.length; i++) {
    if (cfgs[i][0] === channelType) {
      cfgs[i][1] = { ...cfgs[i][1], switch_on: switchOn };
      break;
    }
  }
  // Fire command; next progress push + refreshConfig (on re-open) will reconcile
  api.toggleChannelSwitch(channelType, switchOn);
}

export function updateSettlingDuration(durationMs) {
  timerConfig.value = { ...timerConfig.value, settling_duration_ms: durationMs };
  api.updateSettlingDuration(durationMs);
}

export function updateReverseDuration(durationMs) {
  timerConfig.value = { ...timerConfig.value, reverse_duration_ms: durationMs };
  api.updateReverseDuration(durationMs);
}

export function forceReset() {
  api.forceReset();
}

export function enterPreview() {
  api.enterPreview();
}

export async function exitPreview() {
  previewProgress.value = 0;
  api.exitPreview();
}

export async function setPreviewProgress(progress) {
  const clamped = Math.max(0, Math.min(1, Number(progress)));
  const previous = previewProgress.value;
  previewProgress.value = clamped;
  try {
    await api.updatePreviewProgress(clamped);
  } catch {
    // Rollback on failure
    previewProgress.value = previous;
  }
}

export function setPresetDurations(durations) {
  if (durations.length !== 3) return;
  presetDurations.value = [...durations];
  api.updatePresetSessionDurations(durations);
}

/**
 * Persist preset durations (Save flow).  On success the source ref
 * is updated; on failure the error is propagated for the caller
 * (Settings panel) to display.
 *
 * @param {number[]} durations
 */
export async function saveShortcutPresetDurations(durations) {
  if (durations.length !== 3) {
    throw new Error("expected exactly 3 durations");
  }
  await api.updatePresetSessionDurations(durations);
  presetDurations.value = [...durations];
}

/**
 * Persist a new shortcut bindings map (Save flow).
 *
 * Backend authoritative: it validates conflicts, registers atomically,
 * and only writes the store on success.  If validation fails the
 * promise rejects with the backend-formatted message.
 *
 * @param {import('../api/types').ShortcutBindings} bindings
 */
export async function saveShortcutBindings(bindings) {
  await api.updateShortcutBindings(bindings);
  shortcutBindings.value = bindings;
}

// ── Per-channel config ─────────────────────────────────────────────────

function updateChannelConfigField(channelType, fn) {
  const cfgs = channelConfigs.value;
  const idx = cfgs.findIndex(([t]) => t === channelType);
  if (idx < 0) return;
  const [type, cfg] = cfgs[idx];
  cfgs[idx] = [type, fn(cfg)];
}

export function updateChannelProgressBeginRatio(channelType, ratio) {
  updateChannelConfigField(channelType, (cfg) => {
    cfg.persistent_state_params_table.progress_begin_ratio = ratio;
    return cfg;
  });
  api.updateProgressBeginRatio(channelType, ratio);
}

export function updateChannelTargetValue(channelType, channelValue) {
  updateChannelConfigField(channelType, (cfg) => {
    cfg.persistent_state_params_table.target_channel_value = channelValue;
    return cfg;
  });
  api.updateTargetChannelValue(channelValue);
}

export function updateChannelCurveParams(channelType, steepness) {
  const cp = { normalized_sigmoid: { steepness } };
  updateChannelConfigField(channelType, (cfg) => {
    const t = cfg.persistent_state_params_table;
    t.progress_curve_parameters = cp;
    t.settling_curve_parameters = cp;
    t.reverse_curve_parameters = cp;
    return cfg;
  });
  api.updateProgressCurveParams(channelType, cp);
  api.updateSettlingCurveParams(channelType, cp);
  api.updateReverseCurveParams(channelType, cp);
}

export function setAutostart(enabled) {
  api.setAutostartEnabled(enabled);
}

export async function acknowledgeCrash() {
  lastCrash.value = null;
  api.acknowledgeCrash();
}
