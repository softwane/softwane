<script setup>
import { computed, onMounted, ref } from "vue";
import erodeMark from "./assets/erode-mark.svg";
import { useAppearance } from "./composables/useAppearance";
import { useDraft } from "./composables/useDraft";
import KeyBindingInput from "./components/KeyBindingInput.vue";
import { SHORTCUT_ACTIONS } from "./api/types";
import {
  init,
  refreshConfig,
  channelConfigs,
  timerConfig,
  presetDurations,
  shortcutBindings,
  timerState,
  epoch,
  previewProgress,
  startSession,
  takeBreakNow,
  stopSession,
  toggleChannel,
  updateSettlingDuration,
  updateReverseDuration,
  forceReset,
  enterPreview,
  exitPreview,
  setPreviewProgress,
  saveShortcutPresetDurations,
  saveShortcutBindings,
  updateChannelProgressBeginRatio,
  updateChannelTargetValue,
  updateChannelCurveParams,
} from "./state/engineState";

const isSettingsOpen = ref(false);
const workMinutes = ref(50);

const themeModeOptions = [
  { id: "auto", label: "Auto", description: "Follow the system appearance and switch live." },
  { id: "dark", label: "Dark", description: "Always keep the app in dark mode." },
  { id: "light", label: "Light", description: "Always keep the app in light mode." },
];

const { resolvedTheme, setThemeMode, themeMode } = useAppearance();

// ── Channel helpers ────────────────────────────────────────────────────

function findChannelConfig(type) {
  const entry = channelConfigs.value.find(([t]) => t === type);
  return entry ? entry[1] : null;
}

function getChannelTargetValueDisplay(type, cfg) {
  if (!cfg) return 0;
  const tv = cfg.persistent_state_params_table.target_channel_value;
  if (type === "saturation") return tv.saturation ?? 0;
  if (type === "color_temp") return tv.color_temp_kelvin ?? 6500;
  if (type === "brightness") return tv.brightness ?? 1;
  return 0;
}

function getChannelSteepness(type) {
  const cfg = findChannelConfig(type);
  if (!cfg) return 10;
  return cfg.persistent_state_params_table.progress_curve_parameters.normalized_sigmoid?.steepness ?? 10;
}

function getChannelBeginRatio(type) {
  const cfg = findChannelConfig(type);
  return cfg?.persistent_state_params_table.progress_begin_ratio ?? 0.9;
}

function onChannelTargetChange(type, value) {
  const num = Number(value);
  if (type === "saturation") updateChannelTargetValue(type, { saturation: num });
  else if (type === "color_temp") updateChannelTargetValue(type, { color_temp_kelvin: Math.round(num) });
  else if (type === "brightness") updateChannelTargetValue(type, { brightness: num });
}

// ── Derived from state ────────────────────────────────────────────────

const currentPhase = computed(() => timerState.value?.state ?? "idle");
const isIdle = computed(() => currentPhase.value === "idle");
const isProgress = computed(() => currentPhase.value === "progress");
const isSettling = computed(() => currentPhase.value === "settling");
const isSabi = computed(() => currentPhase.value === "sabi");
const isReverse = computed(() => currentPhase.value === "reverse");
const isPreview = computed(() => currentPhase.value === "preview");
const hasActiveSession = computed(() => !isIdle.value && !isPreview.value);

const phaseLabel = computed(() => {
  const ts = timerState.value;
  if (!ts) return "Idle";
  return ts.state.charAt(0).toUpperCase() + ts.state.slice(1);
});

const elapsedMs = computed(() => {
  void epoch.value;
  const ts = timerState.value;
  if (ts && "elapsed_ms" in ts) return ts.elapsed_ms;
  return 0;
});

const targetDurationMs = computed(() => {
  const ts = timerState.value;
  if (ts && "target_duration_ms" in ts) return ts.target_duration_ms;
  return 0;
});

