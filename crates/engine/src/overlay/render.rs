//! Software Direct2D renderer for the overlay: a DC render target bound to a 32-bpp
//! top-down premultiplied DIB, DirectWrite text. Creates no hardware/GPU D3D device;
//! Direct2D's software target uses WARP internally.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Foundation::{D2DERR_RECREATE_TARGET, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_OPEN,
    D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_ANTIALIAS_MODE_ALIASED, D2D1_CAP_STYLE_FLAT, D2D1_DASH_STYLE_SOLID, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LINE_JOIN_ROUND, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush, ID2D1StrokeStyle,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FEATURE, DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT, DWRITE_LINE_SPACING_METHOD_UNIFORM,
    DWRITE_TEXT_METRICS, DWRITE_TEXT_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteTextFormat, IDWriteTextLayout, IDWriteTextLayout1, IDWriteTypography,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GdiFlush, HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows::core::{Error, Interface, Result, w};
use windows_numerics::{Matrix3x2, Vector2};

use super::colors::{Rgba, SHADOW};
use super::layout::{Layout, Prim, TextMeasure, baseline, line_box};

/// Width given to text layouts; texts never wrap, so it only has to be large.
const MAX_TEXT_W: f32 = 4096.0;

/// 32-bpp top-down DIB selected into its own memory DC.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *const u8,
    w: i32,
    h: i32,
}

impl Surface {
    fn new(w: i32, h: i32) -> Result<Surface> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err(Error::from_thread());
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // negative = top-down rows
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(b) => b,
                Err(e) => {
                    let _ = DeleteDC(dc);
                    return Err(e);
                }
            };
            let old = SelectObject(dc, bitmap.into());
            Ok(Surface { dc, bitmap, old, bits: bits.cast(), w, h })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// One rendered overlay image: `w`×`h` premultiplied BGRA pixels in a memory DC.
pub struct Frame {
    pub w: i32,
    pub h: i32,
    surface: Rc<Surface>,
}

impl Frame {
    /// Premultiplied BGRA of the pixel at (`x`, `y`). Panics outside the frame.
    pub fn pixel(&self, x: i32, y: i32) -> [u8; 4] {
        assert!((0..self.w).contains(&x) && (0..self.h).contains(&y), "pixel ({x}, {y}) outside {}x{}", self.w, self.h);
        let i = ((y * self.w + x) * 4) as usize;
        let len = (self.w * self.h * 4) as usize;
        // SAFETY: the DIB holds w*h 32-bit pixels and lives as long as `surface`.
        let px = unsafe { std::slice::from_raw_parts(self.surface.bits, len) };
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    /// Memory DC holding the image, as the source for `UpdateLayeredWindow`.
    pub fn dc(&self) -> HDC {
        self.surface.dc
    }
}

pub struct Renderer {
    d2d: ID2D1Factory,
    dwrite: IDWriteFactory,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    round_join: ID2D1StrokeStyle,
    typography: IDWriteTypography,
    /// Text formats by (size bits, weight); `RefCell` because measuring takes `&self`.
    formats: RefCell<HashMap<(u32, u16), IDWriteTextFormat>>,
    /// DIB of the last frame, reused while the pixel size stays the same and no `Frame` holds it.
    surface: Option<Rc<Surface>>,
    /// `target` is bound to `surface`. `BindDC` is only needed for a new DC or size, and it
    /// costs about as much as drawing a whole frame.
    bound: bool,
}

/// `x, y, w, h` (logical px) with every edge moved to the nearest whole device pixel at
/// `scale`, the way Chromium paints boxes; keeps thin bars crisp instead of half-covered rows.
fn snapped(x: f32, y: f32, w: f32, h: f32, scale: f32) -> D2D_RECT_F {
    let snap = |v: f32| (v * scale).round() / scale;
    D2D_RECT_F { left: snap(x), top: snap(y), right: snap(x + w), bottom: snap(y + h) }
}

/// CSS clamps a radius larger than half the box to half the box.
fn rounded(rect: D2D_RECT_F, r: f32) -> D2D1_ROUNDED_RECT {
    let r = r.min((rect.right - rect.left) / 2.0).min((rect.bottom - rect.top) / 2.0).max(0.0);
    D2D1_ROUNDED_RECT { rect, radiusX: r, radiusY: r }
}

fn color(c: Rgba) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: f32::from(c.r) / 255.0, g: f32::from(c.g) / 255.0, b: f32::from(c.b) / 255.0, a: f32::from(c.a) / 255.0 }
}

