//! Orchestrator (spec 4.1, 4.5 and 4.6): ties settings, hotkey, game detection, frame capture,
//! sensors and the overlay together, and reports to the UI through a `UiSink`.
//!
//! Locking:
//! - `inner` holds all state. `sync_gate` serializes `sync()`, and with it every frame-capture
//!   start and stop. It is always taken before `inner`, never while holding it.
//! - No lock is held while calling `NativeHandle::register_hotkey`: the native thread may be
//!   waiting for `inner` inside an event.
//! - `FrameCapture::stop` (it joins the ETW threads), dropping an `ExitWatch` and UI events all
//!   happen after `inner` is released.

use crate::autostart;
use crate::effects::{Effects, effective_metrics, plan_effects};
use crate::frames::clock::now_ns;
use crate::frames::etw::{CaptureError, FrameCapture, FrameShared};
use crate::frames::stats::graph_y_max;
use crate::game::{Fg, Game, GameDetector};
use crate::hotkey::{self, HotkeyError};
use crate::model::{
    AppState, Avail, Availability, DriverState, HotkeyResult, MetricsSample, REASON_NEEDS_ADMIN, Status,
};
use crate::native::foreground::{self, ForegroundInfo};
use crate::native::process_watch::{ExitWatch, watch_exit};
use crate::native::{NativeCmd, NativeEvent, NativeHandle, TimerKind, Timers};
use crate::overlay::window::{OverlayCmd, OverlayConfig};
use crate::sensors::Sensors;
use crate::settings::{self, Metrics, Settings};
use crate::{pawnio, process_info};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

/// Columns of the frametime graph sent to the overlay (the Card's width; the Line downsamples).
pub const GRAPH_COLUMNS: usize = 126;

/// Receives the events for the settings window (`"status"`, `"metrics"`, `"availability"`,
/// `"driver"` and `"settings"`).
pub trait UiSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Message for the shortcut field when the saved shortcut could not be registered.
pub fn startup_hotkey_error(combo: &[String], r: &Result<(), HotkeyError>) -> Option<String> {
    r.as_ref().err().map(|e| e.message(combo))
}

struct Inner {
    settings: Settings,
    /// Last status emitted to the UI.
    status: Status,
    availability: Availability,
    driver: DriverState,
    hotkey_error: Option<String>,
    /// The settings page is capturing a new shortcut, with the saved one unregistered.
    capturing: bool,
    detector: GameDetector,
    config_open: bool,
    /// Display name of the detected game.
    game_name: Option<String>,
    /// Latest foreground window: fallback for the game's title once its window is gone.
    last_fg: Option<ForegroundInfo>,
    /// Newest sample: refreshed by the graph timer and drawn first when the overlay shows.
    sample: MetricsSample,
    /// `start` ran: the native thread, the sensors and the frame capture exist.
    started: bool,
    shut_down: bool,
    sensors: Option<Sensors>,
    capture: Option<FrameCapture>,
    /// ETW needs elevation, which never changes for a process: once denied, never retried.
    capture_denied: bool,
    /// Starting the capture failed for another reason; retried when it is needed again.
    capture_failed: bool,
    /// Game whose presents feed the frame tracker and whose exit is watched.
    synced_game: Option<u32>,
    exit_watch: Option<ExitWatch>,
    // What the native thread was told last (only after `start`), so `sync` sends only changes.
    sent_timers: Timers,
    sent_config: Option<OverlayConfig>,
    sent_visible: Option<bool>,
    sent_anchor: Option<Option<isize>>,
    sent_watch: Option<(u32, isize)>,
}

impl Inner {
    fn effective(&self) -> Metrics {
        effective_metrics(&self.settings.metrics, &self.availability, self.driver)
    }

    fn overlay_config(&self) -> OverlayConfig {
        let s = &self.settings;
        OverlayConfig { style: s.style, pos: s.pos, size: s.size, opacity: s.opacity, metrics: self.effective() }
    }

    /// What must run now. A candidate needs ETW to be confirmed, so without a capture it is moot.
    fn plan(&self) -> Effects {
        let can_capture = !self.capture_denied && !self.capture_failed;
        let candidate = can_capture && self.detector.candidate_pid().is_some();
        plan_effects(self.detector.visible(), self.config_open, candidate, &self.effective())
    }
}

pub struct Controller {
    me: Weak<Controller>,
    settings_path: PathBuf,
    sink: Arc<dyn UiSink>,
    /// File name of this executable, which detection ignores.
    self_exe: String,
    frames: Arc<FrameShared>,
    inner: Mutex<Inner>,
    sync_gate: Mutex<()>,
    native: Mutex<Option<Arc<NativeHandle>>>,
}

