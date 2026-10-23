//! Game detection state machine (spec 4.6). Pure: the controller feeds it
//! foreground changes, ETW present streaks and process-exit events.

/// Continuous presenting time required to confirm a candidate as the game.
pub const CONFIRM_MS: u64 = 2000;
/// A candidate without a confirming streak for this long is given up, so ETW and the detect
/// timer can stop (an RDP or VM client, a paused fullscreen player). A foreground or location
/// event for its window, or showing the overlay, re-arms it.
pub const GIVE_UP_MS: u64 = 10_000;

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
    /// `now_ms` of the candidate's first presenting check.
    candidate_since: Option<u64>,
    /// Window (pid, hwnd) whose candidacy timed out: not nominated again until re-armed.
    given_up: Option<(u32, isize)>,
    game: Option<Game>,
    suppressed: Option<u32>,
}

impl GameDetector {
    pub fn new(auto_show: bool) -> Self {
        Self {
            mode: Mode::Hidden,
            auto_show,
            fg: None,
            candidate: None,
            candidate_since: None,
            given_up: None,
            game: None,
            suppressed: None,
        }
    }

    pub fn set_auto_show(&mut self, on: bool) {
        self.auto_show = on;
        if let Some(fg) = self.fg.clone() {
            self.focus(fg);
        }
    }

    /// A foreground change, or the focused window starting or stopping covering its monitor.
    /// Re-arms a candidate that was given up.
    pub fn on_foreground(&mut self, fg: Fg) {
        self.given_up = None;
        self.focus(fg);
    }

