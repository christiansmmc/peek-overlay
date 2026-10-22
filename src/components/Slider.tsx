import type { TargetedKeyboardEvent, TargetedPointerEvent } from 'preact';
import { useRef } from 'preact/hooks';

interface Props {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange(v: number): void;
}

const ARROWS: Record<string, number> = { ArrowRight: 1, ArrowUp: 1, ArrowLeft: -1, ArrowDown: -1 };

/** Custom slider showing "<value>%": pointer drag over the whole 32px hit area, or arrow keys. */
export function Slider({ label, value, min, max, step, onChange }: Props) {
  const dragging = useRef(false);
  const set = (v: number) => {
    if (v !== value) onChange(v);
  };
  const setFrom = (e: TargetedPointerEvent<HTMLDivElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const t = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
    set(Math.round((min + t * (max - min)) / step) * step);
  };
  const onKeyDown = (e: TargetedKeyboardEvent<HTMLDivElement>) => {
    const d = ARROWS[e.key];
    if (d) {
      e.preventDefault();
      set(Math.max(min, Math.min(max, value + d * step)));
    }
  };
  const pct = `${((value - min) / (max - min)) * 100}%`;
  return (
    <div class="slider">
      <div class="slider-head">
        <span class="section-label">{label}</span>
        <span class="slider-value">{value}%</span>
      </div>
      <div
        class="slider-hit"
        tabIndex={0}
        role="slider"
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          dragging.current = true;
          setFrom(e);
        }}
        onPointerMove={(e) => dragging.current && setFrom(e)}
        onPointerUp={() => (dragging.current = false)}
        onPointerCancel={() => (dragging.current = false)}
        onKeyDown={onKeyDown}
      >
        <div class="slider-track" />
        <div class="slider-fill" style={{ width: pct }} />
        <div class="slider-knob" style={{ left: pct }} />
      </div>
    </div>
  );
}
