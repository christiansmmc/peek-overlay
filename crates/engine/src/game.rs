//! Game detection state machine (spec 4.6). Pure: the controller feeds it
//! foreground changes, ETW present streaks and process-exit events.

/// Continuous presenting time required to confirm a candidate as the game.
pub const CONFIRM_MS: u64 = 2000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DetectPath {
    Fullscreen,
    Windowed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fg {
    pub pid: u32,
    pub hwnd: isize,
    pub covers_monitor: bool,
    pub ignored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Game {
    pub pid: u32,
    pub hwnd: isize,
    pub path: DetectPath,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Hidden,
    Auto,
    Manual,
}

pub struct GameDetector {
    mode: Mode,
    auto_show: bool,
    fg: Option<Fg>,
    candidate: Option<Game>,
    game: Option<Game>,
    suppressed: Option<u32>,
}

impl GameDetector {
    pub fn new(auto_show: bool) -> Self {
        Self { mode: Mode::Hidden, auto_show, fg: None, candidate: None, game: None, suppressed: None }
    }

    pub fn set_auto_show(&mut self, on: bool) {
        self.auto_show = on;
        if let Some(fg) = self.fg.clone() {
            self.on_foreground(fg);
        }
    }

    pub fn on_foreground(&mut self, fg: Fg) {
        self.fg = Some(fg.clone());
        match &mut self.game {
            Some(g) if g.pid == fg.pid => {
                g.hwnd = fg.hwnd;
                let g = *g;
                if self.mode == Mode::Hidden
                    && self.auto_show
                    && g.path == DetectPath::Fullscreen
                    && self.suppressed != Some(g.pid)
                {
                    self.mode = Mode::Auto;
                }
            }
            _ => {
                if self.mode == Mode::Auto {
                    self.mode = Mode::Hidden;
                }
            }
        }
        self.refresh_candidate();
    }

    pub fn on_presenting(&mut self, pid: u32, continuous_ms: u64) {
        let Some(c) = self.candidate else { return };
        if continuous_ms < CONFIRM_MS || c.pid != pid {
            return;
        }
        if self.game.map(|g| g.pid) != Some(pid) {
            self.suppressed = None;
        }
        self.game = Some(c);
        self.candidate = None;
        if c.path == DetectPath::Fullscreen
            && self.auto_show
            && self.mode == Mode::Hidden
            && self.suppressed != Some(pid)
        {
            self.mode = Mode::Auto;
        }
    }

    pub fn on_process_exited(&mut self, pid: u32) {
        if self.game.map(|g| g.pid) != Some(pid) {
            return;
        }
        self.game = None;
        self.suppressed = None;
        if self.fg.as_ref().is_some_and(|f| f.pid == pid) {
            self.fg = None;
        }
        if self.mode == Mode::Auto {
            self.mode = Mode::Hidden;
        }
        self.refresh_candidate();
    }

    pub fn toggle(&mut self) {
        self.set_visible(!self.visible());
    }

    pub fn set_visible(&mut self, v: bool) {
        if v == self.visible() {
            return;
        }
        if v {
            self.mode = Mode::Manual;
            self.refresh_candidate();
        } else {
            self.suppressed = self.game.map(|g| g.pid).or(self.suppressed);
            self.mode = Mode::Hidden;
            self.refresh_candidate();
        }
    }

    pub fn visible(&self) -> bool {
        self.mode != Mode::Hidden
    }

    pub fn game(&self) -> Option<Game> {
        self.game
    }

    pub fn candidate_pid(&self) -> Option<u32> {
        self.candidate.map(|c| c.pid)
    }

    pub fn anchor_hwnd(&self) -> Option<isize> {
        self.game.map(|g| g.hwnd).or(self.fg.as_ref().map(|f| f.hwnd))
    }

    fn refresh_candidate(&mut self) {
        let visible = self.visible();
        self.candidate = self.fg.as_ref().and_then(|fg| {
            // The detected game is never re-nominated: returning focus to it is a focus event.
            let path = if fg.ignored || self.game.is_some_and(|g| g.pid == fg.pid) {
                return None;
            } else if fg.covers_monitor && (self.auto_show || visible) {
                DetectPath::Fullscreen
            } else if visible {
                DetectPath::Windowed
            } else {
                return None;
            };
            Some(Game { pid: fg.pid, hwnd: fg.hwnd, path })
        });
    }
}

#[cfg(test)]
mod tests {
use super::*;
fn fg(pid: u32, full: bool) -> Fg { Fg { pid, hwnd: pid as isize * 10, covers_monitor: full, ignored: false } }
fn gpid(d: &GameDetector) -> Option<u32> { d.game().map(|g| g.pid) }

#[test]
fn fullscreen_game_confirmed_after_2s_auto_shows() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true));
    assert_eq!(d.candidate_pid(), Some(1));
    d.on_presenting(1, 1999);
    assert!(d.game().is_none());
    d.on_presenting(1, CONFIRM_MS);
    assert_eq!(gpid(&d), Some(1));
    assert!(d.visible());
    assert_eq!(d.candidate_pid(), None);
}
#[test]
fn ignored_process_is_never_candidate() {
    let mut d = GameDetector::new(true);
    d.on_foreground(Fg { ignored: true, ..fg(2, true) });
    assert_eq!(d.candidate_pid(), None);
}
#[test]
fn windowed_process_is_candidate_only_while_visible() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(3, false));
    assert_eq!(d.candidate_pid(), None);
    d.toggle();
    assert_eq!(d.candidate_pid(), Some(3));
    d.on_presenting(3, CONFIRM_MS);
    assert_eq!(d.game().unwrap().path, DetectPath::Windowed);
    assert!(d.visible());
}
#[test]
fn auto_overlay_hides_on_focus_loss_and_returns() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_foreground(fg(9, false));
    assert!(!d.visible());
    assert_eq!(gpid(&d), Some(1));
    d.on_foreground(fg(1, true));
    assert!(d.visible());
}
#[test]
fn hotkey_hide_suppresses_until_game_exits() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.toggle();
    assert!(!d.visible());
    d.on_foreground(fg(9, false)); d.on_foreground(fg(1, true));
    assert!(!d.visible(), "Alt+Tab não limpa a supressão");
    d.on_process_exited(1);
    assert!(d.game().is_none());
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    assert!(d.visible());
}
#[test]
fn another_game_clears_suppression() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS); d.toggle();
    d.on_foreground(fg(2, true)); d.on_presenting(2, CONFIRM_MS);
    assert!(d.visible());
    assert_eq!(gpid(&d), Some(2));
}
#[test]
fn manual_overlay_survives_focus_loss_and_game_exit() {
    let mut d = GameDetector::new(true);
    d.toggle();
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_foreground(fg(9, false));
    assert!(d.visible());
    d.on_process_exited(1);
    assert!(d.visible());
    assert!(d.game().is_none());
}
#[test]
fn auto_overlay_hides_when_game_exits_and_unrelated_exit_is_ignored() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_process_exited(42);
    assert_eq!(gpid(&d), Some(1));
    d.on_process_exited(1);
    assert!(!d.visible());
}
#[test]
fn auto_show_off_does_not_watch_fullscreen_while_hidden() {
    let mut d = GameDetector::new(false);
    d.on_foreground(fg(1, true));
    assert_eq!(d.candidate_pid(), None);
    assert!(!d.visible());
}
#[test]
fn anchor_prefers_game_window() {
    let mut d = GameDetector::new(true);
    assert_eq!(d.anchor_hwnd(), None);
    d.on_foreground(fg(5, false));
    assert_eq!(d.anchor_hwnd(), Some(50));
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_foreground(fg(5, false));
    assert_eq!(d.anchor_hwnd(), Some(10));
}

    fn hide_and_confirm_tests_helper() -> GameDetector {
        let mut d = GameDetector::new(true);
        d.on_foreground(fg(1, true));
        d.on_presenting(1, CONFIRM_MS);
        d
    }
    #[test]
    fn hotkey_hide_is_not_undone_by_detect_tick() {
        let mut d = hide_and_confirm_tests_helper();
        d.toggle();
        assert_eq!(d.candidate_pid(), None);
        d.on_presenting(1, CONFIRM_MS);
        assert!(!d.visible());
    }
    #[test]
    fn hotkey_hide_survives_alt_tab_and_tick() {
        let mut d = hide_and_confirm_tests_helper();
        d.toggle();
        d.on_foreground(fg(9, false));
        d.on_foreground(fg(1, true));
        assert_eq!(d.candidate_pid(), None);
        d.on_presenting(1, CONFIRM_MS);
        assert!(!d.visible());
    }
    #[test]
    fn returning_to_detected_game_needs_no_candidate() {
        let mut d = hide_and_confirm_tests_helper();
        d.on_foreground(fg(9, false));
        d.on_foreground(fg(1, true));
        assert_eq!(d.candidate_pid(), None);
        assert!(d.visible());
    }
    #[test]
    fn game_exit_leaves_nothing_running() {
        use crate::effects::{Effects, plan_effects};
        use crate::settings::Metrics;
        let m = Metrics::default();
        let mut d = hide_and_confirm_tests_helper();
        d.on_process_exited(1);
        assert_eq!(d.candidate_pid(), None);
        assert_eq!(plan_effects(d.visible(), false, d.candidate_pid().is_some(), &m), Effects::default());
    }
    #[test]
    fn manual_overlay_after_game_exit_has_no_dead_candidate() {
        let mut d = GameDetector::new(true);
        d.toggle();
        d.on_foreground(fg(1, true));
        d.on_presenting(1, CONFIRM_MS);
        d.on_process_exited(1);
        assert!(d.visible());
        assert_eq!(d.candidate_pid(), None);
    }
    #[test]
    fn manual_show_over_fullscreen_uses_fullscreen_path() {
        let mut d = GameDetector::new(false);
        d.on_foreground(fg(4, true));
        d.toggle();
        d.on_presenting(4, CONFIRM_MS);
        assert_eq!(d.game().unwrap().path, DetectPath::Fullscreen);
    }
}
