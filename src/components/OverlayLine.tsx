import { downsampleMax, fmtFps, fmtPair, graphPoints } from '../format';
import type { Metrics, MetricsSample } from '../types';

export interface OverlayProps {
  /** Effective metrics: on in the settings and available. */
  show: Metrics;
  sample: MetricsSample;
  opacity: number;
}

/** Overlay background: rgba(12,12,18) with alpha = opacity (the text stays opaque). */
export const overlayBg = (opacity: number) => `rgba(12,12,18,${(opacity / 100).toFixed(2)})`;

/** "Linha" style: one 26px pill. */
export function OverlayLine({ show, sample: s, opacity }: OverlayProps) {
  const gpu = show.gpuUse || show.gpuTemp;
  const cpu = show.cpuUse || show.cpuTemp;
  return (
    <div class="ovl" style={{ background: overlayBg(opacity) }}>
      {show.fps && (
        <span class="ovl-fps">
          <span>{fmtFps(s.fps)}</span>
          <span class="ovl-unit">FPS</span>
        </span>
      )}
      {show.graph && (
        <svg class="ov-graph" width="44" height="14" viewBox="0 0 44 14">
          <polyline
            points={graphPoints(downsampleMax(s.graph, 44), 44, 14, s.graphMaxMs)}
            fill="none"
            stroke="oklch(0.78 0.13 250)"
            stroke-width="1.4"
            stroke-linejoin="round"
          />
        </svg>
      )}
      {gpu && (
        <span class="ovl-pair">
          <span class="ov-gpu">GPU</span>
          <span>{fmtPair(show.gpuUse, s.gpuUse, show.gpuTemp, s.gpuTemp, 'line')}</span>
        </span>
      )}
      {cpu && (
        <span class="ovl-pair">
          <span class="ov-cpu">CPU</span>
          <span>{fmtPair(show.cpuUse, s.cpuUse, show.cpuTemp, s.cpuTemp, 'line')}</span>
        </span>
      )}
      {!show.fps && !show.graph && !gpu && !cpu && <span class="ovl-none">Nenhuma métrica</span>}
    </div>
  );
}
