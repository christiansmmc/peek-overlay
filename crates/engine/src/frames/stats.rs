use std::collections::{HashMap, VecDeque};

pub const WINDOW_NS: u64 = 500_000_000;
pub const HISTORY_NS: u64 = 3_000_000_000;
pub const STALE_NS: u64 = 1_000_000_000;
pub const STREAK_GAP_NS: u64 = 250_000_000;

/// Keeps present timestamps for the last `HISTORY_NS` and derives FPS,
/// average frametime and a max-per-column frametime graph.
#[derive(Debug, Default)]
pub struct FrameTracker {
    presents: VecDeque<u64>,
}

impl FrameTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Amortized O(1): push, then trim the front.
    pub fn on_present(&mut self, t_ns: u64) {
        self.presents.push_back(t_ns);
        while let Some(&front) = self.presents.front() {
            if t_ns.saturating_sub(front) > HISTORY_NS {
                self.presents.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn clear(&mut self) {
        self.presents.clear();
    }

    pub fn len(&self) -> usize {
        self.presents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.presents.is_empty()
    }

    /// (count, first, last) of presents in (now - WINDOW_NS, now], or None if
    /// fewer than 2 or the stream is stale.
    fn window(&self, now_ns: u64) -> Option<(u64, u64, u64)> {
        let lo = now_ns.saturating_sub(WINDOW_NS);
        let mut it = self
            .presents
            .iter()
            .rev()
            .copied()
            .skip_while(|&t| t > now_ns)
            .take_while(|&t| t > lo);
        let last = it.next()?;
        if now_ns - last > STALE_NS {
            return None;
        }
        let mut first = last;
        let mut n = 1u64;
        for t in it {
            first = t;
            n += 1;
        }
        if n < 2 || last <= first {
            return None;
        }
        Some((n, first, last))
    }

    pub fn fps(&self, now_ns: u64) -> Option<u32> {
        let (n, first, last) = self.window(now_ns)?;
        Some(((n - 1) as f64 * 1e9 / (last - first) as f64).round() as u32)
    }

    pub fn avg_frametime_ms(&self, now_ns: u64) -> Option<f32> {
        let (n, first, last) = self.window(now_ns)?;
        Some(((last - first) as f64 / (n - 1) as f64 / 1e6) as f32)
    }

    /// Frametimes (ms) bucketed into `columns` equal columns over
    /// [now - HISTORY_NS, now]; each column holds the max.
    pub fn graph(&self, now_ns: u64, columns: usize) -> Vec<f32> {
        if columns == 0 {
            return Vec::new();
        }
        let start = now_ns.saturating_sub(HISTORY_NS);
        let mut cols: Vec<Option<f32>> = vec![None; columns];
        let mut prev: Option<u64> = None;
        for &t in &self.presents {
            if let Some(p) = prev {
                if t >= start && t <= now_ns {
                    let idx = (((t - start) as u128 * columns as u128) / HISTORY_NS as u128)
                        .min(columns as u128 - 1) as usize;
                    let ft = (t.saturating_sub(p) as f64 / 1e6) as f32;
                    cols[idx] = Some(cols[idx].map_or(ft, |c| c.max(ft)));
                }
            }
            prev = Some(t);
        }
        let Some(first_val) = cols.iter().flatten().next().copied() else {
            return Vec::new();
        };
        let mut last = first_val;
        cols.iter()
            .map(|c| {
                if let Some(v) = c {
                    last = *v;
                }
                last
            })
            .collect()
    }
}

/// Nice ceiling for the graph's Y axis (minimum 8).
pub fn graph_y_max(columns: &[f32]) -> f32 {
    let max = columns.iter().cloned().fold(0.0_f32, f32::max);
    let v = max * 1.15;
    let step = if v < 20.0 {
        2.0
    } else if v < 50.0 {
        5.0
    } else if v < 200.0 {
        10.0
    } else {
        50.0
    };
    ((v / step).ceil() * step).max(8.0)
}

/// Reduces `values` to `n` contiguous groups keeping each group's max.
pub fn downsample_max(values: &[f32], n: usize) -> Vec<f32> {
    if n >= values.len() {
        return values.to_vec();
    }
    if n == 0 {
        return Vec::new();
    }
    let len = values.len();
    (0..n)
        .map(|i| {
            let a = i * len / n;
            let b = (i + 1) * len / n;
            values[a..b].iter().cloned().fold(f32::MIN, f32::max)
        })
        .collect()
}

/// Tracks, per pid, how long presents have been arriving without a gap.
#[derive(Debug, Default)]
pub struct PresentObserver {
    streaks: HashMap<u32, (u64, u64)>,
}

impl PresentObserver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_present(&mut self, pid: u32, t_ns: u64) {
        let e = self.streaks.entry(pid).or_insert((t_ns, t_ns));
        if t_ns.saturating_sub(e.1) > STREAK_GAP_NS {
            e.0 = t_ns;
        }
        e.1 = t_ns;
    }

    pub fn continuous_ms(&self, pid: u32, now_ns: u64) -> u64 {
        match self.streaks.get(&pid) {
            Some(&(start, last)) if now_ns.saturating_sub(last) <= STREAK_GAP_NS => {
                last.saturating_sub(start) / 1_000_000
            }
            _ => 0,
        }
    }

    pub fn clear(&mut self) {
        self.streaks.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(t: &mut FrameTracker, fps: f64, secs: f64, start: u64) -> u64 {
        let dt = (1e9 / fps) as u64;
        let n = (fps * secs) as u64;
        for i in 0..n { t.on_present(start + i * dt); }
        start + (n - 1) * dt
    }
    #[test]
    fn fps_and_frametime_for_steady_rates() {
        for (fps, ft) in [(142.0, 7.04), (60.0, 16.67), (30.0, 33.33)] {
            let mut t = FrameTracker::new();
            let now = feed(&mut t, fps, 2.0, 1_000_000_000);
            assert_eq!(t.fps(now), Some(fps as u32));
            assert!((t.avg_frametime_ms(now).unwrap() - ft).abs() < 0.01);
        }
    }
    #[test]
    fn stale_after_one_second_without_presents() {
        let mut t = FrameTracker::new();
        let now = feed(&mut t, 60.0, 1.0, 0);
        assert!(t.fps(now + STALE_NS + 1).is_none());
        assert!(t.avg_frametime_ms(now + STALE_NS + 1).is_none());
    }
    #[test]
    fn history_is_bounded_at_3000_fps() {
        let mut t = FrameTracker::new();
        feed(&mut t, 3000.0, 10.0, 0);
        assert!(t.len() <= 9001);
    }
    #[test]
    fn graph_keeps_a_50ms_spike_in_one_column() {
        let mut t = FrameTracker::new();
        let mid = feed(&mut t, 60.0, 1.5, 0);
        let now = feed(&mut t, 60.0, 1.5, mid + 50_000_000);
        let g = t.graph(now, 126);
        assert_eq!(g.len(), 126);
        assert!(g.iter().cloned().fold(0.0, f32::max) >= 49.9);
        assert!(g.iter().filter(|&&v| v > 20.0).count() <= 2);
        assert!(FrameTracker::new().graph(0, 44).is_empty());
    }
    #[test]
    fn y_max_nice_ceiling() {
        for (cols, want) in [(vec![], 8.0), (vec![6.9], 8.0), (vec![7.9], 10.0), (vec![16.7], 20.0),
                             (vec![40.0], 50.0), (vec![100.0], 120.0), (vec![500.0], 600.0)] {
            assert_eq!(graph_y_max(&cols), want, "{cols:?}");
        }
    }
    #[test]
    fn downsample_keeps_max() {
        assert_eq!(downsample_max(&[1.0, 1.0, 9.0, 1.0, 1.0, 1.0], 3), vec![1.0, 9.0, 1.0]);
    }
    #[test]
    fn observer_streak_resets_after_gap_and_quiet() {
        let mut o = PresentObserver::new();
        for i in 0..=150u64 { o.on_present(7, i * 16_666_666); }
        let now = 150 * 16_666_666;
        assert!(o.continuous_ms(7, now) >= 2400);
        assert_eq!(o.continuous_ms(9, now), 0);
        assert_eq!(o.continuous_ms(7, now + 300_000_000), 0);
        o.on_present(7, now + 300_000_000);
        assert_eq!(o.continuous_ms(7, now + 300_000_000), 0);
    }
}
