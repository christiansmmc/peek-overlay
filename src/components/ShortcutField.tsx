import { useEffect, useRef, useState } from 'preact/hooks';
import { api } from '../ipc';
import { keyName, mods, validateCombo } from '../keys';

const MODIFIER_KEYS = ['Control', 'Shift', 'Alt', 'Meta'];

interface Props {
  shortcut: string[];
  /** Startup conflict reported by the backend; stays until a new shortcut is accepted. */
  hotkeyError: string | null;
  /** The backend accepted a new shortcut. */
  onChanged(combo: string[]): void;
}

/** "Atalho mostrar/esconder": click, press a combination, the backend validates and registers it. */
export function ShortcutField({ shortcut, hotkeyError, onChanged }: Props) {
  const [capturing, setCapturing] = useState(false);
  const [partial, setPartial] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  // The key handlers outlive a render; they read the capture state from here.
  const live = useRef(false);
  const onChangedRef = useRef(onChanged);
  onChangedRef.current = onChanged;

  const stop = () => {
    live.current = false;
    setCapturing(false);
    setPartial([]);
  };

  const begin = () => {
    if (live.current) return;
    live.current = true;
    setCapturing(true);
    setPartial([]);
    setError(null);
    api.beginHotkeyCapture().catch((e) => console.error('begin_hotkey_capture failed:', e));
  };

  const cancel = () => {
    if (!live.current) return;
    stop();
    api.endHotkeyCapture().catch((e) => console.error('end_hotkey_capture failed:', e));
  };

  useEffect(() => {
    if (!capturing) return;
    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const held = mods(e);
      if (MODIFIER_KEYS.includes(e.key)) return setPartial(held);
      const key = keyName(e);
      if (key === 'Esc' && held.length === 0) return cancel();
      const combo = [...held, key];
      const invalid = validateCombo(combo);
      if (invalid) {
        setPartial([]);
        return setError(invalid);
      }
      stop();
      api.trySetHotkey(combo).then(
        (r) => {
          if (r.ok) {
            setError(null);
            onChangedRef.current(combo);
          } else {
            setError(r.error);
          }
        },
        (err) => {
          console.error('try_set_hotkey failed:', err);
          api.endHotkeyCapture().catch(() => {});
        },
      );
    };
    const onKeyUp = (e: KeyboardEvent) => setPartial(mods(e));
    window.addEventListener('keydown', onKeyDown, true);
    window.addEventListener('keyup', onKeyUp, true);
    return () => {
      window.removeEventListener('keydown', onKeyDown, true);
      window.removeEventListener('keyup', onKeyUp, true);
    };
  }, [capturing]);

  const shown = error ?? hotkeyError;
  const hint = capturing ? 'Esc cancela' : (shown ?? 'Clique no campo e pressione a nova combinação');
  const keys = capturing ? partial : shortcut;
  return (
    <div class="shortcut">
      <div class="shortcut-text">
        <span class="shortcut-title">Atalho mostrar/esconder</span>
        <span class={shown && !capturing ? 'shortcut-hint err' : 'shortcut-hint'}>{hint}</span>
      </div>
      <button class={capturing ? 'shortcut-field capturing' : error ? 'shortcut-field err' : 'shortcut-field'} onClick={begin} onBlur={cancel}>
        {keys.map((k) => (
          <span key={k} class="keycap">
            {k}
          </span>
        ))}
        {capturing && <span class="shortcut-placeholder">{partial.length ? '+ …' : 'Pressione as teclas…'}</span>}
      </button>
    </div>
  );
}
