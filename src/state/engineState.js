import { ref } from "vue";
import * as api from "../api/commands";

// ── Reactive state ────────────────────────────────────────────────────

/** @type {import('vue').Ref<[ChannelType, ChannelConfig][]>} */
export const channelConfigs = ref([]);

export const timerConfig = ref({ settling_duration_ms: 5000, reverse_duration_ms: 2000 });
export const presetDurations = ref([25 * 60_000, 50 * 60_000, 90 * 60_000]);
export const autostartEnabled = ref(false);
export const silentStart = ref(false);
export const configMutationError = ref("");

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
let _errorTimer = 0;
const _mutationTokens = new Map();

const ENGINE_RECONCILE_DELAY_MS = 80;
const CHANNEL_CONFLICTS = {
  saturation: ["color_temp", "brightness"],
  color_temp: ["saturation"],
  brightness: ["saturation"],
};

function cloneValue(value) {
  return JSON.parse(JSON.stringify(value));
}

function describeError(error) {
  if (error instanceof Error) return error.message;
  return String(error);
}

function clearConfigMutationErrorSoon() {
  window.clearTimeout(_errorTimer);
  _errorTimer = window.setTimeout(() => {
    configMutationError.value = "";
  }, 4000);
}

function setConfigMutationError(message, error) {
  configMutationError.value = error ? `${message}: ${describeError(error)}` : message;
  clearConfigMutationErrorSoon();
}

export function clearConfigMutationError() {
  configMutationError.value = "";
  window.clearTimeout(_errorTimer);
}

function nextMutationToken(key) {
  const token = (_mutationTokens.get(key) ?? 0) + 1;
  _mutationTokens.set(key, token);
  return token;
}

function isLatestMutation(key, token) {
  return _mutationTokens.get(key) === token;
}

