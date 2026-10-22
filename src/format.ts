// Same rules as the real overlay (crates/engine/src/overlay/format.rs and layout.rs) and
// the prototype's `pts()`, so the preview matches what Direct2D draws.
import type { Style } from './types';

const DASH = '—';

export function fmtFps(v: number | null): string {
  return v == null ? DASH : String(v);
}

/** One decimal place with a comma ("6,9"). */
export function fmtMs(v: number | null): string {
  return v == null ? DASH : v.toFixed(1).replace('.', ',');
}

/** "96%" and "71°" joined by " " (Line) or " · " (Card); a missing value is "—". */
export function fmtPair(useOn: boolean, useV: number | null, tempOn: boolean, tempV: number | null, style: Style): string {
  const parts: string[] = [];
  if (useOn) parts.push(useV == null ? DASH : `${useV}%`);
  if (tempOn) parts.push(tempV == null ? DASH : `${tempV}°`);
  return parts.join(style === 'card' ? ' · ' : ' ');
}

/** Reduces `v` to `n` contiguous groups keeping each group's max, so stutters stay visible. */
export function downsampleMax(v: number[], n: number): number[] {
  if (n >= v.length) return v.slice();
  if (n <= 0) return [];
  const len = v.length;
  return Array.from({ length: n }, (_, i) => Math.max(...v.slice(Math.floor((i * len) / n), Math.floor(((i + 1) * len) / n))));
}

/** SVG polyline points: x = i·w/(n−1), y = h − 1 − min(1, v/yMax)·(h − 2). Empty under 2 points. */
export function graphPoints(v: number[], w: number, h: number, yMax: number): string {
  const n = v.length;
  if (n < 2) return '';
  return v
    .map((ms, i) => {
      const ratio = yMax > 0 ? Math.max(0, Math.min(1, ms / yMax)) : 0;
      return `${((i * w) / (n - 1)).toFixed(1)},${(h - 1 - ratio * (h - 2)).toFixed(1)}`;
    })
    .join(' ');
}
