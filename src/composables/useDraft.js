import { computed, ref, toRaw, watch } from "vue";
import cloneDeep from 'lodash/cloneDeep';

/**
 * Draft state pattern with explicit Save / Cancel.
 *
 * Wraps a reactive source (typically a global ref) into:
 *  - `draft`     : a deep-cloned local copy editable in the UI
 *  - `dirty`     : true when draft differs from the last-known source
 *                  (the "baseline").  Comparing to a baseline rather
 *                  than the live source means an external source push
 *                  while the draft is untouched does NOT mark dirty.
 *  - `commit()`  : invoke `commitFn(draft)`; on success the baseline
 *                  is advanced so `dirty` resets cleanly.
 *  - `cancel()`  : reset draft & baseline to the live source value.
 *
 * Source updates are reconciled as follows:
 *  - If draft equals the baseline (i.e. user has not edited), the
 *    draft is rebased onto the new source.  This handles initial
 *    `null → loaded` transitions naturally.
 *  - If draft differs from the baseline (user has unsaved edits), the
 *    draft is kept; the baseline is advanced so `dirty` continues to
 *    reflect divergence from the freshest known-good state rather
 *    than getting "stuck" against a stale snapshot.
 *
 * @template T
 * @param {import('vue').Ref<T>} source     Reactive source ref.
 * @param {(draft: T) => Promise<void> | void} commitFn  Persist callback.
 * @returns {{
 *   draft: import('vue').Ref<T>,
 *   dirty: import('vue').ComputedRef<boolean>,
 *   commit: () => Promise<void>,
 *   cancel: () => void,
 * }}
 */
export function useDraft(source, commitFn) {
  const draft = ref(cloneDeep(source.value));
  const baseline = ref(cloneDeep(source.value));

  const dirty = computed(() => !deepEqual(draft.value, baseline.value));

  watch(
    source,
    (next) => {
      if (deepEqual(draft.value, baseline.value)) {
        draft.value = cloneDeep(next);
      }
      baseline.value = cloneDeep(next);
    },
    { deep: true },
  );

  function cancel() {
    draft.value = cloneDeep(source.value);
    baseline.value = cloneDeep(source.value);
  }

  async function commit() {
    const snapshot = cloneDeep(draft.value);
    await commitFn(snapshot);
    // Advance baseline so the form is no longer dirty even before the
    // source ref propagates the new value (some commitFns update the
    // source asynchronously or only via a server round-trip).
    baseline.value = snapshot;
  }

  return { draft, dirty, commit, cancel };
}

// function cloneDeep(value) {
//   console.log(toRaw(value));
//   if (typeof structuredClone === "function") return structuredClone(toRaw(value));
//   return JSON.parse(JSON.stringify(value));
// }

function deepEqual(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (a === null || b === null) return a === b;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) if (!deepEqual(a[i], b[i])) return false;
    return true;
  }
  if (typeof a === "object") {
    const ka = Object.keys(a);
    const kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) if (!deepEqual(a[k], b[k])) return false;
    return true;
  }
  return false;
}
