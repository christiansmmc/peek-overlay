import { useEffect, useRef, useState } from 'preact/hooks';
import { DriverSheet } from './components/DriverSheet';
import { GeneralToggles } from './components/GeneralToggles';
import { MetricPills } from './components/MetricPills';
import { Preview } from './components/Preview';
import { Segmented } from './components/Segmented';
import { Slider } from './components/Slider';
import { ShortcutField } from './components/ShortcutField';
import { StatusRow } from './components/StatusRow';
import { TitleBar } from './components/TitleBar';
import { Toast } from './components/Toast';
import { api } from './ipc';
import { exampleSample, nextExample } from './mock';
import type { AppState, DriverState, Metrics, MetricsSample, Settings, Style } from './types';

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

export function App() {
  const [st, setSt] = useState<AppState | null>(null);
  const [live, setLive] = useState<MetricsSample | null>(null);
  // Settings updates still waiting for their reply. Meanwhile the screen keeps the local value,
  // and the last reply (not an older `settings` event) becomes the state.
  const pending = useRef(0);
  const [sheetOpen, setSheetOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const driverNow = useRef<DriverState>('missing');
  const example = useExampleSample(st != null && st.status.gameName == null);

  const flash = (text: string) => {
    clearTimeout(toastTimer.current);
    setToast(text);
    toastTimer.current = setTimeout(() => setToast(null), 2600);
  };

  useEffect(() => {
    let alive = true;
    const offs = [
      api.on('status', (status) => setSt((s) => s && { ...s, status })),
      api.on('availability', (availability) => setSt((s) => s && { ...s, availability })),
      api.on('driver', (driver) => {
        const wasInstalling = driverNow.current === 'installing';
        driverNow.current = driver;
        setSt((s) => s && { ...s, driver });
        if (driver === 'installed' && wasInstalling) {
          setSheetOpen(false);
          flash('Driver instalado. Temperatura da CPU ativada.');
        }
      }),
      api.on('settings', (settings) => {
        if (pending.current === 0) setSt((s) => s && { ...s, settings });
      }),
      api.on('metrics', setLive),
    ];
    api.getState().then((s) => {
      if (!alive) return;
      driverNow.current = s.driver;
      setSt(s);
    });
    return () => {
      alive = false;
      clearTimeout(toastTimer.current);
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
        <StatusRow
          status={status}
          onToggle={() => api.setOverlayVisible(!status.overlayVisible)}
          onMarkNotGame={() => api.markNotGame().catch((e) => console.error('mark_not_game failed:', e))}
          onMarkGame={() => api.markGame().catch((e) => console.error('mark_game failed:', e))}
        />
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
          onNeedsDriver={() => setSheetOpen(true)}
        />
        <div class="controls">
          <Segmented label="Estilo" options={STYLES} value={settings.style} onChange={(style) => update({ style })} />
          <Slider label="Tamanho" value={settings.size} min={75} max={200} step={5} onChange={(size) => update({ size })} />
          <Slider label="Opacidade" value={settings.opacity} min={30} max={100} step={5} onChange={(opacity) => update({ opacity })} />
        </div>
        <ShortcutField
          shortcut={settings.shortcut}
          hotkeyError={st.hotkeyError}
          onChanged={(combo) => {
            setSt((s) => s && { ...s, hotkeyError: null });
            flash(`Atalho alterado para ${combo.join(' + ')}`);
          }}
        />
        <GeneralToggles general={settings.general} onToggle={(k) => update({ general: { [k]: !settings.general[k] } })} />
      </div>
      {toast && <Toast text={toast} />}
      {sheetOpen && (
        <DriverSheet
          driver={st.driver}
          onInstall={() => api.installDriver().catch((e) => console.error('install_driver failed:', e))}
          onClose={() => setSheetOpen(false)}
        />
      )}
    </div>
  );
}
