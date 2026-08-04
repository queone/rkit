//! iTerm2 inline-image "winning numbers circle" rendering for `cash5`,
//! ported from Go's `render.go`.
//!
//! `TerminalCapability` (the `isITerm2()` gate) is an injectable seam
//! rather than a bare `env::var` read baked into the call sites: `run_daily`
//! and `display_match_analysis` both carry extensive stdout-content
//! assertions from AC29, and a bare env read would make those tests
//! environment-dependent on any machine actually running iTerm2. The
//! `isITerm2()` check itself stays at the call sites (`mod.rs`,
//! `match_analysis.rs`), matching Go's structure — this module always
//! renders and emits when called.

use ab_glyph::{Font, FontRef, GlyphId, PxScale, ScaleFont, point};
use std::io::{self, Write};

const CIRCLE_SIZE: i32 = 450;
const FONT_SIZE: f32 = 13.5;

const COL_BG: [u8; 4] = [0x14, 0x14, 0x14, 0xff];
const COL_TEXT: [u8; 3] = [0xcc, 0xcc, 0xcc];
const COL_WIN_TEXT: [u8; 3] = [0x0d, 0x0d, 0x0d];
const COL_WIN_BG: [u8; 4] = [0x3a, 0xd5, 0x68, 0xff];
const COL_RING: [u8; 4] = [0x3c, 0x3c, 0x3c, 0xff];
const COL_SPOKE: [u8; 4] = [0x44, 0xee, 0x77, 0xff];

/// The actual "Go Mono" TrueType font Go embeds via
/// `golang.org/x/image/font/gofont/gomono`, extracted from that module's
/// `data.go` (BSD-3-Clause, see `gomono-LICENSE`).
const GOMONO_TTF: &[u8] = include_bytes!("../gomono.ttf");
#[cfg(test)]
const GOMONO_SHA256: &str = "8bc66a0154bbf69cd24e5bde41a12ec9495c8a242c5def2255e4a164900f1ed7";

/// Reports whether the current terminal supports iTerm2's inline-image
/// protocol; injectable so no test's result depends on the real terminal.
pub trait TerminalCapability {
    fn is_iterm2(&self) -> bool;
}

/// Checks `TERM_PROGRAM`, matching Go's `isITerm2()`.
pub struct RealTerminal;

impl TerminalCapability for RealTerminal {
    fn is_iterm2(&self) -> bool {
        std::env::var("TERM_PROGRAM")
            .map(|value| value == "iTerm.app")
            .unwrap_or(false)
    }
}

struct Canvas {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: i32, height: i32, background: [u8; 4]) -> Self {
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for chunk in pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&background);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width && y < self.height
    }

    fn set_pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if !self.in_bounds(x, y) {
            return;
        }
        let index = ((y * self.width + x) * 4) as usize;
        self.pixels[index..index + 4].copy_from_slice(&color);
    }

    /// Alpha-blends `color` onto the existing pixel by `coverage` (0.0-1.0),
    /// matching the compositing Go's `font.Drawer` performs internally for
    /// glyph rendering. Rect/line/ring plotting uses opaque `set_pixel`
    /// instead, matching Go's plain `SetRGBA` calls there.
    fn blend_pixel(&mut self, x: i32, y: i32, color: [u8; 3], coverage: f32) {
        if coverage <= 0.0 || !self.in_bounds(x, y) {
            return;
        }
        let coverage = coverage.min(1.0);
        let index = ((y * self.width + x) * 4) as usize;
        for (channel, &fg) in color.iter().enumerate() {
            let bg = self.pixels[index + channel] as f32;
            self.pixels[index + channel] = (bg + (fg as f32 - bg) * coverage).round() as u8;
        }
    }
}

fn fill_rect(canvas: &mut Canvas, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
    for dy in 0..h {
        for dx in 0..w {
            canvas.set_pixel(x + dx, y + dy, color);
        }
    }
}

