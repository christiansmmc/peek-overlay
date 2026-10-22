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
    /// `y` is the top of the line box (height = size × 1.25). The renderer adds the shadow.
    Text { x: f32, y: f32, size: f32, weight: u16, color: Rgba, text: String },
    Polyline { points: Vec<(f32, f32)>, width: f32, color: Rgba },
}

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
/// Line box height as a multiple of font size.
const LINE_BOX: f32 = 1.25;

fn text(x: f32, y: f32, size: f32, weight: u16, color: Rgba, s: &str) -> Prim {
    Prim::Text { x, y, size, weight, color, text: s.to_string() }
}

/// Top of a line box whose approximate baseline (top + size × 1.0) sits on `baseline`.
fn top_for_baseline(baseline: f32, size: f32) -> f32 {
    baseline - size
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
    // 12px text: box 15, centered in the pill; baseline approximated as top + size.
    let top12 = (LINE_H - 12.0 * LINE_BOX) / 2.0;
    let base = top12 + 12.0;

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
        const ROW_H: f32 = 26.0;
        // The 26px number (line-height 1) has a taller line box than its row; center that box
        // in the row and approximate the baseline as top + size. Smaller texts share it.
        let top = y + (ROW_H - 26.0 * LINE_BOX) / 2.0;
        let base = top + 26.0;
        let num = fmt_fps(s.fps);
        let mut x = CARD_PAD_X;
        body.push(text(x, top, 26.0, 700, WHITE, &num));
        x += m.width(&num, 26.0, 700) + 4.0;
        body.push(text(x, top_for_baseline(base, 10.0), 10.0, 600, WHITE_65, "FPS"));
        let ms = format!("{} ms", fmt_ms(s.frametime_ms));
        let msx = CARD_RIGHT - m.width(&ms, 10.5, 400);
        body.push(text(msx, top_for_baseline(base, 10.5), 10.5, 400, WHITE_65, &ms));
        y += ROW_H;
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
        let row_h = 10.5 * LINE_BOX;
        let val = fmt_pair(use_on, use_v, temp_on, temp_v, Style::Card);
        let vw = m.width(&val, 10.5, 600);
        body.push(text(CARD_PAD_X, y, 10.5, 600, label_color, label));
        body.push(text(CARD_RIGHT - vw, y, 10.5, 600, WHITE, &val));
        if use_on {
            let bx = CARD_PAD_X + 26.0 + CARD_GAP;
            let bw = (CARD_RIGHT - vw - CARD_GAP - bx).max(0.0);
            let by = y + (row_h - 5.0) / 2.0;
            body.push(Prim::RoundRect { x: bx, y: by, w: bw, h: 5.0, r: 3.0, color: BAR_TRACK });
            let fw = bw * f32::from(use_v.unwrap_or(0).min(100)) / 100.0;
            if fw > 0.0 {
                body.push(Prim::RoundRect { x: bx, y: by, w: fw, h: 5.0, r: 3.0_f32.min(fw / 2.0), color: bar_color });
            }
        }
        y += row_h;
    }
    if first {
        body.push(text(CARD_PAD_X, y, 11.0, 500, WHITE_65, "Nenhuma métrica ativa"));
        y += 11.0 * LINE_BOX;
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
        let rects = |l: &Layout| l.prims.iter().filter(|p| matches!(p, Prim::RoundRect { .. })).count();
        assert_eq!(rects(&with), rects(&without) + 2);
        assert_eq!(with.h, without.h);
    }
    #[test]
    fn placeholders_when_no_metrics() {
        let none = Metrics { fps: false, graph: false, gpu_use: false, gpu_temp: false, cpu_use: false, cpu_temp: false };
        assert!(texts(&layout(Style::Line, &none, &sample(), 80, &Fake)).contains(&"Nenhuma métrica".to_string()));
        assert!(texts(&layout(Style::Card, &none, &sample(), 80, &Fake)).contains(&"Nenhuma métrica ativa".to_string()));
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
