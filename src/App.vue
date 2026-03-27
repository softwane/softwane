<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import erodeMark from "./assets/erode-mark.svg";
import { useTimerSession } from "./composables/useTimerSession";
import { animateNativeSnapshot, applyNativeSnapshot, getPreviewSnapshot } from "./preview";

const PAUSE_TRANSITION_DURATION_MS = 2800;
const NEUTRAL_SNAPSHOT = Object.freeze({
  phase: "Stable",
  saturation: 1,
  warmthKelvin: 6500,
  grayscale: 0
});

const isSettingsOpen = ref(false);
const isEndingEarly = ref(false);
const isResetting = ref(false);
const endingEarlySnapshot = ref(null);
const endingEarlyFrozenTime = ref("");
const snapshot = ref({
  phase: "Stable",
  saturation: 1,
  warmthKelvin: 6500,
  grayscale: 0
});
const isCueTransitioning = ref(false);
const lastAppliedNativeSnapshot = ref({ ...NEUTRAL_SNAPSHOT });
const bodyMatrixValues = ref(identityMatrix().map((value) => value.toFixed(6)).join(" "));
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

const {
  autoResumeEnabled,
  beginSession,
  cueStyle,
  displayTime,
  endSessionEarly,
  hasActiveSession,
  isCueSuppressed,
  isWorkDurationSupported,
  pauseTimeDisplay,
  pauseTimeoutMinutes,
  progress,
  previewRemainingFractionalMinutes,
  resetSession,
  sessionStage,
  sessionStatus,
  setCueStyle,
  setRemainingMinutes,
  statusLine,
  workDuration,
  pauseSession
} = useTimerSession();

const progressPercent = computed(() => clamp(progress.value * 100, 0, 100));

function handleProgressScrub(value) {
  if (sessionStage.value !== "Work") {
    return;
  }

  const clampedPercent = clamp(value, 0, 100);
  const remainingMinutes = (1 - clampedPercent / 100) * Math.max(workDuration.value, 0);
  setRemainingMinutes(remainingMinutes);
}

const effectiveSnapshot = computed(() => {
  if (isEndingEarly.value && endingEarlySnapshot.value) {
    return endingEarlySnapshot.value;
  }

  if (sessionStage.value === "Break") {
    return {
      ...snapshot.value,
      phase: "Statue"
    };
  }

  return snapshot.value;
});

const phaseTone = computed(() => {
  switch (effectiveSnapshot.value.phase) {
    case "JND":
      return "phase-jnd";
    case "Evolution":
      return "phase-evolution";
    case "Statue":
      return "phase-statue";
    case "Recovery":
      return "phase-recovery";
    default:
      return "phase-stable";
  }
});

const displayPhaseLabel = computed(() => {
  if (effectiveSnapshot.value.phase === "JND") {
    return "Soft shift";
  }

  if (effectiveSnapshot.value.phase === "Statue") {
    return "Rest";
  }

  return effectiveSnapshot.value.phase;
});

const displayStatusLabel = computed(() => {
  if (sessionStatus.value === "Break") {
    return "Settled";
  }

  return statusLine.value;
});

const showTimerValue = computed(() => sessionStage.value !== "Break" || isEndingEarly.value);
const visualDisplayTime = computed(() => (
  isEndingEarly.value ? endingEarlyFrozenTime.value : displayTime.value
));

const progressStyle = computed(() => ({
  transform: `scaleX(${progress.value})`
}));

const isStartLayerOpen = computed(() => !hasActiveSession.value);
const isCueEnabled = computed(
  () => hasActiveSession.value && !isCueSuppressed.value && !isResetting.value
);
const canPause = computed(
  () =>
    !isResetting.value &&
    !isEndingEarly.value &&
    (sessionStage.value === "Work" || sessionStatus.value === "Paused")
);
const canEndEarly = computed(
  () => !isResetting.value && !isEndingEarly.value && sessionStage.value === "Work"
);
const secondaryStatusLine = computed(() => {
  if (isResetting.value) {
    return "Returning to setup";
  }

  if (isEndingEarly.value) {
    return "Ending gently";
  }

  if (sessionStatus.value === "Paused") {
    return autoResumeEnabled.value
      ? `Silently resumes in ${pauseTimeDisplay.value}`
      : "Paused until you resume";
  }

  if (isCueTransitioning.value) {
    return "Transitioning gently";
  }

  return "";
});
const workDurationMessage = computed(() => {
  if (isWorkDurationSupported.value) {
    return "";
  }

  return "Sessions shorter than 2 minutes are unsupported.";
});

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function kelvinToWarmth(value) {
  return clamp((6500 - value) / 4000, 0, 1);
}

