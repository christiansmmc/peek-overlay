import type { Settings } from '../types';

type General = Settings['general'];

const ROWS: [keyof General, string][] = [
  ['startup', 'Iniciar com o Windows'],
  ['minimized', 'Abrir minimizado na bandeja'],
  ['autoShow', 'Mostrar overlay sozinho ao abrir um jogo'],
];

interface Props {
  general: General;
  onToggle(k: keyof General): void;
}

/** "Geral": three switches; the whole row is clickable. */
export function GeneralToggles({ general, onToggle }: Props) {
  return (
    <div class="general">
      <span class="section-label">Geral</span>
      {ROWS.map(([k, label]) => (
        <div key={k} class={general[k] ? 'toggle-row on' : 'toggle-row'} role="switch" aria-checked={general[k]} onClick={() => onToggle(k)}>
          <span class="toggle-label">{label}</span>
          <span class="switch">
            <span class="switch-knob" />
          </span>
        </div>
      ))}
    </div>
  );
}
