<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import erodeMark from "./assets/erode-mark.svg";
import { useAppearance } from "./composables/useAppearance";
import { useTimerSession } from "./composables/useTimerSession";

const isSettingsOpen = ref(false);
const bodyMatrixValues = ref(identityMatrix().map((v) => v.toFixed(6)).join(" "));
let bodyMatrix = identityMatrix();
let bodyMatrixFrame = null;

const cueStyleOptions = [
  {
    id: "dim",
    label: "Dim fade",
    description: "Make the screen visibly darker and ashier."
  },
  {
    id: "warm",
    label: "Warm drift",
    description: "Balanced warmth and fade for everyday use."
  },
  {
    id: "full",
    label: "Full erosion",
    description: "Push warmth, dimming, and washout hardest."
  }
];

const themeModeOptions = [
  {
    id: "auto",
    label: "Auto",
    description: "Follow the system appearance and switch live."
  },
  {
    id: "dark",
    label: "Dark",
    description: "Always keep the app in dark mode."
  },
  {
    id: "light",
    label: "Light",
    description: "Always keep the app in light mode."
  }
];

const {
  resolvedTheme,
  setThemeMode,
  themeMode
} = useAppearance();

const {
  workDuration,
  channelConfigs,
  startSession,
  takeBreakNow,
  startReverse,
  currentPhase,
  isIdle,
  isForward,
  isSettling,
  isSabi,
  isReverse,
  hasActiveSession,
  isWorkDurationSupported,
  displayTime,
  progress,
  phaseLabel,
  frame,
} = useTimerSession();

const channelEnabled = computed(() => {
  const set = new Set(channelConfigs.value.map((c) => c.channel_type));
  return {
    saturation: set.has("saturation"),
    warmth: set.has("warmth"),
    brightness: set.has("brightness"),
  };
});

function toggleChannel(type, defaultConfig) {
  const idx = channelConfigs.value.findIndex((c) => c.channel_type === type);
  if (idx >= 0) {
    channelConfigs.value = channelConfigs.value.filter((_, i) => i !== idx);
  } else {
    channelConfigs.value = [...channelConfigs.value, defaultConfig];
  }
}

const phaseTone = computed(() => {
  switch (currentPhase.value) {
    case "forward":
      return "phase-forward";
    case "settling":
      return "phase-settling";
    case "sabi":
      return "phase-sabi";
    case "reverse":
      return "phase-reverse";
    default:
      return "phase-idle";
  }
});

const showTimerValue = computed(() => isForward.value || isSettling.value || isIdle.value);

const progressStyle = computed(() => ({
  transform: `scaleX(${progress.value})`,
}));

const secondaryStatusLine = computed(() => {
  if (isSettling.value) return "Settling into rest";
  if (isReverse.value) return "Recovering to neutral";
  if (isSabi.value) return "Press hotkey to return";
  return "";
});

const themeModeSummary = computed(() => {
  if (themeMode.value === "auto") {
    return `Auto, ${resolvedTheme.value === "dark" ? "dark now" : "light now"}`;
  }

  return `${resolvedTheme.value === "dark" ? "Dark" : "Light"} fixed`;
});

function handleProgressScrub(value) {
  if (
    sessionStage.value !== "Work" ||
    sessionStatus.value !== "Running" ||
    isEndingEarly.value ||
    sessionIsEarlyEnding.value
  ) {
    return;
  }

  void setSessionProgressPercent(value);
}
const workDurationMessage = computed(() => {
  if (isWorkDurationSupported.value) return "";
  return "Sessions shorter than 2 minutes are unsupported.";
});

const canTakeBreak = computed(() => isForward.value);
const canstartReverse = computed(() => isForward.value || isSettling.value || isSabi.value);

const isCueActive = computed(() => hasActiveSession.value && !isIdle.value);

// --- Color matrix for in-window preview ---

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function identityMatrix() {
  return [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0];
}

function multiplyColorMatrices(left, right) {
  const result = new Array(20).fill(0);
  for (let row = 0; row < 4; row++) {
    for (let col = 0; col < 5; col++) {
      const i = row * 5 + col;
      if (col === 4) {
        result[i] =
          left[row * 5 + 4] +
          left[row * 5] * right[4] +
          left[row * 5 + 1] * right[9] +
          left[row * 5 + 2] * right[14] +
          left[row * 5 + 3] * right[19];
      } else {
        result[i] =
          left[row * 5] * right[col] +
          left[row * 5 + 1] * right[5 + col] +
          left[row * 5 + 2] * right[10 + col] +
          left[row * 5 + 3] * right[15 + col];
      }
    }
  }
  return result;
}

function interpolateMatrices(from, to, t) {
  return from.map((v, i) => v + (to[i] - v) * t);
}

function easeInOut(t) {
  return t * t * (3 - 2 * t);
}

