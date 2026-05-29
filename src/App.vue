<script setup>
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import softwaneMark from "./assets/softwane-mark.svg";
import { useAppearance } from "./composables/useAppearance";
import { useDraft } from "./composables/useDraft";
import KeyBindingInput from "./components/KeyBindingInput.vue";
import { SHORTCUT_ACTIONS } from "./api/types";
import { SUPPORTED_LOCALES, SYSTEM_LOCALE_SENTINEL } from "./i18n";
import {
  init,
  refreshConfig,
  channelConfigs,
  timerConfig,
  presetDurations,
  autostartEnabled,
  silentStart,
  configMutationError,
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
  setAutostart,
  setSilentStart,
  clearConfigMutationError,
} from "./state/engineState";

const props = defineProps({
  initialLocalePreference: { type: String, default: SYSTEM_LOCALE_SENTINEL },
  setAppLocale: { type: Function, required: true },
});

const { t, locale } = useI18n();
const isSettingsOpen = ref(false);
const workMinutes = ref(50);
const expandedChannelSettings = ref({});
const isShortcutSectionOpen = ref(false);
const channelCompatibilityNotice = ref("");
const projectUrl = "https://github.com/softwane/softwane";
const contributors = [
  { username: "perthonBalans", name: "Anchor4P" },
  { username: "RoderickQiu", name: "Tianrun Qiu" },
  { username: "RUSRUSHB", name: "Eason Z" },
];
let channelCompatibilityNoticeTimer = 0;
const channelConflicts = {
  saturation: ["color_temp", "brightness"],
  color_temp: ["saturation"],
  brightness: ["saturation"],
};

const { resolvedTheme, setThemeMode, themeMode } = useAppearance();
const localePreference = ref(props.initialLocalePreference);
const isUpdatingLocale = ref(false);
const localeOptions = computed(() => [
  { id: SYSTEM_LOCALE_SENTINEL, label: t("language.system") },
  ...SUPPORTED_LOCALES.map((id) => ({ id, label: t(`language.${id}`) })),
]);
const themeModeOptions = computed(() => ([
  { id: "auto", label: t("appearance.themeOption.auto.label"), description: t("appearance.themeOption.auto.description") },
  { id: "dark", label: t("appearance.themeOption.dark.label"), description: t("appearance.themeOption.dark.description") },
  { id: "light", label: t("appearance.themeOption.light.label"), description: t("appearance.themeOption.light.description") },
]));

// ── Channel helpers ────────────────────────────────────────────────────

function findChannelConfig(type) {
  const entry = channelConfigs.value.find(([t]) => t === type);
  return entry ? entry[1] : null;
}

function getChannelTargetValueDisplay(type, cfg) {
  if (!cfg) return 0;
  const tv = cfg.persistent_state_params_table.target_channel_value;
  if (type === "saturation") return tv.data ?? 0;
  if (type === "color_temp") return tv.data ?? 6500;
  if (type === "brightness") return tv.data ?? 1;
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
  if (type === "saturation") updateChannelTargetValue(type, { type: "saturation", data: num });
  else if (type === "color_temp") updateChannelTargetValue(type, { type: "color_temp_kelvin", data: Math.round(num) });
  else if (type === "brightness") updateChannelTargetValue(type, { type: "brightness", data: num });
}

function detectMacOS() {
  if (typeof navigator === "undefined") return false;
  const platform = navigator.userAgentData?.platform || navigator.platform || "";
  return /mac/i.test(platform);
}

// ── Derived from state ────────────────────────────────────────────────

const currentPhase = computed(() => timerState.value?.state ?? "idle");
const isIdle = computed(() => currentPhase.value === "idle");
const isProgress = computed(() => currentPhase.value === "progress");
const isSettling = computed(() => currentPhase.value === "settling");
const isRest = computed(() => currentPhase.value === "rest");
const isReverse = computed(() => currentPhase.value === "reverse");
const isPreview = computed(() => currentPhase.value === "preview");
const hasActiveSession = computed(() => !isIdle.value && !isPreview.value);

const phaseLabel = computed(() => {
  const ts = timerState.value;
  if (!ts) return t("phase.idle");
  return t(`phase.${ts.state}`);
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
    case "rest": return "phase-rest";
    case "reverse": return "phase-reverse";
    default: return "phase-idle";
  }
});