/// A panic on one thread must not take every later call down with it.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Creates or removes the logon task. Failing (no elevation in debug builds) is not an error
/// for the user. Skipped in tests, which must never touch the machine's scheduled tasks.
fn apply_startup(on: bool) {
    if cfg!(test) {
        return;
    }
    let result = if on {
        std::env::current_exe().map_err(|e| e.to_string()).and_then(|exe| autostart::enable(&exe))
    } else {
        autostart::disable()
    };
    if let Err(e) = result {
        crate::trace!("controller: startup task not updated: {e}");
    }
}

fn json<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

impl Controller {
    /// A controller that is not running yet: its methods work on the state alone until `start`.
    pub fn new(settings: Settings, settings_path: PathBuf, sink: Arc<dyn UiSink>) -> Arc<Controller> {
        let self_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Peek.exe".to_string());
        Self::with_self_exe(settings, settings_path, sink, self_exe)
    }

    /// `new`, ignoring `self_exe` instead of this executable (tests drive windows of their own process).
    fn with_self_exe(
        settings: Settings,
        settings_path: PathBuf,
        sink: Arc<dyn UiSink>,
        self_exe: String,
    ) -> Arc<Controller> {
        let detector = GameDetector::new(settings.general.auto_show);
        Arc::new_cyclic(|me| Controller {
            me: me.clone(),
            settings_path,
            sink,
            self_exe,
            frames: FrameShared::new(),
            inner: Mutex::new(Inner {
                settings,
                status: Status::default(),
                availability: Availability::all_ok(),
                driver: DriverState::Missing,
                hotkey_error: None,
                capturing: false,
                detector,
                config_open: false,
                game_name: None,
                last_fg: None,
                sample: MetricsSample::default(),
                started: false,
                shut_down: false,
                sensors: None,
                capture: None,
                capture_denied: false,
                capture_failed: false,
                synced_game: None,
                exit_watch: None,
                sent_timers: Timers::default(),
                sent_config: None,
                sent_visible: None,
                sent_anchor: None,
                sent_watch: None,
            }),
            sync_gate: Mutex::new(()),
            native: Mutex::new(None),
        })
    }

    /// Opens the sensors, starts the native thread (hotkey, timers, focus hook, overlay window)
    /// and registers the saved shortcut; a shortcut in use only sets `hotkey_error`.
    pub fn start(self: &Arc<Self>) {
        {
            let g = self.inner();
            if g.started || g.shut_down {
                return;
            }
        }
        let sensors = Sensors::new();
        let driver = if pawnio::is_installed() { DriverState::Installed } else { DriverState::Missing };
        let me = self.me.clone();
        let native = Arc::new(NativeHandle::spawn(Box::new(move |event| {
            if let Some(c) = me.upgrade() {
                c.on_native_event(event);
            }
        })));
        *lock(&self.native) = Some(Arc::clone(&native));
        let combo = self.inner().settings.shortcut.clone();
        let registered = hotkey::parse(&combo).and_then(|hk| native.register_hotkey(hk));
        drop(native);
        {
            let mut g = self.inner();
            let sensed = sensors.availability();
            g.availability = Availability {
                fps: g.availability.fps.clone(),
                gpu_use: sensed.gpu_use,
                gpu_temp: sensed.gpu_temp,
                cpu_use: sensed.cpu_use,
                cpu_temp: sensed.cpu_temp,
            };
            g.sensors = Some(sensors);
            g.driver = driver;
            g.hotkey_error = startup_hotkey_error(&combo, &registered);
            g.started = true;
        }
        // The first sync configures the overlay (creating its window) with the effective metrics.
        self.sync();
        // Recreating the task refreshes the exe path and covers the first run. No lock is held.
        if self.inner().settings.general.startup {
            apply_startup(true);
        }
    }

    /// Stops everything: the native thread (hotkey, timers, windows), the ETW session and the
    /// exit watch. Idempotent. It joins the native thread, so never call it from a native event.
    pub fn shutdown(&self) {
        {
            let mut g = self.inner();
            if g.shut_down {
                return;
            }
            g.shut_down = true;
        }
        let native = lock(&self.native).take();
        if let Some(native) = native {
            match Arc::try_unwrap(native) {
                Ok(native) => native.quit(),
                // Another thread is mid-call with a clone: ask the thread to quit without waiting.
                Err(native) => native.send(NativeCmd::Quit),
            }
        }
        let (capture, watch) = {
            // Waits for a sync in flight; no later sync starts anything once `shut_down` is set.
            let _gate = lock(&self.sync_gate);
            let mut g = self.inner();
            g.sensors = None;
            (g.capture.take(), g.exit_watch.take())
        };
        if let Some(capture) = capture {
            capture.stop();
        }
        drop(watch);
    }

    pub fn state(&self) -> AppState {
        let g = self.inner();
        AppState {
            settings: g.settings.clone(),
            status: g.status.clone(),
            availability: g.availability.clone(),
            driver: g.driver,
            hotkey_error: g.hotkey_error.clone(),
        }
    }

