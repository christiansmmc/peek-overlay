//! Overlay layout in logical px at 100%. Measurements come from the approved design
//! ("Prévia = overlay real"); text widths only through `TextMeasure`.
use super::colors::*;
use super::format::{fmt_fps, fmt_ms, fmt_pair};
use crate::frames::stats::downsample_max;
use crate::model::MetricsSample;
use crate::settings::{Metrics, Style};

pub trait TextMeasure {
    fn width(&self, text: &str, size: f32, weight: u16) -> f32;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    RoundRect { x: f32, y: f32, w: f32, h: f32, r: f32, color: Rgba },
    /// `y` is the top of a `line_box(size)` line box, so the baseline sits at `y + baseline(size)`.
    /// `letter_spacing` is added after every character (CSS semantics). The renderer adds the shadow.
    Text { x: f32, y: f32, size: f32, weight: u16, color: Rgba, letter_spacing: f32, text: String },
    Polyline { points: Vec<(f32, f32)>, width: f32, color: Rgba },
    /// Usage bar: a `track` rounded rect whose first `fill` px are drawn in `color`. The fill is
    /// clipped to the track's shape, so its right end is square, as in the prototype.
    Bar { x: f32, y: f32, w: f32, h: f32, r: f32, fill: f32, track: Rgba, color: Rgba },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub w: f32,
    pub h: f32,
    pub prims: Vec<Prim>,
}

const LINE_H: f32 = 26.0;
const LINE_PAD_X: f32 = 12.0;
const LINE_GAP: f32 = 12.0;
const CARD_W: f32 = 148.0;
const CARD_PAD_TOP: f32 = 9.0;
const CARD_PAD_BOTTOM: f32 = 10.0;
const CARD_PAD_X: f32 = 11.0;
const CARD_GAP: f32 = 6.0;
const CARD_RIGHT: f32 = CARD_W - CARD_PAD_X;
const CARD_NUM: f32 = 26.0;
/// The Card's FPS number has `letter-spacing: -.02em`.
const CARD_NUM_TRACKING: f32 = -0.02 * CARD_NUM;
/// Segoe UI Variable Display font metrics (DirectWrite: ascent 2210, descent 514 and no line
/// gap, in 2048 units per em).
const ASCENT: f32 = 2210.0 / 2048.0;
const DESCENT: f32 = 514.0 / 2048.0;

/// Height of a `line-height: normal` line box. Chromium rounds ascent and descent to whole px
/// on their own; doing the same keeps the overlay aligned with the settings preview.
pub fn line_box(size: f32) -> f32 {
    (ASCENT * size).round() + (DESCENT * size).round()
}

/// Distance from the top of a `line_box(size)` box to its baseline. The renderer places every
/// `Prim::Text` baseline at `y + baseline(size)`.
pub fn baseline(size: f32) -> f32 {
    (ASCENT * size).round()
}

fn text(x: f32, y: f32, size: f32, weight: u16, color: Rgba, s: &str) -> Prim {
    Prim::Text { x, y, size, weight, color, letter_spacing: 0.0, text: s.to_string() }
}

/// Top of a line box whose baseline sits on `base`.
fn top_for_baseline(base: f32, size: f32) -> f32 {
    base - baseline(size)
}

fn graph_points(values: &[f32], max_ms: f32, ox: f32, oy: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    let n = values.len();
    if n < 2 {
        return Vec::new();
    }
    values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let ratio = if max_ms > 0.0 { (v / max_ms).clamp(0.0, 1.0) } else { 0.0 };
            (ox + i as f32 * w / (n - 1) as f32, oy + h - 1.0 - ratio * (h - 2.0))
        })
        .collect()
}

pub fn layout(style: Style, metrics: &Metrics, sample: &MetricsSample, opacity: u32, m: &dyn TextMeasure) -> Layout {
    match style {
        Style::Line => line(metrics, sample, opacity, m),
        Style::Card => card(metrics, sample, opacity, m),
    }
}

