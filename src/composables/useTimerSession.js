import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";

const STORAGE_KEY = "erode.session-config";
const SESSION_EVENT = "session-updated";
const DEFAULT_WORK_DURATION_MINUTES = 50;
const MIN_SUPPORTED_WORK_DURATION_MINUTES = 2;
const MAX_WORK_DURATION_MINUTES = 120;
const DEFAULT_REVERSE_MAX_DURATION_MS = 30000;

const DEFAULT_CHANNEL_CONFIGS = [
  {
    channel_type: "saturation",
    target_saturation: 0.18,
    curve_steepness: 10.0,
    settle_duration_ms: 5000,
  },
  {
    channel_type: "warmth",
    target_kelvin: 2500,
    curve_steepness: 10.0,
    settle_duration_ms: 5000,
  },
];

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function clampDuration(value) {
  if (!Number.isFinite(value)) return DEFAULT_WORK_DURATION_MINUTES;
  return clamp(value, 0, MAX_WORK_DURATION_MINUTES);
}

function formatClock(totalSeconds) {
  const safe = Math.max(totalSeconds, 0);
  const h = Math.floor(safe / 3600);
  const m = Math.floor((safe % 3600) / 60);
  const s = Math.floor(safe % 60);

  if (h > 0) {
    return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function phaseKind(phase) {
  if (!phase) return "idle";
  if (typeof phase === "string") return phase.toLowerCase();
  return (phase.kind ?? "idle").toLowerCase();
}

export function useTimerSession() {
  const workDuration = ref(DEFAULT_WORK_DURATION_MINUTES);
  const channelConfigs = ref(structuredClone(DEFAULT_CHANNEL_CONFIGS));
  const reverseMaxDurationMs = ref(DEFAULT_REVERSE_MAX_DURATION_MS);

  const phase = ref({ kind: "idle" });
  const elapsedSeconds = ref(0);
  const targetDurationSeconds = ref(0);
  const channelStates = ref([]);
  const frame = ref({ saturation: 1, warmthKelvin: 6500, brightness: 1 });

  let unlistenSession = null;
  let isApplyingBackend = false;

  const currentPhase = computed(() => phaseKind(phase.value));
  const isIdle = computed(() => currentPhase.value === "idle");
  const isForward = computed(() => currentPhase.value === "forward");
  const isSettling = computed(() => currentPhase.value === "settling");
  const isSabi = computed(() => currentPhase.value === "sabi");
  const isReverse = computed(() => currentPhase.value === "reverse");
  const hasActiveSession = computed(() => !isIdle.value);

  const isWorkDurationSupported = computed(
    () =>
      Number.isFinite(workDuration.value) &&
      workDuration.value >= MIN_SUPPORTED_WORK_DURATION_MINUTES
  );

  const remainingSeconds = computed(() => {
    if (!isForward.value) return 0;
    return Math.max(0, targetDurationSeconds.value - elapsedSeconds.value);
  });

  const displayTime = computed(() => {
    if (isForward.value || isSettling.value) {
      return formatClock(remainingSeconds.value);
    }
    if (isIdle.value) {
      return formatClock(Math.max(workDuration.value, 0) * 60);
    }
    return "";
  });

  const progress = computed(() => {
    if (!isForward.value || targetDurationSeconds.value <= 0) return 0;
    return clamp(elapsedSeconds.value / targetDurationSeconds.value, 0, 1);
  });

  const phaseLabel = computed(() => {
    switch (currentPhase.value) {
      case "forward":
        return "Forward";
      case "settling":
        return "Settling";
      case "sabi":
        return "Sabi";
      case "reverse":
        return "Reverse";
      default:
        return "Idle";
    }
  });

  // --- Persistence ---

  function hydrateFromStorage() {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    try {
      const saved = JSON.parse(raw);
      if (typeof saved.workDuration === "number") {
        workDuration.value = clampDuration(saved.workDuration);
      }
      if (Array.isArray(saved.channels) && saved.channels.length > 0) {
        channelConfigs.value = saved.channels;
      }
      if (typeof saved.reverseMaxDurationMs === "number") {
        reverseMaxDurationMs.value = saved.reverseMaxDurationMs;
      }
    } catch {
      window.localStorage.removeItem(STORAGE_KEY);
    }
  }

  function persistConfig() {
    window.localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        workDuration: clampDuration(workDuration.value),
        channels: channelConfigs.value,
        reverseMaxDurationMs: reverseMaxDurationMs.value,
      })
    );
  }

  // --- Backend communication ---

  function applyPayload(payload) {
    if (!payload) return;
    isApplyingBackend = true;

    phase.value = payload.phase ?? { kind: "idle" };
    elapsedSeconds.value = payload.elapsedSeconds ?? 0;
    targetDurationSeconds.value = payload.targetDurationSeconds ?? 0;
    channelStates.value = payload.channels ?? [];
    frame.value = payload.frame ?? { saturation: 1, warmthKelvin: 6500, brightness: 1 };

    if (payload.workDurationMinutes && !isIdle.value) {
      workDuration.value = clampDuration(payload.workDurationMinutes);
    }

    isApplyingBackend = false;
  }

  async function invokeSession(command, payload = {}) {
    if (!hasTauriRuntime()) return null;
    try {
      return await invoke(command, payload);
    } catch {
      return null;
    }
  }

  async function syncState() {
    const payload = await invokeSession("get_session_state");
    applyPayload(payload);
  }

  // --- User actions ---

  async function startSession() {
    if (!isWorkDurationSupported.value) return false;

    const payload = await invokeSession("start_session", {
      workDurationMinutes: clampDuration(workDuration.value),
      channels: channelConfigs.value,
      reverseMaxDurationMs: reverseMaxDurationMs.value,
    });

    applyPayload(payload);
    return phaseKind(payload?.phase) === "forward";
  }

  async function takeBreakNow() {
    const payload = await invokeSession("take_break_now");
    applyPayload(payload);
  }

  async function startReverse() {
    const payload = await invokeSession("start_reverse");
    applyPayload(payload);
  }

  async function updateChannels() {
    if (isApplyingBackend || !hasActiveSession.value || !hasTauriRuntime()) return;
    const payload = await invokeSession("update_channels", {
      channels: channelConfigs.value,
    });
    applyPayload(payload);
  }

  // --- Lifecycle ---

  onMounted(async () => {
    hydrateFromStorage();

    if (hasTauriRuntime()) {
      unlistenSession = await listen(SESSION_EVENT, (event) => {
        applyPayload(event.payload);
      });
      await syncState();
      return;
    }
  });

  onUnmounted(() => {
    if (typeof unlistenSession === "function") {
      unlistenSession();
      unlistenSession = null;
    }
  });

  watch(workDuration, (value) => {
    const safe = clampDuration(value);
    if (safe !== value) {
      workDuration.value = safe;
      return;
    }
  });

  watch([channelConfigs, reverseMaxDurationMs], () => {
    if (isApplyingBackend) return;
    persistConfig();
    void updateChannels();
  });

  watch(workDuration, () => {
    if (isApplyingBackend) return;
    persistConfig();
  });

  return {
    workDuration,
    channelConfigs,
    reverseMaxDurationMs,

    phase,
    currentPhase,
    isIdle,
    isForward,
    isSettling,
    isSabi,
    isReverse,
    hasActiveSession,
    isWorkDurationSupported,

    elapsedSeconds,
    targetDurationSeconds,
    remainingSeconds,
    displayTime,
    progress,
    phaseLabel,
    channelStates,
    frame,

    startSession,
    takeBreakNow,
    startReverse,
  };
}