const remainingMs = computed(() => {
  void epoch.value;
  if (isProgress.value || isSettling.value || isReverse.value) {
    return Math.max(0, targetDurationMs.value - elapsedMs.value);
  }
  return 0;
});

function formatClock(totalSeconds) {
  const m = Math.floor(totalSeconds / 60);
  const s = Math.floor(totalSeconds % 60);
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

const displayTime = computed(() => {
  if (isProgress.value || isSettling.value)
    return formatClock(Math.ceil(remainingMs.value / 1000));
  if (isIdle.value)
    return formatClock(Math.max(workMinutes.value, 0) * 60);
  return "";
});

const progressValue = computed(() => {
  if (isPreview.value && timerState.value && "progress" in timerState.value)
    return Math.max(0, Math.min(1, timerState.value.progress));
  if (!isProgress.value || targetDurationMs.value <= 0) return 0;
  return Math.max(0, Math.min(1, elapsedMs.value / targetDurationMs.value));
});

const channelEnabled = computed(() => {
  const map = {};
  for (const [type, cfg] of channelConfigs.value) map[type] = cfg.switch_on;
  return map;
});

const phaseTone = computed(() => {
  switch (currentPhase.value) {
    case "progress": return "phase-forward";
    case "settling": return "phase-settling";
    case "sabi": return "phase-sabi";
    case "reverse": return "phase-reverse";
    default: return "phase-idle";
  }
});

const showTimerValue = computed(() => isProgress.value || isSettling.value || isIdle.value);
const progressStyle = computed(() => ({ transform: `scaleX(${progressValue.value})` }));

const secondaryStatusLine = computed(() => {
  if (isSettling.value) return "Settling into rest";
  if (isReverse.value) return "Recovering to neutral";
  if (isSabi.value) return "Press hotkey to return";
  return "";
});

const themeModeSummary = computed(() => {
  if (themeMode.value === "auto")
    return `Auto, ${resolvedTheme.value === "dark" ? "dark now" : "light now"}`;
  return `${resolvedTheme.value === "dark" ? "Dark" : "Light"} fixed`;
});

const canTakeBreak = computed(() => isProgress.value);
const canStop = computed(() => isProgress.value || isSettling.value || isSabi.value);
const canStartNewSession = computed(() => isIdle.value);
const isStartLayerOpen = ref(true);

function hasChannel(type) { return channelConfigs.value.some(([t]) => t === type); }
function channelLabel(type) { return type === "color_temp" ? "Warmth" : type.charAt(0).toUpperCase() + type.slice(1); }

function openStartLayer() {
  isStartLayerOpen.value = true;
}

function onStartSession() {
  const ms = Math.max(1, Math.min(120, Number(workMinutes.value) || 50)) * 60_000;
  startSession(ms);
  isStartLayerOpen.value = false;
}

function onToggleChannel(type, checked) { toggleChannel(type, checked); }

async function onOpenSettings() {
  await refreshConfig();
  isSettingsOpen.value = true;
}

function onCloseSettings() { isSettingsOpen.value = false; }

// ── Preset durations (Save / Cancel) ──────────────────────────────────

const presetDraft = useDraft(presetDurations, async (d) => {
  await saveShortcutPresetDurations(d);
});
const presetSaveError = ref("");

function onDraftDurationChange(index, value) {
  const v = Math.max(60_000, Number(value) * 60_000 || 60_000);
  presetDraft.draft.value = [
    ...presetDraft.draft.value.slice(0, index),
    v,
    ...presetDraft.draft.value.slice(index + 1),
  ];
}

async function onPresetSave() {
  presetSaveError.value = "";
  try {
    await presetDraft.commit();
  } catch (e) {
    presetSaveError.value = String(e);
  }
}

function onPresetCancel() {
  presetSaveError.value = "";
  presetDraft.cancel();
}

// ── Shortcut bindings (Save / Cancel) ─────────────────────────────────

const shortcutLabels = {
  start_preset1: "Start preset 1",
  start_preset2: "Start preset 2",
  start_preset3: "Start preset 3",
  take_break_now: "Take a break now",
  stop_session: "Stop session",
  toggle_preview: "Toggle preview",
  force_reset: "Force reset",
  toggle_main_window: "Toggle window",
};

const shortcutDraft = useDraft(shortcutBindings, async (b) => {
  await saveShortcutBindings(b);
});
const shortcutSaveError = ref("");

function setShortcutBinding(action, binding) {
  if (!shortcutDraft.draft.value) return;
  shortcutDraft.draft.value = {
    ...shortcutDraft.draft.value,
    [action]: binding,
  };
}

/** Stringify a binding's (sorted modifiers, code) for collision matching. */
function bindingFingerprint(b) {
  if (!b) return "";
  const mods = [...b.modifiers].sort().join("+");
  return `${mods}::${b.code}`;
}

/**
 * Collisions in the current draft: returns a Set of action ids that
 * share their fingerprint with at least one other action.
 */
const shortcutConflicts = computed(() => {
  const conflicts = new Set();
  if (!shortcutDraft.draft.value) return conflicts;
  const fpToAction = new Map();
  for (const action of SHORTCUT_ACTIONS) {
    const b = shortcutDraft.draft.value[action];
    const fp = bindingFingerprint(b);
    if (!fp) continue;
    if (fpToAction.has(fp)) {
      conflicts.add(fpToAction.get(fp));
      conflicts.add(action);
    } else {
      fpToAction.set(fp, action);
    }
  }
  return conflicts;
});

const shortcutHasConflict = computed(() => shortcutConflicts.value.size > 0);

const conflictBannerText = computed(() => {
  const ids = [...shortcutConflicts.value];
  if (ids.length === 0) return "";
  const labels = ids.map((id) => shortcutLabels[id] ?? id);
  return `Conflict: ${labels.join(", ")} share the same combination.`;
});

async function onShortcutSave() {
  shortcutSaveError.value = "";
  if (shortcutHasConflict.value) {
    shortcutSaveError.value = "Resolve conflicts before saving.";
    return;
  }
  try {
    await shortcutDraft.commit();
  } catch (e) {
    shortcutSaveError.value = String(e);
  }
}

function onShortcutCancel() {
  shortcutSaveError.value = "";
  shortcutDraft.cancel();
}

// ── Lifecycle ─────────────────────────────────────────────────────────

onMounted(() => { init(); });
</script>

<template>
  <main class="app-shell">
    <section class="timer-app">
      <header class="topbar">
        <img class="brand-mark" :src="erodeMark" alt="Erode" />
        <button class="ghost-button ghost-button-utility" type="button" @click="onOpenSettings">Tune</button>
      </header>

      <section class="timer-core">
        <span :class="['phase-dot', phaseTone]"></span>
        <p class="status-line">{{ phaseLabel }}</p>
        <h1 :class="['timer-value', { 'is-hidden-in-break': !showTimerValue }]">
          {{ showTimerValue ? displayTime : "\u00A0" }}
        </h1>
        <p :class="['status-line', 'status-line-secondary', { 'is-empty': !secondaryStatusLine }]">
          {{ secondaryStatusLine || "\u00A0" }}
        </p>
      </section>

      <section class="progress-panel">
        <div class="progress-track"><div class="progress-fill" :style="progressStyle"></div></div>
      </section>

      <section class="action-row">
        <button v-if="canTakeBreak" class="action-button action-primary" type="button" @click="takeBreakNow">Take a break now</button>
        <button v-if="canStop" class="action-button" type="button" @click="stopSession">Stop</button>
        <button v-if="canStartNewSession" class="action-button action-primary" type="button" @click="openStartLayer">Start a new session</button>
      </section>
    </section>

    <!-- Start layer (for inputting next session's target duration) -->
    <section :class="['settings-layer', 'start-layer', { 'is-active': isStartLayerOpen && isIdle }]" role="dialog" :aria-hidden="!isIdle" aria-label="Start timer">
      <div class="settings-sheet">
        <header class="settings-header">
          <div><p class="settings-kicker">Start timer</p><h2 class="settings-title">Set this session first</h2></div>
        </header>
        <div class="field-grid">
          <label class="field">
            <span>Work time</span>
            <div class="input-with-unit"><input v-model.number="workMinutes" type="number" max="120" step="1" /><span class="input-unit">min</span></div>
          </label>
        </div>
        <div class="channel-toggles">
          <label v-if="hasChannel('saturation')" class="channel-toggle">
            <input type="checkbox" :checked="channelEnabled.saturation" @change="onToggleChannel('saturation', $event.target.checked)" /><span>Saturation</span>
          </label>
          <label v-if="hasChannel('color_temp')" class="channel-toggle">
            <input type="checkbox" :checked="channelEnabled.color_temp" @change="onToggleChannel('color_temp', $event.target.checked)" /><span>Warmth</span>
          </label>
          <label v-if="hasChannel('brightness')" class="channel-toggle">
            <input type="checkbox" :checked="channelEnabled.brightness" @change="onToggleChannel('brightness', $event.target.checked)" /><span>Brightness</span>
          </label>
        </div>
        <button class="start-button" type="button" @click="onStartSession">Start session</button>
      </div>
    </section>

    <!-- Settings layer -->
    <section :class="['settings-layer', 'mode-settings-layer', { 'is-active': isSettingsOpen }]" role="dialog" :aria-hidden="!isSettingsOpen" aria-label="Channel settings" @click.self="onCloseSettings">
      <div class="settings-sheet settings-page">
        <header class="settings-header">
          <div><h1 class="settings-kicker">Mode settings</h1></div>
          <button class="icon-button" type="button" aria-label="Close settings" @click="onCloseSettings">&times;</button>
        </header>

        <!-- Per-channel config -->
        <div class="channel-settings-list">
          <template v-for="[type] in channelConfigs" :key="type">
            <div class="channel-setting-card">
              <label class="field">
                <input type="checkbox" :checked="channelEnabled[type]" @change="onToggleChannel(type, $event.target.checked)" />
                <strong>{{ channelLabel(type) }}</strong>
              </label>

              <div class="channel-config-controls" v-if="channelEnabled[type]">
                <!-- Begin ratio -->
                <label class="field field-compact">
                  <span>Begin at {{ (getChannelBeginRatio(type) * 100).toFixed(0) }}%</span>
                  <input type="range" min="0" max="1" step="0.01" :value="getChannelBeginRatio(type)" @input="updateChannelProgressBeginRatio(type, Number($event.target.value))" />
                </label>

                <!-- Target value (type-specific) -->
                <template v-if="type === 'saturation'">
                  <label class="field field-compact">
                    <span>Target saturation {{ (getChannelTargetValueDisplay(type, findChannelConfig(type)) * 100).toFixed(0) }}%</span>
                    <input type="range" min="0" max="1" step="0.01" :value="getChannelTargetValueDisplay(type, findChannelConfig(type))" @input="onChannelTargetChange(type, $event.target.value)" />
                  </label>
                </template>
                <template v-else-if="type === 'color_temp'">
                  <label class="field field-compact">
                    <span>Target warmth {{ getChannelTargetValueDisplay(type, findChannelConfig(type)) }} K</span>
                    <input type="range" min="1000" max="6500" step="100" :value="getChannelTargetValueDisplay(type, findChannelConfig(type))" @input="onChannelTargetChange(type, $event.target.value)" />
                  </label>
                </template>
                <template v-else-if="type === 'brightness'">
                  <label class="field field-compact">
                    <span>Target brightness {{ (getChannelTargetValueDisplay(type, findChannelConfig(type)) * 100).toFixed(0) }}%</span>
                    <input type="range" min="0" max="1" step="0.01" :value="getChannelTargetValueDisplay(type, findChannelConfig(type))" @input="onChannelTargetChange(type, $event.target.value)" />
                  </label>
                </template>

                <!-- Curve steepness -->
                <label class="field field-compact">
                  <span>Steepness {{ getChannelSteepness(type).toFixed(1) }}</span>
                  <input type="range" min="0.5" max="30" step="0.5" :value="getChannelSteepness(type)" @input="updateChannelCurveParams(type, Number($event.target.value))" />
                </label>
              </div>
            </div>
          </template>
        </div>

        <!-- Preview -->
        <section class="settings-group">
          <template v-if="!isPreview">
            <button class="action-button" type="button" @click="enterPreview">Open preview</button>
          </template>
          <template v-else>
            <div class="settings-group-header"><span class="settings-group-title">Preview ({{ (previewProgress * 100).toFixed(0) }}%)</span></div>
            <input type="range" min="0" max="1" step="0.01" :value="previewProgress" @input="setPreviewProgress($event.target.value)" />
            <button class="action-button" type="button" @click="exitPreview">Close preview</button>
          </template>
        </section>

        <!-- Preset durations (Save / Cancel) -->
        <section class="settings-group">
          <div class="settings-group-header">
            <span class="settings-group-title">Shortcut durations</span>
            <div class="settings-group-actions">
              <button class="ghost-button" type="button" :disabled="!presetDraft.dirty.value" @click="onPresetCancel">Cancel</button>
              <button class="ghost-button ghost-button-primary" type="button" :disabled="!presetDraft.dirty.value" @click="onPresetSave">Save</button>
            </div>
          </div>
          <div class="field-grid">
            <label v-for="(d, i) in presetDraft.draft.value" :key="i" class="field field-compact">
              <span>Preset {{ i + 1 }}</span>
              <div class="input-with-unit">
                <input type="number" min="1" max="180" step="1" :value="Math.round(d / 60_000)" @input="onDraftDurationChange(i, $event.target.value)" />
                <span class="input-unit">min</span>
              </div>
            </label>
          </div>
          <p v-if="presetSaveError" class="settings-error">{{ presetSaveError }}</p>
        </section>

        <!-- Shortcut bindings (Save / Cancel) -->
        <section class="settings-group" v-if="shortcutDraft.draft.value">
          <div class="settings-group-header">
            <span class="settings-group-title">Shortcuts</span>
            <div class="settings-group-actions">
              <button class="ghost-button" type="button" :disabled="!shortcutDraft.dirty.value" @click="onShortcutCancel">Cancel</button>
              <button class="ghost-button ghost-button-primary" type="button" :disabled="!shortcutDraft.dirty.value || shortcutHasConflict" @click="onShortcutSave">Save</button>
            </div>
          </div>
          <p v-if="shortcutHasConflict" class="settings-error">{{ conflictBannerText }}</p>
          <div class="shortcut-list">
            <div v-for="action in SHORTCUT_ACTIONS" :key="action" class="shortcut-row">
              <span class="shortcut-label">{{ shortcutLabels[action] }}</span>
              <KeyBindingInput
                :model-value="shortcutDraft.draft.value[action]"
                :invalid="shortcutConflicts.has(action)"
                @update:model-value="setShortcutBinding(action, $event)"
              />
            </div>
          </div>
          <p v-if="shortcutSaveError" class="settings-error">{{ shortcutSaveError }}</p>
        </section>

        <!-- Theme -->
        <section class="settings-group">
          <div class="settings-group-header">
            <span class="settings-group-title">Theme</span>
            <span class="settings-group-meta">{{ themeModeSummary }}</span>
          </div>
          <div class="cue-style-list" role="radiogroup" aria-label="Theme mode">
            <button v-for="option in themeModeOptions" :key="option.id" :class="['cue-style-card', { active: themeMode === option.id }]" type="button" role="radio" :aria-checked="themeMode === option.id" @click="setThemeMode(option.id)">
              <strong>{{ option.label }}</strong><span>{{ option.description }}</span>
            </button>
          </div>
        </section>
      </div>
    </section>
  </main>
</template>
