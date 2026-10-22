// Key naming for the shortcut field. Copies the prototype's `keyName`, `mods` and `KEYMAP`; the
// names are the ones `hotkey::parse` (crates/engine/src/hotkey.rs) accepts.

const KEYMAP: Record<string, string> = {
  Space: 'Espaço',
  Insert: 'Insert',
  Home: 'Home',
  End: 'End',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
  Delete: 'Del',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Escape: 'Esc',
  Tab: 'Tab',
  Enter: 'Enter',
  Backspace: 'Backspace',
  Backquote: '`',
  Minus: '-',
  Equal: '=',
};

export function keyName(e: Pick<KeyboardEvent, 'code' | 'key'>): string {
  const c = e.code;
  if (/^Key[A-Z]$/.test(c)) return c.slice(3);
  if (/^Digit\d$/.test(c)) return c.slice(5);
  if (/^F\d{1,2}$/.test(c)) return c;
  if (/^Numpad/.test(c)) return 'Num ' + c.slice(6);
  if (KEYMAP[c]) return KEYMAP[c];
  return e.key.length === 1 ? e.key.toUpperCase() : e.key;
}

/** Held modifiers in the fixed display order. */
export function mods(e: Pick<KeyboardEvent, 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string[] {
  const m: string[] = [];
  if (e.ctrlKey) m.push('Ctrl');
  if (e.altKey) m.push('Alt');
  if (e.shiftKey) m.push('Shift');
  if (e.metaKey) m.push('Win');
  return m;
}

const MODIFIER_NAMES = ['Ctrl', 'Alt', 'Shift', 'Win'];

/** Error text for a combo that can't be a global shortcut, or null (the same text as the backend's). */
export function validateCombo(combo: string[]): string | null {
  return combo.some((k) => MODIFIER_NAMES.includes(k)) ? null : 'Use pelo menos Ctrl, Alt ou Shift junto com a tecla.';
}
