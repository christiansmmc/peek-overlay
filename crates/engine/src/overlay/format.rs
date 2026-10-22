use crate::settings::Style;

const DASH: &str = "—";

pub fn fmt_fps(v: Option<u32>) -> String {
    v.map_or_else(|| DASH.to_string(), |n| n.to_string())
}

/// One decimal place with a comma ("6,9").
pub fn fmt_ms(v: Option<f32>) -> String {
    v.map_or_else(|| DASH.to_string(), |n| format!("{n:.1}").replace('.', ","))
}

/// "96%" and "71°" parts joined by " " (Line) or " · " (Card); a missing value is "—".
pub fn fmt_pair(use_on: bool, use_v: Option<u8>, temp_on: bool, temp_v: Option<u8>, style: Style) -> String {
    let mut parts = Vec::new();
    if use_on {
        parts.push(use_v.map_or_else(|| DASH.to_string(), |v| format!("{v}%")));
    }
    if temp_on {
        parts.push(temp_v.map_or_else(|| DASH.to_string(), |v| format!("{v}°")));
    }
    let sep = match style {
        Style::Line => " ",
        Style::Card => " · ",
    };
    parts.join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_values_pt_br() {
        assert_eq!(fmt_ms(Some(6.94)), "6,9");
        assert_eq!(fmt_ms(None), "—");
        assert_eq!(fmt_fps(Some(142)), "142");
        assert_eq!(fmt_fps(None), "—");
        assert_eq!(fmt_pair(true, Some(96), true, Some(71), Style::Line), "96% 71°");
        assert_eq!(fmt_pair(true, Some(96), true, Some(71), Style::Card), "96% · 71°");
        assert_eq!(fmt_pair(false, None, true, Some(64), Style::Card), "64°");
        assert_eq!(fmt_pair(true, None, true, Some(64), Style::Card), "— · 64°");
    }
}
