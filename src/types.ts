// Mirror of the Rust types exchanged with the UI (crates/engine/src/model.rs and settings.rs).
export type Style = 'line' | 'card';
export type Pos = 'tl' | 'tc' | 'tr' | 'ml' | 'mr' | 'bl' | 'bc' | 'br';
export interface Metrics { fps: boolean; graph: boolean; gpuUse: boolean; gpuTemp: boolean; cpuUse: boolean; cpuTemp: boolean }
export interface Settings { version: number; metrics: Metrics; style: Style; pos: Pos; size: number; opacity: number; shortcut: string[]; general: { startup: boolean; minimized: boolean; autoShow: boolean; hideOutsideGame: boolean }; gameRules: { games: string[]; notGames: string[] } }
export interface Status { overlayVisible: boolean; overlayWaiting: boolean; gameName: string | null; lastAppName: string | null }
export type Avail = { state: 'ok' } | { state: 'noSensor'; reason: string };
export interface Availability { fps: Avail; gpuUse: Avail; gpuTemp: Avail; cpuUse: Avail; cpuTemp: Avail }
export type DriverState = 'missing' | 'installing' | 'failed' | 'installed';
export interface MetricsSample { fps: number | null; frametimeMs: number | null; graph: number[]; graphMaxMs: number; gpuUse: number | null; gpuTemp: number | null; cpuUse: number | null; cpuTemp: number | null }
export interface AppState { settings: Settings; status: Status; availability: Availability; driver: DriverState; hotkeyError: string | null }
export interface HotkeyResult { ok: boolean; error: string | null }
export interface Events { status: Status; metrics: MetricsSample; availability: Availability; driver: DriverState; settings: Settings }
