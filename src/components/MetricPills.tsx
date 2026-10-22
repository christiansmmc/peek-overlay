import type { Availability, DriverState, Metrics } from '../types';

const PILLS: [keyof Metrics, string][] = [
  ['fps', 'FPS'],
  ['graph', 'Gráfico de frametime'],
  ['gpuUse', 'GPU uso'],
  ['gpuTemp', 'GPU temp.'],
  ['cpuUse', 'CPU uso'],
  ['cpuTemp', 'CPU temp.'],
];

/** The frametime graph comes from the same frame capture as FPS. */
const SOURCE: Record<keyof Metrics, keyof Availability> = {
  fps: 'fps',
  graph: 'fps',
  gpuUse: 'gpuUse',
  gpuTemp: 'gpuTemp',
  cpuUse: 'cpuUse',
  cpuTemp: 'cpuTemp',
};

interface PillState {
  kind: 'normal' | 'disabled' | 'driver';
  sub: string;
  subClass: string;
  title: string;
}

function pillState(k: keyof Metrics, availability: Availability, driver: DriverState): PillState {
  // Without PawnIO the CPU temperature can't be read at all: offer the driver first.
  if (k === 'cpuTemp' && driver !== 'installed') {
    const failed = driver === 'failed';
    return {
      kind: 'driver',
      sub: failed ? 'instalação falhou' : 'requer driver',
      subClass: failed ? 'err' : 'warn',
      title: 'Precisa do driver PawnIO (gratuito)',
    };
  }
  const a = availability[SOURCE[k]];
  if (a.state === 'noSensor') return { kind: 'disabled', sub: 'sem sensor', subClass: '', title: a.reason };
  return { kind: 'normal', sub: '', subClass: '', title: '' };
}

interface Props {
  metrics: Metrics;
  availability: Availability;
  driver: DriverState;
  onToggle(k: keyof Metrics): void;
  /** "CPU temp." clicked while the driver is not installed (opens the driver sheet). */
  onNeedsDriver(): void;
}

/** "Métricas": one pill per metric; unavailable ones are dashed and don't respond. */
export function MetricPills({ metrics, availability, driver, onToggle, onNeedsDriver }: Props) {
  return (
    <div class="metrics">
      <span class="section-label">Métricas</span>
      <div class="pills">
        {PILLS.map(([k, label]) => {
          const p = pillState(k, availability, driver);
          const on = p.kind === 'normal' && metrics[k];
          const click = () => {
            if (p.kind === 'normal') onToggle(k);
            else if (p.kind === 'driver') onNeedsDriver();
          };
          return (
            <button key={k} class={on ? 'pill on' : p.kind === 'disabled' ? 'pill disabled' : 'pill'} title={p.title} onClick={click}>
              <span class="pill-dot">
                {on && (
                  <svg width="8" height="8" viewBox="0 0 8 8">
                    <path d="M1.5 4.2L3.2 5.8L6.5 2.3" fill="none" stroke="#0b0b12" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
                  </svg>
                )}
              </span>
              <span>{label}</span>
              {p.sub && <span class={`pill-sub ${p.subClass}`}>{p.sub}</span>}
            </button>
          );
        })}
      </div>
    </div>
  );
}