/// Draws a 2x2-pixel-wide line, matching Go's `drawLine`.
fn draw_line(canvas: &mut Canvas, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 4]) {
    let dx = (x1 - x0) as f64;
    let dy = (y1 - y0) as f64;
    let steps = dx.abs().max(dy.abs()) as i32 + 1;
    for i in 0..steps {
        let t = i as f64 / (steps - 1).max(1) as f64;
        let x = (x0 as f64 + t * dx).round() as i32;
        let y = (y0 as f64 + t * dy).round() as i32;
        canvas.set_pixel(x, y, color);
        canvas.set_pixel(x + 1, y, color);
        canvas.set_pixel(x, y + 1, color);
        canvas.set_pixel(x + 1, y + 1, color);
    }
}

/// Draws a ~2px-thick ring, matching Go's `drawRing`.
fn draw_ring(canvas: &mut Canvas, cx: i32, cy: i32, r: i32, color: [u8; 4]) {
    let fr = r as f64;
    let y_min = (cy - r - 2).max(0);
    let y_max = (cy + r + 2).min(canvas.height - 1);
    let x_min = (cx - r - 2).max(0);
    let x_max = (cx + r + 2).min(canvas.width - 1);
    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f64).sqrt();
            if d >= fr - 0.5 && d < fr + 1.5 {
                canvas.set_pixel(x, y, color);
            }
        }
    }
}

/// The angle (radians) for number `n` (1-45): 8 degrees apart, starting at
/// 12 o'clock, matching Go's `renderCircle` loop.
fn number_angle_radians(n: i32) -> f64 {
    (-90.0 + (n - 1) as f64 * 8.0) * std::f64::consts::PI / 180.0
}

/// A point at `radius` from `(cx, cy)` at `angle` radians, matching Go's
/// `cx + round(radius*cos(theta))` / `cy + round(radius*sin(theta))`.
fn point_on_circle(cx: i32, cy: i32, radius: i32, angle: f64) -> (i32, i32) {
    let x = cx + (radius as f64 * angle.cos()).round() as i32;
    let y = cy + (radius as f64 * angle.sin()).round() as i32;
    (x, y)
}

struct TextMetrics<'f> {
    font: FontRef<'f>,
    scale: PxScale,
}

impl<'f> TextMetrics<'f> {
    fn new(font: FontRef<'f>, size_px: f32) -> Self {
        Self {
            font,
            scale: PxScale::from(size_px),
        }
    }

    fn width(&self, text: &str) -> f32 {
        let scaled = self.font.as_scaled(self.scale);
        let mut width = 0.0f32;
        let mut prev: Option<GlyphId> = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(prev_id) = prev {
                width += scaled.kern(prev_id, id);
            }
            width += scaled.h_advance(id);
            prev = Some(id);
        }
        width
    }

    /// Half-height of the ascent+descent box, matching Go's
    /// `(m.Ascent.Round()-m.Descent.Round())/2` (`ScaleFont::descent()` is
    /// already negative in `ab_glyph`'s convention, unlike Go's
    /// always-non-negative `font.Metrics.Descent`, so addition here is the
    /// equivalent operation).
    fn baseline_offset(&self) -> f32 {
        let scaled = self.font.as_scaled(self.scale);
        (scaled.ascent() + scaled.descent()) / 2.0
    }

    /// Draws `text` with its baseline positioned so the glyph's
    /// ascent+descent box is vertically centered on `cy`, and horizontally
    /// centered on `cx`, matching Go's `textAt`.
    fn draw_centered(&self, canvas: &mut Canvas, text: &str, cx: i32, cy: i32, color: [u8; 3]) {
        let half_width = self.width(text) / 2.0;
        let origin_x = cx as f32 - half_width;
        let baseline_y = cy as f32 + self.baseline_offset();

        let scaled = self.font.as_scaled(self.scale);
        let mut cursor_x = origin_x;
        let mut prev: Option<GlyphId> = None;
        for c in text.chars() {
            let id = scaled.glyph_id(c);
            if let Some(prev_id) = prev {
                cursor_x += scaled.kern(prev_id, id);
            }
            let glyph = id.with_scale_and_position(self.scale, point(cursor_x, baseline_y));
            if let Some(outlined) = self.font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let origin = (bounds.min.x.round() as i32, bounds.min.y.round() as i32);
                outlined.draw(|gx, gy, coverage| {
                    canvas.blend_pixel(origin.0 + gx as i32, origin.1 + gy as i32, color, coverage);
                });
            }
            cursor_x += scaled.h_advance(id);
            prev = Some(id);
        }
    }
}