function cueStyleModifiers(style) {
  switch (style) {
    case "dim":
      return { warmth: 0.2, grayscale: 1.05, saturation: 0.72 };
    case "full":
      return { warmth: 1.15, grayscale: 1.15, saturation: 0.9 };
    default:
      return { warmth: 1, grayscale: 0.72, saturation: 1 };
  }
}

function identityMatrix() {
  return [
    1, 0, 0, 0, 0,
    0, 1, 0, 0, 0,
    0, 0, 1, 0, 0,
    0, 0, 0, 1, 0
  ];
}

function multiplyColorMatrices(left, right) {
  const result = new Array(20).fill(0);

  for (let row = 0; row < 4; row += 1) {
    for (let col = 0; col < 5; col += 1) {
      const index = row * 5 + col;

      if (col === 4) {
        result[index] =
          left[row * 5 + 4] +
          left[row * 5] * right[4] +
          left[row * 5 + 1] * right[9] +
          left[row * 5 + 2] * right[14] +
          left[row * 5 + 3] * right[19];
      } else {
        result[index] =
          left[row * 5] * right[col] +
          left[row * 5 + 1] * right[5 + col] +
          left[row * 5 + 2] * right[10 + col] +
          left[row * 5 + 3] * right[15 + col];
      }
    }
  }

  return result;
}

function interpolateMatrices(from, to, amount) {
  return from.map((value, index) => value + (to[index] - value) * amount);
}

function easeInOut(amount) {
  return amount * amount * (3 - 2 * amount);
}

function saturationMatrix(amount) {
  const r = 0.2126;
  const g = 0.7152;
  const b = 0.0722;
  const inv = 1 - amount;

  return [
    inv * r + amount, inv * g, inv * b, 0, 0,
    inv * r, inv * g + amount, inv * b, 0, 0,
    inv * r, inv * g, inv * b + amount, 0, 0,
    0, 0, 0, 1, 0
  ];
}

function warmthMatrix(amount) {
  return [
    1 + amount * 0.16, 0, 0, 0, 0,
    0, 1 + amount * 0.02, 0, 0, 0,
    0, 0, 1 - amount * 0.22, 0, 0,
    0, 0, 0, 1, 0
  ];
}

function brightnessMatrix(amount) {
  return [
    amount, 0, 0, 0, 0,
    0, amount, 0, 0, 0,
    0, 0, amount, 0, 0,
    0, 0, 0, 1, 0
  ];
}

const targetBodyMatrix = computed(() => {
  const modifiers = cueStyleModifiers(cueStyle.value);
  const warmth = clamp(kelvinToWarmth(effectiveSnapshot.value.warmthKelvin) * modifiers.warmth, 0, 1);
  const saturation = clamp(effectiveSnapshot.value.saturation * modifiers.saturation, 0, 1);
  const grayscale = clamp(effectiveSnapshot.value.grayscale * modifiers.grayscale, 0, 1);
  const brightness = 1 - warmth * 0.06 - grayscale * 0.08;
  const chroma = clamp(saturation * (1 - grayscale), 0, 1);

  const matrix = [
    brightnessMatrix(brightness),
    warmthMatrix(warmth),
    saturationMatrix(chroma)
  ].reduce((combined, current) => multiplyColorMatrices(combined, current), identityMatrix());

  return matrix;
});

const resolvedBodyMatrix = computed(() => (
  isCueEnabled.value ? targetBodyMatrix.value : identityMatrix()
));

function stopBodyMatrixAnimation() {
  if (bodyMatrixFrame) {
    window.cancelAnimationFrame(bodyMatrixFrame);
    bodyMatrixFrame = null;
  }
}

function updateBodyMatrixValues(matrix) {
  bodyMatrix = matrix;
  bodyMatrixValues.value = matrix.map((value) => value.toFixed(6)).join(" ");
}

