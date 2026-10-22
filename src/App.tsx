import { useEffect, useRef, useState } from 'preact/hooks';
import { GeneralToggles } from './components/GeneralToggles';
import { MetricPills } from './components/MetricPills';
import { Preview } from './components/Preview';
import { Segmented } from './components/Segmented';
import { Slider } from './components/Slider';
import { StatusRow } from './components/StatusRow';
import { TitleBar } from './components/TitleBar';
import { api } from './ipc';
import { exampleSample, nextExample } from './mock';
import type { AppState, Metrics, MetricsSample, Settings, Style } from './types';

/** Partial settings for `update_settings`, which merges nested objects. */
type SettingsPatch = Partial<Omit<Settings, 'metrics' | 'general'>> & {
  metrics?: Partial<Metrics>;
  general?: Partial<Settings['general']>;
};

const merge = (s: Settings, p: SettingsPatch): Settings => ({
  ...s,
  ...p,
  metrics: { ...s.metrics, ...p.metrics },
  general: { ...s.general, ...p.general },
});

/** A detected game whose first sample hasn't arrived yet: every value is "—". */
const NO_DATA: MetricsSample = { fps: null, frametimeMs: null, graph: [], graphMaxMs: 0, gpuUse: null, gpuTemp: null, cpuUse: null, cpuTemp: null };

const STYLES: [Style, string][] = [
  ['line', 'Linha'],
  ['card', 'Card'],
];

/** Animated example values for the preview while no game is detected (spec 4.3). */
function useExampleSample(active: boolean): MetricsSample {
  const [sample, setSample] = useState(exampleSample);
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setSample(nextExample), 500);
    return () => clearInterval(id);
  }, [active]);
  return sample;
}

/** Static shortcut row. Task 15 replaces it with the capture field (ShortcutField). */
function ShortcutRowPlaceholder({ shortcut }: { shortcut: string[] }) {
  return (
    <div class="shortcut">
      <div class="shortcut-text">
        <span class="shortcut-title">Atalho mostrar/esconder</span>
        <span class="shortcut-hint">Clique no campo e pressione a nova combinação</span>
      </div>
      <button class="shortcut-field" tabIndex={-1}>
        {shortcut.map((k) => (
          <span key={k} class="keycap">
            {k}
          </span>
        ))}
      </button>
    </div>
  );
}

export function App() {
  const [st, setSt] = useState<AppState | null>(null);
  const [live, setLive] = useState<MetricsSample | null>(null);
  // Settings updates still waiting for their reply. Meanwhile the screen keeps the local value,
  // and the last reply (not an older `settings` event) becomes the state.
  const pending = useRef(0);
  const example = useExampleSample(st != null && st.status.gameName == null);

  useEffect(() => {
    let alive = true;
    const offs = [
      api.on('status', (status) => setSt((s) => s && { ...s, status })),
      api.on('availability', (availability) => setSt((s) => s && { ...s, availability })),
      api.on('driver', (driver) => setSt((s) => s && { ...s, driver })),
      api.on('settings', (settings) => {
        if (pending.current === 0) setSt((s) => s && { ...s, settings });
      }),
      api.on('metrics', setLive),
    ];
    api.getState().then((s) => {
      if (alive) setSt(s);
    });
    return () => {
      alive = false;
      for (const off of offs) off.then((f) => f());
    };
  }, []);

  if (!st) {
    return (
      <div class="app">
        <TitleBar />
      </div>
    );
  }

  const { settings, status } = st;
  const update = (patch: SettingsPatch) => {
    setSt((s) => s && { ...s, settings: merge(s.settings, patch) });
    pending.current++;
    api.updateSettings(patch).then(
      (next) => {
        if (--pending.current === 0) setSt((s) => s && { ...s, settings: next });
      },
      (err) => {
        console.error('update_settings failed:', err);
        if (--pending.current === 0) api.getState().then(setSt);
      },
    );
  };

  return (
    <div class="app">
      <TitleBar />
      <div class="panel">
        <StatusRow status={status} shortcut={settings.shortcut} onToggle={() => api.setOverlayVisible(!status.overlayVisible)} />
        <Preview
          settings={settings}
          availability={st.availability}
          driver={st.driver}
          overlayVisible={status.overlayVisible}
          sample={status.gameName != null ? (live ?? NO_DATA) : example}
          onPos={(pos) => update({ pos })}
        />
        <MetricPills
          metrics={settings.metrics}
          availability={st.availability}
          driver={st.driver}
          onToggle={(k) => update({ metrics: { [k]: !settings.metrics[k] } })}
        />
        <div class="controls">
          <Segmented label="Estilo" options={STYLES} value={settings.style} onChange={(style) => update({ style })} />
          <Slider label="Tamanho" value={settings.size} min={75} max={200} step={5} onChange={(size) => update({ size })} />
          <Slider label="Opacidade" value={settings.opacity} min={30} max={100} step={5} onChange={(opacity) => update({ opacity })} />
        </div>
        <ShortcutRowPlaceholder shortcut={settings.shortcut} />
        <GeneralToggles general={settings.general} onToggle={(k) => update({ general: { [k]: !settings.general[k] } })} />
      </div>
    </div>
  );
}