/// Places numbers 1-45 evenly around a circle. Winning numbers get a green
/// spoke from the center plus a highlighted background behind the label.
/// Matches Go's `renderCircle`.
fn render_circle(highlighted: &[bool; 46], metrics: &TextMetrics) -> Vec<u8> {
    let size = CIRCLE_SIZE;
    let mut canvas = Canvas::new(size, size, COL_BG);

    let cx = size / 2;
    let cy = size / 2;
    let ring_r = (size as f64 * 0.38).round() as i32;
    let num_r = (size as f64 * 0.44).round() as i32;
    let spoke_r = ring_r - 2;

    draw_ring(&mut canvas, cx, cy, ring_r, COL_RING);

    for n in 1..=45 {
        let angle = number_angle_radians(n);
        let (nx, ny) = point_on_circle(cx, cy, num_r, angle);
        let mut text_color = COL_TEXT;
        if highlighted[n as usize] {
            let (sx, sy) = point_on_circle(cx, cy, spoke_r, angle);
            draw_line(&mut canvas, cx, cy, sx, sy, COL_SPOKE);
            text_color = COL_WIN_TEXT;
            let tw = metrics.width("00").round() as i32;
            let th = {
                let scaled = metrics.font.as_scaled(metrics.scale);
                scaled.ascent().round() as i32 + (-scaled.descent()).round() as i32
            };
            let pad = 3;
            fill_rect(
                &mut canvas,
                nx - tw / 2 - pad,
                ny - th / 2 - pad,
                tw + 2 * pad,
                th + 2 * pad,
                COL_WIN_BG,
            );
        }
        metrics.draw_centered(&mut canvas, &format!("{n:02}"), nx, ny, text_color);
    }

    canvas.pixels
}

fn encode_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buffer, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(pixels)
            .map_err(|error| error.to_string())?;
    }
    Ok(buffer)
}