function animateBodyMatrix(targetMatrix, durationMs) {
  stopBodyMatrixAnimation();

  const startMatrix = [...bodyMatrix];
  const distance = startMatrix.reduce(
    (total, value, index) => total + Math.abs(value - targetMatrix[index]),
    0
  );

  if (distance < 0.0001) {
    updateBodyMatrixValues(targetMatrix);
    isCueTransitioning.value = false;
    return;
  }

  isCueTransitioning.value = true;
  const startedAt = window.performance.now();

  const tick = (now) => {
    const elapsed = now - startedAt;
    const progress = clamp(elapsed / durationMs, 0, 1);
    updateBodyMatrixValues(interpolateMatrices(startMatrix, targetMatrix, easeInOut(progress)));

    if (progress < 1) {
      bodyMatrixFrame = window.requestAnimationFrame(tick);
      return;
    }

    updateBodyMatrixValues(targetMatrix);
    bodyMatrixFrame = null;
    isCueTransitioning.value = false;
  };

  bodyMatrixFrame = window.requestAnimationFrame(tick);
}

function waitForMs(durationMs) {
  return new Promise((resolve) => {
    window.setTimeout(resolve, durationMs);
  });
}

async function handleResetSession() {
  if (isResetting.value || isEndingEarly.value) {
    return;
  }

  isSettingsOpen.value = false;
  isResetting.value = true;

  const fromSnapshot = {
    ...effectiveSnapshot.value
  };

  try {
    await animateNativeSnapshot(
      fromSnapshot,
      NEUTRAL_SNAPSHOT,
      cueStyle.value,
      PAUSE_TRANSITION_DURATION_MS
    );
  } finally {
    lastAppliedNativeSnapshot.value = { ...NEUTRAL_SNAPSHOT };
    resetSession();
    isResetting.value = false;
  }
}

function handleBeginSession() {
  if (isResetting.value || isEndingEarly.value) {
    return;
  }

  beginSession();
}

async function handleEndSessionEarly() {
  if (isEndingEarly.value || isResetting.value || sessionStage.value !== "Work") {
    return;
  }

  const payload = await getPreviewSnapshot(workDuration.value, 0, cueStyle.value);
  isEndingEarly.value = true;
  endingEarlyFrozenTime.value = displayTime.value;
  endingEarlySnapshot.value = payload.snapshot;
  lastAppliedNativeSnapshot.value = { ...payload.snapshot };
  endSessionEarly();
  await waitForMs(PAUSE_TRANSITION_DURATION_MS);
  endingEarlySnapshot.value = null;
  endingEarlyFrozenTime.value = "";
  isEndingEarly.value = false;
}

async function refreshPreview() {
  if (!hasActiveSession.value && !isWorkDurationSupported.value) {
    snapshot.value = { ...NEUTRAL_SNAPSHOT };
    return;
  }

  const payload = await getPreviewSnapshot(
    workDuration.value,
    previewRemainingFractionalMinutes.value,
    cueStyle.value
  );
  snapshot.value = payload.snapshot;

  if (isCueEnabled.value) {
    lastAppliedNativeSnapshot.value = { ...payload.snapshot };
  }
}

watch([previewRemainingFractionalMinutes, sessionStage, workDuration, cueStyle], () => {
  void refreshPreview();
});

watch(
  cueStyle,
  () => {
    if (!hasActiveSession.value || isCueSuppressed.value || isResetting.value) {
      return;
    }

    void applyNativeSnapshot(effectiveSnapshot.value, cueStyle.value);
  }
);

watch(
  isCueEnabled,
  (enabled, wasEnabled) => {
    if (isResetting.value || enabled === wasEnabled) {
      return;
    }

    if (!enabled) {
      const fromSnapshot = { ...lastAppliedNativeSnapshot.value };
      lastAppliedNativeSnapshot.value = { ...NEUTRAL_SNAPSHOT };
      void animateNativeSnapshot(
        fromSnapshot,
        NEUTRAL_SNAPSHOT,
        cueStyle.value,
        PAUSE_TRANSITION_DURATION_MS
      );
      return;
    }

    const nextSnapshot = { ...effectiveSnapshot.value };
    lastAppliedNativeSnapshot.value = nextSnapshot;
    void animateNativeSnapshot(
      NEUTRAL_SNAPSHOT,
      nextSnapshot,
      cueStyle.value,
      PAUSE_TRANSITION_DURATION_MS
    );
  }
);

watch(
  resolvedBodyMatrix,
  (matrix) => {
    animateBodyMatrix(matrix, PAUSE_TRANSITION_DURATION_MS);
  },
  { immediate: true }
);