    /// Applies a partial settings object, saves it and emits `"settings"`. The shortcut is kept:
    /// it only changes through `try_set_hotkey`, which registers it first.
    pub fn update_settings(&self, patch: serde_json::Value) -> Result<Settings, String> {
        let (settings, startup_changed) = {
            let mut g = self.inner();
            let mut next = settings::apply_patch(&g.settings, patch)?;
            next.shortcut = g.settings.shortcut.clone();
            settings::save(&self.settings_path, &next).map_err(|e| e.to_string())?;
            g.detector.set_auto_show(next.general.auto_show);
            let startup_changed = next.general.startup != g.settings.general.startup;
            g.settings = next.clone();
            (next, startup_changed)
        };
        // `schtasks` takes ~100 ms: run it with `inner` released.
        if startup_changed {
            apply_startup(settings.general.startup);
        }
        self.sync();
        self.emit("settings", &settings);
        Ok(settings)
    }

    pub fn toggle_overlay(&self) {
        self.inner().detector.toggle();
        self.sync();
    }

    pub fn set_overlay_visible(&self, visible: bool) {
        self.inner().detector.set_visible(visible);
        self.sync();
    }

    /// While the page captures a new shortcut, the current one must not be consumed by the system.
    pub fn begin_hotkey_capture(&self) {
        self.inner().capturing = true;
        self.send(NativeCmd::UnregisterHotkey);
    }

    /// Registers the saved shortcut again (the new one after a successful `try_set_hotkey`).
    /// Idempotent.
    pub fn end_hotkey_capture(&self) {
        let combo = {
            let mut g = self.inner();
            g.capturing = false;
            g.settings.shortcut.clone()
        };
        let registered = self.register(&combo);
        self.inner().hotkey_error = startup_hotkey_error(&combo, &registered);
    }

    /// Ends the capture too: whatever the outcome, a shortcut is registered on return.
    pub fn try_set_hotkey(&self, combo: Vec<String>) -> HotkeyResult {
        let old = {
            let mut g = self.inner();
            g.capturing = false;
            g.settings.shortcut.clone()
        };
        let attempt = hotkey::parse(&combo).and_then(|hk| match hotkey::known_conflict(&combo) {
            Some(app) => Err(HotkeyError::KnownConflict { app }),
            None => self.native().map_or(Ok(()), |native| native.register_hotkey(hk)),
        });
        if let Err(e) = attempt {
            // A failed RegisterHotKey has already dropped the old shortcut.
            let registered = self.register(&old);
            self.inner().hotkey_error = startup_hotkey_error(&old, &registered);
            return HotkeyResult { ok: false, error: Some(e.message(&combo)) };
        }
        let settings = {
            let mut g = self.inner();
            g.settings.shortcut = combo;
            if let Err(e) = settings::save(&self.settings_path, &g.settings) {
                crate::trace!("controller: saving the shortcut failed: {e}");
            }
            g.hotkey_error = None;
            g.settings.clone()
        };
        self.emit("settings", &settings);
        HotkeyResult { ok: true, error: None }
    }

    /// Installs PawnIO with winget on a background thread, reporting through `"driver"`.
    pub fn install_driver(self: &Arc<Self>) {
        {
            let mut g = self.inner();
            if g.driver == DriverState::Installing {
                return;
            }
            g.driver = DriverState::Installing;
        }
        self.emit("driver", &DriverState::Installing);
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("pawnio-install".into())
            .spawn(move || me.finish_install(pawnio::install::run_install()));
        if let Err(e) = spawned {
            self.finish_install(Err(e.to_string()));
        }
    }