/// Frames `png_bytes` as an iTerm2 inline-image escape sequence.
fn iterm2_escape(png_bytes: &[u8]) -> String {
    format!(
        "\x1b]1337;File=inline=1;preserveAspectRatio=1:{}\x07\n",
        base64_encode(png_bytes)
    )
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        output.push(BASE64_ALPHABET[(b0 >> 2) as usize] as char);
        output
            .push(BASE64_ALPHABET[(((b0 & 0x03) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char);
        output.push(match b1 {
            Some(b1) => {
                BASE64_ALPHABET[(((b1 & 0x0f) << 2) | (b2.unwrap_or(0) >> 6)) as usize] as char
            }
            None => '=',
        });
        output.push(match b2 {
            Some(b2) => BASE64_ALPHABET[(b2 & 0x3f) as usize] as char,
            None => '=',
        });
    }
    output
}

/// Renders the winning-numbers circle (highlighting `winners`) and emits it
/// as an iTerm2 inline image prefixed by `indent`. Silently no-ops if the
/// embedded font fails to parse, matching Go's `displayCircleImage`'s
/// silent return on a font-load error. Callers must gate on
/// [`TerminalCapability::is_iterm2`] themselves — this function always
/// renders and emits when called, matching Go's structure (the check lives
/// at the call sites, not in `render.go`).
pub fn display_circle_image<W: Write>(
    winners: &[i32],
    indent: &str,
    out: &mut W,
) -> io::Result<()> {
    let Ok(font) = FontRef::try_from_slice(GOMONO_TTF) else {
        return Ok(());
    };
    let metrics = TextMetrics::new(font, FONT_SIZE);

    let mut highlighted = [false; 46];
    for &n in winners {
        if (1..=45).contains(&n) {
            highlighted[n as usize] = true;
        }
    }

    let pixels = render_circle(&highlighted, &metrics);
    let Ok(png_bytes) = encode_png(&pixels, CIRCLE_SIZE as u32, CIRCLE_SIZE as u32) else {
        return Ok(());
    };

    write!(out, "{indent}")?;
    write!(out, "{}", iterm2_escape(&png_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_matches_pinned_checksum() {
        // Reuses the already-pinned `openssl` crate's SHA-256, matching
        // `mdview`'s `STYLESHEET_SHA256` check -- no hashing dependency
        // added.
        let digest = openssl::sha::sha256(GOMONO_TTF);
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(hex, GOMONO_SHA256);
    }

    #[test]
    fn embedded_font_parses() {
        assert!(FontRef::try_from_slice(GOMONO_TTF).is_ok());
    }

    #[test]
    fn number_angle_starts_at_twelve_oclock_and_advances_eight_degrees() {
        assert!((number_angle_radians(1) - (-std::f64::consts::FRAC_PI_2)).abs() < 1e-9);
        let expected_step = 8.0_f64.to_radians();
        assert!((number_angle_radians(2) - number_angle_radians(1) - expected_step).abs() < 1e-9);
    }

    #[test]
    fn point_on_circle_places_number_one_directly_above_center() {
        let (x, y) = point_on_circle(100, 100, 50, number_angle_radians(1));
        assert_eq!(x, 100);
        assert_eq!(y, 50);
    }

    #[test]
    fn point_on_circle_places_number_twelve_directly_right_of_center() {
        // n=12: angle = -90 + 11*8 = -2 degrees -- close to due east (n
        // that lands exactly at 0 degrees is n=12.25, not an integer), so
        // check n=12 and n=13 straddle due east instead.
        let (x12, y12) = point_on_circle(100, 100, 50, number_angle_radians(12));
        let (x13, y13) = point_on_circle(100, 100, 50, number_angle_radians(13));
        assert!(x12 > 100 && x13 > 100);
        assert!(y12 < 100 && y13 > 100);
    }

    #[test]
    fn fill_rect_and_set_pixel_clip_to_canvas_bounds() {
        let mut canvas = Canvas::new(4, 4, [0, 0, 0, 255]);
        fill_rect(&mut canvas, 2, 2, 10, 10, [255, 255, 255, 255]);
        // Out-of-bounds pixels are silently dropped, not a panic.
        canvas.set_pixel(-1, -1, [1, 2, 3, 4]);
        canvas.set_pixel(100, 100, [1, 2, 3, 4]);
        let index = ((3 * 4 + 3) * 4) as usize;
        assert_eq!(&canvas.pixels[index..index + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn blend_pixel_interpolates_toward_foreground_by_coverage() {
        let mut canvas = Canvas::new(2, 2, [0, 0, 0, 255]);
        canvas.blend_pixel(0, 0, [255, 255, 255], 0.5);
        assert_eq!(&canvas.pixels[0..3], &[128, 128, 128]);
        canvas.blend_pixel(1, 0, [255, 255, 255], 0.0);
        assert_eq!(&canvas.pixels[4..7], &[0, 0, 0]);
    }

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn iterm2_escape_frames_exactly() {
        let escape = iterm2_escape(b"foo");
        assert_eq!(
            escape,
            "\x1b]1337;File=inline=1;preserveAspectRatio=1:Zm9v\x07\n"
        );
    }

    #[test]
    fn encode_png_produces_valid_png_signature() {
        let pixels = vec![0u8; 2 * 2 * 4];
        let png_bytes = encode_png(&pixels, 2, 2).unwrap();
        assert_eq!(
            &png_bytes[0..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
    }

    #[test]
    fn display_circle_image_writes_indent_and_valid_escape_sequence() {
        let mut out = Vec::new();
        display_circle_image(&[1, 15, 45], "  ", &mut out).unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("  \x1b]1337;File=inline=1;preserveAspectRatio=1:"));
        assert!(text.ends_with("\x07\n"));
    }
}
