import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { deriveLocalSnapshot } from "../preview";

const STORAGE_KEY = "erode.timer-session";
const SESSION_EVENT = "timer-session-updated";
const DEFAULT_WORK_DURATION_MINUTES = 50;
const DEFAULT_PAUSE_TIMEOUT_MINUTES = 10;
const MIN_SUPPORTED_WORK_DURATION_MINUTES = 2;
const MAX_WORK_DURATION_MINUTES = 120;
const DEFAULT_CUE_STYLE = "warm";
const FALLBACK_TICK_MS = 250;

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function clampDurationForStorage(value) {
  if (!Number.isFinite(value)) {
    return DEFAULT_WORK_DURATION_MINUTES;
  }

  return clamp(value, 0, MAX_WORK_DURATION_MINUTES);
}

function formatClock(totalSeconds) {
  const safeSeconds = Math.max(totalSeconds, 0);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const seconds = Math.floor(safeSeconds % 60);

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

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function neutralSnapshot() {
  return {
    phase: "Stable",
    saturation: 1,
    warmthKelvin: 6500,
  };
}

function normalizeSnapshot(snapshot) {
  if (!snapshot) {
    return neutralSnapshot();
  }

  const phaseMap = {
    stable: "Stable",
    jnd: "JND",
    evolution: "Evolution",
    statue: "Statue",
    recovery: "Recovery"
  };
  const normalizedPhase = String(snapshot.phase ?? "")
    .trim()
    .toLowerCase();

  return {
    phase: phaseMap[normalizedPhase] ?? snapshot.phase ?? "Stable",
    saturation: snapshot.saturation ?? 1,
    warmthKelvin: snapshot.warmthKelvin ?? snapshot.warmth_kelvin ?? 6500,
  };
}

function nowMs() {
  return Date.now();
}

function ceilSeconds(durationMs) {
  if (durationMs <= 0) {
    return 0;
  }

  return Math.ceil(durationMs / 1000);
}

export function useTimerSession() {
  const workDuration = ref(DEFAULT_WORK_DURATION_MINUTES);
  const autoResumeEnabled = ref(true);
  const pauseTimeoutMinutes = ref(DEFAULT_PAUSE_TIMEOUT_MINUTES);
  const cueStyle = ref(DEFAULT_CUE_STYLE);
  const sessionStage = ref("Idle");
  const sessionStatus = ref("Idle");
  const sessionRemainingSeconds = ref(DEFAULT_WORK_DURATION_MINUTES * 60);
  const pauseRemainingSeconds = ref(0);
  const isEarlyEnding = ref(false);
  const isCueTransitioning = ref(false);
  const sessionSnapshot = ref(neutralSnapshot());
  let unlistenSession = null;
  let isApplyingBackendState = false;

  let fallbackTickHandle = null;
  let fallbackWorkEndAt = null;
  let fallbackPausedRemainingMs = 0;
  let fallbackPauseResumeAt = null;

  const hasActiveSession = computed(() => sessionStage.value !== "Idle");
  const isWorkDurationSupported = computed(
    () => Number.isFinite(workDuration.value) && workDuration.value >= MIN_SUPPORTED_WORK_DURATION_MINUTES
  );
  const currentSeconds = computed(() => {
    if (sessionStage.value === "Work") {
      return sessionRemainingSeconds.value;
    }

    if (sessionStage.value === "Break") {
      return 0;
    }

    return Math.max(workDuration.value, 0) * 60;
  });
  const displayTime = computed(() => formatClock(currentSeconds.value));
  const remainingSeconds = computed(() => currentSeconds.value);
  const progress = computed(() => {
    if (sessionStage.value === "Work") {
      const total = Math.max(workDuration.value * 60, 1);
      return Math.max(0, Math.min(1, (total - sessionRemainingSeconds.value) / total));
    }

    if (sessionStage.value === "Break") {
      return 1;
    }

    return 0;
  });
  const statusLine = computed(() => sessionStatus.value.toLowerCase());
  const isCueSuppressed = computed(() => sessionStatus.value === "Paused" || sessionStage.value === "Idle");
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

  function setCueStyle(value) {
    cueStyle.value = normalizeCueStyle(value);
  }

  function applySessionPayload(payload) {
    if (!payload) {
      return;
    }

    isApplyingBackendState = true;
    sessionStage.value = payload.sessionStage ?? "Idle";
    sessionStatus.value = payload.sessionStatus ?? "Idle";
    sessionRemainingSeconds.value = payload.remainingSeconds ?? 0;
    pauseRemainingSeconds.value = payload.pauseRemainingSeconds ?? 0;
    isEarlyEnding.value = Boolean(payload.isEarlyEnding);
    isCueTransitioning.value = Boolean(payload.isCueTransitioning);
    sessionSnapshot.value = normalizeSnapshot(payload.snapshot);

    if (payload.sessionStage !== "Idle" && typeof payload.workDurationMinutes === "number") {
      workDuration.value = clampDurationForStorage(payload.workDurationMinutes);
    }

    isApplyingBackendState = false;
  }

  async function invokeSession(command, payload = {}) {
    if (!hasTauriRuntime()) {
      return null;
    }

    try {
      return await invoke(command, payload);
    } catch {
      return null;
    }
  }

  async function syncSessionState() {
    const payload = await invokeSession("get_timer_session_state");
    applySessionPayload(payload);
  }

  async function beginSession() {
    if (!isWorkDurationSupported.value) {
      return false;
    }

    if (!hasTauriRuntime()) {
      beginFallbackSession();
      return true;
    }

    const payload = await invokeSession("start_timer_session", {
      workDurationMinutes: clampDurationForStorage(workDuration.value),
      cueStyle: cueStyle.value,
      autoResumeEnabled: autoResumeEnabled.value,
      pauseTimeoutMinutes: pauseTimeoutMinutes.value
    });

    applySessionPayload(payload);
    return payload?.sessionStage === "Work";
  }

  async function pauseSession() {
    if (!hasTauriRuntime()) {
      toggleFallbackPause();
      return;
    }

    const payload = await invokeSession("toggle_pause_timer_session", {
      autoResumeEnabled: autoResumeEnabled.value,
      pauseTimeoutMinutes: pauseTimeoutMinutes.value
    });

    applySessionPayload(payload);
  }

  async function resetSession() {
    if (!hasTauriRuntime()) {
      resetFallbackSession();
      return;
    }

    const payload = await invokeSession("reset_timer_session");
    applySessionPayload(payload);
  }

  async function endSessionEarly() {
    if (!hasTauriRuntime()) {
      endFallbackSessionEarly();
      return;
    }

    const payload = await invokeSession("end_timer_session_early");
    applySessionPayload(payload);
  }

  async function syncActiveSessionSettings() {
    if (isApplyingBackendState || !hasActiveSession.value || !hasTauriRuntime()) {
      return;
    }

    const payload = await invokeSession("update_timer_session_settings", {
      cueStyle: cueStyle.value,
      autoResumeEnabled: autoResumeEnabled.value,
      pauseTimeoutMinutes: pauseTimeoutMinutes.value
    });

    applySessionPayload(payload);
  }

  async function setSessionProgressPercent(value) {
    const clampedPercent = clamp(value, 0, 100);
    const remainingSeconds = Math.round((1 - clampedPercent / 100) * Math.max(workDuration.value, 0) * 60);

    if (!hasTauriRuntime()) {
      if (sessionStage.value !== "Work" || sessionStatus.value !== "Running" || isEarlyEnding.value) {
        return;
      }

      fallbackWorkEndAt = nowMs() + remainingSeconds * 1_000;
      syncFallbackSession();
      return;
    }

    const payload = await invokeSession("set_timer_session_remaining_seconds", {
      remainingSeconds
    });

    applySessionPayload(payload);
  }

  function syncFallbackSession() {
    if (sessionStage.value === "Idle") {
      sessionStatus.value = "Idle";
      sessionRemainingSeconds.value = workDuration.value * 60;
      pauseRemainingSeconds.value = 0;
      isEarlyEnding.value = false;
      isCueTransitioning.value = false;
      sessionSnapshot.value = neutralSnapshot();
      return;
    }

    if (sessionStage.value === "Break") {
      sessionStatus.value = "Break";
      sessionRemainingSeconds.value = 0;
      pauseRemainingSeconds.value = 0;
      isEarlyEnding.value = false;
      isCueTransitioning.value = false;
      sessionSnapshot.value = deriveLocalSnapshot(workDuration.value, 0);
      return;
    }

    if (sessionStatus.value === "Paused") {
      if (autoResumeEnabled.value && fallbackPauseResumeAt) {
        const pauseRemainingMs = Math.max(0, fallbackPauseResumeAt - nowMs());
        pauseRemainingSeconds.value = ceilSeconds(pauseRemainingMs);

        if (pauseRemainingMs <= 0) {
          sessionStatus.value = "Running";
          fallbackWorkEndAt = nowMs() + fallbackPausedRemainingMs;
          fallbackPauseResumeAt = null;
        }
      } else {
        pauseRemainingSeconds.value = 0;
      }

      sessionRemainingSeconds.value = ceilSeconds(fallbackPausedRemainingMs);
      sessionSnapshot.value = neutralSnapshot();
      return;
    }

    const remainingMs = Math.max(0, (fallbackWorkEndAt ?? nowMs()) - nowMs());
    sessionRemainingSeconds.value = ceilSeconds(remainingMs);
    pauseRemainingSeconds.value = 0;
    isEarlyEnding.value = false;
    isCueTransitioning.value = false;
    sessionSnapshot.value = deriveLocalSnapshot(workDuration.value, remainingMs / 60_000);

    if (remainingMs <= 0) {
      sessionStage.value = "Break";
      sessionStatus.value = "Break";
      sessionRemainingSeconds.value = 0;
      sessionSnapshot.value = deriveLocalSnapshot(workDuration.value, 0);
    }
  }

  function ensureFallbackTicker() {
    if (fallbackTickHandle) {
      return;
    }

    fallbackTickHandle = window.setInterval(() => {
      syncFallbackSession();
    }, FALLBACK_TICK_MS);
  }

  function beginFallbackSession() {
    sessionStage.value = "Work";
    sessionStatus.value = "Running";
    fallbackPausedRemainingMs = 0;
    fallbackPauseResumeAt = null;
    fallbackWorkEndAt = nowMs() + clampDurationForStorage(workDuration.value) * 60_000;
    syncFallbackSession();
    ensureFallbackTicker();
  }

  function toggleFallbackPause() {
    if (sessionStage.value !== "Work" && sessionStatus.value !== "Paused") {
      return;
    }

    if (sessionStatus.value === "Paused") {
      sessionStatus.value = "Running";
      fallbackWorkEndAt = nowMs() + fallbackPausedRemainingMs;
      fallbackPauseResumeAt = null;
      syncFallbackSession();
      return;
    }

    fallbackPausedRemainingMs = Math.max(0, (fallbackWorkEndAt ?? nowMs()) - nowMs());

    if (fallbackPausedRemainingMs <= 0) {
      endFallbackSessionEarly();
      return;
    }

    sessionStatus.value = "Paused";
    fallbackWorkEndAt = null;
    fallbackPauseResumeAt = autoResumeEnabled.value
      ? nowMs() + clamp(pauseTimeoutMinutes.value, 1, 120) * 60_000
      : null;
    syncFallbackSession();
  }

  function resetFallbackSession() {
    sessionStage.value = "Idle";
    sessionStatus.value = "Idle";
    fallbackWorkEndAt = null;
    fallbackPausedRemainingMs = 0;
    fallbackPauseResumeAt = null;
    syncFallbackSession();
  }

  function endFallbackSessionEarly() {
    sessionStage.value = "Break";
    sessionStatus.value = "Break";
    fallbackWorkEndAt = null;
    fallbackPausedRemainingMs = 0;
    fallbackPauseResumeAt = null;
    syncFallbackSession();
  }

  onMounted(async () => {
    hydrateFromStorage();

    if (hasTauriRuntime()) {
      unlistenSession = await listen(SESSION_EVENT, (event) => {
        applySessionPayload(event.payload);
      });

      await syncSessionState();
      return;
    }

    syncFallbackSession();
    ensureFallbackTicker();
  });

  onUnmounted(() => {
    if (typeof unlistenSession === "function") {
      unlistenSession();
      unlistenSession = null;
    }

    if (fallbackTickHandle) {
      clearInterval(fallbackTickHandle);
      fallbackTickHandle = null;
    }
  });

  watch(workDuration, (value) => {
    const safeDuration = clampDurationForStorage(value);

    if (safeDuration !== value) {
      workDuration.value = safeDuration;
      return;
    }

    if (!hasActiveSession.value && !hasTauriRuntime()) {
      syncFallbackSession();
    }
  });

  watch([cueStyle, autoResumeEnabled, pauseTimeoutMinutes], () => {
    if (isApplyingBackendState) {
      return;
    }

    persistSessionConfig();
    void syncActiveSessionSettings();

    if (!hasTauriRuntime()) {
      syncFallbackSession();
    }
  });

  watch(workDuration, () => {
    if (isApplyingBackendState) {
      return;
    }

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
    pauseSession,
    pauseTimeDisplay,
    pauseTimeoutMinutes,
    progress,
    remainingSeconds,
    resetSession,
    sessionSnapshot,
    isEarlyEnding,
    isCueTransitioning,
    sessionStage,
    sessionStatus,
    setSessionProgressPercent,
    setCueStyle,
    statusLine,
    workDuration
  };
}
