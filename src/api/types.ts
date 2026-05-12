// ── 通道类型枚举（与 ChannelType: serde rename_all = snake_case 对齐）
export type ChannelType = "saturation" | "color_temp" | "brightness";

// ── 通道值（tagged union，后端 ChannelValue）
export type ChannelValue =
  | { saturation: number }
  | { color_temp_kelvin: number }
  | { brightness: number };

// ── 缓动曲线参数
export type CurveParameters = { normalized_sigmoid: { steepness: number } };

// ── 单状态参数表
export interface PersistentStateParamsTable {
  progress_curve_parameters: CurveParameters;
  settling_curve_parameters: CurveParameters;
  reverse_curve_parameters: CurveParameters;
  progress_begin_ratio: number;
  target_channel_value: ChannelValue;
}

// ── 单通道完整配置
export interface ChannelConfig {
  switch_on: boolean;
  persistent_state_params_table: PersistentStateParamsTable;
}

// ── Timer 配置
export interface TimerConfig {
  settling_duration_ms: number;
  reverse_duration_ms: number;
}

// ── 启动一次性快照（get_available_stored_config 返回值）
export interface StoredConfig {
  channel_configs: [ChannelType, ChannelConfig][];
  timer_config: TimerConfig;
}

// ── Timer 状态快照（ProgressPayload 内嵌，Tag 字段名为 `state`）
export type TimerStateSnapshot =
  | { state: "idle" }
  | { state: "preview"; progress: number }
  | { state: "progress"; elapsed_ms: number; target_duration_ms: number }
  | { state: "settling"; elapsed_ms: number; target_duration_ms: number }
  | { state: "rest" }
  | { state: "reverse"; elapsed_ms: number; target_duration_ms: number };

// ── 前端状态推送（Channel<ProgressPayload> 负载）
export interface ProgressPayload {
  timer_state: TimerStateSnapshot;
}

// ── Crash 负载
export interface CrashPayload {
  message: string;
  thread: string;
  time: number;
}

// ── 快捷键 ────────────────────────────────────────────────────────────

/** 与后端 `ShortcutAction` 枚举对齐（serde rename_all = snake_case）。 */
export type ShortcutAction =
  | "start_preset1"
  | "start_preset2"
  | "start_preset3"
  | "take_break_now"
  | "stop_session"
  | "toggle_preview"
  | "force_reset"
  | "toggle_main_window";

export const SHORTCUT_ACTIONS: ShortcutAction[] = [
  "start_preset1",
  "start_preset2",
  "start_preset3",
  "take_break_now",
  "stop_session",
  "toggle_preview",
  "force_reset",
  "toggle_main_window",
];

export type ModifierKey = "alt" | "shift" | "control" | "meta";

export const MODIFIER_KEYS_ORDER: ModifierKey[] = [
  "control",
  "alt",
  "shift",
  "meta",
];

export interface KeyBinding {
  modifiers: ModifierKey[];
  /** [`KeyboardEvent.code`] form, e.g. `"Digit1"`, `"KeyB"`. */
  code: string;
}

/** Map keyed by `ShortcutAction`.  Keys must be exhaustive. */
export type ShortcutBindings = Record<ShortcutAction, KeyBinding>;