    /// Closing the window mid-capture (Alt+F4: no blur, no IPC) ends the capture, or the app
    /// would stay without its shortcut until restarted.
    pub fn set_config_open(&self, open: bool) {
        let abandoned_capture = {
            let mut g = self.inner();
            g.config_open = open;
            !open && g.capturing
        };
        if abandoned_capture {
            self.end_hotkey_capture();
        }
        self.sync();
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    fn native(&self) -> Option<Arc<NativeHandle>> {
        lock(&self.native).clone()
    }

    /// Without a native thread (before `start`, after `shutdown`) commands are dropped.
    fn send(&self, cmd: NativeCmd) {
        if let Some(native) = self.native() {
            native.send(cmd);
        }
    }

    fn emit<T: Serialize>(&self, event: &str, payload: &T) {
        self.sink.emit(event, json(payload));
    }

    /// Registers `combo` as the global hotkey (no-op without a native thread). Never call it
    /// holding `inner`, nor from the native thread.
    fn register(&self, combo: &[String]) -> Result<(), HotkeyError> {
        let hk = hotkey::parse(combo)?;
        self.native().map_or(Ok(()), |native| native.register_hotkey(hk))
    }

    fn finish_install(&self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                let (availability, settings) = {
                    let mut g = self.inner();
                    if let Some(sensors) = g.sensors.as_mut() {
                        sensors.reload_cpu_temp();
                        g.availability.cpu_temp = sensors.availability().cpu_temp;
                    }
                    g.driver = DriverState::Installed;
                    g.settings.metrics.cpu_temp = true;
                    if let Err(e) = settings::save(&self.settings_path, &g.settings) {
                        crate::trace!("controller: saving settings failed: {e}");
                    }
                    (g.availability.clone(), g.settings.clone())
                };
                self.sync();
                self.emit("availability", &availability);
                self.emit("settings", &settings);
                self.emit("driver", &DriverState::Installed);
            }
            Err(e) => {
                crate::trace!("controller: PawnIO install failed: {e}");
                self.inner().driver = DriverState::Failed;
                self.emit("driver", &DriverState::Failed);
            }
        }
    }

    /// Runs on the native thread: never registers the hotkey from here.
    fn on_native_event(&self, event: NativeEvent) {
        match event {
            NativeEvent::Hotkey => self.toggle_overlay(),
            NativeEvent::Foreground(info) => self.on_foreground(info),
            NativeEvent::Timer(TimerKind::Detect) => self.on_detect_tick(),
            NativeEvent::Timer(TimerKind::Sample) => self.on_sample_tick(),
            NativeEvent::Timer(TimerKind::Graph) => self.on_graph_tick(),
            // The overlay window already redrew and placed itself on the native thread.
            NativeEvent::DisplayChanged => {}
        }
    }

    fn on_foreground(&self, info: ForegroundInfo) {
        crate::trace!(
            "controller: foreground pid={} exe={:?} covers_monitor={}",
            info.pid,
            info.exe_name,
            info.covers_monitor
        );
        {
            let mut g = self.inner();
            // A game whose exit can't be waited on (OpenProcess denied) ends when its window dies.
            // Not when its own process takes the focus: a live game may recreate its window.
            if let Some(game) = g.detector.game()
                && g.exit_watch.is_none()
                && info.pid != game.pid
                && !foreground::is_window_alive(game.hwnd)
            {
                g.detector.on_process_exited(game.pid);
            }
            let ignored = process_info::is_ignored(&info.exe_name, &self.self_exe);
            g.detector.on_foreground(Fg {
                pid: info.pid,
                hwnd: info.hwnd,
                covers_monitor: info.covers_monitor,
                ignored,
            });
            g.last_fg = Some(info);
        }
        self.send(NativeCmd::Overlay(OverlayCmd::ReassertTopmost));
        self.sync();
    }

    fn on_process_exited(&self, pid: u32) {
        self.inner().detector.on_process_exited(pid);
        self.sync();
    }

    fn on_detect_tick(&self) {
        {
            let mut g = self.inner();
            if let Some(pid) = g.detector.candidate_pid() {
                let continuous_ms = lock(&self.frames.observer).continuous_ms(pid, now_ns());
                g.detector.on_presenting(pid, continuous_ms);
            }
        }
        self.sync();
    }

    fn on_sample_tick(&self) {
        let (sample, overlay_shown, config_open) = {
            let mut g = self.inner();
            let sample = self.take_sample(&mut g);
            g.sample = sample.clone();
            (sample, g.sent_visible == Some(true), g.config_open)
        };
        if config_open {
            self.emit("metrics", &sample);
        }
        // A hidden overlay window would redraw on every Data for nothing.
        if overlay_shown {
            self.send(NativeCmd::Overlay(OverlayCmd::Data(sample)));
        }
    }

    fn on_graph_tick(&self) {
        let sample = {
            let mut g = self.inner();
            if g.sent_visible != Some(true) || !g.effective().graph {
                return;
            }
            let graph = lock(&self.frames.tracker).graph(now_ns(), GRAPH_COLUMNS);
            g.sample.graph_max_ms = graph_y_max(&graph);
            g.sample.graph = graph;
            g.sample.clone()
        };
        self.send(NativeCmd::Overlay(OverlayCmd::Data(sample)));
    }

    /// Reads the effective metrics: sensors, plus FPS, frametime and graph of the detected game.
    fn take_sample(&self, g: &mut Inner) -> MetricsSample {
        let wanted = g.effective();
        let game_pid = g.detector.game().map(|game| game.pid);
        let sensed = g.sensors.as_mut().map(|s| s.sample(&wanted, game_pid)).unwrap_or_default();
        let now = now_ns();
        let tracker = lock(&self.frames.tracker);
        let (fps, frametime_ms) =
            if wanted.fps { (tracker.fps(now), tracker.avg_frametime_ms(now)) } else { (None, None) };
        let graph = if wanted.graph { tracker.graph(now, GRAPH_COLUMNS) } else { Vec::new() };
        drop(tracker);
        MetricsSample {
            fps,
            frametime_ms,
            graph_max_ms: graph_y_max(&graph),
            graph,
            gpu_use: sensed.gpu_use,
            gpu_temp: sensed.gpu_temp,
            cpu_use: sensed.cpu_use,
            cpu_temp: sensed.cpu_temp,
        }
    }

    /// Brings the frame capture, timers, overlay and UI status in line with the state. Called
    /// after every change; sends only what changed.
    fn sync(&self) {
        let _gate = lock(&self.sync_gate);
        let mut events: Vec<(&str, serde_json::Value)> = Vec::new();
        let mut cmds: Vec<NativeCmd> = Vec::new();
        let (to_stop, old_watch) = {
            let mut g = self.inner();
            let live = g.started && !g.shut_down;

            // 1. The detected game changed: retarget the tracker, name it, watch its exit.
            let game = g.detector.game();
            let old_watch = if game.map(|x| x.pid) != g.synced_game { self.retarget(&mut g, game, live) } else { None };

            // 2-3. Effects, then the frame capture.
            let mut fx = g.plan();
            if live && fx.etw && g.capture.is_none() && !g.capture_denied && !g.capture_failed {
                match FrameCapture::start(Arc::clone(&self.frames)) {
                    Ok(capture) => {
                        crate::trace!("controller: frame capture started");
                        g.capture = Some(capture);
                    }
                    Err(CaptureError::AccessDenied) => {
                        crate::trace!("controller: frame capture needs elevation");
                        g.capture_denied = true;
                        g.availability.fps = Avail::NoSensor { reason: REASON_NEEDS_ADMIN.into() };
                        events.push(("availability", json(&g.availability)));
                    }
                    Err(CaptureError::Other(e)) => {
                        crate::trace!("controller: frame capture failed: {e}");
                        g.capture_failed = true;
                    }
                }
                if g.capture.is_none() {
                    fx = g.plan();
                }
            }
            if !fx.etw {
                g.capture_failed = false;
            }
            let to_stop = if fx.etw { None } else { g.capture.take() };

            if live {
                // 4. Timers. A sample timer that turns on primes the sensors: PDH rates need two
                // collects, so the first tick would otherwise show "—".
                let timers = Timers { sample: fx.sample_timer, graph: fx.graph_timer, detect: fx.detect_timer };
                if timers != g.sent_timers {
                    if timers.sample && !g.sent_timers.sample {
                        g.sample = self.take_sample(&mut g);
                    }
                    g.sent_timers = timers;
                    cmds.push(NativeCmd::SetTimers(timers));
                }

                // 5. A focused window that may still go fullscreen is watched for size changes.
                let watch = self.location_target(&g);
                if g.sent_watch != watch {
                    g.sent_watch = watch;
                    cmds.push(NativeCmd::WatchLocation(watch));
                }

                // 6. Overlay: configuration, then anchor and a fresh frame before it shows.
                let config = g.overlay_config();
                if g.sent_config.as_ref() != Some(&config) {
                    g.sent_config = Some(config.clone());
                    cmds.push(NativeCmd::Overlay(OverlayCmd::Configure(config)));
                }
                let visible = g.detector.visible();
                if visible {
                    let anchor = g.detector.anchor_hwnd();
                    if g.sent_anchor != Some(anchor) {
                        g.sent_anchor = Some(anchor);
                        cmds.push(NativeCmd::Overlay(OverlayCmd::Anchor(anchor)));
                    }
                }
                if g.sent_visible != Some(visible) {
                    if visible {
                        cmds.push(NativeCmd::Overlay(OverlayCmd::Data(g.sample.clone())));
                    }
                    g.sent_visible = Some(visible);
                    cmds.push(NativeCmd::Overlay(OverlayCmd::Show(visible)));
                }
            }

            // 7. Status.
            let status = Status { overlay_visible: g.detector.visible(), game_name: g.game_name.clone() };
            if status != g.status {
                events.push(("status", json(&status)));
                g.status = status;
            }
            (to_stop, old_watch)
        };
        for cmd in cmds {
            self.send(cmd);
        }
        if let Some(capture) = to_stop {
            capture.stop();
            crate::trace!("controller: frame capture stopped");
        }
        drop(old_watch);
        for (event, payload) in events {
            self.sink.emit(event, payload);
        }
    }

    /// The foreground window, unless it is ignored, already covers its monitor, is the detected
    /// game or is gone: spec 4.6 path 1 is a state, and a window may go fullscreen after the focus.
    fn location_target(&self, g: &Inner) -> Option<(u32, isize)> {
        let fg = g.last_fg.as_ref()?;
        let watch = !fg.covers_monitor
            && g.detector.game().map(|game| game.pid) != Some(fg.pid)
            && !process_info::is_ignored(&fg.exe_name, &self.self_exe)
            && foreground::is_window_alive(fg.hwnd);
        watch.then_some((fg.pid, fg.hwnd))
    }

    /// Points the frame tracker at the new game (or none), names it and replaces the exit watch.
    /// Returns the old watch, to be dropped once `inner` is released.
    fn retarget(&self, g: &mut Inner, game: Option<Game>, live: bool) -> Option<ExitWatch> {
        let pid = game.map(|x| x.pid);
        g.synced_game = pid;
        self.frames.set_target(pid);
        g.game_name = game.map(|x| game_name(x, g.last_fg.as_ref()));
        crate::trace!("controller: game pid={pid:?} name={:?}", g.game_name);
        let old = g.exit_watch.take();
        if let Some(game) = game
            && live
        {
            g.exit_watch = watch_exit(game.pid, self.exit_callback(game.pid));
        }
        old
    }

    fn exit_callback(&self, pid: u32) -> Box<dyn FnOnce() + Send> {
        let me = self.me.clone();
        Box::new(move || {
            // This runs on a thread-pool wait thread, and the sync that follows drops this very
            // watch, which waits for the callback to return: hand the work to another thread.
            let _ = std::thread::Builder::new().name("game-exit".into()).spawn(move || {
                if let Some(c) = me.upgrade() {
                    c.on_process_exited(pid);
                }
            });
        })
    }
}

