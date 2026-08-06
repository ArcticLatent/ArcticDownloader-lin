// Coalesces rapid-fire calls into one invocation after the last call.
export function debounce(fn, delayMs) {
  let timer = null;
  return (...args) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      fn(...args);
    }, delayMs);
  };
}