const showTimerValue = computed(() => isProgress.value || isSettling.value || isIdle.value);
const progressStyle = computed(() => ({ transform: `scaleX(${progressValue.value})` }));

const secondaryStatusLine = computed(() => {
  if (isSettling.value) return t("phase.settlingSecondary");
  if (isReverse.value) return t("phase.reverseSecondary");
  if (isRest.value) return t("phase.restSecondary");
  return "";
});

const themeModeSummary = computed(() => {
  if (themeMode.value === "auto")
    return t("appearance.themeSummaryAuto", {
      current: resolvedTheme.value === "dark"
        ? t("appearance.themeCurrentDark")
        : t("appearance.themeCurrentLight"),
    });
  return t("appearance.themeSummaryFixed", {
    current: resolvedTheme.value === "dark"
      ? t("appearance.themeLabelDark")
      : t("appearance.themeLabelLight"),
  });
});

const canTakeBreak = computed(() => isProgress.value);
const canStop = computed(() => isProgress.value || isSettling.value || isRest.value);
const canStartNewSession = computed(() => isIdle.value);
const isStartLayerOpen = ref(false);
const enabledChannelCount = computed(() => Object.values(channelEnabled.value).filter(Boolean).length);
const cueSummaryText = computed(() => {
  if (enabledChannelCount.value === 0) return t("cue.noActiveCue");
  const active = channelConfigs.value
    .filter(([type]) => channelEnabled.value[type])
    .map(([type]) => channelLabel(type));
  return t("cue.activeSummary", { active: active.join(" + ") });
});

function hasChannel(type) { return channelConfigs.value.some(([t]) => t === type); }
function channelLabel(type) { return t(`cue.channel.${type}.label`); }

function clearChannelCompatibilityNotice() {
  window.clearTimeout(channelCompatibilityNoticeTimer);
  channelCompatibilityNotice.value = "";
}

function showChannelCompatibilityNotice(turnedOffTypes, enabledType) {
  window.clearTimeout(channelCompatibilityNoticeTimer);
  const names = turnedOffTypes.map(channelLabel).join(" / ");
  channelCompatibilityNotice.value = t("cue.compatibilityNotice", {
    names,
    enabled: channelLabel(enabledType),
  });
  channelCompatibilityNoticeTimer = window.setTimeout(() => {
    channelCompatibilityNotice.value = "";
  }, 4200);
}

function channelDescription(type) {
  return t(`cue.channel.${type}.description`);
}

function formatPercent(value) {
  return `${(value * 100).toFixed(0)}%`;
}

function formatMinutes(durationMs) {
  return `${Math.round(durationMs / 60_000)} ${t("session.minutesShort")}`;
}

function channelTargetSummary(type) {
  const value = getChannelTargetValueDisplay(type, findChannelConfig(type));
  if (type === "color_temp") return t("cue.channel.color_temp.targetSummary", { value });
  if (type === "brightness") return t("cue.channel.brightness.targetSummary", { value: formatPercent(value) });
  return t("cue.channel.saturation.targetSummary", { value: formatPercent(value) });
}

function channelTimingSummary(type) {
  return t("cue.timingSummary", { value: formatPercent(getChannelBeginRatio(type)) });
}

function channelRampSummary(type) {
  const steepness = getChannelSteepness(type);
  if (steepness < 6) return t("cue.ramp.slow");
  if (steepness > 18) return t("cue.ramp.sharp");
  return t("cue.ramp.balanced");
}

function isChannelExpanded(type) {
  return Boolean(expandedChannelSettings.value[type]);
}

function toggleChannelDetails(type) {
  expandedChannelSettings.value = {
    ...expandedChannelSettings.value,
    [type]: !expandedChannelSettings.value[type],
  };
}

function openStartLayer() {
  const preferredDuration = presetDurations.value[1] ?? presetDurations.value[0];
  if (preferredDuration) workMinutes.value = Math.round(preferredDuration / 60_000);
  isStartLayerOpen.value = true;
}

function closeStartLayer() {
  isStartLayerOpen.value = false;
}

function onStartPreset(durationMs) {
  workMinutes.value = Math.round(durationMs / 60_000);
  startSession(durationMs);
  closeStartLayer();
}

function onStartSession() {
  const ms = customStartMinutes.value * 60_000;
  startSession(ms);
  closeStartLayer();
}

