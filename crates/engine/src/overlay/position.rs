use crate::settings::Pos;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub const MARGIN_PX: f32 = 12.0;

/// Top-left of a `w`×`h` window placed at `pos` inside `monitor` (rcMonitor),
/// with a margin of `round(12 × dpi_scale)`. Middle positions center with integer division.
pub fn place(monitor: Rect, pos: Pos, w: i32, h: i32, dpi_scale: f32) -> (i32, i32) {
    let m = (MARGIN_PX * dpi_scale).round() as i32;
    let left = monitor.x + m;
    let center = monitor.x + (monitor.w - w) / 2;
    let right = monitor.x + monitor.w - m - w;
    let top = monitor.y + m;
    let middle = monitor.y + (monitor.h - h) / 2;
    let bottom = monitor.y + monitor.h - m - h;
    match pos {
        Pos::Tl => (left, top),
        Pos::Tc => (center, top),
        Pos::Tr => (right, top),
        Pos::Ml => (left, middle),
        Pos::Mr => (right, middle),
        Pos::Bl => (left, bottom),
        Pos::Bc => (center, bottom),
        Pos::Br => (right, bottom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn place_corners_and_edges() {
        let m = Rect { x: 0, y: 0, w: 1920, h: 1080 };
        assert_eq!(place(m, Pos::Tr, 148, 100, 1.0), (1920 - 12 - 148, 12));
        assert_eq!(place(m, Pos::Bc, 148, 100, 1.0), ((1920 - 148) / 2, 1080 - 12 - 100));
        assert_eq!(place(m, Pos::Ml, 148, 100, 1.0), (12, (1080 - 100) / 2));
    }

    #[test]
    fn place_on_left_monitor_with_150_percent_dpi() {
        let m = Rect { x: -2560, y: 0, w: 2560, h: 1440 };
        assert_eq!(place(m, Pos::Tl, 222, 150, 1.5), (-2560 + 18, 18));
        assert_eq!(place(m, Pos::Br, 222, 150, 1.5), (-18 - 222, 1440 - 18 - 150));
    }
}