function delay(ms) {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

// ── Init (call once in App.vue onMounted) ─────────────────────────────

export async function init() {
  if (_initialized) return;
  _initialized = true;

  const [config, durations, crash, shortcuts, autostart, silent] = await Promise.all([
    api.getAvailableStoredConfig().catch(() => null),
    api.getPresetSessionDurations().catch(() => null),
    api.getLastCrash().catch(() => null),
    api.getShortcutBindings().catch(() => null),
    api.isAutostartEnabled().catch(() => null),
    api.getSilentStart().catch(() => null),
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
  if (typeof autostart === "boolean") {
    autostartEnabled.value = autostart;
  }
  if (typeof silent === "boolean") {
    silentStart.value = silent;
  }

  api.registerProgressChannel(onProgress);
}

function applyStoredConfig(config) {
  if (!config) return;
  channelConfigs.value = config.channel_configs;
  timerConfig.value = config.timer_config;
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
    applyStoredConfig(config);
    return config;
  } finally {
    _refreshRunning = false;
  }
}

async function loadConfigAfterEngineTick() {
  await delay(ENGINE_RECONCILE_DELAY_MS);
  return api.getAvailableStoredConfig();
}

async function optimisticMutation({
  key,
  applyLocal,
  restoreLocal,
  commit,
  reconcile,
  verify,
  errorMessage,
}) {
  const token = nextMutationToken(key);
  clearConfigMutationError();
  applyLocal();

  try {
    await commit();
  } catch (error) {
    if (isLatestMutation(key, token)) {
      restoreLocal();
      setConfigMutationError(errorMessage, error);
    }
    return false;
  }

  if (!isLatestMutation(key, token)) return true;

  try {
    const authoritative = reconcile ? await reconcile() : undefined;
    if (!isLatestMutation(key, token)) return true;
    if (authoritative?.channel_configs && authoritative?.timer_config) {
      applyStoredConfig(authoritative);
    }
    if (verify && !verify(authoritative)) {
      setConfigMutationError(errorMessage);
      return false;
    }
  } catch (error) {
    if (isLatestMutation(key, token)) {
      restoreLocal();
      setConfigMutationError(errorMessage, error);
    }
    return false;
  }

  return true;
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
  const previous = cloneValue(channelConfigs.value);
  return optimisticMutation({
    key: `channel:${channelType}:switch`,
    applyLocal: () => {
      if (switchOn) {
        for (const conflict of CHANNEL_CONFLICTS[channelType] ?? []) {
          updateChannelConfigField(conflict, (cfg) => ({ ...cfg, switch_on: false }));
        }
      }
      updateChannelConfigField(channelType, (cfg) => ({ ...cfg, switch_on: switchOn }));
    },
    restoreLocal: () => {
      channelConfigs.value = previous;
    },
    commit: () => api.toggleChannelSwitch(channelType, switchOn),
    reconcile: loadConfigAfterEngineTick,
    verify: (config) => {
      const entry = config?.channel_configs?.find(([type]) => type === channelType);
      return entry ? entry[1].switch_on === switchOn : false;
    },
    errorMessage: "Could not update this channel",
  });
}

export function updateSettlingDuration(durationMs) {
  const previous = cloneValue(timerConfig.value);
  return optimisticMutation({
    key: "timer:settling_duration",
    applyLocal: () => {
      timerConfig.value = { ...timerConfig.value, settling_duration_ms: durationMs };
    },
    restoreLocal: () => {
      timerConfig.value = previous;
    },
    commit: () => api.updateSettlingDuration(durationMs),
    reconcile: loadConfigAfterEngineTick,
    verify: (config) => config?.timer_config?.settling_duration_ms === durationMs,
    errorMessage: "Could not update settling duration",
  });
}

export function updateReverseDuration(durationMs) {
  const previous = cloneValue(timerConfig.value);
  return optimisticMutation({
    key: "timer:reverse_duration",
    applyLocal: () => {
      timerConfig.value = { ...timerConfig.value, reverse_duration_ms: durationMs };
    },
    restoreLocal: () => {
      timerConfig.value = previous;
    },
    commit: () => api.updateReverseDuration(durationMs),
    reconcile: loadConfigAfterEngineTick,
    verify: (config) => config?.timer_config?.reverse_duration_ms === durationMs,
    errorMessage: "Could not update reverse duration",
  });
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
  const previous = cloneValue(channelConfigs.value);
  return optimisticMutation({
    key: `channel:${channelType}:progress_begin_ratio`,
    applyLocal: () => {
      updateChannelConfigField(channelType, (cfg) => ({
        ...cfg,
        persistent_state_params_table: {
          ...cfg.persistent_state_params_table,
          progress_begin_ratio: ratio,
        },
      }));
    },
    restoreLocal: () => {
      channelConfigs.value = previous;
    },
    commit: () => api.updateProgressBeginRatio(channelType, ratio),
    reconcile: loadConfigAfterEngineTick,
    verify: (config) => {
      const entry = config?.channel_configs?.find(([type]) => type === channelType);
      return entry ? entry[1].persistent_state_params_table.progress_begin_ratio === ratio : false;
    },
    errorMessage: "Could not update channel timing",
  });
}

export function updateChannelTargetValue(channelType, channelValue) {
  const previous = cloneValue(channelConfigs.value);
  return optimisticMutation({
    key: `channel:${channelType}:target_value`,
    applyLocal: () => {
      updateChannelConfigField(channelType, (cfg) => ({
        ...cfg,
        persistent_state_params_table: {
          ...cfg.persistent_state_params_table,
          target_channel_value: channelValue,
        },
      }));
    },
    restoreLocal: () => {
      channelConfigs.value = previous;
    },
    commit: () => api.updateTargetChannelValue(channelValue),
    reconcile: loadConfigAfterEngineTick,
    verify: (config) => {
      const entry = config?.channel_configs?.find(([type]) => type === channelType);
      return entry
        ? JSON.stringify(entry[1].persistent_state_params_table.target_channel_value) === JSON.stringify(channelValue)
        : false;
    },
    errorMessage: "Could not update channel target",
  });
}

export function updateChannelCurveParams(channelType, steepness) {
  const cp = { normalized_sigmoid: { steepness } };
  const previous = cloneValue(channelConfigs.value);
  return optimisticMutation({
    key: `channel:${channelType}:curve_params`,
    applyLocal: () => {
      updateChannelConfigField(channelType, (cfg) => ({
        ...cfg,
        persistent_state_params_table: {
          ...cfg.persistent_state_params_table,
          progress_curve_parameters: cp,
          settling_curve_parameters: cp,
          reverse_curve_parameters: cp,
        },
      }));
    },
    restoreLocal: () => {
      channelConfigs.value = previous;
    },
    commit: () => Promise.all([
      api.updateProgressCurveParams(channelType, cp),
      api.updateSettlingCurveParams(channelType, cp),
      api.updateReverseCurveParams(channelType, cp),
    ]),
    reconcile: loadConfigAfterEngineTick,
    verify: (config) => {
      const entry = config?.channel_configs?.find(([type]) => type === channelType);
      if (!entry) return false;
      const t = entry[1].persistent_state_params_table;
      return JSON.stringify(t.progress_curve_parameters) === JSON.stringify(cp)
        && JSON.stringify(t.settling_curve_parameters) === JSON.stringify(cp)
        && JSON.stringify(t.reverse_curve_parameters) === JSON.stringify(cp);
    },
    errorMessage: "Could not update channel curve",
  });
}

export function setAutostart(enabled) {
  const previous = autostartEnabled.value;
  return optimisticMutation({
    key: "system:autostart",
    applyLocal: () => {
      autostartEnabled.value = enabled;
    },
    restoreLocal: () => {
      autostartEnabled.value = previous;
    },
    commit: () => api.setAutostartEnabled(enabled),
    reconcile: api.isAutostartEnabled,
    verify: (value) => value === enabled,
    errorMessage: "Could not update launch at login",
  });
}

export function setSilentStart(enabled) {
  const previous = silentStart.value;
  return optimisticMutation({
    key: "system:silent_start",
    applyLocal: () => {
      silentStart.value = enabled;
    },
    restoreLocal: () => {
      silentStart.value = previous;
    },
    commit: () => api.setSilentStart(enabled),
    reconcile: api.getSilentStart,
    verify: (value) => value === enabled,
    errorMessage: "Could not update silent start",
  });
}

export async function acknowledgeCrash() {
  lastCrash.value = null;
  api.acknowledgeCrash();
}