    fn focus(&mut self, fg: Fg) {
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

    /// A presenting check of the candidate (each detect tick): `continuous_ms` is how long `pid`
    /// has presented without a gap, `now_ms` a monotonic clock.
    pub fn on_presenting(&mut self, pid: u32, continuous_ms: u64, now_ms: u64) {
        let Some(c) = self.candidate else { return };
        if c.pid != pid {
            return;
        }
        if continuous_ms < CONFIRM_MS {
            let since = *self.candidate_since.get_or_insert(now_ms);
            // Only the time spent not presenting counts: a streak under way may still confirm.
            if now_ms.saturating_sub(since).saturating_sub(continuous_ms) >= GIVE_UP_MS {
                self.given_up = Some((c.pid, c.hwnd));
                self.set_candidate(None);
            }
            return;
        }
        if self.game.map(|g| g.pid) != Some(pid) {
            self.suppressed = None;
        }
        self.game = Some(c);
        self.set_candidate(None);
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
            // The user asks for this window's metrics: a timed-out candidate gets another chance.
            self.given_up = None;
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
        let next = self.fg.as_ref().and_then(|fg| {
            // The detected game is never re-nominated: returning focus to it is a focus event.
            let path = if fg.ignored
                || self.game.is_some_and(|g| g.pid == fg.pid)
                || self.given_up == Some((fg.pid, fg.hwnd))
            {
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
        self.set_candidate(next);
    }

    /// A different candidate starts with a fresh clock.
    fn set_candidate(&mut self, next: Option<Game>) {
        if next != self.candidate {
            self.candidate_since = None;
        }
        self.candidate = next;
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
    d.on_presenting(1, 1999, 0);
    assert!(d.game().is_none());
    d.on_presenting(1, CONFIRM_MS, 0);
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
    d.on_presenting(3, CONFIRM_MS, 0);
    assert_eq!(d.game().unwrap().path, DetectPath::Windowed);
    assert!(d.visible());
}
#[test]
fn auto_overlay_hides_on_focus_loss_and_returns() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0);
    d.on_foreground(fg(9, false));
    assert!(!d.visible());
    assert_eq!(gpid(&d), Some(1));
    d.on_foreground(fg(1, true));
    assert!(d.visible());
}
#[test]
fn hotkey_hide_suppresses_until_game_exits() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0);
    d.toggle();
    assert!(!d.visible());
    d.on_foreground(fg(9, false)); d.on_foreground(fg(1, true));
    assert!(!d.visible(), "Alt+Tab não limpa a supressão");
    d.on_process_exited(1);
    assert!(d.game().is_none());
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0);
    assert!(d.visible());
}
#[test]
fn another_game_clears_suppression() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0); d.toggle();
    d.on_foreground(fg(2, true)); d.on_presenting(2, CONFIRM_MS, 0);
    assert!(d.visible());
    assert_eq!(gpid(&d), Some(2));
}
#[test]
fn manual_overlay_survives_focus_loss_and_game_exit() {
    let mut d = GameDetector::new(true);
    d.toggle();
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0);
    d.on_foreground(fg(9, false));
    assert!(d.visible());
    d.on_process_exited(1);
    assert!(d.visible());
    assert!(d.game().is_none());
}
#[test]
fn auto_overlay_hides_when_game_exits_and_unrelated_exit_is_ignored() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0);
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
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS, 0);
    d.on_foreground(fg(5, false));
    assert_eq!(d.anchor_hwnd(), Some(10));
}

    fn hide_and_confirm_tests_helper() -> GameDetector {
        let mut d = GameDetector::new(true);
        d.on_foreground(fg(1, true));
        d.on_presenting(1, CONFIRM_MS, 0);
        d
    }
    #[test]
    fn hotkey_hide_is_not_undone_by_detect_tick() {
        let mut d = hide_and_confirm_tests_helper();
        d.toggle();
        assert_eq!(d.candidate_pid(), None);
        d.on_presenting(1, CONFIRM_MS, 0);
        assert!(!d.visible());
    }
    #[test]
    fn hotkey_hide_survives_alt_tab_and_tick() {
        let mut d = hide_and_confirm_tests_helper();
        d.toggle();
        d.on_foreground(fg(9, false));
        d.on_foreground(fg(1, true));
        assert_eq!(d.candidate_pid(), None);
        d.on_presenting(1, CONFIRM_MS, 0);
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
        d.on_presenting(1, CONFIRM_MS, 0);
        d.on_process_exited(1);
        assert!(d.visible());
        assert_eq!(d.candidate_pid(), None);
    }
    #[test]
    fn manual_show_over_fullscreen_uses_fullscreen_path() {
        let mut d = GameDetector::new(false);
        d.on_foreground(fg(4, true));
        d.toggle();
        d.on_presenting(4, CONFIRM_MS, 0);
        assert_eq!(d.game().unwrap().path, DetectPath::Fullscreen);
    }

    #[test]
    fn candidate_without_a_confirming_streak_is_given_up_until_a_focus_event() {
        let mut d = GameDetector::new(true);
        d.on_foreground(fg(1, true));
        d.on_presenting(1, 0, 1_000); // the first check starts the candidate's clock
        d.on_presenting(1, 0, 1_000 + GIVE_UP_MS - 1);
        assert_eq!(d.candidate_pid(), Some(1));
        d.on_presenting(1, 0, 1_000 + GIVE_UP_MS);
        assert_eq!(d.candidate_pid(), None, "given up: ETW and the detect timer may stop");
        // Re-evaluations that are not focus events keep it given up.
        d.set_auto_show(true);
        d.on_presenting(1, CONFIRM_MS, 20_000);
        assert_eq!((d.candidate_pid(), gpid(&d)), (None, None));

        // A foreground (or location) event for the window re-arms it, with a fresh clock.
        d.on_foreground(fg(1, true));
        assert_eq!(d.candidate_pid(), Some(1));
        d.on_presenting(1, 0, 50_000);
        d.on_presenting(1, 0, 50_000 + GIVE_UP_MS - 1);
        assert_eq!(d.candidate_pid(), Some(1));
        d.on_presenting(1, CONFIRM_MS, 50_000 + GIVE_UP_MS);
        assert_eq!(gpid(&d), Some(1));
        assert!(d.visible());
    }
    #[test]
    fn a_streak_under_way_at_the_deadline_still_confirms() {
        let mut d = GameDetector::new(true);
        d.on_foreground(fg(1, true));
        d.on_presenting(1, 0, 0);
        d.on_presenting(1, 1_500, GIVE_UP_MS + 500); // presenting since 9 s
        assert_eq!(d.candidate_pid(), Some(1));
        d.on_presenting(1, CONFIRM_MS, GIVE_UP_MS + 1_000);
        assert_eq!(gpid(&d), Some(1));
    }
    #[test]
    fn each_candidate_gets_its_own_time() {
        let mut d = GameDetector::new(true);
        d.on_foreground(fg(1, true));
        d.on_presenting(1, 0, 0);
        d.on_foreground(fg(2, true));
        d.on_presenting(2, 0, 9_000);
        d.on_presenting(2, 0, 9_000 + GIVE_UP_MS - 1);
        assert_eq!(d.candidate_pid(), Some(2));
    }
    #[test]
    fn showing_the_overlay_re_arms_a_given_up_candidate() {
        let mut d = GameDetector::new(true);
        d.on_foreground(fg(1, true));
        d.on_presenting(1, 0, 0);
        d.on_presenting(1, 0, GIVE_UP_MS);
        assert_eq!(d.candidate_pid(), None);
        d.toggle();
        assert_eq!(d.candidate_pid(), Some(1), "the hotkey asks for this window's metrics");
        d.on_presenting(1, CONFIRM_MS, GIVE_UP_MS + 3_000);
        assert_eq!(gpid(&d), Some(1));
    }
}