fn line(mt: &Metrics, s: &MetricsSample, opacity: u32, m: &dyn TextMeasure) -> Layout {
    let mut body: Vec<Prim> = Vec::new();
    let mut x = LINE_PAD_X;
    let mut first = true;
    let gap = |x: &mut f32, first: &mut bool| {
        if !*first {
            *x += LINE_GAP;
        }
        *first = false;
    };
    // 12px line boxes are centered in the pill; the 9.5px "FPS" shares their baseline and
    // fits inside them, so the FPS group is as tall as a 12px box.
    let top12 = (LINE_H - line_box(12.0)) / 2.0;
    let base = top12 + baseline(12.0);

    if mt.fps {
        gap(&mut x, &mut first);
        let num = fmt_fps(s.fps);
        body.push(text(x, top12, 12.0, 600, WHITE, &num));
        x += m.width(&num, 12.0, 600) + 3.0;
        body.push(text(x, top_for_baseline(base, 9.5), 9.5, 600, WHITE_65, "FPS"));
        x += m.width("FPS", 9.5, 600);
    }
    if mt.graph {
        gap(&mut x, &mut first);
        let vals = downsample_max(&s.graph, 44);
        let oy = (LINE_H - 14.0) / 2.0;
        let points = graph_points(&vals, s.graph_max_ms, x, oy, 44.0, 14.0);
        if !points.is_empty() {
            body.push(Prim::Polyline { points, width: 1.4, color: GRAPH });
        }
        x += 44.0;
    }
    for (use_on, temp_on, label, label_color, use_v, temp_v) in [
        (mt.gpu_use, mt.gpu_temp, "GPU", GPU_LABEL, s.gpu_use, s.gpu_temp),
        (mt.cpu_use, mt.cpu_temp, "CPU", CPU_LABEL, s.cpu_use, s.cpu_temp),
    ] {
        if !(use_on || temp_on) {
            continue;
        }
        gap(&mut x, &mut first);
        body.push(text(x, top12, 12.0, 600, label_color, label));
        x += m.width(label, 12.0, 600) + 5.0;
        let val = fmt_pair(use_on, use_v, temp_on, temp_v, Style::Line);
        body.push(text(x, top12, 12.0, 600, WHITE, &val));
        x += m.width(&val, 12.0, 600);
    }
    if first {
        let msg = "Nenhuma métrica";
        body.push(text(x, top12, 12.0, 500, WHITE_65, msg));
        x += m.width(msg, 12.0, 500);
    }
    let w = x + LINE_PAD_X;
    let mut prims = vec![Prim::RoundRect { x: 0.0, y: 0.0, w, h: LINE_H, r: LINE_H / 2.0, color: background(opacity) }];
    prims.extend(body);
    Layout { w, h: LINE_H, prims }
}

