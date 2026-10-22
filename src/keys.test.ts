import { expect, test } from 'vitest';
import { keyName, mods, validateCombo } from './keys';

test('keyName mapeia como o protótipo', () => {
  const k = (code: string, key = '') => keyName({ code, key });
  expect(k('KeyO')).toBe('O');
  expect(k('Digit5')).toBe('5');
  expect(k('F12')).toBe('F12');
  expect(k('Numpad3')).toBe('Num 3');
  expect(k('Space')).toBe('Espaço');
  expect(k('ArrowUp')).toBe('↑');
  expect(k('Backquote')).toBe('`');
});
test('mods em ordem fixa', () => {
  expect(mods({ ctrlKey: true, altKey: true, shiftKey: true, metaKey: true })).toEqual(['Ctrl', 'Alt', 'Shift', 'Win']);
});
test('validateCombo exige modificador', () => {
  expect(validateCombo(['O'])).toBe('Use pelo menos Ctrl, Alt ou Shift junto com a tecla.');
  expect(validateCombo(['Ctrl', 'O'])).toBeNull();
});
