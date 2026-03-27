import { computed, onMounted, onUnmounted, ref, watch } from "vue";

const STORAGE_KEY = "erode.timer-session";
const DEFAULT_PAUSE_TIMEOUT_MINUTES = 10;
const MIN_SUPPORTED_WORK_DURATION_MINUTES = 2;
const MAX_WORK_DURATION_MINUTES = 120;
const DEFAULT_CUE_STYLE = "warm";

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function clampDurationForStorage(value) {
  if (!Number.isFinite(value)) {
    return 50;
  }

  return clamp(value, 0, MAX_WORK_DURATION_MINUTES);
}

function formatClock(totalSeconds) {
  const safeSeconds = Math.max(totalSeconds, 0);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const seconds = safeSeconds % 60;

  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function normalizeCueStyle(value) {
  switch (value) {
    case "color":
      return "dim";
    case "dim":
    case "warm":
    case "full":
      return value;
    default:
      return DEFAULT_CUE_STYLE;
  }
}

export function useTimerSession() {
  const workDuration = ref(50);
  const workRemainingSeconds = ref(50 * 60);
  const autoResumeEnabled = ref(true);
  const pauseTimeoutMinutes = ref(DEFAULT_PAUSE_TIMEOUT_MINUTES);
  const cueStyle = ref(DEFAULT_CUE_STYLE);
  const sessionStage = ref("Idle");
  const isPaused = ref(false);
  const pauseRemainingSeconds = ref(0);
  let pauseResumeAt = null;
  let tickHandle = null;

  const hasActiveSession = computed(() => sessionStage.value !== "Idle");
  const isWorkDurationSupported = computed(
    () => Number.isFinite(workDuration.value) && workDuration.value >= MIN_SUPPORTED_WORK_DURATION_MINUTES
  );
  const currentSeconds = computed(() => {
    if (sessionStage.value === "Work") {
      return workRemainingSeconds.value;
    }

    if (sessionStage.value === "Break") {
      return 0;
    }

    return workDuration.value * 60;
  });
  const displayTime = computed(() => formatClock(currentSeconds.value));
  const remainingSeconds = computed(() => currentSeconds.value);
  const remainingMinutes = computed(() => Math.ceil(currentSeconds.value / 60));
  const workRemainingMinutes = computed(() => Math.ceil(workRemainingSeconds.value / 60));
  const workRemainingFractionalMinutes = computed(() => workRemainingSeconds.value / 60);
  const previewRemainingFractionalMinutes = computed(() => {
    if (sessionStage.value === "Work") {
      return workRemainingFractionalMinutes.value;
    }

    if (sessionStage.value === "Break") {
      return 0;
    }

    return workDuration.value;
  });
  const progress = computed(() => {
    if (sessionStage.value === "Work") {
      const total = Math.max(workDuration.value * 60, 1);
      return Math.max(0, Math.min(1, (total - workRemainingSeconds.value) / total));
    }

    if (sessionStage.value === "Break") {
      return 1;
    }

    return 0;
  });
  const sessionStatus = computed(() => {
    if (isPaused.value) {
      return "Paused";
    }

    switch (sessionStage.value) {
      case "Work":
        return "Running";
      case "Break":
        return "Break";
      default:
        return "Idle";
    }
  });
  const statusLine = computed(() => sessionStatus.value.toLowerCase());
  const isCueSuppressed = computed(() => isPaused.value || sessionStage.value === "Idle");
  const pauseTimeDisplay = computed(() => formatClock(pauseRemainingSeconds.value));

  function hydrateFromStorage() {
    const raw = window.localStorage.getItem(STORAGE_KEY);

    if (!raw) {
      return;
    }

    try {
      const saved = JSON.parse(raw);

      if (typeof saved.workDuration === "number") {
        workDuration.value = clampDurationForStorage(saved.workDuration);
      }

      if (typeof saved.cueStyle === "string") {
        cueStyle.value = normalizeCueStyle(saved.cueStyle);
      }

      if (typeof saved.autoResumeEnabled === "boolean") {
        autoResumeEnabled.value = saved.autoResumeEnabled;
      }

      if (typeof saved.pauseTimeoutMinutes === "number") {
        pauseTimeoutMinutes.value = clamp(saved.pauseTimeoutMinutes, 1, 120);
      }
    } catch {
      window.localStorage.removeItem(STORAGE_KEY);
    }
  }

  function persistSessionConfig() {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        workDuration: clampDurationForStorage(workDuration.value),
        cueStyle: cueStyle.value,
        autoResumeEnabled: autoResumeEnabled.value,
        pauseTimeoutMinutes: pauseTimeoutMinutes.value
      })
    );
  }

  function resetWorkTimeline() {
    workRemainingSeconds.value = Math.max(workDuration.value, 0) * 60;
  }

  function beginSession() {
    if (!isWorkDurationSupported.value) {
      return false;
    }

    resetWorkTimeline();
    sessionStage.value = "Work";
    isPaused.value = false;
    pauseRemainingSeconds.value = 0;
    pauseResumeAt = null;
    return true;
  }

  function enterBreak() {
    workRemainingSeconds.value = 0;
    sessionStage.value = "Break";
    isPaused.value = false;
    pauseRemainingSeconds.value = 0;
    pauseResumeAt = null;
  }

  function setCueStyle(value) {
    cueStyle.value = normalizeCueStyle(value);
  }

  function pauseSession() {
    if (sessionStage.value !== "Work" && !isPaused.value) {
      return;
    }

    if (isPaused.value) {
      isPaused.value = false;
      pauseRemainingSeconds.value = 0;
      pauseResumeAt = null;
      return;
    }

    isPaused.value = true;
    pauseResumeAt = autoResumeEnabled.value
      ? Date.now() + pauseTimeoutMinutes.value * 60 * 1000
      : null;
    syncPauseCountdown();
  }

  function resetSession() {
    resetWorkTimeline();
    sessionStage.value = "Idle";
    isPaused.value = false;
    pauseRemainingSeconds.value = 0;
    pauseResumeAt = null;
  }

  function endSessionEarly() {
    if (sessionStage.value === "Work") {
      enterBreak();
    }
  }

  function setRemainingMinutes(value) {
    workRemainingSeconds.value = clamp(value * 60, 0, Math.max(workDuration.value, 0) * 60);
  }

  function syncPauseCountdown() {
    if (!pauseResumeAt) {
      pauseRemainingSeconds.value = 0;
      return;
    }

    pauseRemainingSeconds.value = Math.max(0, Math.ceil((pauseResumeAt - Date.now()) / 1000));
  }

  function tickWorkTimeline() {
    if (workRemainingSeconds.value > 0) {
      workRemainingSeconds.value -= 1;
    }

    if (workRemainingSeconds.value <= 0) {
      workRemainingSeconds.value = 0;
      enterBreak();
    }
  }

  function startTimer() {
    if (tickHandle) {
      clearInterval(tickHandle);
    }

    tickHandle = window.setInterval(() => {
      if (sessionStage.value === "Idle") {
        return;
      }

      if (isPaused.value) {
        syncPauseCountdown();

        if (autoResumeEnabled.value && pauseRemainingSeconds.value <= 0) {
          isPaused.value = false;
          pauseRemainingSeconds.value = 0;
          pauseResumeAt = null;
        }

        return;
      }

      if (sessionStage.value === "Work") {
        tickWorkTimeline();
        return;
      }
    }, 1000);
  }

  onMounted(() => {
    hydrateFromStorage();
    resetWorkTimeline();
    startTimer();
  });

  onUnmounted(() => {
    if (tickHandle) {
      clearInterval(tickHandle);
    }
  });

  watch(workDuration, (value) => {
    const safeDuration = Math.max(value, 0);
    workRemainingSeconds.value = clamp(workRemainingSeconds.value, 0, safeDuration * 60);
  });

  watch(pauseTimeoutMinutes, (value) => {
    pauseTimeoutMinutes.value = clamp(value, 1, 120);

    if (isPaused.value && autoResumeEnabled.value) {
      pauseResumeAt = Date.now() + pauseTimeoutMinutes.value * 60 * 1000;
      syncPauseCountdown();
    }
  });

  watch(autoResumeEnabled, (value) => {
    if (isPaused.value) {
      pauseResumeAt = value ? Date.now() + pauseTimeoutMinutes.value * 60 * 1000 : null;
      syncPauseCountdown();
    }
  });

  watch([workDuration, cueStyle, autoResumeEnabled, pauseTimeoutMinutes], () => {
    persistSessionConfig();
  });

  return {
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
    remainingMinutes,
    remainingSeconds,
    resetSession,
    sessionStage,
    sessionStatus,
    setCueStyle,
    setRemainingMinutes,
    statusLine,
    workDuration,
    pauseSession
  };
}