fn create_target(d2d: &ID2D1Factory) -> Result<(ID2D1DCRenderTarget, ID2D1SolidColorBrush)> {
    let props = D2D1_RENDER_TARGET_PROPERTIES {
        r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        // 96 DPI: one DIP is one pixel; the scale comes from the transform.
        dpiX: 96.0,
        dpiY: 96.0,
        usage: D2D1_RENDER_TARGET_USAGE_NONE,
        minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
    };
    unsafe {
        let target = d2d.CreateDCRenderTarget(&props)?;
        target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
        let brush = target.CreateSolidColorBrush(&color(Rgba { r: 255, g: 255, b: 255, a: 255 }), None)?;
        Ok((target, brush))
    }
}

impl Renderer {
    pub fn new() -> Result<Self> {
        unsafe {
            let d2d: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let (target, brush) = create_target(&d2d)?;
            let round_join = d2d.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_FLAT,
                    endCap: D2D1_CAP_STYLE_FLAT,
                    dashCap: D2D1_CAP_STYLE_FLAT,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                },
                None,
            )?;
            let typography = dwrite.CreateTypography()?;
            typography.AddFontFeature(DWRITE_FONT_FEATURE { nameTag: DWRITE_FONT_FEATURE_TAG_TABULAR_FIGURES, parameter: 1 })?;
            Ok(Renderer {
                d2d,
                dwrite,
                target,
                brush,
                round_join,
                typography,
                formats: RefCell::new(HashMap::new()),
                surface: None,
                bound: false,
            })
        }
    }

    /// Cached "Segoe UI Variable Display" format whose line box is `line_box(size)` tall with
    /// the baseline at `baseline(size)`, matching the layout's contract.
    fn format(&self, size: f32, weight: u16) -> Result<IDWriteTextFormat> {
        let key = (size.to_bits(), weight);
        if let Some(f) = self.formats.borrow().get(&key) {
            return Ok(f.clone());
        }
        let f = unsafe {
            let f = self.dwrite.CreateTextFormat(
                w!("Segoe UI Variable Display"),
                None,
                DWRITE_FONT_WEIGHT(i32::from(weight)),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("pt-BR"),
            )?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            f.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line_box(size), baseline(size))?;
            f
        };
        self.formats.borrow_mut().insert(key, f.clone());
        Ok(f)
    }

    /// One line of text with tabular figures and `letter_spacing` after every character.
    /// Measuring and drawing build it the same way.
    fn text_layout(&self, text: &str, size: f32, weight: u16, letter_spacing: f32) -> Result<IDWriteTextLayout> {
        let format = self.format(size, weight)?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let all = DWRITE_TEXT_RANGE { startPosition: 0, length: wide.len() as u32 };
        unsafe {
            let tl = self.dwrite.CreateTextLayout(&wide, &format, MAX_TEXT_W, line_box(size))?;
            tl.SetTypography(&self.typography, all)?;
            if letter_spacing != 0.0 {
                tl.cast::<IDWriteTextLayout1>()?.SetCharacterSpacing(0.0, letter_spacing, 0.0, all)?;
            }
            Ok(tl)
        }
    }

    /// Reuses the cached DIB when it has the requested size and no `Frame` still holds it.
    fn surface(&mut self, w: i32, h: i32) -> Result<Rc<Surface>> {
        if let Some(s) = &self.surface
            && s.w == w
            && s.h == h
            && Rc::strong_count(s) == 1
        {
            return Ok(s.clone());
        }
        let s = Rc::new(Surface::new(w, h)?);
        self.surface = Some(s.clone());
        self.bound = false;
        Ok(s)
    }

    /// Draws `layout` (logical px) at `scale` into a `ceil(w·scale)×ceil(h·scale)` frame.
    pub fn render(&mut self, layout: &Layout, scale: f32) -> Result<Frame> {
        let w = ((layout.w * scale).ceil() as i32).max(1);
        let h = ((layout.h * scale).ceil() as i32).max(1);
        let surface = self.surface(w, h)?;
        unsafe {
            if !self.bound {
                self.target.BindDC(surface.dc, &RECT { left: 0, top: 0, right: w, bottom: h })?;
                self.bound = true;
            }
            self.target.BeginDraw();
            self.target.SetTransform(&Matrix3x2::scale(scale, scale));
            self.target.Clear(Some(&D2D1_COLOR_F::default()));
            let drawn = layout.prims.iter().try_for_each(|p| self.draw(p, scale));
            let ended = self.target.EndDraw(None, None);
            if let Err(e) = &ended
                && e.code() == D2DERR_RECREATE_TARGET
            {
                (self.target, self.brush) = create_target(&self.d2d)?;
                self.bound = false;
            }
            drawn?;
            ended?;
            // Make sure GDI finished writing the DIB before anyone reads its bits.
            let _ = GdiFlush();
        }
        Ok(Frame { w, h, surface })
    }

    fn draw(&self, prim: &Prim, scale: f32) -> Result<()> {
        unsafe {
            match prim {
                Prim::RoundRect { x, y, w, h, r, color: c } => {
                    self.brush.SetColor(&color(*c));
                    self.target.FillRoundedRectangle(&rounded(snapped(*x, *y, *w, *h, scale), *r), &self.brush);
                }
                Prim::Bar { x, y, w, h, r, fill, track, color: c } => {
                    let rect = snapped(*x, *y, *w, *h, scale);
                    let shape = rounded(rect, *r);
                    self.brush.SetColor(&color(*track));
                    self.target.FillRoundedRectangle(&shape, &self.brush);
                    let end = snapped(*x, *y, *fill, *h, scale).right;
                    if end > rect.left {
                        self.target.PushAxisAlignedClip(&D2D_RECT_F { right: end, ..rect }, D2D1_ANTIALIAS_MODE_ALIASED);
                        self.brush.SetColor(&color(*c));
                        self.target.FillRoundedRectangle(&shape, &self.brush);
                        self.target.PopAxisAlignedClip();
                    }
                }
                Prim::Text { x, y, size, weight, color: c, letter_spacing, text } => {
                    let tl = self.text_layout(text, *size, *weight, *letter_spacing)?;
                    self.brush.SetColor(&color(SHADOW));
                    self.target.DrawTextLayout(Vector2 { X: *x, Y: y + 1.0 }, &tl, &self.brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                    self.brush.SetColor(&color(*c));
                    self.target.DrawTextLayout(Vector2 { X: *x, Y: *y }, &tl, &self.brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
                Prim::Polyline { points, width, color: c } => {
                    let Some((first, rest)) = points.split_first() else { return Ok(()) };
                    let geometry = self.d2d.CreatePathGeometry()?;
                    let sink = geometry.Open()?;
                    sink.BeginFigure(Vector2 { X: first.0, Y: first.1 }, D2D1_FIGURE_BEGIN_HOLLOW);
                    let rest: Vec<Vector2> = rest.iter().map(|&(x, y)| Vector2 { X: x, Y: y }).collect();
                    sink.AddLines(&rest);
                    sink.EndFigure(D2D1_FIGURE_END_OPEN);
                    sink.Close()?;
                    self.brush.SetColor(&color(*c));
                    self.target.DrawGeometry(&geometry, &self.brush, *width, &self.round_join);
                }
            }
        }
        Ok(())
    }
}

impl TextMeasure for Renderer {
    /// Advance width of `text` drawn as `render` draws it (same format and typography) without
    /// letter spacing; the layout adds that itself.
    fn width(&self, text: &str, size: f32, weight: u16) -> f32 {
        let mut m = DWRITE_TEXT_METRICS::default();
        match self.text_layout(text, size, weight, 0.0).and_then(|tl| unsafe { tl.GetMetrics(&mut m) }) {
            Ok(()) => m.widthIncludingTrailingWhitespace,
            Err(e) => {
                crate::trace!("overlay: measuring {text:?} failed: {e}");
                text.chars().count() as f32 * size * 0.55
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MetricsSample;
    use crate::overlay::layout::layout;
    use crate::settings::{Metrics, Style};

    fn sample() -> MetricsSample {
        MetricsSample { fps: Some(142), frametime_ms: Some(6.9), graph: vec![7.0; 126], graph_max_ms: 8.0,
                        gpu_use: Some(96), gpu_temp: Some(71), cpu_use: Some(38), cpu_temp: Some(64) }
    }
    #[test]
    fn renders_card_with_transparent_corner_and_background_alpha() {
        let mut r = Renderer::new().unwrap();
        let l = layout(Style::Card, &Metrics { cpu_temp: true, ..Metrics::default() }, &sample(), 80, &r);
        let f = r.render(&l, 1.0).unwrap();
        assert_eq!((f.w, f.h), (148, l.h.ceil() as i32));
        assert_eq!(f.pixel(0, 0)[3], 0);
        let a = f.pixel(74, f.h - 3)[3];
        assert!((202..=206).contains(&a), "alpha {a}");
        assert_eq!(r.render(&l, 1.5).unwrap().w, (148.0f32 * 1.5).ceil() as i32);
    }
    #[test]
    fn same_size_frames_reuse_the_dib_and_show_the_new_content() {
        let mut r = Renderer::new().unwrap();
        let alpha_at_bottom = |r: &mut Renderer, opacity: u32| {
            let l = layout(Style::Card, &Metrics::default(), &sample(), opacity, &*r);
            let f = r.render(&l, 1.0).unwrap();
            (f.dc(), f.pixel(74, f.h - 3)[3])
        };
        let (dc80, a80) = alpha_at_bottom(&mut r, 80);
        let (dc30, a30) = alpha_at_bottom(&mut r, 30);
        assert_eq!(dc80, dc30);
        assert!((202..=206).contains(&a80), "alpha {a80}");
        assert!((75..=79).contains(&a30), "alpha {a30}");
        // A frame still alive is never drawn over: the next frame gets its own DIB.
        let l = layout(Style::Card, &Metrics::default(), &sample(), 80, &r);
        let held = r.render(&l, 1.0).unwrap();
        let next = r.render(&layout(Style::Card, &Metrics::default(), &sample(), 30, &r), 1.0).unwrap();
        assert_ne!(held.dc(), next.dc());
        assert!((202..=206).contains(&held.pixel(74, held.h - 3)[3]));
    }
    fn bgra(c: crate::overlay::colors::Rgba) -> [u8; 4] {
        [c.b, c.g, c.r, c.a]
    }
    #[test]
    fn bars_snap_to_whole_pixels_and_the_fill_ends_square() {
        let mut r = Renderer::new().unwrap();
        let l = layout(Style::Card, &Metrics { cpu_temp: true, ..Metrics::default() }, &sample(), 100, &r);
        let f = r.render(&l, 1.0).unwrap();
        let bars: Vec<_> = l.prims.iter().filter_map(|p| match p { Prim::Bar { x, y, w, h, r, fill, color, .. } => Some((*x, *y, *w, *h, *r, *fill, *color)), _ => None }).collect();
        assert_eq!(bars.len(), 2);
        for (x, y, w, h, r, fill, color) in bars {
            // Chromium paints boxes on whole pixels: a 5px bar at y = 77.5 covers rows 78..=82.
            let (top, bottom, end) = (y.round() as i32, (y + h).round() as i32, (x + fill).round() as i32);
            let probe = x.round() as i32 + 4;
            assert_eq!(f.pixel(probe, top - 1), bgra(crate::overlay::colors::background(100)));
            assert_eq!(f.pixel(probe, top), bgra(color));
            assert_eq!(f.pixel(probe, bottom - 1), bgra(color));
            assert_eq!(f.pixel(probe, bottom), bgra(crate::overlay::colors::background(100)));
            // The fill is clipped by the track: before the track's rounded right end, the fill's
            // last column is full even on the top row (a rounded fill end would not be).
            if fill + r < w {
                assert_eq!(f.pixel(end - 1, top), bgra(color));
                assert_ne!(f.pixel(end, top + 2), bgra(color));
            }
        }
    }
}