/// `FileDescription` of the executable, else the window title, else the exe name (spec 4.6).
fn game_name(game: Game, last_fg: Option<&ForegroundInfo>) -> String {
    let info = foreground::describe(game.hwnd).or_else(|| last_fg.filter(|f| f.pid == game.pid).cloned());
    let description = process_info::exe_path(game.pid).as_deref().and_then(process_info::file_description);
    let title = info.as_ref().map(|i| i.title.as_str());
    let exe = info.as_ref().map(|i| i.exe_name.as_str());
    process_info::display_name(description.as_deref(), title, exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Pos;

    struct Rec(std::sync::Mutex<Vec<String>>);
    impl UiSink for Rec { fn emit(&self, e: &str, _: serde_json::Value) { self.0.lock().unwrap().push(e.into()); } }
    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn startup_hotkey_error_message() {
        assert_eq!(startup_hotkey_error(&s(&["Ctrl", "Shift", "O"]), &Err(HotkeyError::InUse)).as_deref(),
                   Some("Ctrl + Shift + O já é usado por outro app. Escolha outra."));
        assert_eq!(startup_hotkey_error(&s(&["Ctrl", "Shift", "O"]), &Ok(())), None);
    }
    #[test]
    fn update_settings_persists_and_rejects_invalid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let sink = std::sync::Arc::new(Rec(Default::default()));
        let c = Controller::new(Settings::default(), path.clone(), sink.clone());
        assert!(!c.state().status.overlay_visible);
        assert_eq!(c.update_settings(serde_json::json!({"size": 150})).unwrap().size, 150);
        assert_eq!(crate::settings::load(&path).0.size, 150);
        assert!(sink.0.lock().unwrap().contains(&"settings".to_string()));
        assert!(c.update_settings(serde_json::json!({"pos": "center"})).is_err());
        assert_eq!(c.state().settings.pos, Pos::Tr);
    }

    /// Records every event with its payload.
    #[derive(Default)]
    struct Log(std::sync::Mutex<Vec<(String, serde_json::Value)>>);
    impl UiSink for Log {
        fn emit(&self, e: &str, p: serde_json::Value) { self.0.lock().unwrap().push((e.into(), p)); }
    }
    impl Log {
        fn payloads(&self, event: &str) -> Vec<serde_json::Value> {
            self.0.lock().unwrap().iter().filter(|(e, _)| e == event).map(|(_, p)| p.clone()).collect()
        }
    }

    fn controller(settings: Settings) -> (Arc<Controller>, Arc<Log>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(Log::default());
        let c = Controller::new(settings, dir.path().join("settings.json"), log.clone());
        (c, log, dir)
    }

    #[test]
    fn toggling_without_native_thread_emits_status_once_per_change() {
        let (c, log, _dir) = controller(Settings::default());
        c.toggle_overlay();
        c.set_overlay_visible(true);
        c.set_overlay_visible(false);
        assert_eq!(log.payloads("status"), vec![
            serde_json::json!({"overlayVisible": true, "gameName": null}),
            serde_json::json!({"overlayVisible": false, "gameName": null}),
        ]);
    }

    #[test]
    fn try_set_hotkey_blocks_bad_and_known_combos_and_saves_good_ones() {
        let (c, log, dir) = controller(Settings::default());
        let r = c.try_set_hotkey(s(&["Ctrl", "Shift", "M"]));
        assert_eq!(r, HotkeyResult { ok: false, error: Some("Ctrl + Shift + M já é usado por Discord. Escolha outra.".into()) });
        let r = c.try_set_hotkey(s(&["O"]));
        assert_eq!(r.error.as_deref(), Some("Use pelo menos Ctrl, Alt ou Shift junto com a tecla."));
        assert_eq!(c.state().settings.shortcut, s(&["Ctrl", "Shift", "O"]));
        assert!(log.payloads("settings").is_empty());

        assert_eq!(c.try_set_hotkey(s(&["Ctrl", "Alt", "P"])), HotkeyResult { ok: true, error: None });
        assert_eq!(c.state().settings.shortcut, s(&["Ctrl", "Alt", "P"]));
        assert_eq!(crate::settings::load(&dir.path().join("settings.json")).0.shortcut, s(&["Ctrl", "Alt", "P"]));
        assert_eq!(log.payloads("settings")[0]["shortcut"], serde_json::json!(["Ctrl", "Alt", "P"]));
    }

    #[test]
    fn update_settings_keeps_the_registered_shortcut() {
        let (c, _log, _dir) = controller(Settings::default());
        let saved = c.update_settings(serde_json::json!({"shortcut": ["Ctrl", "Alt", "K"], "style": "line"})).unwrap();
        assert_eq!(saved.shortcut, s(&["Ctrl", "Shift", "O"]));
        assert_eq!(saved.style, crate::settings::Style::Line);
    }

    fn top_level_window(title: windows::core::PCWSTR) -> isize {
        use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WS_POPUP};
        let hwnd = unsafe {
            CreateWindowExW(Default::default(), windows::core::w!("STATIC"), title, WS_POPUP, 0, 0, 10, 10, None, None, None, None)
        }
        .unwrap();
        hwnd.0 as isize
    }

    fn destroy(hwnd: isize) {
        use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
        unsafe { DestroyWindow(windows::Win32::Foundation::HWND(hwnd as *mut _)).unwrap() };
    }

    fn focus(c: &Controller, hwnd: isize, pid: u32, covers_monitor: bool) {
        let info = ForegroundInfo { hwnd, pid, exe_name: "game.exe".into(), title: "Jogo Teste".into(), covers_monitor };
        c.on_native_event(NativeEvent::Foreground(info));
    }

    /// 2.1 s of steady presents by `pid` up to now, then a detect tick.
    fn present_and_tick(c: &Controller, pid: u32) {
        let now = now_ns();
        for i in (0..=126u64).rev() {
            c.frames.on_present(pid, now - i * 16_666_666);
        }
        c.on_native_event(NativeEvent::Timer(TimerKind::Detect));
    }

    /// A fullscreen window of this process, detected as the game; returns its handle.
    fn detect_game(c: &Controller, log: &Log) -> isize {
        let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
        focus(c, hwnd, std::process::id(), true);
        assert!(log.payloads("status").is_empty(), "a candidate is not a game yet");
        present_and_tick(c, std::process::id());
        assert_eq!(log.payloads("status"), vec![serde_json::json!({"overlayVisible": true, "gameName": "Jogo Teste"})]);
        hwnd
    }

    #[test]
    fn fullscreen_presenter_is_detected_named_and_lost_with_its_window() {
        let (c, log, _dir) = controller(Settings::default());
        let hwnd = detect_game(&c, &log);
        assert_eq!(c.frames.target(), Some(std::process::id()));

        // Its window is gone (no exit watch here): the next focus change ends the detection.
        destroy(hwnd);
        let other = ForegroundInfo { hwnd: 1, pid: 4242, exe_name: "notepad.exe".into(), title: String::new(), covers_monitor: false };
        c.on_native_event(NativeEvent::Foreground(other));
        assert_eq!(log.payloads("status").last(), Some(&serde_json::json!({"overlayVisible": false, "gameName": null})));
        assert_eq!(c.frames.target(), None);
    }

    #[test]
    fn recreated_game_window_keeps_the_game_and_its_suppression() {
        let (c, log, _dir) = controller(Settings::default());
        let pid = std::process::id();
        let old = detect_game(&c, &log);
        c.toggle_overlay();
        let hidden = serde_json::json!({"overlayVisible": false, "gameName": "Jogo Teste"});
        assert_eq!(log.payloads("status").last(), Some(&hidden));

        // The game replaces its window: same process, new handle, the old one is dead.
        destroy(old);
        let new = top_level_window(windows::core::w!("Jogo Teste"));
        focus(&c, new, pid, true);
        present_and_tick(&c, pid);
        assert_eq!(c.inner().detector.game().map(|g| (g.pid, g.hwnd)), Some((pid, new)));
        assert_eq!(log.payloads("status").last(), Some(&hidden), "the hotkey hide still holds");
        assert_eq!(log.payloads("status").len(), 2);
        destroy(new);
    }

    #[test]
    fn watched_game_is_not_ended_by_a_dead_window() {
        let (c, log, _dir) = controller(Settings::default());
        let pid = std::process::id();
        let hwnd = detect_game(&c, &log);
        // Its exit is waited on (this process never exits during the test), so the dead
        // window is not taken as the end of the game.
        c.inner().exit_watch = watch_exit(pid, Box::new(|| {}));
        destroy(hwnd);
        let other = ForegroundInfo { hwnd: 1, pid: 4242, exe_name: "notepad.exe".into(), title: String::new(), covers_monitor: false };
        c.on_native_event(NativeEvent::Foreground(other));
        assert_eq!(c.inner().detector.game().map(|g| g.pid), Some(pid));
        assert_eq!(log.payloads("status").last(), Some(&serde_json::json!({"overlayVisible": false, "gameName": "Jogo Teste"})));
        drop(c.inner().exit_watch.take());
    }

    /// Moves and sizes `hwnd` to exactly cover the monitor it is on.
    fn cover_its_monitor(hwnd: isize) {
        use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
        use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos};
        let h = windows::Win32::Foundation::HWND(hwnd as *mut _);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        assert!(unsafe { GetMonitorInfoW(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST), &mut mi) }.as_bool());
        let rc = mi.rcMonitor;
        let (w, cy) = (rc.right - rc.left, rc.bottom - rc.top);
        unsafe { SetWindowPos(h, None, rc.left, rc.top, w, cy, SWP_NOZORDER | SWP_NOACTIVATE) }.unwrap();
    }

    #[test]
    fn window_that_goes_fullscreen_after_the_focus_becomes_a_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let log = Arc::new(Log::default());
        let settings = Settings { shortcut: s(&["Ctrl", "Alt", "Shift", "F8"]), ..Settings::default() };
        // This test's own windows play the game, so this executable must not be the ignored one.
        let path = dir.path().join("settings.json");
        let c = Controller::with_self_exe(settings, path, log.clone(), "perf-overlay.exe".into());
        c.start();
        // Let the startup report of the real foreground window pass first.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let pid = std::process::id();
        let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
        focus(&c, hwnd, pid, false);
        assert_eq!(c.inner().detector.candidate_pid(), None, "windowed and hidden: not a candidate");
        assert_eq!(c.inner().sent_watch, Some((pid, hwnd)));

        // No focus change from here on: only the location hook can tell.
        cover_its_monitor(hwnd);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while c.inner().detector.candidate_pid().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(c.inner().detector.candidate_pid(), Some(pid));
        assert_eq!(c.inner().sent_watch, None, "a window covering its monitor is no longer watched");
        c.shutdown();
        destroy(hwnd);
    }

    #[test]
    fn closing_the_config_window_mid_capture_registers_the_shortcut_again() {
        let combo = s(&["Ctrl", "Alt", "Shift", "F7"]);
        let hk = hotkey::parse(&combo).unwrap();
        let (c, _log, _dir) = controller(Settings { shortcut: combo, ..Settings::default() });
        c.start();
        assert_eq!(c.state().hotkey_error, None);
        c.set_config_open(true);
        c.begin_hotkey_capture();
        // The capture released the shortcut: another window can take it.
        let probe = NativeHandle::spawn(Box::new(|_| {}));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while probe.register_hotkey(hk).is_err() {
            assert!(std::time::Instant::now() < deadline, "the capture did not release the shortcut");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        probe.quit();

        // Alt+F4 mid-capture: the window is destroyed with no blur and no end_hotkey_capture.
        c.set_config_open(false);
        let probe = NativeHandle::spawn(Box::new(|_| {}));
        assert_eq!(probe.register_hotkey(hk), Err(HotkeyError::InUse), "the shortcut is registered again");
        probe.quit();
        assert_eq!(c.state().hotkey_error, None);

        // A capture that ended through try_set_hotkey leaves nothing to restore.
        c.set_config_open(true);
        c.begin_hotkey_capture();
        assert!(c.try_set_hotkey(s(&["Ctrl", "Alt", "Shift", "F6"])).ok);
        assert!(!c.inner().capturing);
        c.shutdown();
    }

    #[test]
    fn started_controller_reports_a_busy_shortcut_and_streams_metrics_to_an_open_config() {
        let combo = s(&["Ctrl", "Alt", "Shift", "F10"]);
        let holder = NativeHandle::spawn(Box::new(|_| {}));
        holder.register_hotkey(hotkey::parse(&combo).unwrap()).unwrap();
        let (c, log, _dir) = controller(Settings { shortcut: combo, ..Settings::default() });
        c.start();
        assert_eq!(c.state().hotkey_error.as_deref(), Some("Ctrl + Alt + Shift + F10 já é usado por outro app. Escolha outra."));

        c.set_config_open(true);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while log.payloads("metrics").is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let metrics = log.payloads("metrics");
        assert!(!metrics.is_empty(), "no metrics within 3 s");
        assert!(metrics[0]["fps"].is_null(), "no game, no FPS");
        // Without elevation the capture is denied and FPS becomes unavailable, once.
        let fps = c.state().availability.fps;
        assert!(fps == Avail::Ok || fps == Avail::NoSensor { reason: REASON_NEEDS_ADMIN.into() }, "{fps:?}");
        assert!(log.payloads("availability").len() <= 1);

        c.shutdown();
        let after = log.0.lock().unwrap().len();
        std::thread::sleep(std::time::Duration::from_millis(700));
        assert_eq!(log.0.lock().unwrap().len(), after, "nothing runs after shutdown");
        holder.quit();
    }
}