onMounted(() => {
  document.body.style.filter = "url(#erode-color-filter)";
  void refreshPreview();
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
        <button class="ghost-button ghost-button-utility" type="button" @click="isSettingsOpen = true">
          Tune
        </button>
      </header>

      <section class="timer-core">
        <span :class="['phase-dot', phaseTone]"></span>
        <p class="status-line">{{ displayPhaseLabel }} · {{ displayStatusLabel }}</p>
        <h1
          :class="[
            'timer-value',
            { 'is-fading-out': isEndingEarly, 'is-hidden-in-break': !showTimerValue }
          ]"
        >
          {{ showTimerValue ? visualDisplayTime : "\u00A0" }}
        </h1>
        <p :class="['status-line', 'status-line-secondary', { 'is-empty': !secondaryStatusLine }]">
          {{ secondaryStatusLine || "\u00A0" }}
        </p>
      </section>

      <section class="progress-panel">
        <div class="progress-track">
          <div class="progress-fill" :style="progressStyle"></div>
        </div>
        <input
          class="progress-scrubber"
          type="range"
          min="0"
          max="100"
          step="0.1"
          :value="progressPercent"
          :disabled="sessionStage !== 'Work'"
          aria-label="Session progress"
          @input="handleProgressScrub(Number($event.target.value))"
        />
      </section>

      <TransitionGroup name="action-pill" tag="section" class="action-row">
        <button
          v-if="canPause"
          key="pause"
          class="action-button action-primary"
          type="button"
          @click="pauseSession"
        >
          {{ sessionStatus === "Paused" ? "Resume" : "Pause" }}
        </button>
        <button
          v-if="canEndEarly"
          key="end-early"
          class="action-button"
          type="button"
          @click="handleEndSessionEarly"
        >
          End early
        </button>
        <button
          v-if="hasActiveSession"
          key="reset"
          class="action-button"
          type="button"
          :disabled="isResetting"
          @click="handleResetSession"
        >
          Reset
        </button>
      </TransitionGroup>
    </section>

    <section
      :class="['settings-layer', 'start-layer', { 'is-active': isStartLayerOpen }]"
      role="dialog"
      :aria-hidden="!isStartLayerOpen"
      :aria-modal="isStartLayerOpen ? 'true' : 'false'"
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
              <input v-model.number="workDuration" type="number" max="120" step="1" />
              <span class="input-unit">min</span>
            </div>
          </label>
        </div>

        <p v-if="workDurationMessage" class="field-hint field-hint-error">
          {{ workDurationMessage }}
        </p>

        <button
          class="start-button"
          type="button"
          :disabled="isResetting || isEndingEarly"
          @click="handleBeginSession"
        >
          Start session
        </button>
      </div>
    </section>

    <section
      :class="['settings-layer', 'mode-settings-layer', { 'is-active': isSettingsOpen }]"
      role="dialog"
      :aria-hidden="!isSettingsOpen"
      :aria-modal="isSettingsOpen ? 'true' : 'false'"
      aria-label="Mode settings"
      @click.self="isSettingsOpen = false"
    >
      <div class="settings-sheet settings-page">
        <header class="settings-header">
          <div>
            <p class="settings-kicker">Mode settings</p>
            <h2 class="settings-title">How the cue appears</h2>
          </div>
          <button class="icon-button" type="button" aria-label="Close settings" @click="isSettingsOpen = false">
            ×
          </button>
        </header>

        <div class="cue-style-list">
          <button
            v-for="option in cueStyleOptions"
            :key="option.id"
            :class="['cue-style-card', { active: cueStyle === option.id }]"
            type="button"
            @click="setCueStyle(option.id)"
          >
            <strong>{{ option.label }}</strong>
            <span>{{ option.description }}</span>
          </button>
        </div>

        <label class="field">
          <span>
            <input v-model="autoResumeEnabled" type="checkbox" />
            Enable silent auto resume
          </span>
        </label>

        <label v-if="autoResumeEnabled" class="field">
          <span>Silent auto resume after</span>
          <div class="input-with-unit">
            <input v-model.number="pauseTimeoutMinutes" type="number" min="1" max="120" step="1" />
            <span class="input-unit">min</span>
          </div>
        </label>
      </div>
    </section>
  </main>
</template>
