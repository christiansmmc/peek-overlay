import { expect, it } from 'vitest';
import { chipLabel } from './status';

const off = { overlayVisible: false, overlayWaiting: false, gameName: null, lastAppName: null };

it('chip label follows the overlay state', () => {
  expect(chipLabel(off)).toBe('Overlay oculto');
  expect(chipLabel({ ...off, overlayVisible: true })).toBe('Overlay ativo');
  expect(chipLabel({ ...off, overlayVisible: true, overlayWaiting: true })).toBe('Overlay ativo · aguardando jogo');
});
