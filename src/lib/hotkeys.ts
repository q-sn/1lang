/**
 * Accelerators use the global-hotkey format: modifiers + KeyboardEvent.code,
 * e.g. "Ctrl+Alt+KeyT".
 */

const MODIFIER_CODES = new Set([
  'ControlLeft', 'ControlRight', 'ShiftLeft', 'ShiftRight', 'AltLeft', 'AltRight', 'MetaLeft', 'MetaRight',
]);

export function accelFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push('Ctrl');
  if (e.altKey) mods.push('Alt');
  if (e.shiftKey) mods.push('Shift');
  if (e.metaKey) mods.push('Super');
  const isFn = /^F\d{1,2}$/.test(e.code);
  // Plain letters without modifiers would hijack typing everywhere.
  if (mods.length === 0 && !isFn) return null;
  return [...mods, e.code].join('+');
}

const PRETTY: Record<string, string> = {
  Space: 'Space', Backquote: '`', Minus: '-', Equal: '=', BracketLeft: '[', BracketRight: ']',
  Backslash: '\\', Semicolon: ';', Quote: "'", Comma: ',', Period: '.', Slash: '/',
  ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', Escape: 'Esc', Enter: 'Enter',
  Super: 'Win',
};

/** "Ctrl+Alt+KeyT" → ["Ctrl", "Alt", "T"] */
export function prettyAccel(accel: string): string[] {
  if (!accel) return [];
  return accel.split('+').map((p) => {
    if (p.startsWith('Key')) return p.slice(3);
    if (p.startsWith('Digit')) return p.slice(5);
    if (p.startsWith('Numpad')) return 'Num' + p.slice(6);
    return PRETTY[p] ?? p;
  });
}
