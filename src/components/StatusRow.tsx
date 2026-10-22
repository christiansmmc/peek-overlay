import type { Status } from '../types';

interface Props {
  status: Status;
  shortcut: string[];
  onToggle(): void;
}

/** Overlay status chip (toggles the real overlay), detected game and current shortcut. */
export function StatusRow({ status, shortcut, onToggle }: Props) {
  const on = status.overlayVisible;
  return (
    <div class="status">
      <button class="status-chip" title="Mostrar/esconder o overlay" onClick={onToggle}>
        <span class={on ? 'status-dot on' : 'status-dot'} />
        <span>{on ? 'Overlay ativo' : 'Overlay oculto'}</span>
      </button>
      {status.gameName != null ? (
        <div class="status-game">
          <span class="status-game-label">Jogo detectado</span>
          <span class="status-game-name">{status.gameName}</span>
        </div>
      ) : (
        <span class="status-nogame">Nenhum jogo detectado</span>
      )}
      <span class="status-shortcut">{shortcut.join(' + ')}</span>
    </div>
  );
}
