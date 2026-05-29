<script setup>
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

/**
 * VS Code-style key recorder.
 *
 * The native input is `readonly` so the user cannot type characters
 * into it — values change only via captured KeyboardEvents on focus.
 *
 * Pure-modifier presses (Alt, Shift, Control, Meta on their own) are
 * intentionally ignored: every binding must commit on a non-modifier
 * key release and must include at least one modifier (validated upstream).
 */

const props = defineProps({
  modelValue: { type: Object, default: () => null },
  placeholder: { type: String, default: "Press a key combo..." },
  disabled: { type: Boolean, default: false },
  invalid: { type: Boolean, default: false },
});

const emit = defineEmits(["update:modelValue"]);

const recording = ref(false);
const { t } = useI18n();

const displayText = computed(() => {
  if (recording.value) return t("shortcuts.recording");
  if (!props.modelValue) return "";
  return formatBinding(props.modelValue);
});

function onKeyDown(e) {
  e.preventDefault();
  e.stopPropagation();

  // Ignore pure modifier presses — wait for a real key.
  if (["Alt", "Shift", "Control", "Meta"].includes(e.key)) return;

  const modifiers = [];
  if (e.ctrlKey) modifiers.push("control");
  if (e.altKey) modifiers.push("alt");
  if (e.shiftKey) modifiers.push("shift");
  if (e.metaKey) modifiers.push("meta");

  // Frontend pre-validation: require at least one modifier.
  // (Backend repeats this check.)
  if (modifiers.length === 0) return;

  emit("update:modelValue", { modifiers, code: e.code });
  recording.value = false;
  e.currentTarget?.blur();
}

function formatBinding(b) {
  const parts = [];
  if (b.modifiers.includes("control")) parts.push(t("shortcuts.modifier.ctrl"));
  if (b.modifiers.includes("alt")) parts.push(altLabel());
  if (b.modifiers.includes("shift")) parts.push(t("shortcuts.modifier.shift"));
  if (b.modifiers.includes("meta")) parts.push(metaLabel());
  parts.push(prettyCode(b.code));
  return parts.join(" + ");
}

function altLabel() {
  if (typeof navigator !== "undefined" && /mac/i.test(navigator.platform || ""))
    return t("shortcuts.modifier.option");
  return t("shortcuts.modifier.alt");
}

function metaLabel() {
  if (typeof navigator !== "undefined" && /mac/i.test(navigator.platform || ""))
    return t("shortcuts.modifier.cmd");
  return t("shortcuts.modifier.win");
}

function prettyCode(code) {
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Numpad")) return t("shortcuts.modifier.num", { key: code.slice(6) });
  if (code === "Space") return t("shortcuts.modifier.space");
  if (code === "Escape") return t("shortcuts.modifier.esc");
  return code;
}
</script>

<template>
  <input
    type="text"
    readonly
    :class="['key-binding-input', { 'is-recording': recording, 'is-invalid': invalid }]"
    :disabled="disabled"
    :value="displayText"
    :placeholder="placeholder"
    @keydown="onKeyDown"
    @focus="recording = true"
    @blur="recording = false"
  />
</template>
