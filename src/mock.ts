// In-browser stand-in for the Tauri backend (`npm run dev` outside Tauri), plus the animated
// example values the preview shows while no game is detected (the prototype's `tick()`).
//
// URL states: ?driver=missing|failed|installed, ?igpu=1 (GPU temp. without sensor), ?nogame=1,
// ?waiting=1 (no game, overlay on and waiting for it), ?hotkeyError=1, ?installFails=1.
import type { Api } from './ipc';
import type { AppState, Events, MetricsSample, Settings } from './types';

const DEFAULT_SETTINGS: Settings = {
  version: 1,
  metrics: { fps: true, graph: true, gpuUse: true, gpuTemp: true, cpuUse: true, cpuTemp: false },
  style: 'card',
  pos: 'tr',
  size: 100,
  opacity: 80,
  shortcut: ['Ctrl', 'Shift', 'O'],
  general: { startup: true, minimized: true, autoShow: true, hideOutsideGame: true },
  gameRules: { games: [], notGames: [] },
};

// Same texts as crates/engine/src/model.rs and hotkey.rs.
const REASON_NO_GPU_TEMP = 'Esta GPU não informa a temperatura.';
const KNOWN_CONFLICTS: Record<string, string> = {
  'Ctrl+Shift+M': 'Discord',
  'Alt+Z': 'NVIDIA App',
  'Alt+R': 'AMD Software',
  'Win+G': 'Xbox Game Bar',
  'Ctrl+Shift+Esc': 'Windows',
  'Ctrl+Shift+S': 'Steam',
  'Shift+Tab': 'Steam',
};
const MODIFIERS = ['Ctrl', 'Alt', 'Shift', 'Win'];

/** Example frametime axis, fixed at 0–14 ms as in the prototype. */
const EXAMPLE_GRAPH_MAX_MS = 14;

/** First example values (the prototype's initial state). */
export function exampleSample(): MetricsSample {
  const graph = Array.from({ length: 48 }, () => 6.6 + Math.random() * 0.8);
  return {
    fps: 142,
    frametimeMs: graph[graph.length - 1],
    graph,
    graphMaxMs: EXAMPLE_GRAPH_MAX_MS,
    gpuUse: 96,
    gpuTemp: 71,
    cpuUse: 38,
    cpuTemp: 64,
  };
}

/** Next example values, every 500 ms: small jitter and occasional FPS dips. */
export function nextExample(s: MetricsSample): MetricsSample {
  const spike = Math.random() < 0.05;
  const fps = Math.round(spike ? 96 + Math.random() * 20 : 138 + Math.random() * 9);
  const ms = 1000 / fps + (Math.random() - 0.5) * 0.4;
  const j = (v: number | null, d: number, lo: number, hi: number) =>
    Math.max(lo, Math.min(hi, Math.round((v ?? lo) + (Math.random() - 0.5) * d)));
  return {
    fps,
    frametimeMs: ms,
    graph: [...s.graph.slice(1), ms],
    graphMaxMs: EXAMPLE_GRAPH_MAX_MS,
    gpuUse: j(s.gpuUse, 4, 88, 99),
    gpuTemp: j(s.gpuTemp, 2, 68, 75),
    cpuUse: j(s.cpuUse, 6, 28, 52),
    cpuTemp: j(s.cpuTemp, 2, 60, 69),
  };
}

/** Recursive object merge, like `settings::apply_patch` in Rust. */
function merge(base: unknown, patch: unknown): unknown {
  if (base && patch && typeof base === 'object' && typeof patch === 'object' && !Array.isArray(base) && !Array.isArray(patch)) {
    const out: Record<string, unknown> = { ...(base as Record<string, unknown>) };
    for (const [k, v] of Object.entries(patch)) out[k] = k in out ? merge(out[k], v) : v;
    return out;
  }
  return patch;
}

/** Rounds to the nearest multiple of 5, then clamps (Settings::normalized). */
const snap = (v: number, min: number, max: number) => Math.max(min, Math.min(max, Math.round(v / 5) * 5));

