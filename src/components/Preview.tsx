import { useLayoutEffect, useRef, useState } from 'preact/hooks';
import type { Availability, DriverState, Metrics, MetricsSample, Pos, Settings } from '../types';
import { OverlayCard } from './OverlayCard';
import { OverlayLine } from './OverlayLine';

interface PosDef {
  name: string;
  top?: string;
  left?: string;
  right?: string;
  bottom?: string;
  /** Centering translate, applied before the scale. */
  tx?: string;
  /** transform-origin */
  o: string;
  /** Center of the position target. */
  x: string;
  y: string;
}

// The prototype's POS table.
const POS: Record<Pos, PosDef> = {
  tl: { name: 'Canto superior esquerdo', top: '8px', left: '8px', o: 'top left', x: '14px', y: '14px' },
  tc: { name: 'Centro superior', top: '8px', left: '50%', tx: 'translateX(-50%) ', o: 'top center', x: '50%', y: '14px' },
  tr: { name: 'Canto superior direito', top: '8px', right: '8px', o: 'top right', x: 'calc(100% - 14px)', y: '14px' },
  ml: { name: 'Centro esquerdo', top: '50%', left: '8px', tx: 'translateY(-50%) ', o: 'left center', x: '14px', y: '50%' },
  mr: { name: 'Centro direito', top: '50%', right: '8px', tx: 'translateY(-50%) ', o: 'right center', x: 'calc(100% - 14px)', y: '50%' },
  bl: { name: 'Canto inferior esquerdo', bottom: '8px', left: '8px', o: 'bottom left', x: '14px', y: 'calc(100% - 14px)' },
  bc: { name: 'Centro inferior', bottom: '8px', left: '50%', tx: 'translateX(-50%) ', o: 'bottom center', x: '50%', y: 'calc(100% - 14px)' },
  br: { name: 'Canto inferior direito', bottom: '8px', right: '8px', o: 'bottom right', x: 'calc(100% - 14px)', y: 'calc(100% - 14px)' },
};

/** What the overlay draws: metrics that are on and available (effects::effective_metrics). */
export function effectiveMetrics(m: Metrics, a: Availability, driver: DriverState): Metrics {
  const ok = (k: keyof Availability) => a[k].state === 'ok';
  return {
    fps: m.fps && ok('fps'),
    graph: m.graph && ok('fps'),
    gpuUse: m.gpuUse && ok('gpuUse'),
    gpuTemp: m.gpuTemp && ok('gpuTemp'),
    cpuUse: m.cpuUse && ok('cpuUse'),
    cpuTemp: m.cpuTemp && ok('cpuTemp') && driver === 'installed',
  };
}

interface Props {
  settings: Settings;
  availability: Availability;
  driver: DriverState;
  overlayVisible: boolean;
  sample: MetricsSample;
  onPos(pos: Pos): void;
}

/** Game scene with the 8 position targets and the overlay at its size, shrunk to fit if needed. */
export function Preview({ settings, availability, driver, overlayVisible, sample, onPos }: Props) {
  const boxRef = useRef<HTMLDivElement>(null);
  const ovRef = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState(1);

  // The overlay keeps 8px from every edge: scale it down when size × its box doesn't fit.
  useLayoutEffect(() => {
    const el = ovRef.current;
    const box = boxRef.current;
    if (!el || !box || !el.offsetWidth) return;
    const s = settings.size / 100;
    const next = Math.min(1, (box.clientWidth - 16) / (el.offsetWidth * s), (box.clientHeight - 16) / (el.offsetHeight * s));
    if (Math.abs(next - fit) > 0.004) setFit(next);
  });

  const p = POS[settings.pos];
  const scale = (settings.size / 100) * fit;
  const overlay = { show: effectiveMetrics(settings.metrics, availability, driver), sample, opacity: settings.opacity };
  return (
    <div class="preview">
      <div class="preview-head">
        <span class="section-label">Prévia · clique para posicionar</span>
        <span class="preview-pos">
          {p.name}
          {fit < 0.995 ? ' · prévia reduzida' : ''}
        </span>
      </div>
      <div ref={boxRef} class="preview-box">
        <span class="preview-scene">cena do jogo · 1920×1080</span>
        {(Object.entries(POS) as [Pos, PosDef][]).map(([k, v]) => (
          <button key={k} class={k === settings.pos ? 'pv-spot on' : 'pv-spot'} title={v.name} style={{ left: v.x, top: v.y }} onClick={() => onPos(k)}>
            <span class="pv-dot" />
          </button>
        ))}
        <div
          ref={ovRef}
          class="ov"
          style={{
            top: p.top ?? 'auto',
            left: p.left ?? 'auto',
            right: p.right ?? 'auto',
            bottom: p.bottom ?? 'auto',
            transform: `${p.tx ?? ''}scale(${scale.toFixed(3)})`,
            transformOrigin: p.o,
            opacity: overlayVisible ? 1 : 0.3,
          }}
        >
          {settings.style === 'line' ? <OverlayLine {...overlay} /> : <OverlayCard {...overlay} />}
        </div>
      </div>
    </div>
  );
}