fn card(mt: &Metrics, s: &MetricsSample, opacity: u32, m: &dyn TextMeasure) -> Layout {
    let mut body: Vec<Prim> = Vec::new();
    let mut y = CARD_PAD_TOP;
    let mut first = true;
    let next_row = |y: &mut f32, first: &mut bool| {
        if !*first {
            *y += CARD_GAP;
        }
        *first = false;
    };

    if mt.fps {
        next_row(&mut y, &mut first);
        // The number has `line-height: 1`: a 26px box whose baseline sits at the rounded ascent
        // plus the floored (negative) half-leading, as in Chromium. The row aligns the three
        // texts on one baseline and is as tall as the parts above and below it.
        let num_base = baseline(CARD_NUM) + ((CARD_NUM - line_box(CARD_NUM)) / 2.0).floor();
        let above = num_base.max(baseline(10.0)).max(baseline(10.5));
        let below = (CARD_NUM - num_base).max(line_box(10.0) - baseline(10.0)).max(line_box(10.5) - baseline(10.5));
        let base = y + above;
        let num = fmt_fps(s.fps);
        let mut x = CARD_PAD_X;
        body.push(Prim::Text {
            x,
            y: top_for_baseline(base, CARD_NUM),
            size: CARD_NUM,
            weight: 700,
            color: WHITE,
            letter_spacing: CARD_NUM_TRACKING,
            text: num.clone(),
        });
        x += m.width(&num, CARD_NUM, 700) + CARD_NUM_TRACKING * num.chars().count() as f32 + 4.0;
        body.push(text(x, top_for_baseline(base, 10.0), 10.0, 600, WHITE_65, "FPS"));
        let ms = format!("{} ms", fmt_ms(s.frametime_ms));
        let msx = CARD_RIGHT - m.width(&ms, 10.5, 400);
        body.push(text(msx, top_for_baseline(base, 10.5), 10.5, 400, WHITE_65, &ms));
        y += above + below;
    }
    if mt.graph {
        next_row(&mut y, &mut first);
        let points = graph_points(&s.graph, s.graph_max_ms, CARD_PAD_X, y, 126.0, 26.0);
        if !points.is_empty() {
            body.push(Prim::Polyline { points, width: 1.5, color: GRAPH });
        }
        y += 26.0;
    }
    for (use_on, temp_on, label, label_color, bar_color, use_v, temp_v) in [
        (mt.gpu_use, mt.gpu_temp, "GPU", GPU_LABEL, GPU_BAR, s.gpu_use, s.gpu_temp),
        (mt.cpu_use, mt.cpu_temp, "CPU", CPU_LABEL, CPU_BAR, s.cpu_use, s.cpu_temp),
    ] {
        if !(use_on || temp_on) {
            continue;
        }
        next_row(&mut y, &mut first);
        let row_h = line_box(10.5);
        let val = fmt_pair(use_on, use_v, temp_on, temp_v, Style::Card);
        let vw = m.width(&val, 10.5, 600);
        body.push(text(CARD_PAD_X, y, 10.5, 600, label_color, label));
        body.push(text(CARD_RIGHT - vw, y, 10.5, 600, WHITE, &val));
        if use_on {
            let bx = CARD_PAD_X + 26.0 + CARD_GAP;
            let bw = (CARD_RIGHT - vw - CARD_GAP - bx).max(0.0);
            let by = y + (row_h - 5.0) / 2.0;
            let fill = bw * f32::from(use_v.unwrap_or(0).min(100)) / 100.0;
            body.push(Prim::Bar { x: bx, y: by, w: bw, h: 5.0, r: 3.0, fill, track: BAR_TRACK, color: bar_color });
        }
        y += row_h;
    }
    if first {
        body.push(text(CARD_PAD_X, y, 11.0, 500, WHITE_65, "Nenhuma métrica ativa"));
        y += line_box(11.0);
    }
    let h = y + CARD_PAD_BOTTOM;
    let mut prims = vec![Prim::RoundRect { x: 0.0, y: 0.0, w: CARD_W, h, r: 10.0, color: background(opacity) }];
    prims.extend(body);
    Layout { w: CARD_W, h, prims }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;
    impl TextMeasure for Fake {
        fn width(&self, t: &str, size: f32, _: u16) -> f32 {
            t.chars().count() as f32 * size * 0.5
        }
    }
    fn sample() -> MetricsSample {
        MetricsSample {
            fps: Some(142),
            frametime_ms: Some(6.9),
            graph: vec![7.0; 126],
            graph_max_ms: 8.0,
            gpu_use: Some(96),
            gpu_temp: Some(71),
            cpu_use: Some(38),
            cpu_temp: Some(64),
        }
    }
    fn all_on() -> Metrics {
        Metrics { cpu_temp: true, ..Metrics::default() }
    }
    fn texts(l: &Layout) -> Vec<String> {
        l.prims.iter().filter_map(|p| match p { Prim::Text { text, .. } => Some(text.clone()), _ => None }).collect()
    }
    fn right_edge(l: &Layout, s: &str) -> f32 {
        l.prims
            .iter()
            .find_map(|p| match p {
                Prim::Text { x, text, size, weight, .. } if text == s => Some(x + Fake.width(text, *size, *weight)),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn line_pill_is_26_high_with_all_items() {
        let l = layout(Style::Line, &all_on(), &sample(), 80, &Fake);
        assert_eq!(l.h, 26.0);
        match &l.prims[0] {
            Prim::RoundRect { r, h, color, .. } => assert_eq!((*r, *h, *color), (13.0, 26.0, background(80))),
            p => panic!("{p:?}"),
        }
        let t = texts(&l);
        for want in ["142", "FPS", "GPU", "96% 71°", "CPU", "38% 64°"] {
            assert!(t.contains(&want.to_string()), "{want}");
        }
        assert_eq!(l.prims.iter().filter(|p| matches!(p, Prim::Polyline { .. })).count(), 1);
    }
    #[test]
    fn card_is_148_wide_and_shows_ms() {
        let l = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
        assert_eq!(l.w, 148.0);
        assert!(texts(&l).contains(&"6,9 ms".to_string()));
        assert!(texts(&l).contains(&"96% · 71°".to_string()));
    }
    #[test]
    fn card_temp_only_keeps_value_position_without_bar() {
        let with = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
        let without = layout(Style::Card, &Metrics { gpu_use: false, ..all_on() }, &sample(), 80, &Fake);
        assert_eq!(right_edge(&with, "96% · 71°"), right_edge(&without, "71°"));
        let bars = |l: &Layout| l.prims.iter().filter(|p| matches!(p, Prim::Bar { .. })).count();
        assert_eq!(bars(&with), bars(&without) + 1);
        assert_eq!(with.h, without.h);
    }
    #[test]
    fn placeholders_when_no_metrics() {
        let none = Metrics { fps: false, graph: false, gpu_use: false, gpu_temp: false, cpu_use: false, cpu_temp: false };
        assert!(texts(&layout(Style::Line, &none, &sample(), 80, &Fake)).contains(&"Nenhuma métrica".to_string()));
        assert!(texts(&layout(Style::Card, &none, &sample(), 80, &Fake)).contains(&"Nenhuma métrica ativa".to_string()));
    }
    fn text_prim<'a>(l: &'a Layout, s: &str) -> &'a Prim {
        l.prims.iter().find(|p| matches!(p, Prim::Text { text, .. } if text == s)).unwrap()
    }
    fn text_top(l: &Layout, s: &str) -> f32 {
        match text_prim(l, s) { Prim::Text { y, .. } => *y, _ => unreachable!() }
    }
    fn text_baseline(l: &Layout, s: &str) -> f32 {
        match text_prim(l, s) { Prim::Text { y, size, .. } => y + baseline(*size), _ => unreachable!() }
    }
    #[test]
    fn vertical_metrics_match_the_prototype_in_chromium() {
        // Measured from the prototype's overlay markup rendered by Edge at 100%.
        let card = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
        assert_eq!(card.h, 117.0);
        for s in ["142", "FPS", "6,9 ms"] {
            assert_eq!(text_baseline(&card, s), 32.0, "{s}");
        }
        assert_eq!((text_top(&card, "GPU"), text_baseline(&card, "GPU")), (73.0, 84.0));
        assert_eq!((text_top(&card, "CPU"), text_baseline(&card, "CPU")), (93.0, 104.0));
        let line = layout(Style::Line, &all_on(), &sample(), 80, &Fake);
        for s in ["142", "FPS", "GPU", "96% 71°"] {
            assert_eq!(text_baseline(&line, s), 18.0, "{s}");
        }
        assert_eq!(text_top(&line, "142"), 5.0);
    }
    #[test]
    fn card_number_keeps_the_prototype_tracking() {
        let l = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
        let tracking = match text_prim(&l, "142") { Prim::Text { letter_spacing, .. } => *letter_spacing, _ => unreachable!() };
        assert_eq!(tracking, -0.02 * 26.0);
        // "FPS" follows the tracked number (CSS adds letter-spacing after every character) plus the 4px gap.
        let fps_x = match text_prim(&l, "FPS") { Prim::Text { x, .. } => *x, _ => unreachable!() };
        assert!((fps_x - (11.0 + Fake.width("142", 26.0, 700) + 3.0 * tracking + 4.0)).abs() < 1e-4, "{fps_x}");
    }
    #[test]
    fn card_bar_fill_follows_usage() {
        let l = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
        let bars: Vec<_> = l.prims.iter().filter_map(|p| match p { Prim::Bar { w, fill, color, .. } => Some((*w, *fill, *color)), _ => None }).collect();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].2, GPU_BAR);
        assert!((bars[0].1 - bars[0].0 * 0.96).abs() < 1e-4);
        assert_eq!(bars[1].2, CPU_BAR);
        assert!((bars[1].1 - bars[1].0 * 0.38).abs() < 1e-4);
    }
    #[test]
    fn graph_points_stay_inside_the_layout() {
        let mut s = sample();
        s.graph = vec![1.0, 30.0, 7.0];
        for style in [Style::Line, Style::Card] {
            let l = layout(style, &all_on(), &s, 80, &Fake);
            let pts = l.prims.iter().find_map(|p| match p { Prim::Polyline { points, .. } => Some(points.clone()), _ => None }).unwrap();
            assert!(pts.iter().all(|&(x, y)| (0.0..=l.w).contains(&x) && (0.0..=l.h).contains(&y)), "{style:?}");
        }
    }
}
