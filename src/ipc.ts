// Commands and events of the Tauri shell (src-tauri/src/commands.rs); the browser uses the mock.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { createMock } from './mock';
import type { AppState, Events, HotkeyResult, Settings } from './types';

export interface Api {
  getState(): Promise<AppState>;
  updateSettings(patch: object): Promise<Settings>;
  setOverlayVisible(v: boolean): Promise<void>;
  beginHotkeyCapture(): Promise<void>;
  endHotkeyCapture(): Promise<void>;
  trySetHotkey(combo: string[]): Promise<HotkeyResult>;
  installDriver(): Promise<void>;
  markNotGame(): Promise<Settings>;
  markGame(): Promise<Settings>;
  minimize(): Promise<void>;
  close(): Promise<void>;
  on<K extends keyof Events>(e: K, cb: (p: Events[K]) => void): Promise<() => void>;
}

function tauriApi(): Api {
  return {
    getState: () => invoke<AppState>('get_state'),
    updateSettings: (patch) => invoke<Settings>('update_settings', { patch }),
    setOverlayVisible: (visible) => invoke<void>('set_overlay_visible', { visible }),
    beginHotkeyCapture: () => invoke<void>('begin_hotkey_capture'),
    endHotkeyCapture: () => invoke<void>('end_hotkey_capture'),
    trySetHotkey: (combo) => invoke<HotkeyResult>('try_set_hotkey', { combo }),
    installDriver: () => invoke<void>('install_driver'),
    markNotGame: () => invoke<Settings>('mark_not_game'),
    markGame: () => invoke<Settings>('mark_game'),
    minimize: () => getCurrentWindow().minimize(),
    // Destroys the window and its WebView (spec 4.3); the app stays in the tray.
    close: () => getCurrentWindow().close(),
    on: (e, cb) => listen<Events[typeof e]>(e, (ev) => cb(ev.payload)),
  };
}

export const api: Api = '__TAURI_INTERNALS__' in window ? tauriApi() : createMock(new URLSearchParams(location.search));