export function createMock(q: URLSearchParams): Api {
  const driverParam = q.get('driver');
  const waiting = q.get('waiting') === '1';
  const st: AppState = {
    settings: structuredClone(DEFAULT_SETTINGS),
    // Visible, so the screen matches the prototype's initial "Overlay ativo".
    status: {
      overlayVisible: true,
      overlayWaiting: false,
      gameName: q.get('nogame') === '1' || waiting ? null : 'The Witcher 3',
      lastAppName: 'Hollow Knight',
    },
    availability: {
      fps: { state: 'ok' },
      gpuUse: { state: 'ok' },
      gpuTemp: q.get('igpu') === '1' ? { state: 'noSensor', reason: REASON_NO_GPU_TEMP } : { state: 'ok' },
      cpuUse: { state: 'ok' },
      cpuTemp: { state: 'ok' },
    },
    driver: driverParam === 'failed' || driverParam === 'installed' ? driverParam : 'missing',
    hotkeyError: q.get('hotkeyError') === '1' ? 'Ctrl + Shift + O já é usado por outro app. Escolha outra.' : null,
  };
  const installFails = q.get('installFails') === '1';
  st.status.overlayWaiting = waitingNow();

  const listeners: { [K in keyof Events]: Set<(p: Events[K]) => void> } = {
    status: new Set(),
    metrics: new Set(),
    availability: new Set(),
    driver: new Set(),
    settings: new Set(),
  };
  const emit = <K extends keyof Events>(e: K, p: Events[K]) => {
    for (const cb of listeners[e]) cb(structuredClone(p));
  };

  // The mock pretends the detected game is in front, so the overlay only waits when there is none.
  function waitingNow() {
    return st.status.overlayVisible && st.settings.general.hideOutsideGame && st.status.gameName == null;
  }
  const refreshWaiting = () => {
    const overlayWaiting = waitingNow();
    if (overlayWaiting === st.status.overlayWaiting) return;
    st.status = { ...st.status, overlayWaiting };
    emit('status', st.status);
  };
  // Fake path for the rules lists, which hold paths in the real engine.
  const fakePath = (name: string) => `C:\\Games\\${name}.exe`;
  const without = (list: string[], path: string) => list.filter((p) => p !== path);

  // Like the controller's sample timer: the game's values while one is detected, sensors only otherwise.
  let sample = exampleSample();
  setInterval(() => {
    sample = nextExample(sample);
    emit('metrics', st.status.gameName != null ? sample : { ...sample, fps: null, frametimeMs: null, graph: [] });
  }, 500);

  return {
    getState: async () => structuredClone(st),
    async updateSettings(patch) {
      const next = merge(st.settings, patch) as Settings;
      st.settings = {
        ...next,
        size: snap(next.size, 75, 200),
        opacity: snap(next.opacity, 30, 100),
        // The shortcut only changes through trySetHotkey, as in the controller.
        shortcut: st.settings.shortcut,
      };
      emit('settings', st.settings);
      refreshWaiting();
      return structuredClone(st.settings);
    },
    async setOverlayVisible(visible) {
      if (st.status.overlayVisible === visible) return;
      st.status = { ...st.status, overlayVisible: visible };
      st.status.overlayWaiting = waitingNow();
      emit('status', st.status);
    },
    async markNotGame() {
      const name = st.status.gameName;
      if (name == null) throw 'Nenhum jogo detectado.';
      const path = fakePath(name);
      const { games, notGames } = st.settings.gameRules;
      st.settings = { ...st.settings, gameRules: { games: without(games, path), notGames: [...without(notGames, path), path] } };
      st.status = { ...st.status, lastAppName: name, gameName: null };
      st.status.overlayWaiting = waitingNow();
      emit('settings', st.settings);
      emit('status', st.status);
      return structuredClone(st.settings);
    },
    async markGame() {
      const name = st.status.lastAppName;
      if (name == null) throw 'Nenhum app para marcar.';
      const path = fakePath(name);
      const { games, notGames } = st.settings.gameRules;
      st.settings = { ...st.settings, gameRules: { games: [...without(games, path), path], notGames: without(notGames, path) } };
      emit('settings', st.settings);
      // Detection takes a moment to pick the app up, like the controller's poll.
      setTimeout(() => {
        st.status = { ...st.status, gameName: name };
        st.status.overlayWaiting = waitingNow();
        emit('status', st.status);
      }, 2000);
      return structuredClone(st.settings);
    },
    async beginHotkeyCapture() {},
    async endHotkeyCapture() {},
    async trySetHotkey(combo) {
      if (!combo.some((k) => MODIFIERS.includes(k))) {
        return { ok: false, error: 'Use pelo menos Ctrl, Alt ou Shift junto com a tecla.' };
      }
      const app = KNOWN_CONFLICTS[combo.join('+')];
      if (app) return { ok: false, error: `${combo.join(' + ')} já é usado por ${app}. Escolha outra.` };
      st.settings = { ...st.settings, shortcut: [...combo] };
      st.hotkeyError = null;
      emit('settings', st.settings);
      return { ok: true, error: null };
    },
    // Returns right away and reports through `driver`, like the controller's install thread.
    async installDriver() {
      if (st.driver === 'installing') return;
      st.driver = 'installing';
      emit('driver', st.driver);
      setTimeout(() => {
        if (installFails) {
          st.driver = 'failed';
          emit('driver', st.driver);
          return;
        }
        st.driver = 'installed';
        st.settings = { ...st.settings, metrics: { ...st.settings.metrics, cpuTemp: true } };
        emit('availability', st.availability);
        emit('settings', st.settings);
        emit('driver', st.driver);
      }, 2600);
    },
    // The browser can't minimize or close the page; the real window does it in Tauri.
    async minimize() {},
    async close() {},
    async on(e, cb) {
      listeners[e].add(cb);
      return () => listeners[e].delete(cb);
    },
  };
}