function saturationMatrix(amount) {
  const r = 0.2126, g = 0.7152, b = 0.0722;
  const inv = 1 - amount;
  return [
    inv * r + amount, inv * g, inv * b, 0, 0,
    inv * r, inv * g + amount, inv * b, 0, 0,
    inv * r, inv * g, inv * b + amount, 0, 0,
    0, 0, 0, 1, 0,
  ];
}

function warmthMatrix(amount) {
  return [
    1 + amount * 0.16, 0, 0, 0, 0,
    0, 1 + amount * 0.02, 0, 0, 0,
    0, 0, 1 - amount * 0.22, 0, 0,
    0, 0, 0, 1, 0,
  ];
}

function brightnessMatrix(amount) {
  return [
    amount, 0, 0, 0, 0,
    0, amount, 0, 0, 0,
    0, 0, amount, 0, 0,
    0, 0, 0, 1, 0,
  ];
}

function kelvinToWarmth(k) {
  return clamp((6500 - k) / 4500, 0, 1);
}

const targetBodyMatrix = computed(() => {
  const f = frame.value;
  const warmth = kelvinToWarmth(f.warmthKelvin ?? 6500);
  const sat = clamp(f.saturation ?? 1, 0, 1);
  const bright = clamp(f.brightness ?? 1, 0, 1);

  return [brightnessMatrix(bright), warmthMatrix(warmth), saturationMatrix(sat)].reduce(
    (acc, m) => multiplyColorMatrices(acc, m),
    identityMatrix()
  );
});

const resolvedBodyMatrix = computed(() =>
  isCueActive.value ? targetBodyMatrix.value : identityMatrix()
);

function stopBodyMatrixAnimation() {
  if (bodyMatrixFrame) {
    window.cancelAnimationFrame(bodyMatrixFrame);
    bodyMatrixFrame = null;
  }
}

function updateBodyMatrixValues(matrix) {
  bodyMatrix = matrix;
  bodyMatrixValues.value = matrix.map((v) => v.toFixed(6)).join(" ");
}

function animateBodyMatrix(target, durationMs) {
  stopBodyMatrixAnimation();
  const start = [...bodyMatrix];
  const dist = start.reduce((t, v, i) => t + Math.abs(v - target[i]), 0);
  if (dist < 0.0001) {
    updateBodyMatrixValues(target);
    return;
  }
  const t0 = window.performance.now();
  const tick = (now) => {
    const p = clamp((now - t0) / durationMs, 0, 1);
    updateBodyMatrixValues(interpolateMatrices(start, target, easeInOut(p)));
    if (p < 1) {
      bodyMatrixFrame = window.requestAnimationFrame(tick);
    } else {
      bodyMatrixFrame = null;
    }
  };
  bodyMatrixFrame = window.requestAnimationFrame(tick);
}

watch(
  resolvedBodyMatrix,
  (matrix) => {
    const isLive = isForward.value || isSettling.value || isReverse.value;
    animateBodyMatrix(matrix, isLive ? 300 : 2800);
  },
  { immediate: true }
);

function handlestartSession() {
  startSession();
}

onMounted(() => {
  document.body.style.filter = "url(#erode-color-filter)";

  if (window.__TAURI__) {
    const { listen: tauriListen } = window.__TAURI__.event;
    tauriListen("tray-take-break", () => takeBreakNow());
    tauriListen("tray-start-reverse", () => startReverse());
  }
});

onUnmounted(() => {
  stopBodyMatrixAnimation();
  document.body.style.filter = "";
});
</script>

