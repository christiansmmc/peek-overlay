import { fmtFps, fmtMs, fmtPair, graphPoints } from '../format';
import type { OverlayProps } from './OverlayLine';
import { overlayBg } from './OverlayLine';

interface RowProps {
  kind: 'gpu' | 'cpu';
  useOn: boolean;
  use: number | null;
  tempOn: boolean;
  temp: number | null;
}

/** "GPU"/"CPU" label, usage bar and "96% · 71°". */
function BarRow({ kind, useOn, use, tempOn, temp }: RowProps) {
  return (
    <div class="ovc-row">
      <span class={`ov-${kind}`}>{kind.toUpperCase()}</span>
      {/* With only the temperature on, the bar is hidden but keeps its space. */}
      <span class="ovc-bar" style={{ visibility: useOn ? 'visible' : 'hidden' }}>
        <span class={`ovc-fill ${kind}`} style={{ width: `${Math.min(100, use ?? 0)}%` }} />
      </span>
      <span>{fmtPair(useOn, use, tempOn, temp, 'card')}</span>
    </div>
  );
}

/** "Card" style: 148px wide, one row per metric group. */
export function OverlayCard({ show, sample: s, opacity }: OverlayProps) {
  const gpu = show.gpuUse || show.gpuTemp;
  const cpu = show.cpuUse || show.cpuTemp;
  return (
    <div class="ovc" style={{ background: overlayBg(opacity) }}>
      {show.fps && (
        <div class="ovc-fps">
          <span class="ovc-num">{fmtFps(s.fps)}</span>
          <span class="ovc-unit">FPS</span>
          <span class="ovc-ms">{fmtMs(s.frametimeMs)} ms</span>
        </div>
      )}
      {show.graph && (
        <svg class="ov-graph" width="126" height="26" viewBox="0 0 126 26">
          <polyline
            points={graphPoints(s.graph, 126, 26, s.graphMaxMs)}
            fill="none"
            stroke="oklch(0.78 0.13 250)"
            stroke-width="1.5"
            stroke-linejoin="round"
          />
        </svg>
      )}
      {gpu && <BarRow kind="gpu" useOn={show.gpuUse} use={s.gpuUse} tempOn={show.gpuTemp} temp={s.gpuTemp} />}
      {cpu && <BarRow kind="cpu" useOn={show.cpuUse} use={s.cpuUse} tempOn={show.cpuTemp} temp={s.cpuTemp} />}
      {!show.fps && !show.graph && !gpu && !cpu && <span class="ovc-none">Nenhuma métrica ativa</span>}
    </div>
  );
}
