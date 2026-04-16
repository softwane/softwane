import { invoke } from "@tauri-apps/api/core";

export async function previewFrame(
  saturation = 1.0,
  warmthKelvin = 6500,
  brightness = 1.0
) {
  try {
    return await invoke("preview_frame", {
      saturation,
      warmthKelvin,
      brightness,
    });
  } catch {
    return {
      frame: { saturation, warmthKelvin, brightness },
      applyResult: { applied: false, backend: "browser-preview" },
    };
  }
}

export async function resetDisplay() {
  try {
    return await invoke("reset_display");
  } catch {
    return { applied: false, backend: "browser-preview" };
  }
}
