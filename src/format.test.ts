import { expect, test } from 'vitest';
import { fmtFps, fmtMs, fmtPair, downsampleMax, graphPoints } from './format';

test('formata como o overlay real', () => {
  expect(fmtPair(true, 96, true, 71, 'line')).toBe('96% 71°');
  expect(fmtPair(true, 96, true, 71, 'card')).toBe('96% · 71°');
  expect(fmtPair(true, null, true, 64, 'card')).toBe('— · 64°');
  expect(fmtPair(false, null, true, 64, 'card')).toBe('64°');
  expect(fmtMs(6.94)).toBe('6,9');
  expect(fmtMs(null)).toBe('—');
  expect(fmtFps(null)).toBe('—');
});
test('downsampleMax preserva picos', () => {
  expect(downsampleMax([1, 1, 9, 1, 1, 1], 3)).toEqual([1, 9, 1]);
});
test('graphPoints segue o protótipo', () => {
  expect(graphPoints([0, 14], 44, 14, 14)).toBe('0.0,13.0 44.0,1.0');
});
