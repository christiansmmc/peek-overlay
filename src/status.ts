import type { Status } from './types';

/** Text of the overlay chip: the overlay can be on but hidden until the game is in front. */
export function chipLabel(s: Status): string {
  if (!s.overlayVisible) return 'Overlay oculto';
  return s.overlayWaiting ? 'Overlay ativo · aguardando jogo' : 'Overlay ativo';
}
