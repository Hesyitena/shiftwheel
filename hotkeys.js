// Turns raw uiohook key events into "taps" of the wheel keys (Shift/Ctrl/Alt).
//
// Windows repeats keydown ~30x per second while a key is held, so repeats must be ignored. The old
// approach ("ignore keydown until its keyup") broke whenever a keyup went missing: the key then counted
// as held forever and every later tap was ignored until Shiftwheel restarted. Keyups do go missing,
// e.g. Shift pressed as left but released as right (NumLock + numpad), or swallowed by Alt+Tab.
//
// So instead: a keydown is a repeat only if the same key (either side) went down less than
// REPEAT_GAP ms ago. A held key keeps refreshing that time; a lost keyup can block at most one tap.
const REPEAT_GAP = 600; // ms, longer than Windows' default 500 ms auto-repeat delay

function createTaps(keyToMode) {
  const lastDown = new Map(); // mode -> time of its latest keydown

  return {
    /** Returns the wheel mode for a fresh tap, or null for repeats and unrelated keys. */
    down(keycode, now) {
      const mode = keyToMode[keycode];
      if (!mode) return null;
      const prev = lastDown.get(mode);
      lastDown.set(mode, now);
      return prev !== undefined && now - prev < REPEAT_GAP ? null : mode;
    },
    /** Any side's keyup releases the mode, so left-down/right-up pairs can't get stuck. */
    up(keycode) {
      const mode = keyToMode[keycode];
      if (mode) lastDown.delete(mode);
    },
    reset() {
      lastDown.clear();
    },
  };
}

module.exports = { createTaps, REPEAT_GAP };
