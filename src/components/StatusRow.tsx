import { chipLabel } from '../status';
import type { Status } from '../types';

interface Props {
  status: Status;
  onToggle(): void;
  onMarkNotGame(): void;
  onMarkGame(): void;
}

/** Overlay status chip (toggles the real overlay), and the detected game or the last app, each with its marking link. */
export function StatusRow({ status, onToggle, onMarkNotGame, onMarkGame }: Props) {
  return (
    <div class="status">
      <button class="status-chip" title="Mostrar/esconder o overlay" onClick={onToggle}>
        <span class={status.overlayVisible ? 'status-dot on' : 'status-dot'} />
        <span>{chipLabel(status)}</span>
      </button>
      {status.gameName != null ? (
        <div class="status-game">
          <span class="status-game-label">Jogo detectado</span>
          <span class="status-game-line">
            <span class="status-game-name">{status.gameName}</span>
            <span class="status-sep">·</span>
            <button class="status-link" onClick={onMarkNotGame}>não é jogo</button>
          </span>
        </div>
      ) : status.lastAppName != null ? (
        <div class="status-game">
          <span class="status-game-label">Nenhum jogo · último app</span>
          <span class="status-game-line">
            <span class="status-game-name">{status.lastAppName}</span>
            <span class="status-sep">·</span>
            <button class="status-link" onClick={onMarkGame}>marcar como jogo</button>
          </span>
        </div>
      ) : (
        <span class="status-nogame">Nenhum jogo detectado</span>
      )}
    </div>
  );
}
