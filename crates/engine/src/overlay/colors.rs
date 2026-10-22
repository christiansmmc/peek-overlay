#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba {
    Rgba { r, g, b, a }
}

pub const WHITE: Rgba = rgba(255, 255, 255, 255);
pub const WHITE_65: Rgba = rgba(255, 255, 255, 166);
pub const GPU_LABEL: Rgba = rgba(0xB6, 0xA8, 0xFF, 255);
pub const CPU_LABEL: Rgba = rgba(0x73, 0xBD, 0xFF, 255);
pub const GRAPH: Rgba = rgba(0x73, 0xBD, 0xFF, 255);
pub const GPU_BAR: Rgba = rgba(0xA4, 0x92, 0xFB, 255);
pub const CPU_BAR: Rgba = rgba(0x52, 0xA9, 0xFE, 255);
pub const BAR_TRACK: Rgba = rgba(255, 255, 255, 36);
pub const SHADOW: Rgba = rgba(0, 0, 0, 217);

/// Background rgb(12,12,18) with alpha = opacity percent.
pub fn background(opacity_pct: u32) -> Rgba {
    let pct = opacity_pct.min(100);
    rgba(12, 12, 18, ((pct * 255 + 50) / 100) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_follow_handoff() {
        assert_eq!(GPU_LABEL, Rgba { r: 0xB6, g: 0xA8, b: 0xFF, a: 255 });
        assert_eq!(CPU_BAR, Rgba { r: 0x52, g: 0xA9, b: 0xFE, a: 255 });
        assert_eq!(background(80).a, 204);
        assert_eq!(background(30).a, 77);
    }
}