async function onToggleChannel(type, checked) {
  const turnedOffTypes = checked
    ? (channelConflicts[type] ?? []).filter((conflict) => channelEnabled.value[conflict])
    : [];
  const updated = await toggleChannel(type, checked);
  if (updated && turnedOffTypes.length > 0) {
    showChannelCompatibilityNotice(turnedOffTypes, type);
  } else if (turnedOffTypes.length === 0) {
    clearChannelCompatibilityNotice();
  }
}

async function onOpenSettings() {
  clearConfigMutationError();
  await refreshConfig();
  isSettingsOpen.value = true;
}

function onCloseSettings() { 
  exitPreview();
  isSettingsOpen.value = false; 
}

async function openExternalUrl(url) {
  try {
    await openUrl(url);
  } catch (error) {
    console.error("Failed to open external URL", error);
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

function openProjectPage() {
  return openExternalUrl(projectUrl);
}

function openContributorProfile(username) {
  return openExternalUrl(`https://github.com/${username}`);
}

// ── Preset durations (Save / Cancel) ──────────────────────────────────

const presetDraft = useDraft(presetDurations, async (d) => {
  await saveShortcutPresetDurations(d);
});
const presetSaveError = ref("");
const presetSummaryText = computed(() => presetDraft.draft.value.map(formatMinutes).join(" / "));
const startPresetOptions = computed(() => presetDurations.value.map((durationMs, index) => ({
  id: `preset-${index + 1}`,
  index,
  durationMs,
  minutes: Math.round(durationMs / 60_000),
})));
const customStartMinutes = computed(() => Math.max(1, Math.min(120, Number(workMinutes.value) || 50)));
const customStartLabel = computed(() => t("startLayer.startMinutes", { minutes: customStartMinutes.value }));

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

// ── Shortcut bindings (autosave / Cancel) ─────────────────────────────

const shortcutLabels = {
  start_preset1: "shortcuts.action.start_preset1",
  start_preset2: "shortcuts.action.start_preset2",
  start_preset3: "shortcuts.action.start_preset3",
  take_break_now: "shortcuts.action.take_break_now",
  stop_session: "shortcuts.action.stop_session",
  toggle_preview: "shortcuts.action.toggle_preview",
  force_reset: "shortcuts.action.force_reset",
  toggle_main_window: "shortcuts.action.toggle_main_window",
};

const shortcutDraft = useDraft(shortcutBindings, saveShortcutBindings);
const shortcutSaveError = ref("");
const configuredShortcutCount = computed(() => {
  if (!shortcutDraft.draft.value) return 0;
  return SHORTCUT_ACTIONS.filter((action) => shortcutDraft.draft.value[action]).length;
});

let shortcutSaveToken = 0;
let shortcutSaveQueue = Promise.resolve();

async function setShortcutBinding(action, binding) {
  if (!shortcutDraft.draft.value) return;
  const next = {
    ...shortcutDraft.draft.value,
    [action]: binding,
  };
  shortcutDraft.draft.value = next;
  const token = ++shortcutSaveToken;

  if (findShortcutConflicts(next).size > 0) {
    shortcutSaveError.value = t("shortcuts.resolveConflict");
    return;
  }

  shortcutSaveError.value = "";
  try {
    shortcutSaveQueue = shortcutSaveQueue
      .catch(() => {})
      .then(() => saveShortcutBindings(next));
    await shortcutSaveQueue;
  } catch (e) {
    if (token === shortcutSaveToken) shortcutSaveError.value = String(e);
  }
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
function findShortcutConflicts(bindings) {
  const conflicts = new Set();
  if (!bindings) return conflicts;
  const fpToAction = new Map();
  for (const action of SHORTCUT_ACTIONS) {
    const b = bindings[action];
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
}

const shortcutConflicts = computed(() => findShortcutConflicts(shortcutDraft.draft.value));

const shortcutHasConflict = computed(() => shortcutConflicts.value.size > 0);

const conflictBannerText = computed(() => {
  const ids = [...shortcutConflicts.value];
  if (ids.length === 0) return "";
  const labels = ids.map((id) => t(shortcutLabels[id] ?? id));
  return t("shortcuts.conflictBanner", { labels: labels.join(", ") });
});

function onShortcutCancel() {
  shortcutSaveError.value = "";
  shortcutDraft.cancel();
}

async function onLocalePreferenceChange(event) {
  const nextPreference = event.target.value;
  isUpdatingLocale.value = true;
  try {
    await props.setAppLocale(nextPreference);
    localePreference.value = nextPreference;
  } finally {
    isUpdatingLocale.value = false;
  }
}

watch(
  () => props.initialLocalePreference,
  (next) => {
    localePreference.value = next;
  },
  { immediate: true },
);

watch(
  () => locale.value,
  () => {
    if (shortcutSaveError.value === t("shortcuts.resolveConflict")) {
      shortcutSaveError.value = t("shortcuts.resolveConflict");
    }
  },
);

// ── Lifecycle ─────────────────────────────────────────────────────────

onMounted(async () => {
    init();
    const win = getCurrentWindow();
    await win.show();
    await win.setFocus();
});
</script>

<template>
  <main class="app-shell">
    <section class="timer-app">
      <header class="topbar">
        <img class="brand-mark" :src="softwaneMark" :alt="t('app.name')" />
        <button class="ghost-button ghost-button-utility" type="button" @click="onOpenSettings">{{ t("app.settingsButton") }}</button>
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
        <button v-if="canTakeBreak" class="action-button action-primary" type="button" @click="takeBreakNow">{{ t("actions.takeBreakNow") }}</button>
        <button v-if="canStop" class="action-button" type="button" @click="stopSession">{{ t("actions.stop") }}</button>
        <button v-if="canStartNewSession" class="action-button action-primary" type="button" @click="openStartLayer">{{ t("actions.startNewSession") }}</button>
      </section>
    </section>

    <!-- Start layer (for inputting next session's target duration) -->
    <section :class="['settings-layer', 'start-layer', { 'is-active': isStartLayerOpen && isIdle }]" role="dialog" :aria-hidden="!(isStartLayerOpen && isIdle)" :aria-label="t('startLayer.ariaLabel')" @click.self="closeStartLayer">
      <div class="settings-sheet">
        <header class="settings-header">
          <div><p class="settings-kicker">{{ t("startLayer.kicker") }}</p><h2 class="settings-title">{{ t("startLayer.title") }}</h2></div>
          <button class="icon-button" type="button" :aria-label="t('startLayer.closeAria')" @click="closeStartLayer">&times;</button>
        </header>
        <div class="start-preset-list" :aria-label="t('startLayer.quickStartAria')">
          <button
            v-for="option in startPresetOptions"
            :key="option.id"
            class="start-preset-button"
            type="button"
            @click="onStartPreset(option.durationMs)"
          >
            <strong>{{ option.minutes }}</strong>
            <span>{{ t("session.minutesShort") }}</span>
          </button>
        </div>
        <div class="field-grid">
          <label class="field">
            <span>{{ t("startLayer.customTime") }}</span>
            <div class="input-with-unit"><input v-model.number="workMinutes" type="number" min="1" max="120" step="1" /><span class="input-unit">{{ t("session.minutesShort") }}</span></div>
          </label>
        </div>
        <div class="channel-toggles">
          <label v-if="hasChannel('saturation')" class="channel-toggle">
            <input type="checkbox" :checked="channelEnabled.saturation" @change="onToggleChannel('saturation', $event.target.checked)" /><span>{{ channelLabel("saturation") }}</span>
          </label>
          <label v-if="hasChannel('color_temp')" class="channel-toggle">
            <input type="checkbox" :checked="channelEnabled.color_temp" @change="onToggleChannel('color_temp', $event.target.checked)" /><span>{{ channelLabel("color_temp") }}</span>
          </label>
          <label v-if="hasChannel('brightness')" class="channel-toggle">
            <input type="checkbox" :checked="channelEnabled.brightness" @change="onToggleChannel('brightness', $event.target.checked)" /><span>{{ channelLabel("brightness") }}</span>
          </label>
        </div>
        <p v-if="channelCompatibilityNotice" class="compatibility-note" role="status">{{ channelCompatibilityNotice }}</p>
        <div class="start-actions">
          <button class="ghost-button" type="button" @click="closeStartLayer">{{ t("actions.cancel") }}</button>
          <button class="start-button" type="button" @click="onStartSession">{{ customStartLabel }}</button>
        </div>
      </div>
    </section>

    <!-- Settings layer -->
    <section :class="['settings-layer', 'mode-settings-layer', { 'is-active': isSettingsOpen }]" role="dialog" :aria-hidden="!isSettingsOpen" :aria-label="t('settings.ariaLabel')" @click.self="onCloseSettings">
      <div class="settings-sheet settings-page">
        <header class="settings-header">
          <div>
            <p class="settings-kicker">{{ t("settings.ariaLabel") }}</p>
            <h1 class="settings-title">{{ t("app.settingsTitle") }}</h1>
          </div>
          <button class="icon-button" type="button" :aria-label="t('settings.closeAria')" @click="onCloseSettings">&times;</button>
        </header>

        <p v-if="configMutationError" class="settings-error" role="status">{{ configMutationError }}</p>

        <section class="settings-summary" :aria-label="t('settings.summaryAria')">
          <div>
            <span>{{ t("settings.cue") }}</span>
            <strong>{{ cueSummaryText }}</strong>
          </div>
          <div>
            <span>{{ t("settings.presets") }}</span>
            <strong>{{ presetSummaryText }}</strong>
          </div>
          <div>
            <span>{{ t("settings.app") }}</span>
            <strong>{{ themeModeSummary }}</strong>
          </div>
        </section>

        <section class="settings-section" aria-labelledby="cue-settings-title">
          <div class="settings-section-header">
            <div>
              <p class="settings-kicker">{{ t("cue.kicker") }}</p>
              <h2 id="cue-settings-title" class="settings-section-title">{{ t("cue.title") }}</h2>
            </div>
            <span class="settings-group-meta">{{ t("settings.changesApplyImmediately") }}</span>
          </div>

          <div class="preview-control">
            <div>
              <strong>{{ isPreview ? t("cue.previewTitle", { percent: formatPercent(previewProgress) }) : t("cue.previewTitleIdle") }}</strong>
              <span>{{ t("cue.previewDescription") }}</span>
            </div>
            <div class="preview-actions">
              <template v-if="!isPreview">
                <button class="action-button" type="button" @click="enterPreview">{{ t("actions.openPreview") }}</button>
              </template>
              <template v-else>
                <input :aria-label="t('cue.previewIntensityAria')" type="range" min="0" max="1" step="0.01" :value="previewProgress" @input="setPreviewProgress($event.target.value)" />
                <button class="action-button" type="button" @click="exitPreview">{{ t("actions.closePreview") }}</button>
              </template>
            </div>
          </div>

          <p v-if="channelCompatibilityNotice" class="compatibility-note" role="status">{{ channelCompatibilityNotice }}</p>

          <div class="channel-settings-list">
            <article v-for="[type] in channelConfigs" :key="type" :class="['channel-setting-card', { 'is-disabled': !channelEnabled[type] }]">
              <div class="channel-setting-head">
                <label class="setting-switch">
                  <input type="checkbox" :checked="channelEnabled[type]" @change="onToggleChannel(type, $event.target.checked)" />
                  <span>
                    <strong>{{ channelLabel(type) }}</strong>
                    <small>{{ channelDescription(type) }}</small>
                  </span>
                </label>
                <button v-if="channelEnabled[type]" class="text-button" type="button" :aria-expanded="isChannelExpanded(type)" @click="toggleChannelDetails(type)">
                  {{ isChannelExpanded(type) ? t("actions.hide") : t("actions.details") }}
                </button>
              </div>

              <div v-if="channelEnabled[type]" class="channel-metrics" :aria-label="t('cue.channelSummaryAria')">
                <span>{{ channelTimingSummary(type) }}</span>
                <span>{{ channelTargetSummary(type) }}</span>
                <span>{{ channelRampSummary(type) }}</span>
              </div>

              <div class="channel-config-controls" v-if="channelEnabled[type] && isChannelExpanded(type)">
                <label class="field field-compact">
                  <span>{{ t("cue.beginRatioControl", { value: formatPercent(getChannelBeginRatio(type)) }) }}</span>
                  <input type="range" min="0" max="1" step="0.01" :value="getChannelBeginRatio(type)" @input="updateChannelProgressBeginRatio(type, Number($event.target.value))" />
                </label>

                <template v-if="type === 'saturation'">
                  <label class="field field-compact">
                    <span>{{ t("cue.channel.saturation.targetControl", { value: formatPercent(getChannelTargetValueDisplay(type, findChannelConfig(type))) }) }}</span>
                    <input type="range" min="0.2" max="1" step="0.01" :value="getChannelTargetValueDisplay(type, findChannelConfig(type))" @input="onChannelTargetChange(type, $event.target.value)" />
                  </label>
                </template>
                <template v-else-if="type === 'color_temp'">
                  <label class="field field-compact">
                    <span>{{ t("cue.channel.color_temp.targetControl", { value: getChannelTargetValueDisplay(type, findChannelConfig(type)) }) }}</span>
                    <input type="range" min="1000" max="6500" step="100" :value="getChannelTargetValueDisplay(type, findChannelConfig(type))" @input="onChannelTargetChange(type, $event.target.value)" />
                  </label>
                </template>
                <template v-else-if="type === 'brightness'">
                  <label class="field field-compact">
                    <span>{{ t("cue.channel.brightness.targetControl", { value: formatPercent(getChannelTargetValueDisplay(type, findChannelConfig(type))) }) }}</span>
                    <input type="range" min="0" max="1" step="0.01" :value="getChannelTargetValueDisplay(type, findChannelConfig(type))" @input="onChannelTargetChange(type, $event.target.value)" />
                  </label>
                </template>

                <label class="field field-compact">
                  <span>{{ t("cue.ramp.shapeControl", { value: getChannelSteepness(type).toFixed(1) }) }}</span>
                  <input type="range" min="0.5" max="30" step="0.5" :value="getChannelSteepness(type)" @input="updateChannelCurveParams(type, Number($event.target.value))" />
                </label>
              </div>
            </article>
          </div>
        </section>

        <section class="settings-section" aria-labelledby="session-settings-title">
          <div class="settings-section-header">
            <div>
              <p class="settings-kicker">{{ t("session.kicker") }}</p>
              <h2 id="session-settings-title" class="settings-section-title">{{ t("session.title") }}</h2>
            </div>
            <div class="settings-group-actions">
              <button class="ghost-button" type="button" :disabled="!presetDraft.dirty.value" @click="onPresetCancel">{{ t("actions.cancel") }}</button>
              <button class="ghost-button ghost-button-primary" type="button" :disabled="!presetDraft.dirty.value" @click="onPresetSave">{{ t("actions.save") }}</button>
            </div>
          </div>
          <div class="field-grid preset-duration-grid">
            <label v-for="(d, i) in presetDraft.draft.value" :key="i" class="field field-compact">
              <span>{{ t("session.preset", { index: i + 1 }) }}</span>
              <div class="input-with-unit">
                <input type="number" min="1" max="180" step="1" :value="Math.round(d / 60_000)" @input="onDraftDurationChange(i, $event.target.value)" />
                <span class="input-unit">{{ t("session.minutesShort") }}</span>
              </div>
            </label>
          </div>
          <p v-if="presetSaveError" class="settings-error">{{ presetSaveError }}</p>
        </section>

        <section class="settings-section" v-if="shortcutDraft.draft.value" aria-labelledby="controls-settings-title">
          <div class="settings-section-header">
            <div>
              <p class="settings-kicker">{{ t("shortcuts.kicker") }}</p>
              <h2 id="controls-settings-title" class="settings-section-title">{{ t("shortcuts.title") }}</h2>
            </div>
            <div class="settings-group-actions">
              <span class="settings-group-meta">{{ t("settings.configuredCount", { count: configuredShortcutCount }) }}</span>
              <button class="text-button" type="button" :aria-expanded="isShortcutSectionOpen" @click="isShortcutSectionOpen = !isShortcutSectionOpen">
                {{ isShortcutSectionOpen ? t("actions.hide") : t("actions.show") }}
              </button>
              <button class="ghost-button" type="button" :disabled="!shortcutDraft.dirty.value" @click="onShortcutCancel">{{ t("actions.cancel") }}</button>
            </div>
          </div>
          <p v-if="shortcutHasConflict" class="settings-error">{{ conflictBannerText }}</p>
          <p v-if="!isShortcutSectionOpen" class="settings-section-note">
            {{ t("shortcuts.sectionNote") }}
          </p>
          <div v-if="isShortcutSectionOpen" class="shortcut-list">
            <div v-for="action in SHORTCUT_ACTIONS" :key="action" :class="['shortcut-row', { 'is-invalid': shortcutConflicts.has(action) }]">
              <span class="shortcut-label">{{ t(shortcutLabels[action]) }}</span>
              <KeyBindingInput
                :model-value="shortcutDraft.draft.value[action]"
                :placeholder="t('shortcuts.placeholder')"
                :invalid="shortcutConflicts.has(action)"
                @update:model-value="setShortcutBinding(action, $event)"
              />
            </div>
          </div>
          <p v-if="shortcutSaveError" class="settings-error">{{ shortcutSaveError }}</p>
        </section>

        <section class="settings-section" aria-labelledby="app-settings-title">
          <div class="settings-section-header">
            <div>
              <p class="settings-kicker">{{ t("appearance.kicker") }}</p>
              <h2 id="app-settings-title" class="settings-section-title">{{ t("appearance.title") }}</h2>
            </div>
            <span class="settings-group-meta">{{ t("settings.changesApplyImmediately") }}</span>
          </div>

          <div class="settings-subsection">
            <div class="settings-group-header">
              <span class="settings-group-title">{{ t("appearance.theme") }}</span>
              <span class="settings-group-meta">{{ themeModeSummary }}</span>
            </div>
            <div class="cue-style-list" role="radiogroup" :aria-label="t('appearance.themeModeAria')">
              <button v-for="option in themeModeOptions" :key="option.id" :class="['cue-style-card', { active: themeMode === option.id }]" type="button" role="radio" :aria-checked="themeMode === option.id" @click="setThemeMode(option.id)">
                <strong>{{ option.label }}</strong><span>{{ option.description }}</span>
              </button>
            </div>
          </div>

          <div class="settings-subsection">
            <div class="settings-group-header">
              <span class="settings-group-title">{{ t("language.label") }}</span>
            </div>
            <div class="locale-option-list" role="radiogroup" :aria-label="t('language.label')">
              <button
                v-for="option in localeOptions"
                :key="option.id"
                :class="['locale-option', { active: localePreference === option.id }]"
                type="button"
                role="radio"
                :aria-checked="localePreference === option.id"
                :disabled="isUpdatingLocale"
                @click="onLocalePreferenceChange({ target: { value: option.id } })"
              >
                {{ option.label }}
              </button>
            </div>
          </div>

          <div class="settings-subsection">
            <div class="settings-group-header">
              <span class="settings-group-title">{{ t("appearance.startup") }}</span>
            </div>
            <div class="startup-toggle-list">
              <label class="field startup-toggle">
                <input type="checkbox" :checked="autostartEnabled" @change="setAutostart($event.target.checked)" />
                <span>
                  <strong>{{ t("appearance.launchAtLogin") }}</strong>
                  <small>{{ t("appearance.launchAtLoginDescription") }}</small>
                </span>
              </label>
              <label class="field startup-toggle">
                <input type="checkbox" :checked="silentStart" @change="setSilentStart($event.target.checked)" />
                <span>
                  <strong>{{ t("appearance.silentStart") }}</strong>
                  <small>{{ t("appearance.silentStartDescription") }}</small>
                </span>
              </label>
            </div>
          </div>
        </section>

        <section class="settings-section" aria-labelledby="about-settings-title">
          <div class="settings-section-header">
            <div>
              <p class="settings-kicker">{{ t("about.kicker") }}</p>
              <h2 id="about-settings-title" class="settings-section-title">{{ t("about.title") }}</h2>
            </div>
          </div>

          <div class="about-panel">
            <div class="about-project">
              <img class="about-mark" :src="softwaneMark" alt="" aria-hidden="true" />
              <div class="about-copy">
                <strong>{{ t("app.tagline") }}</strong>
                <span>{{ t("about.description") }}</span>
              </div>
              <button class="ghost-button ghost-button-primary about-link" type="button" @click="openProjectPage">
                {{ t("actions.github") }}
              </button>
            </div>

            <div class="contributor-list" :aria-label="t('about.contributorsAria')">
              <div class="settings-group-header">
                <span class="settings-group-title">{{ t("about.contributors") }}</span>
                <span class="settings-group-meta">{{ t("about.active") }}</span>
              </div>
              <div class="contributor-links">
                <button
                  v-for="contributor in contributors"
                  :key="contributor.username"
                  class="contributor-link"
                  type="button"
                  @click="openContributorProfile(contributor.username)"
                >
                  @{{ contributor.username }}
                </button>
                <span class="contributor-note">{{ t("about.andMore") }}</span>
              </div>
            </div>
          </div>
        </section>
      </div>
    </section>
  </main>
</template>