<template>
  <svg width="0" height="0" aria-hidden="true" class="visual-filter-defs">
    <filter id="erode-color-filter" color-interpolation-filters="sRGB">
      <feColorMatrix type="matrix" :values="bodyMatrixValues" />
    </filter>
  </svg>

  <main class="app-shell">
    <section class="timer-app">
      <header class="topbar">
        <img class="brand-mark" :src="erodeMark" alt="Erode" />
        <button
          class="ghost-button ghost-button-utility"
          type="button"
          @click="isSettingsOpen = true"
        >
          Tune
        </button>
      </header>

      <section class="timer-core">
        <span :class="['phase-dot', phaseTone]"></span>
        <p class="status-line">{{ phaseLabel }}</p>
        <h1
          :class="['timer-value', { 'is-hidden-in-break': !showTimerValue }]"
        >
          {{ showTimerValue ? displayTime : "\u00A0" }}
        </h1>
        <p
          :class="[
            'status-line',
            'status-line-secondary',
            { 'is-empty': !secondaryStatusLine },
          ]"
        >
          {{ secondaryStatusLine || "\u00A0" }}
        </p>
      </section>

      <section class="progress-panel">
        <div class="progress-track">
          <div class="progress-fill" :style="progressStyle"></div>
        </div>
      </section>

      <section class="action-row">
        <button
          v-if="canTakeBreak"
          key="take-break"
          class="action-button action-primary"
          type="button"
          @click="takeBreakNow"
        >
          Take a break now
        </button>
        <button
          v-if="canstartReverse"
          key="stop"
          class="action-button"
          type="button"
          @click="startReverse"
        >
          Stop
        </button>
        <button
          v-if="canstartReverse"
          key="start-reverse"
          class="action-button action-primary"
          type="button"
          @click="startReverse"
        >
          Return from break
        </button>
      </section>
    </section>

    <!-- Start layer (shown when Idle) -->
    <section
      :class="['settings-layer', 'start-layer', { 'is-active': isIdle }]"
      role="dialog"
      :aria-hidden="!isIdle"
      :aria-modal="isIdle ? 'true' : 'false'"
      aria-label="Start timer"
    >
      <div class="settings-sheet">
        <header class="settings-header">
          <div>
            <p class="settings-kicker">Start timer</p>
            <h2 class="settings-title">Set this session first</h2>
          </div>
        </header>

        <div class="field-grid">
          <label class="field">
            <span>Work time</span>
            <div class="input-with-unit">
              <input
                v-model.number="workDuration"
                type="number"
                max="120"
                step="1"
              />
              <span class="input-unit">min</span>
            </div>
          </label>
        </div>

        <p v-if="workDurationMessage" class="field-hint field-hint-error">
          {{ workDurationMessage }}
        </p>

        <div class="channel-toggles">
          <label class="channel-toggle">
            <input
              type="checkbox"
              :checked="channelEnabled.saturation"
              @change="
                toggleChannel('saturation', {
                  channel_type: 'saturation',
                  target_saturation: 0.18,
                  curve_steepness: 10,
                  settle_duration_ms: 5000,
                })
              "
            />
            <span>Saturation</span>
          </label>
          <label class="channel-toggle">
            <input
              type="checkbox"
              :checked="channelEnabled.warmth"
              @change="
                toggleChannel('warmth', {
                  channel_type: 'warmth',
                  target_kelvin: 2500,
                  curve_steepness: 10,
                  settle_duration_ms: 5000,
                })
              "
            />
            <span>Warmth</span>
          </label>
          <label class="channel-toggle">
            <input
              type="checkbox"
              :checked="channelEnabled.brightness"
              @change="
                toggleChannel('brightness', {
                  channel_type: 'brightness',
                  target_brightness: 0.6,
                  curve_steepness: 8,
                  settle_duration_ms: 6000,
                })
              "
            />
            <span>Brightness</span>
          </label>
        </div>

        <button
          class="start-button"
          type="button"
          @click="handlestartSession"
        >
          Start session
        </button>
      </div>
    </section>

    <!-- Settings layer -->
    <section
      :class="[
        'settings-layer',
        'mode-settings-layer',
        { 'is-active': isSettingsOpen },
      ]"
      role="dialog"
      :aria-hidden="!isSettingsOpen"
      :aria-modal="isSettingsOpen ? 'true' : 'false'"
      aria-label="Channel settings"
      @click.self="isSettingsOpen = false"
    >
      <div class="settings-sheet settings-page">
        <header class="settings-header">
          <div>
            <p class="settings-kicker">Mode settings</p>
            <h2 class="settings-title">Appearance and cue</h2>
          </div>
          <button
            class="icon-button"
            type="button"
            aria-label="Close settings"
            @click="isSettingsOpen = false"
          >
            &times;
          </button>
        </header>

        <div class="channel-settings-list">
          <div class="channel-setting-card">
            <label class="field">
              <input
                type="checkbox"
                :checked="channelEnabled.saturation"
                @change="
                  toggleChannel('saturation', {
                    channel_type: 'saturation',
                    target_saturation: 0.18,
                    curve_steepness: 10,
                    settle_duration_ms: 5000,
                  })
                "
              />
              <strong>Saturation</strong>
            </label>
            <p class="channel-description">
              Gradually desaturates the screen toward grayscale.
            </p>
          </div>

          <div class="channel-setting-card">
            <label class="field">
              <input
                type="checkbox"
                :checked="channelEnabled.warmth"
                @change="
                  toggleChannel('warmth', {
                    channel_type: 'warmth',
                    target_kelvin: 2500,
                    curve_steepness: 10,
                    settle_duration_ms: 5000,
                  })
                "
              />
              <strong>Warmth</strong>
            </label>
            <p class="channel-description">
              Shifts color temperature toward a warm amber tone.
            </p>
          </div>
        </label>

        <section class="settings-group">
          <div class="settings-group-header">
            <span class="settings-group-title">Theme</span>
            <span class="settings-group-meta">{{ themeModeSummary }}</span>
          </div>

          <div class="cue-style-list" role="radiogroup" aria-label="Theme mode">
            <button
              v-for="option in themeModeOptions"
              :key="option.id"
              :class="['cue-style-card', { active: themeMode === option.id }]"
              type="button"
              role="radio"
              :aria-checked="themeMode === option.id"
              @click="setThemeMode(option.id)"
            >
              <strong>{{ option.label }}</strong>
              <span>{{ option.description }}</span>
            </button>
          </div>
        </section>
      </div>
    </section>
  </main>
</template>
