// Coalesces rapid-fire calls into one invocation after the last call.
/**
 * @template {unknown[]} Args
 * @param {(...args: Args) => void} fn
 * @param {number} delayMs
 * @returns {(...args: Args) => void}
 */
export function debounce(fn, delayMs) {
  /** @type {ReturnType<typeof setTimeout> | null} */
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, delayMs);
  };
}
