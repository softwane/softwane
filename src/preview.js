import { invoke } from "@tauri-apps/api/core";

let activeAnimationId = 0;

export function cancelNativeAnimation() {
  activeAnimationId += 1;
}

function sigmoid(x, k = 10) {
  return 1 / (1 + Math.exp(-k * (x - 0.5)));
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function resolveCueWindows(sessionDurationMinutes) {
  const safeDuration = Math.max(sessionDurationMinutes, 0);
  const cappedDuration = Math.min(safeDuration, 50);
  const prewarmMinutes = cappedDuration * 0.1;
  const evolutionMinutes = cappedDuration * 0.1;

  return {
    prewarmMinutes,
    evolutionMinutes
  };
}

export function deriveLocalSnapshot(sessionDurationMinutes, remainingMinutes) {
  const { prewarmMinutes, evolutionMinutes } = resolveCueWindows(sessionDurationMinutes);
  const stableThreshold = prewarmMinutes + evolutionMinutes;

  if (remainingMinutes > stableThreshold) {
    return {
      phase: "Stable",
      saturation: 1,
      warmthKelvin: 6500,
      grayscale: 0
    };
  }

  const jndRemaining = remainingMinutes - evolutionMinutes;
  const progress = clamp((prewarmMinutes - jndRemaining) / Math.max(prewarmMinutes, 0.001), 0, 1);
  const curve = sigmoid(progress);

  if (remainingMinutes > evolutionMinutes) {
    return {
      phase: "JND",
      saturation: 1 - curve * 0.28,
      warmthKelvin: 6500 - curve * 1200,
      grayscale: curve * 0.18
    };
  }

  const evolutionProgress = clamp(
    (evolutionMinutes - remainingMinutes) / Math.max(evolutionMinutes, 0.001),
    0,
    1
  );
  const evolutionCurve = sigmoid(evolutionProgress);

  return {
    phase: remainingMinutes <= 0 ? "Statue" : "Evolution",
    saturation: 0.72 - evolutionCurve * 0.54,
    warmthKelvin: 5300 - evolutionCurve * 2800,
    grayscale: 0.18 + evolutionCurve * 0.74
  };
}

function normalizeCommandSnapshot(snapshot) {
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
    phase: phaseMap[normalizedPhase] ?? "Stable",
    saturation: snapshot.saturation,
    warmthKelvin: snapshot.warmth_kelvin,
    grayscale: snapshot.grayscale
  };
}

export async function applyNativeSnapshot(snapshot, cueStyle = "warm") {
  cancelNativeAnimation();

  try {
    const applyResult = await invoke("apply_effect_snapshot", {
      phase: snapshot.phase,
      saturation: snapshot.saturation,
      grayscale: snapshot.grayscale,
      warmthKelvin: Math.round(snapshot.warmthKelvin),
      cueStyle
    });

    return applyResult;
  } catch {
    return {
      applied: false,
      backend: "browser-preview"
    };
  }
}

export async function animateNativeSnapshot(fromSnapshot, toSnapshot, cueStyle = "warm", durationMs = 2800) {
  if (typeof window === "undefined" || typeof window.requestAnimationFrame !== "function") {
    return applyNativeSnapshot(toSnapshot, cueStyle);
  }

  activeAnimationId += 1;
  const animationId = activeAnimationId;

  const startedAt = window.performance.now();
  let lastAppliedAt = 0;
  const minFrameGapMs = 80;

  return new Promise((resolve) => {
    const tick = async (now) => {
      if (activeAnimationId !== animationId) {
        return resolve({ cancelled: true });
      }

      const progress = clamp((now - startedAt) / durationMs, 0, 1);
      const eased = progress * progress * (3 - 2 * progress);
      const snapshot = {
        phase: progress >= 1 ? toSnapshot.phase : fromSnapshot.phase,
        saturation: fromSnapshot.saturation + (toSnapshot.saturation - fromSnapshot.saturation) * eased,
        warmthKelvin: fromSnapshot.warmthKelvin + (toSnapshot.warmthKelvin - fromSnapshot.warmthKelvin) * eased,
        grayscale: fromSnapshot.grayscale + (toSnapshot.grayscale - fromSnapshot.grayscale) * eased
      };

      if (progress >= 1 || now - lastAppliedAt >= minFrameGapMs) {
        lastAppliedAt = now;
        try {
          await invoke("apply_effect_snapshot", {
            phase: snapshot.phase,
            saturation: snapshot.saturation,
            grayscale: snapshot.grayscale,
            warmthKelvin: Math.round(snapshot.warmthKelvin),
            cueStyle
          });
        } catch {
          // ignore
        }
      }

      if (activeAnimationId !== animationId) {
        return resolve({ cancelled: true });
      }

      if (progress < 1) {
        window.requestAnimationFrame((nextNow) => {
          void tick(nextNow);
        });
        return;
      }

      resolve({
        snapshot: {
          phase: toSnapshot.phase,
          saturation: toSnapshot.saturation,
          warmthKelvin: toSnapshot.warmthKelvin,
          grayscale: toSnapshot.grayscale
        }
      });
    };

    window.requestAnimationFrame((now) => {
      void tick(now);
    });
  });
}

export async function getPreviewSnapshot(sessionDurationMinutes, remainingMinutes, cueStyle = "warm") {
  cancelNativeAnimation();

  try {
    const payload = await invoke("preview_effect", {
      sessionDurationMinutes,
      remainingMinutes,
      cueStyle
    });

    return {
      snapshot: normalizeCommandSnapshot(payload.snapshot),
      applyResult: payload.apply_result ?? payload.applyResult
    };
  } catch {
    return {
      snapshot: deriveLocalSnapshot(sessionDurationMinutes, remainingMinutes),
      applyResult: {
        applied: false,
        backend: "browser-preview"
      }
    };
  }
}
